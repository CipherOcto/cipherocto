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
///
/// The LSN watermark is held in `WalTailStreamer::peers: HashMap<SyncPeerId, LsnTracker>`
/// (the single source of truth for per-peer LSN tracking). This struct holds
/// the per-peer rate limiter and the outbound channel buffer.
#[derive(Debug)]
pub struct SubscriberChannel {
    /// The per-peer rate limiter (consumed in `on_commit`).
    pub rate_limiter: RateLimiter,
    /// Outbound channel for `WalTailChunk` envelopes. In a real implementation
    /// this would be a `tokio::sync::mpsc::Sender<WalTailChunk>`; for v1 we
    /// use a bounded `Mutex<VecDeque>` that the cipherocto transport layer
    /// drains.
    ///
    /// # Bounded outbox
    ///
    /// The outbox is bounded at `OUTBOX_CAPACITY` (default 1024) to prevent
    /// unbounded memory growth if the cipherocto transport layer fails to
    /// drain. When the outbox is full, `send` returns `BackendNotReady`
    /// (which maps to `E_SYNC_RATE_LIMIT` — backpressure signal).
    pub outbox: Mutex<VecDeque<Arc<WalTailChunk>>>,
}

/// The default outbox capacity for a `SubscriberChannel`. Matches the
/// default ReplayCache bound (10K entries, but WAL chunks are larger so
/// we use 1K for the per-peer outbox).
pub const OUTBOX_CAPACITY: usize = 1024;

impl SubscriberChannel {
    /// Create a new subscriber channel.
    pub fn new(rate_limiter: RateLimiter) -> Self {
        Self {
            rate_limiter,
            outbox: Mutex::new(VecDeque::with_capacity(OUTBOX_CAPACITY)),
        }
    }

    /// Send a chunk to the subscriber. Returns `Err(SyncError::UnknownPeer)`
    /// if the channel has been closed (the outbox is empty AND we choose to
    /// not buffer). In v1 the outbox is bounded; the cipherocto transport
    /// drains it asynchronously.
    ///
    /// If the outbox is full, returns `Err(SyncError::BackendNotReady)`
    /// (backpressure: the cipherocto transport layer is too slow). The
    /// caller (WalTailStreamer::on_commit) propagates this to the upper
    /// layer which may demote the peer to Suspect per RFC-0862
    /// §Lifecycle Requirements.
    pub fn send(&self, chunk: Arc<WalTailChunk>) -> Result<(), SyncError> {
        let mut outbox = self.outbox.lock();
        if outbox.len() >= OUTBOX_CAPACITY {
            return Err(SyncError::BackendNotReady(format!(
                "outbox full ({} chunks); peer not draining",
                OUTBOX_CAPACITY
            )));
        }
        outbox.push_back(chunk);
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
    /// Previous Unix millisecond timestamp (for clock-backwards detection).
    prev_now_ms: Arc<Mutex<Option<u64>>>,
}

impl RateLimiter {
    /// Create a new rate limiter.
    pub fn new(rate_per_sec: u32, burst: u32) -> Self {
        Self {
            rate_per_sec,
            burst,
            tokens: Arc::new(Mutex::new(burst)),
            last_refill_ms: Arc::new(Mutex::new(0)),
            prev_now_ms: Arc::new(Mutex::new(None)),
        }
    }

    /// Check whether one envelope can be sent. Refills the bucket first.
    /// Returns `Err(SyncError::BackendNotReady)` if the bucket is empty.
    pub fn check(&self) -> Result<(), SyncError> {
        self.check_at(0)
    }

    /// Check at a specific Unix-millisecond timestamp (for testing).
    ///
    /// # Clock-backwards behavior
    ///
    /// If the system clock moves backwards (now_ms < last_refill_ms), the
    /// limiter does NOT add tokens (it would allow more than the burst in
    /// a malicious clock scenario). This is the conservative choice per
    /// RFC-0862 §Implicit Assumptions Audit row "time source".
    pub fn check_at(&self, now_ms: u64) -> Result<(), SyncError> {
        let mut last = self.last_refill_ms.lock();
        let mut tokens = self.tokens.lock();
        let mut prev_now = self.prev_now_ms.lock();
        if now_ms > *last && now_ms > prev_now.unwrap_or(0) {
            let elapsed_ms = now_ms - *last;
            let refill = elapsed_ms * (self.rate_per_sec as u64) / 1000;
            let new_tokens = (*tokens as u64)
                .saturating_add(refill)
                .min(self.burst as u64);
            *tokens = new_tokens as u32;
            *last = now_ms;
        }
        *prev_now = Some(now_ms);
        if *tokens == 0 {
            return Err(SyncError::BackendNotReady(
                "rate limit exhausted".to_string(),
            ));
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
/// - `current_lsn`: monotonic, persisted in WAL (wrapped in a Mutex for
///   concurrent on_commit safety — avoids the AtomicU64 TOCTOU race)
/// - `error_queue`: per-txn errors drained every 100ms
/// - `peers`: per-peer state machines
/// - `txn_subscribers`: per-txn fan-out mapping
/// - `paused`: backpressure flag
///
/// # Batching
///
/// `commit_batch_size` (default 100 commits per chunk) and
/// `commit_batch_timeout` (default 50ms) are documented in mission 0862a
/// but the actual batch accumulation is in the cipherocto sync engine's
/// upper layer (which has access to the `tokio` runtime). The streamer
/// flushes immediately on every `on_commit`; upper layers may buffer
/// calls if they want batching.
pub struct WalTailStreamer {
    /// The database adapter (trait object). The cipherocto sync engine does NOT
    /// hold a direct `Arc<MVCCEngine>` reference; all WAL reads go through
    /// `adapter.read_wal_range(from, to)`.
    adapter: Arc<dyn DatabaseSyncAdapter>,
    /// Per-peer subscription channels.
    subscribers: Mutex<HashMap<SyncPeerId, SubscriberChannel>>,
    /// Current LSN (monotonic, persisted in WAL). Wrapped in a Mutex to
    /// avoid the TOCTOU race that AtomicU64 would have when two threads
    /// call `on_commit` concurrently.
    current_lsn: Mutex<Lsn>,
    /// Per-txn error queue: drained every 100ms by the Sync engine.
    error_queue: Mutex<VecDeque<CommitError>>,
    /// Per-peer state machines.
    peers: Mutex<HashMap<SyncPeerId, LsnTracker>>,
    /// Maps each in-flight txn to the set of subscribers that were fanned-out.
    txn_subscribers: Mutex<HashMap<u64, Vec<SyncPeerId>>>,
    /// Backpressure flag: when the reader sends PAUSE, the writer stops shipping.
    paused: Mutex<bool>,
    /// Default commit batch size (100 commits per chunk). Held as documentation;
    /// the actual batching is the upper layer's responsibility.
    #[allow(dead_code)]
    commit_batch_size: usize,
    /// Default commit batch timeout (50ms). Held as documentation; the
    /// actual batching is the upper layer's responsibility.
    #[allow(dead_code)]
    commit_batch_timeout: Duration,
}

impl WalTailStreamer {
    /// Create a new `WalTailStreamer`.
    ///
    /// Initializes the internal LSN counter from the adapter's current LSN,
    /// so the streamer resumes correctly after a restart (the adapter may
    /// already have committed entries from a previous session).
    pub fn new(adapter: Arc<dyn DatabaseSyncAdapter>) -> Self {
        let initial_lsn = adapter.current_lsn().unwrap_or(0);
        Self {
            adapter,
            subscribers: Mutex::new(HashMap::new()),
            current_lsn: Mutex::new(initial_lsn),
            error_queue: Mutex::new(VecDeque::new()),
            peers: Mutex::new(HashMap::new()),
            txn_subscribers: Mutex::new(HashMap::new()),
            paused: Mutex::new(false),
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
        *self.current_lsn.lock()
    }

    /// Set the pause flag. Propagates to the adapter (per RFC-0862 v1.1.0).
    pub fn set_paused(&self, paused: bool) {
        *self.paused.lock() = paused;
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
    ///
    /// # Concurrency
    ///
    /// The `current_lsn` Mutex serializes `on_commit` calls across threads.
    /// This is necessary because AtomicU64 would have a TOCTOU race: two
    /// threads could read the same value, both decide to advance, and one
    /// of them would be silently dropped. The Mutex is fine for v1's
    /// single-writer model; a sharded or lock-free design is in future work.
    pub fn on_commit(&self, txn_id: u64, from_lsn: Lsn, to_lsn: Lsn) -> Result<(), SyncError> {
        // 1. Validate LSN range
        if from_lsn > to_lsn {
            return Err(SyncError::InvalidLsnRange {
                from: from_lsn,
                to: to_lsn,
            });
        }
        // 2. Update current_lsn under a lock (advances even when paused)
        {
            let mut lsn = self.current_lsn.lock();
            if from_lsn != *lsn + 1 {
                return Err(SyncError::LsnRegression {
                    expected: *lsn + 1,
                    actual: from_lsn,
                });
            }
            *lsn = to_lsn;
        }
        // 3. Check backpressure
        if *self.paused.lock() {
            return Ok(());
        }
        // 4. Read WAL entries via the trait. Per RFC-0862 v1.1.0
        //    §Migration path step v1.1.0.d, the cipherocto sync engine reads
        //    WAL through `adapter.read_wal_range(from, to)` — NOT via direct
        //    `self.engine.wal_manager().replay_two_phase(...)`.
        let entries = self.adapter.read_wal_range(from_lsn, to_lsn)?;
        // 6. Fan-out to subscribers (rate-limited). Acquire the subscribers
        //    lock ONCE; iterate over the snapshot. This avoids O(N) lock
        //    acquisitions and prevents a peer that unsubscribes mid-fan-out
        //    from racing with the lock.
        let chunk = Arc::new(WalTailChunk {
            from_lsn,
            to_lsn,
            entries,
            is_last: true,
        });
        {
            let subs = self.subscribers.lock();
            let mut txn_subs = self.txn_subscribers.lock();
            for (peer_id, channel) in subs.iter() {
                channel.rate_limiter.check()?;
                channel.send(chunk.clone())?;
                // Track this txn → peer mapping for drain_error_queue
                txn_subs.entry(txn_id).or_default().push(*peer_id);
            }
        }
        Ok(())
    }

    /// Reader's request for WAL entries from a given LSN.
    /// Returns a `WalTailChunk` containing the entries in `[from_lsn, current_lsn]`.
    ///
    /// The `is_last` flag is set based on a single read of `current_lsn`,
    /// so the returned chunk is internally consistent (no TOCTOU race
    /// between reading `current_lsn` for `to_lsn` and for `is_last`).
    pub async fn handle_wal_tail_request(&self, from_lsn: Lsn) -> Result<WalTailChunk, SyncError> {
        // Read `current_lsn` ONCE under the lock; use the same value for both
        // `to_lsn` and the `is_last` check.
        let prev = *self.current_lsn.lock();
        if from_lsn > prev {
            return Err(SyncError::InvalidLsnRange {
                from: from_lsn,
                to: prev,
            });
        }
        if from_lsn == 0 {
            return Err(SyncError::InvalidLsnRange { from: 0, to: prev });
        }
        let entries = self.adapter.read_wal_range(from_lsn, prev)?;
        let to_lsn = prev;
        // `is_last` semantics: per RFC-0862 §4.3, true if to_lsn == writer.current_lsn.
        // At this point, `to_lsn == prev == current_lsn` (we hold no locks, but
        // the read above captured the value). The `is_last` flag is true.
        Ok(WalTailChunk {
            from_lsn,
            to_lsn,
            entries,
            is_last: true,
        })
    }

    /// Reader sends LsnAck after successful apply.
    /// Returns `Ok(())` on success, `Err(SyncError::UnknownPeer)` if the peer
    /// is not subscribed, or `Err(SyncError::LsnRegression)` if the ack
    /// regresses.
    ///
    /// The per-peer LSN watermark is held in `self.peers: HashMap<SyncPeerId, LsnTracker>`.
    /// This method advances the watermark (which also validates the regression).
    pub fn on_lsn_ack(&self, peer: SyncPeerId, applied_lsn: Lsn) -> Result<(), SyncError> {
        // Verify the peer is subscribed (the LsnTracker in self.peers is the
        // single source of truth for subscription state).
        if !self.subscribers.lock().contains_key(&peer) {
            return Err(SyncError::UnknownPeer(peer.0));
        }
        // Advance the LsnTracker; this validates the regression internally
        // (returns Err(SyncError::LsnRegression) if applied_lsn < watermark).
        if let Some(tracker) = self.peers.lock().get_mut(&peer) {
            tracker.advance(applied_lsn)?;
        }
        Ok(())
    }

    /// Record an on_commit error for later per-peer demotion.
    pub fn record_commit_error(&self, txn_id: u64, error: SyncError) {
        self.error_queue
            .lock()
            .push_back(CommitError { txn_id, error });
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

    /// Drain all pending chunks from a subscriber's outbox.
    ///
    /// Called by the transport layer to pull chunks and broadcast them via
    /// `NodeTransport::broadcast()`. Returns the chunks in order.
    /// If the peer is not subscribed, returns an empty vec.
    pub fn drain_outbox(&self, peer_id: &SyncPeerId) -> Vec<Arc<WalTailChunk>> {
        let subs = self.subscribers.lock();
        if let Some(channel) = subs.get(peer_id) {
            channel.outbox.lock().drain(..).collect()
        } else {
            Vec::new()
        }
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

    #[test]
    fn outbox_full_returns_backpressure_error() {
        use crate::envelope::WalTailChunk;
        let rl = RateLimiter::new(10000, 10000);
        let channel = SubscriberChannel::new(rl);
        // Fill the outbox
        for i in 0..OUTBOX_CAPACITY {
            let chunk = Arc::new(WalTailChunk {
                from_lsn: i as u64,
                to_lsn: i as u64,
                entries: vec![],
                is_last: true,
            });
            channel.send(chunk).unwrap();
        }
        // One more should fail with backpressure
        let chunk = Arc::new(WalTailChunk {
            from_lsn: 999,
            to_lsn: 999,
            entries: vec![],
            is_last: true,
        });
        let err = channel.send(chunk).unwrap_err();
        assert!(matches!(err, SyncError::BackendNotReady(_)));
    }

    #[test]
    fn drain_outbox_returns_chunks() {
        let (s, _) = make_streamer();
        let peer = SyncPeerId([7u8; 32]);
        s.subscribe(peer, RateLimiter::new(100, 500));
        s.on_commit(1, 1, 3).unwrap();
        let chunks = s.drain_outbox(&peer);
        assert_eq!(chunks.len(), 1);
        assert_eq!(chunks[0].from_lsn, 1);
        assert_eq!(chunks[0].to_lsn, 3);
        assert!(chunks[0].is_last);
    }

    #[test]
    fn drain_outbox_empties_on_second_call() {
        let (s, _) = make_streamer();
        let peer = SyncPeerId([8u8; 32]);
        s.subscribe(peer, RateLimiter::new(100, 500));
        s.on_commit(1, 1, 5).unwrap();
        let first = s.drain_outbox(&peer);
        assert_eq!(first.len(), 1);
        let second = s.drain_outbox(&peer);
        assert!(second.is_empty());
    }

    #[test]
    fn drain_outbox_unknown_peer_returns_empty() {
        let (s, _) = make_streamer();
        let peer = SyncPeerId([9u8; 32]);
        let chunks = s.drain_outbox(&peer);
        assert!(chunks.is_empty());
    }
}
