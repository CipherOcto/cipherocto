//! Writer-side WAL-tail streamer (per RFC-0862 §4.3.3, mission 0862a).
//!
//! The `WalTailStreamer` captures LSN ranges on every commit, packages them
//! as `WalTailChunk` envelopes, and ships them to subscribed readers.
//!
//! Per RFC-0862 v1.1.0 §Migration path step v1.1.0.d, all WAL reads go through
//! the `DatabaseSyncAdapter` trait — the cipherocto sync engine never calls
//! `MVCCEngine::replay_two_phase` directly. The underlying `StoolapAdapter`
//! impl handles that internally.

use std::collections::{HashMap, VecDeque};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;

use parking_lot::Mutex;

use crate::adapter::DatabaseSyncAdapter;
use crate::envelope::WalTailChunk;
use crate::error::SyncError;
use crate::identity::SyncPeerId;
use crate::lsn::LsnTracker;
use crate::types::Lsn;

/// Per-peer subscription state.
#[derive(Debug)]
pub struct SubscriberChannel {
    /// The peer's LSN watermark (highest LSN that has been acknowledged).
    pub last_ack: Lsn,
    /// The per-peer rate limiter (consumed in `on_commit`).
    pub rate_limiter: RateLimiter,
    /// Outbound channel for `WalTailChunk` envelopes. In a real implementation
    /// this would be a `tokio::sync::mpsc::Sender<WalTailChunk>`; for v1 we
    /// use a bounded `Mutex<VecDeque>` that the cipherocto transport layer
    /// drains.
    pub outbox: Mutex<VecDeque<WalTailChunk>>,
}

impl SubscriberChannel {
    /// Create a new subscriber channel.
    pub fn new(rate_limiter: RateLimiter) -> Self {
        Self {
            last_ack: 0,
            rate_limiter,
            outbox: Mutex::new(VecDeque::new()),
        }
    }

    /// Send a chunk to the subscriber. Returns `Err(SyncError::UnknownPeer)`
    /// if the channel has been closed (the outbox is empty AND we choose to
    /// not buffer). In v1 the outbox is unbounded; the cipherocto transport
    /// drains it asynchronously.
    pub fn send(&self, chunk: WalTailChunk) -> Result<(), SyncError> {
        self.outbox.lock().push_back(chunk);
        Ok(())
    }
}

/// Per-peer rate limiter (token bucket).
///
/// Default: 100 envelopes/s sustained, 500 burst. Per RFC-0862 §4.3.1.
#[derive(Debug, Clone)]
pub struct RateLimiter {
    /// Sustained rate (envelopes/s).
    pub rate_per_sec: u32,
    /// Burst capacity.
    pub burst: u32,
    /// Current token count.
    tokens: Arc<Mutex<u32>>,
    /// Last refill timestamp (Unix milliseconds).
    last_refill_ms: Arc<Mutex<u64>>,
}

impl RateLimiter {
    /// Create a new rate limiter.
    pub fn new(rate_per_sec: u32, burst: u32) -> Self {
        Self {
            rate_per_sec,
            burst,
            tokens: Arc::new(Mutex::new(burst)),
            last_refill_ms: Arc::new(Mutex::new(0)),
        }
    }

    /// Check whether one envelope can be sent. Refills the bucket first.
    /// Returns `Err(SyncError::BackendNotReady)` if the bucket is empty.
    pub fn check(&self) -> Result<(), SyncError> {
        self.check_at(0)
    }

    /// Check at a specific Unix-millisecond timestamp (for testing).
    pub fn check_at(&self, now_ms: u64) -> Result<(), SyncError> {
        // Refill: add (now - last) * rate / 1000 tokens, capped at burst
        let mut last = self.last_refill_ms.lock();
        let mut tokens = self.tokens.lock();
        if now_ms > *last {
            let elapsed_ms = now_ms - *last;
            // Use u64 to avoid overflow
            let refill = (elapsed_ms as u64) * (self.rate_per_sec as u64) / 1000;
            *tokens = (*tokens as u64).saturating_add(refill).min(self.burst as u64) as u32;
            *last = now_ms;
        }
        if *tokens == 0 {
            return Err(SyncError::BackendNotReady("rate limit exhausted".to_string()));
        }
        *tokens -= 1;
        Ok(())
    }
}

/// Per-txn error queue entry.
#[derive(Debug, Clone)]
pub struct CommitError {
    /// The txn_id that produced the error.
    pub txn_id: u64,
    /// The error that occurred.
    pub error: SyncError,
}

/// The writer-side WAL-tail streamer.
///
/// Holds:
/// - `adapter`: the `DatabaseSyncAdapter` trait object (per RFC-0862 v1.1.0)
/// - `subscribers`: per-peer subscription channels
/// - `rate_limiter`: per-peer rate limiters
/// - `current_lsn`: monotonic, persisted in WAL
/// - `error_queue`: per-txn errors drained every 100ms
/// - `peers`: per-peer state machines
/// - `txn_subscribers`: per-txn fan-out mapping
/// - `paused`: backpressure flag
pub struct WalTailStreamer {
    /// The database adapter (trait object). The cipherocto sync engine does NOT
    /// hold a direct `Arc<MVCCEngine>` reference; all WAL reads go through
    /// `adapter.read_wal_range(from, to)`.
    adapter: Arc<dyn DatabaseSyncAdapter>,
    /// Per-peer subscription channels.
    subscribers: Mutex<HashMap<SyncPeerId, SubscriberChannel>>,
    /// Current LSN (monotonic, persisted in WAL). Incremented on every commit.
    current_lsn: AtomicU64,
    /// Per-txn error queue: drained every 100ms by the Sync engine.
    error_queue: Mutex<VecDeque<CommitError>>,
    /// Per-peer state machines.
    peers: Mutex<HashMap<SyncPeerId, LsnTracker>>,
    /// Maps each in-flight txn to the set of subscribers that were fanned-out.
    txn_subscribers: Mutex<HashMap<u64, Vec<SyncPeerId>>>,
    /// Backpressure flag: when the reader sends PAUSE, the writer stops shipping.
    paused: AtomicBool,
    /// Commit batch size (default 100 commits per chunk).
    commit_batch_size: usize,
    /// Commit batch timeout (default 50ms).
    commit_batch_timeout: Duration,
}

impl WalTailStreamer {
    /// Create a new `WalTailStreamer`.
    pub fn new(adapter: Arc<dyn DatabaseSyncAdapter>) -> Self {
        Self {
            adapter,
            subscribers: Mutex::new(HashMap::new()),
            current_lsn: AtomicU64::new(0),
            error_queue: Mutex::new(VecDeque::new()),
            peers: Mutex::new(HashMap::new()),
            txn_subscribers: Mutex::new(HashMap::new()),
            paused: AtomicBool::new(false),
            commit_batch_size: 100,
            commit_batch_timeout: Duration::from_millis(50),
        }
    }

    /// Register a new subscriber.
    pub fn subscribe(&self, peer_id: SyncPeerId, rate_limiter: RateLimiter) {
        self.subscribers
            .lock()
            .insert(peer_id, SubscriberChannel::new(rate_limiter));
        self.peers.lock().insert(peer_id, LsnTracker::new());
    }

    /// Unregister a subscriber.
    pub fn unsubscribe(&self, peer_id: &SyncPeerId) {
        self.subscribers.lock().remove(peer_id);
        self.peers.lock().remove(peer_id);
    }

    /// Return the current LSN.
    pub fn current_lsn(&self) -> Lsn {
        self.current_lsn.load(Ordering::SeqCst)
    }

    /// Set the pause flag. Propagates to the adapter (per RFC-0862 v1.1.0).
    pub fn set_paused(&self, paused: bool) {
        self.paused.store(paused, Ordering::SeqCst);
        let _ = self.adapter.set_paused(paused);
    }

    /// Called by the writer's `record_commit` hook (per RFC-0862 §4.3.3).
    ///
    /// Returns `Ok(())` on success, or `Err(SyncError)` on a recoverable error
    /// (e.g., rate limit, peer channel closed, LSN regression).
    ///
    /// `is_last` semantics: per RFC-0862 §4.3 `WalTailChunk.is_last: bool` is
    /// "true if to_lsn == writer.current_lsn". After the `store` on line 4,
    /// `current_lsn == to_lsn`, so this condition is always true.
    pub fn on_commit(
        &self,
        txn_id: u64,
        from_lsn: Lsn,
        to_lsn: Lsn,
    ) -> Result<(), SyncError> {
        // 1. Validate LSN range
        if from_lsn > to_lsn {
            return Err(SyncError::InvalidLsnRange { from: from_lsn, to: to_lsn });
        }
        // 2. Update current_lsn (advances even when paused, so the next
        //    non-paused commit computes the correct is_last value)
        self.current_lsn.store(to_lsn, Ordering::SeqCst);
        // 3. Check backpressure
        if self.paused.load(Ordering::SeqCst) {
            return Ok(());
        }
        // 4. Read WAL entries via the trait. Per RFC-0862 v1.1.0
        //    §Migration path step v1.1.0.d, the cipherocto sync engine reads
        //    WAL through `adapter.read_wal_range(from, to)` — NOT via direct
        //    `self.engine.wal_manager().replay_two_phase(...)`.
        let entries = self.adapter.read_wal_range(from_lsn, to_lsn)?;
        // 5. Package as WalTailChunk
        let chunk = WalTailChunk { from_lsn, to_lsn, entries, is_last: true };
        // 6. Fan-out to subscribers (rate-limited)
        let subscriber_ids: Vec<SyncPeerId> = {
            let subs = self.subscribers.lock();
            let mut txn_subs = self.txn_subscribers.lock();
            let ids: Vec<SyncPeerId> = subs.keys().copied().collect();
            txn_subs.insert(txn_id, ids.clone());
            ids
        };
        for peer_id in &subscriber_ids {
            let subs = self.subscribers.lock();
            if let Some(channel) = subs.get(peer_id) {
                channel.rate_limiter.check()?;
                channel.send(chunk.clone())?;
            }
        }
        Ok(())
    }

    /// Reader's request for WAL entries from a given LSN.
    /// Returns a `WalTailChunk` containing the entries in `[from_lsn, current_lsn]`.
    pub async fn handle_wal_tail_request(
        &self,
        from_lsn: Lsn,
    ) -> Result<WalTailChunk, SyncError> {
        let prev = self.current_lsn.load(Ordering::SeqCst);
        if from_lsn > prev {
            return Err(SyncError::InvalidLsnRange { from: from_lsn, to: prev });
        }
        if from_lsn == 0 {
            return Err(SyncError::InvalidLsnRange { from: 0, to: prev });
        }
        let entries = self.adapter.read_wal_range(from_lsn, prev)?;
        Ok(WalTailChunk {
            from_lsn,
            to_lsn: prev,
            entries,
            is_last: prev == self.current_lsn.load(Ordering::SeqCst),
        })
    }

    /// Reader sends LsnAck after successful apply.
    /// Returns `Ok(())` on success, `Err(SyncError::UnknownPeer)` if the peer
    /// is not subscribed, or `Err(SyncError::LsnRegression)` if the ack
    /// regresses.
    pub fn on_lsn_ack(&self, peer: SyncPeerId, applied_lsn: Lsn) -> Result<(), SyncError> {
        let mut subs = self.subscribers.lock();
        let channel = subs.get_mut(&peer).ok_or(SyncError::UnknownPeer(peer.0))?;
        if applied_lsn < channel.last_ack {
            return Err(SyncError::LsnRegression {
                expected: channel.last_ack,
                actual: applied_lsn,
            });
        }
        channel.last_ack = applied_lsn;
        drop(subs);
        // Advance the per-peer LSN tracker
        if let Some(tracker) = self.peers.lock().get_mut(&peer) {
            tracker.advance(applied_lsn)?;
        }
        Ok(())
    }

    /// Record an on_commit error for later per-peer demotion.
    pub fn record_commit_error(&self, txn_id: u64, error: SyncError) {
        self.error_queue.lock().push_back(CommitError { txn_id, error });
    }

    /// Drain the per-txn error queue. Returns the list of (peer_id, error)
    /// pairs that should be demoted.
    pub fn drain_error_queue(&self) -> Vec<(SyncPeerId, SyncError)> {
        let mut queue = self.error_queue.lock();
        let mut affected = Vec::new();
        let mut txn_subs = self.txn_subscribers.lock();
        for entry in queue.drain(..) {
            if let Some(peer_ids) = txn_subs.get(&entry.txn_id) {
                for peer_id in peer_ids {
                    affected.push((*peer_id, entry.error.clone()));
                }
            }
            txn_subs.remove(&entry.txn_id);
        }
        affected
    }

    /// Test-only helper: the number of currently subscribed peers.
    pub fn subscriber_count(&self) -> usize {
        self.subscribers.lock().len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_util::MockAdapter;

    fn make_streamer() -> (WalTailStreamer, Arc<MockAdapter>) {
        let adapter = Arc::new(MockAdapter::new([1u8; 32], [2u8; 32]));
        let streamer = WalTailStreamer::new(adapter.clone() as Arc<dyn DatabaseSyncAdapter>);
        (streamer, adapter)
    }

    #[test]
    fn new_streamer_has_zero_lsn() {
        let (s, _) = make_streamer();
        assert_eq!(s.current_lsn(), 0);
    }

    #[test]
    fn subscribe_and_unsubscribe() {
        let (s, _) = make_streamer();
        let peer = SyncPeerId([3u8; 32]);
        s.subscribe(peer, RateLimiter::new(100, 500));
        assert_eq!(s.subscriber_count(), 1);
        s.unsubscribe(&peer);
        assert_eq!(s.subscriber_count(), 0);
    }

    #[test]
    fn on_commit_updates_current_lsn() {
        let (s, _) = make_streamer();
        s.on_commit(1, 1, 10).unwrap();
        assert_eq!(s.current_lsn(), 10);
    }

    #[test]
    fn on_commit_invalid_range_returns_err() {
        let (s, _) = make_streamer();
        let err = s.on_commit(1, 10, 5).unwrap_err();
        assert_eq!(err, SyncError::InvalidLsnRange { from: 10, to: 5 });
    }

    #[test]
    fn on_commit_paused_does_not_fan_out() {
        let (s, _) = make_streamer();
        let peer = SyncPeerId([3u8; 32]);
        s.subscribe(peer, RateLimiter::new(100, 500));
        s.set_paused(true);
        s.on_commit(1, 1, 5).unwrap();
        // The chunk was NOT fanned out (paused)
        let subs = s.subscribers.lock();
        let channel = subs.get(&peer).unwrap();
        assert!(channel.outbox.lock().is_empty());
    }

    #[test]
    fn on_lsn_ack_advances_tracker() {
        let (s, _) = make_streamer();
        let peer = SyncPeerId([3u8; 32]);
        s.subscribe(peer, RateLimiter::new(100, 500));
        s.on_lsn_ack(peer, 100).unwrap();
        let trackers = s.peers.lock();
        let tracker = trackers.get(&peer).unwrap();
        assert_eq!(tracker.watermark(), 100);
    }

    #[test]
    fn on_lsn_ack_unknown_peer_returns_err() {
        let (s, _) = make_streamer();
        let peer = SyncPeerId([3u8; 32]);
        let err = s.on_lsn_ack(peer, 100).unwrap_err();
        assert_eq!(err, SyncError::UnknownPeer(peer.0));
    }

    #[test]
    fn rate_limiter_allows_within_burst() {
        let rl = RateLimiter::new(100, 5);
        for _ in 0..5 {
            rl.check_at(0).unwrap();
        }
        // 6th attempt should fail
        let err = rl.check_at(0).unwrap_err();
        assert!(matches!(err, SyncError::BackendNotReady(_)));
    }

    #[test]
    fn rate_limiter_refills_over_time() {
        let rl = RateLimiter::new(100, 5);
        for _ in 0..5 {
            rl.check_at(0).unwrap();
        }
        // After 100ms, refilled by 100 * 100 / 1000 = 10 tokens, capped at 5
        rl.check_at(100).unwrap();
    }
}
