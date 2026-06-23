//! `SyncSessionManager` — the orchestrator that ties all sync modules together.
//!
//! Per the E2E test plan (`docs/e2e/2026-06-23-stoolap-data-sync-e2e-test-plan.md`),
//! L3+ tests require a session manager that owns:
//! - [`WalTailStreamer`] — writer-side WAL-tail fan-out
//! - [`SegmentIndexer`] — snapshot segment lookup and regeneration
//! - [`MissionKeyRing`] — per-mission AEAD + HMAC keys
//! - [`ReplayCacheManager`] — per-peer envelope dedup
//! - Per-peer [`Peer`] lifecycle state machines
//!
//! The `SyncSessionManager` is the single entry point for the cipherocto sync
//! engine. It is generic over `DatabaseSyncAdapter` (the trait boundary per
//! RFC-0862 v1.1.0).

use std::collections::HashMap;
use std::sync::Arc;

use parking_lot::Mutex;

use crate::adapter::DatabaseSyncAdapter;
use crate::config::{SyncConfig, SyncRole};
use crate::error::SyncError;
use crate::identity::{SyncNodeId, SyncPeerId};
use crate::keyring::{KeyRing, MissionKeyRing};
use crate::lsn::LsnTracker;
use crate::replay_cache::ReplayCacheManager;
use crate::segment::SegmentIndexer;
use crate::state::{Peer, SyncLifecycle, TransitionTrigger};
use crate::stream::{RateLimiter, WalTailStreamer};
use crate::types::Lsn;

/// Per-peer session state tracked by the manager.
///
/// Combines the lifecycle [`Peer`] state machine with the LSN watermark
/// and replay cache for a single remote peer.
#[derive(Debug)]
pub struct PeerSession {
    /// The lifecycle state machine (Init → Connecting → … → Terminated).
    pub peer: Peer,
    /// LSN watermark tracker for this peer.
    pub lsn_tracker: LsnTracker,
    /// Replay cache for this peer (envelope dedup).
    pub replay_cache: crate::replay_cache::ReplayCache,
}

/// The sync session manager.
///
/// Orchestrates the per-peer lifecycle, WAL-tail streaming, snapshot segment
/// handling, and anti-entropy for a single sync session. This is the struct
/// that the cipherocto sync engine instantiates and drives.
///
/// # Concurrency
///
/// All mutable state is behind `parking_lot::Mutex`. The manager is `Send + Sync`
/// (suitable for `Arc<SyncSessionManager>`). The cipherocto async runtime
/// (`tokio`) wraps every method call at the boundary via `tokio::task::spawn_blocking`.
pub struct SyncSessionManager {
    /// The database adapter (trait object, per RFC-0862 v1.1.0).
    adapter: Arc<dyn DatabaseSyncAdapter>,
    /// The session configuration.
    config: SyncConfig,
    /// The local node's derived identity.
    node_id: SyncNodeId,
    /// The writer-side WAL-tail streamer.
    streamer: WalTailStreamer,
    /// The snapshot segment indexer.
    segment_indexer: SegmentIndexer,
    /// The per-mission key ring (AEAD + HMAC).
    keyring: Arc<MissionKeyRing>,
    /// Per-peer replay caches.
    replay_caches: Mutex<ReplayCacheManager>,
    /// Per-peer session state (lifecycle + LSN watermark).
    peers: Mutex<HashMap<SyncPeerId, PeerSession>>,
}

impl SyncSessionManager {
    /// Create a new `SyncSessionManager` from an adapter and config.
    ///
    /// Derives the local `SyncNodeId` from `config.public_key` and
    /// `config.mission_id`. Constructs the `WalTailStreamer` and
    /// `SegmentIndexer` from the adapter. Derives the `MissionKeyRing`
    /// from a caller-supplied `mission_root_key`.
    pub fn new(
        adapter: Arc<dyn DatabaseSyncAdapter>,
        config: SyncConfig,
        mission_root_key: &[u8; 32],
    ) -> Result<Self, SyncError> {
        // Validate role (per RFC-0862 §4.1, G8).
        match config.role {
            SyncRole::Replicator | SyncRole::Observer => {}
        }

        let node_id = SyncNodeId::derive(&config.public_key, &config.mission_id);
        let keyring = Arc::new(MissionKeyRing::derive(mission_root_key, config.mission_id));
        let streamer = WalTailStreamer::new(adapter.clone());
        let segment_indexer = SegmentIndexer::new(adapter.clone());

        Ok(Self {
            adapter,
            config,
            node_id,
            streamer,
            segment_indexer,
            keyring,
            replay_caches: Mutex::new(ReplayCacheManager::new()),
            peers: Mutex::new(HashMap::new()),
        })
    }

    /// Return the local `SyncNodeId`.
    pub fn node_id(&self) -> SyncNodeId {
        self.node_id
    }

    /// Return a reference to the session config.
    pub fn config(&self) -> &SyncConfig {
        &self.config
    }

    /// Return a reference to the key ring.
    pub fn keyring(&self) -> &Arc<MissionKeyRing> {
        &self.keyring
    }

    /// Return a reference to the adapter.
    pub fn adapter(&self) -> &Arc<dyn DatabaseSyncAdapter> {
        &self.adapter
    }

    /// Return a reference to the WAL-tail streamer.
    pub fn streamer(&self) -> &WalTailStreamer {
        &self.streamer
    }

    /// Return a reference to the segment indexer.
    pub fn segment_indexer(&self) -> &SegmentIndexer {
        &self.segment_indexer
    }

    // ── Peer lifecycle management ──────────────────────────────────────

    /// Register a new remote peer and subscribe it to the WAL-tail streamer.
    ///
    /// Transitions the peer through `Init → Connecting` (via
    /// `LocalConfigMatched`). The peer starts in the `Init` state; the
    /// cipherocto sync engine drives further transitions via
    /// [`transition_peer`].
    pub fn subscribe_peer(&self, peer_id: SyncPeerId) -> Result<(), SyncError> {
        let mut peer = Peer::new(peer_id);
        peer.transition(
            SyncLifecycle::Connecting,
            TransitionTrigger::LocalConfigMatched,
        )?;

        let rate_limiter =
            RateLimiter::new(self.config.rate_limit_per_sec, self.config.rate_limit_burst);
        self.streamer.subscribe(peer_id, rate_limiter);

        let session = PeerSession {
            peer,
            lsn_tracker: LsnTracker::new(),
            replay_cache: crate::replay_cache::ReplayCache::default(),
        };
        self.peers.lock().insert(peer_id, session);
        Ok(())
    }

    /// Unregister a remote peer and unsubscribe it from the WAL-tail streamer.
    pub fn unsubscribe_peer(&self, peer_id: &SyncPeerId) {
        self.streamer.unsubscribe(peer_id);
        self.peers.lock().remove(peer_id);
    }

    /// Transition a peer's lifecycle state.
    ///
    /// The cipherocto sync engine calls this when a peer's connection state
    /// changes (e.g., TLS handshake completed, signature verified, heartbeat
    /// timeout). The transition MUST be valid per the RFC-0862 transition
    /// table (checked by [`Peer::transition`]).
    pub fn transition_peer(
        &self,
        peer_id: SyncPeerId,
        to: SyncLifecycle,
        trigger: TransitionTrigger,
    ) -> Result<SyncLifecycle, SyncError> {
        let mut peers = self.peers.lock();
        let session = peers
            .get_mut(&peer_id)
            .ok_or(SyncError::UnknownPeer(peer_id.0))?;
        session.peer.transition(to, trigger)
    }

    /// Return the current lifecycle state of a peer.
    pub fn peer_state(&self, peer_id: SyncPeerId) -> Option<SyncLifecycle> {
        self.peers.lock().get(&peer_id).map(|s| s.peer.state)
    }

    /// Return the number of currently subscribed peers.
    pub fn peer_count(&self) -> usize {
        self.peers.lock().len()
    }

    // ── Writer-side operations ─────────────────────────────────────────

    /// Called by the writer's `record_commit` hook after a successful commit.
    ///
    /// Advances the streamer's LSN counter and fans out a `WalTailChunk` to
    /// all subscribed peers (rate-limited, backpressure-aware).
    pub fn on_commit(&self, txn_id: u64, from_lsn: Lsn, to_lsn: Lsn) -> Result<(), SyncError> {
        self.streamer.on_commit(txn_id, from_lsn, to_lsn)
    }

    /// Set the pause flag on the streamer (backpressure).
    ///
    /// When `paused = true`, the writer skips fan-out in `on_commit`; the
    /// LSN counter still advances. When `paused = false`, normal fan-out
    /// resumes.
    pub fn set_paused(&self, paused: bool) {
        self.streamer.set_paused(paused);
    }

    /// Return the current writer LSN.
    pub fn current_lsn(&self) -> Lsn {
        self.streamer.current_lsn()
    }

    // ── Reader-side operations ─────────────────────────────────────────

    /// Apply a `WalTailChunk` received from the writer.
    ///
    /// For each WAL entry in the chunk:
    /// 1. Check the replay cache (skip if already applied).
    /// 2. Call `adapter.apply_wal_entry(entry)`.
    /// 3. Insert the envelope_id into the replay cache.
    ///
    /// Returns the number of entries successfully applied.
    pub fn apply_wal_tail(
        &self,
        peer_id: SyncPeerId,
        chunk: &crate::envelope::WalTailChunk,
    ) -> Result<u32, SyncError> {
        let mut applied = 0u32;
        let mut caches = self.replay_caches.lock();
        let cache = caches.cache_for(peer_id);
        for entry in &chunk.entries {
            // Derive an envelope_id from the entry bytes (BLAKE3 for determinism).
            let envelope_id = blake3_hash(entry);
            if cache.contains(&envelope_id) {
                continue;
            }
            self.adapter.apply_wal_entry(entry)?;
            cache.insert(envelope_id, 0); // timestamp not critical for dedup
            applied += 1;
        }
        Ok(applied)
    }

    /// Handle an LSN acknowledgment from a reader.
    ///
    /// Advances the per-peer LSN watermark in the streamer. Validates
    /// monotonicity (rejects LSN regression).
    pub fn on_lsn_ack(&self, peer_id: SyncPeerId, applied_lsn: Lsn) -> Result<(), SyncError> {
        self.streamer.on_lsn_ack(peer_id, applied_lsn)
    }

    // ── Snapshot segment operations ────────────────────────────────────

    /// Handle a `SegmentRequest` from a reader.
    ///
    /// Delegates to the [`SegmentIndexer`], which reads the segment via the
    /// adapter and packages it as a [`SyncSegment`](crate::segment::SyncSegment).
    pub async fn handle_segment_request(
        &self,
        table_id: crate::types::TableId,
        segment_index: crate::types::SegmentIndex,
        expected_root: [u8; 32],
    ) -> Result<crate::segment::SegmentLookupResult, SyncError> {
        self.segment_indexer
            .handle_segment_request(table_id, segment_index, expected_root)
            .await
    }

    /// Request a snapshot regeneration for a table.
    pub async fn regenerate_snapshot(
        &self,
        table_id: crate::types::TableId,
    ) -> Result<crate::segment::SegmentLookupResult, SyncError> {
        self.segment_indexer.regenerate_snapshot(table_id).await
    }

    // ── Anti-entropy summary ───────────────────────────────────────────

    /// Build a `SyncSummary` for a table (writer-side).
    ///
    /// Takes pre-computed `SegmentMetadata` and produces the signed summary
    /// with HMAC binding to the local node.
    pub fn build_summary(
        &self,
        table_id: crate::types::TableId,
        segments: Vec<crate::summary::SegmentMetadata>,
    ) -> Result<crate::summary::SyncSummary, SyncError> {
        use crate::summary::MerkleSegmentTree;

        let tree = MerkleSegmentTree::from_segments(&segments);
        let root = tree.root();
        let segment_count = segments.len() as u32;
        let lsn_watermark = self.adapter.current_lsn()?;

        // Build the summary body for HMAC binding.
        let mut body = Vec::with_capacity(4 + 4 + 32 + 8);
        body.extend_from_slice(&table_id.to_le_bytes());
        body.extend_from_slice(&segment_count.to_le_bytes());
        body.extend_from_slice(&root);
        body.extend_from_slice(&lsn_watermark.to_le_bytes());

        let node_id = self.node_id();
        let hmac = self.keyring.summary_hmac(&body, node_id.as_bytes());

        Ok(crate::summary::SyncSummary {
            table_id,
            segment_count,
            segment_root: root,
            lsn_watermark,
            hmac,
        })
    }

    // ── Heartbeat / error queue ────────────────────────────────────────

    /// Drain the streamer's per-txn error queue and return affected peers.
    ///
    /// The cipherocto sync engine calls this periodically (every 100ms) to
    /// demote peers that have experienced commit errors.
    pub fn drain_error_queue(&self) -> Vec<(SyncPeerId, SyncError)> {
        self.streamer.drain_error_queue()
    }

    /// Check whether any peer has exceeded the heartbeat timeout.
    ///
    /// Returns the list of peers that should be transitioned to `Suspect`.
    /// The cipherocto sync engine drives the actual transition via
    /// [`transition_peer`].
    pub fn check_heartbeat_timeouts(&self, now_unix_secs: u64) -> Vec<SyncPeerId> {
        let suspect_threshold =
            self.config.heartbeat_interval_secs * self.config.suspect_multiplier;
        let peers = self.peers.lock();
        peers
            .iter()
            .filter(|(_, session)| {
                session.peer.state == SyncLifecycle::Streaming
                    && session.peer.last_heartbeat_unix > 0
                    && now_unix_secs.saturating_sub(session.peer.last_heartbeat_unix)
                        > suspect_threshold
            })
            .map(|(peer_id, _)| *peer_id)
            .collect()
    }

    /// Record a heartbeat from a peer (updates `last_heartbeat_unix`).
    pub fn record_heartbeat(&self, peer_id: SyncPeerId, now_unix_secs: u64) {
        if let Some(session) = self.peers.lock().get_mut(&peer_id) {
            session.peer.last_heartbeat_unix = now_unix_secs;
        }
    }

    // ── Convenience: reader-side apply from peer ───────────────────────

    /// Convenience method: build a `WalTailRequest` for catch-up from a given LSN.
    ///
    /// The cipherocto sync engine sends this to the writer after a reconnect.
    /// The writer responds with a `WalTailChunk` via [`handle_wal_tail_request`].
    pub fn request_wal_tail_from(
        &self,
        from_lsn: Lsn,
    ) -> Result<crate::envelope::WalTailRequest, SyncError> {
        if from_lsn == 0 {
            return Err(SyncError::InvalidLsnRange { from: 0, to: 0 });
        }
        Ok(crate::envelope::WalTailRequest { from_lsn })
    }

    // ── Periodic orchestration (tick) ──────────────────────────────────

    /// Periodic tick: check peer health, detect timeouts, return actions.
    ///
    /// The cipherocto sync engine calls this every N seconds (configurable).
    /// Returns a list of [`TickAction`]s that the engine should execute
    /// (e.g., transition a peer to Suspect, send a SummaryRequest).
    pub fn tick(&self, now_unix_secs: u64) -> Vec<TickAction> {
        let mut actions = Vec::new();
        let peers = self.peers.lock();

        for (peer_id, session) in peers.iter() {
            match session.peer.state {
                SyncLifecycle::Streaming => {
                    // Check heartbeat timeout
                    let suspect_threshold =
                        self.config.heartbeat_interval_secs * self.config.suspect_multiplier;
                    if session.peer.last_heartbeat_unix > 0
                        && now_unix_secs.saturating_sub(session.peer.last_heartbeat_unix)
                            > suspect_threshold
                    {
                        actions.push(TickAction::TransitionToSuspect(*peer_id));
                    }
                }
                SyncLifecycle::Suspect => {
                    // Transition to Reconnecting after a brief delay
                    // (the caller handles the actual reconnection attempt)
                    actions.push(TickAction::TransitionToReconnecting(*peer_id));
                }
                SyncLifecycle::Reconnecting => {
                    // Attempt reconnection
                    actions.push(TickAction::AttemptReconnect(*peer_id));
                }
                _ => {}
            }
        }

        actions
    }

    /// Select peers for anti-entropy gossip using DRS criteria.
    ///
    /// Returns the best N peers based on:
    /// 1. Liveness (peers in Streaming state preferred)
    /// 2. LSN watermark (peers with lower LSN are better targets for catch-up)
    /// 3. Diversity (simplified: prefer peers with different node_id prefixes)
    pub fn select_gossip_peers(&self, max_peers: usize) -> Vec<SyncPeerId> {
        let peers = self.peers.lock();
        let mut candidates: Vec<(SyncPeerId, u64)> = peers
            .iter()
            .filter(|(_, session)| {
                // Only select peers that are active (Streaming) or recently connected
                matches!(
                    session.peer.state,
                    SyncLifecycle::Streaming | SyncLifecycle::Connecting
                )
            })
            .map(|(peer_id, session)| {
                // Score: lower LSN = better target for catch-up gossip
                let lsn_score = session.lsn_tracker.watermark();
                (*peer_id, lsn_score)
            })
            .collect();

        // Sort by LSN (ascending) — peers with lower LSN are better gossip targets
        candidates.sort_by_key(|(_, lsn)| *lsn);

        // Deduplicate by node_id prefix (simplified diversity check)
        let mut selected = Vec::new();
        let mut seen_prefixes = std::collections::HashSet::new();
        for (peer_id, _) in &candidates {
            let prefix = peer_id.0[0..4].to_vec();
            if seen_prefixes.insert(prefix) {
                selected.push(*peer_id);
                if selected.len() >= max_peers {
                    break;
                }
            }
        }

        selected
    }

    /// Return the current LSN watermark for a peer.
    pub fn peer_lsn_watermark(&self, peer_id: SyncPeerId) -> Option<Lsn> {
        self.peers
            .lock()
            .get(&peer_id)
            .map(|s| s.lsn_tracker.watermark())
    }

    /// Return the list of all subscribed peers and their states.
    pub fn peer_states(&self) -> Vec<(SyncPeerId, SyncLifecycle)> {
        self.peers
            .lock()
            .iter()
            .map(|(id, session)| (*id, session.peer.state))
            .collect()
    }
}

/// Actions returned by [`SyncSessionManager::tick`].
///
/// The cipherocto sync engine executes these actions (e.g., transition
/// peers, send requests) based on the periodic health check.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TickAction {
    /// Transition a peer to Suspect (heartbeat timeout).
    TransitionToSuspect(SyncPeerId),
    /// Transition a peer from Suspect to Reconnecting.
    TransitionToReconnecting(SyncPeerId),
    /// Attempt to reconnect to a peer in Reconnecting state.
    AttemptReconnect(SyncPeerId),
}

/// BLAKE3-256 hash helper for deriving envelope IDs.
fn blake3_hash(data: &[u8]) -> [u8; 32] {
    let mut hasher = blake3::Hasher::new();
    hasher.update(data);
    *hasher.finalize().as_bytes()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_util::MockAdapter;

    fn sample_mission_root_key() -> [u8; 32] {
        let mut k = [0u8; 32];
        for (i, byte) in k.iter_mut().enumerate() {
            *byte = i as u8;
        }
        k
    }

    fn sample_config(role: SyncRole) -> SyncConfig {
        let mut mission_id = [0u8; 32];
        mission_id[0] = 0xAB;
        SyncConfig::new(mission_id, role, vec![0x01; 32])
    }

    fn make_manager(role: SyncRole) -> (SyncSessionManager, Arc<MockAdapter>) {
        let config = sample_config(role);
        let mission_id = config.mission_id;
        let node_id = SyncNodeId::derive(&config.public_key, &mission_id);
        let adapter = Arc::new(MockAdapter::new(mission_id, *node_id.as_bytes()));
        let mgr =
            SyncSessionManager::new(adapter.clone(), config, &sample_mission_root_key()).unwrap();
        (mgr, adapter)
    }

    #[test]
    fn new_manager_succeeds_for_valid_roles() {
        let (mgr, _) = make_manager(SyncRole::Replicator);
        assert_eq!(mgr.config().role, SyncRole::Replicator);
    }

    #[test]
    fn subscribe_and_unsubscribe_peer() {
        let (mgr, _) = make_manager(SyncRole::Replicator);
        let peer = SyncPeerId([3u8; 32]);
        mgr.subscribe_peer(peer).unwrap();
        assert_eq!(mgr.peer_count(), 1);
        assert_eq!(mgr.peer_state(peer), Some(SyncLifecycle::Connecting));
        mgr.unsubscribe_peer(&peer);
        assert_eq!(mgr.peer_count(), 0);
    }

    #[test]
    fn transition_peer_streaming() {
        let (mgr, _) = make_manager(SyncRole::Replicator);
        let peer = SyncPeerId([3u8; 32]);
        mgr.subscribe_peer(peer).unwrap();
        mgr.transition_peer(
            peer,
            SyncLifecycle::Authenticating,
            TransitionTrigger::TlsHandshakeComplete,
        )
        .unwrap();
        mgr.transition_peer(
            peer,
            SyncLifecycle::Streaming,
            TransitionTrigger::SignatureValid,
        )
        .unwrap();
        assert_eq!(mgr.peer_state(peer), Some(SyncLifecycle::Streaming));
    }

    #[test]
    fn transition_unknown_peer_errors() {
        let (mgr, _) = make_manager(SyncRole::Replicator);
        let peer = SyncPeerId([99u8; 32]);
        let err = mgr
            .transition_peer(
                peer,
                SyncLifecycle::Streaming,
                TransitionTrigger::SignatureValid,
            )
            .unwrap_err();
        assert!(matches!(err, SyncError::UnknownPeer(_)));
    }

    #[test]
    fn on_commit_advances_lsn() {
        let (mgr, _) = make_manager(SyncRole::Replicator);
        assert_eq!(mgr.current_lsn(), 0);
        mgr.on_commit(1, 1, 10).unwrap();
        assert_eq!(mgr.current_lsn(), 10);
    }

    #[test]
    fn apply_wal_tail_deduplicates() {
        let (mgr, _adapter) = make_manager(SyncRole::Observer);
        let peer = SyncPeerId([3u8; 32]);
        mgr.subscribe_peer(peer).unwrap();

        let chunk = crate::envelope::WalTailChunk {
            from_lsn: 1,
            to_lsn: 1,
            entries: vec![b"entry1".to_vec()],
            is_last: true,
        };
        let applied = mgr.apply_wal_tail(peer, &chunk).unwrap();
        assert_eq!(applied, 1);
        // Apply same chunk again — dedup should skip.
        let applied2 = mgr.apply_wal_tail(peer, &chunk).unwrap();
        assert_eq!(applied2, 0);
    }

    #[test]
    fn build_summary_produces_hmac() {
        let (mgr, _) = make_manager(SyncRole::Replicator);
        let segments = vec![crate::summary::SegmentMetadata {
            segment_index: 0,
            payload_hash: [1u8; 32],
            lsn_watermark: 10,
            byte_size: 1024,
        }];
        let summary = mgr.build_summary(42, segments).unwrap();
        assert_eq!(summary.table_id, 42);
        assert_eq!(summary.segment_count, 1);
        assert_ne!(summary.hmac, [0u8; 32]);
    }

    #[test]
    fn on_lsn_ack_advances_tracker() {
        let (mgr, _) = make_manager(SyncRole::Replicator);
        let peer = SyncPeerId([3u8; 32]);
        mgr.subscribe_peer(peer).unwrap();
        mgr.on_lsn_ack(peer, 100).unwrap();
    }

    #[test]
    fn check_heartbeat_timeouts_empty_when_no_peers() {
        let (mgr, _) = make_manager(SyncRole::Replicator);
        let timeouts = mgr.check_heartbeat_timeouts(1000);
        assert!(timeouts.is_empty());
    }

    #[test]
    fn check_heartbeat_timeouts_detects_stale_peer() {
        let (mgr, _) = make_manager(SyncRole::Replicator);
        let peer = SyncPeerId([3u8; 32]);
        mgr.subscribe_peer(peer).unwrap();
        // Transition to Streaming.
        mgr.transition_peer(
            peer,
            SyncLifecycle::Authenticating,
            TransitionTrigger::TlsHandshakeComplete,
        )
        .unwrap();
        mgr.transition_peer(
            peer,
            SyncLifecycle::Streaming,
            TransitionTrigger::SignatureValid,
        )
        .unwrap();
        // Record heartbeat at t=100.
        mgr.record_heartbeat(peer, 100);
        // At t=120 (> 10s threshold), the peer should be detected as stale.
        let timeouts = mgr.check_heartbeat_timeouts(120);
        assert!(timeouts.contains(&peer));
    }

    #[test]
    fn request_wal_tail_from_rejects_zero() {
        let (mgr, _) = make_manager(SyncRole::Replicator);
        let err = mgr.request_wal_tail_from(0).unwrap_err();
        assert!(matches!(err, SyncError::InvalidLsnRange { .. }));
    }

    #[test]
    fn drain_error_queue_returns_empty_initially() {
        let (mgr, _) = make_manager(SyncRole::Replicator);
        let errors = mgr.drain_error_queue();
        assert!(errors.is_empty());
    }

    #[test]
    fn node_id_matches_derivation() {
        let (mgr, _) = make_manager(SyncRole::Replicator);
        let config = mgr.config();
        let expected = SyncNodeId::derive(&config.public_key, &config.mission_id);
        assert_eq!(mgr.node_id(), expected);
    }

    #[test]
    fn set_paused_propagates_to_adapter() {
        let (mgr, adapter) = make_manager(SyncRole::Replicator);
        assert!(!adapter.is_paused());
        mgr.set_paused(true);
        assert!(adapter.is_paused());
        mgr.set_paused(false);
        assert!(!adapter.is_paused());
    }

    #[test]
    fn tick_returns_suspect_for_stale_peer() {
        let (mgr, _) = make_manager(SyncRole::Replicator);
        let peer = SyncPeerId([3u8; 32]);
        mgr.subscribe_peer(peer).unwrap();
        mgr.transition_peer(
            peer,
            SyncLifecycle::Authenticating,
            TransitionTrigger::TlsHandshakeComplete,
        )
        .unwrap();
        mgr.transition_peer(
            peer,
            SyncLifecycle::Streaming,
            TransitionTrigger::SignatureValid,
        )
        .unwrap();
        // Record heartbeat at t=100
        mgr.record_heartbeat(peer, 100);
        // Tick at t=120 (> 10s suspect threshold)
        let actions = mgr.tick(120);
        assert!(actions.contains(&TickAction::TransitionToSuspect(peer)));
    }

    #[test]
    fn tick_returns_empty_for_healthy_peers() {
        let (mgr, _) = make_manager(SyncRole::Replicator);
        let peer = SyncPeerId([3u8; 32]);
        mgr.subscribe_peer(peer).unwrap();
        mgr.transition_peer(
            peer,
            SyncLifecycle::Authenticating,
            TransitionTrigger::TlsHandshakeComplete,
        )
        .unwrap();
        mgr.transition_peer(
            peer,
            SyncLifecycle::Streaming,
            TransitionTrigger::SignatureValid,
        )
        .unwrap();
        // Record heartbeat at t=100
        mgr.record_heartbeat(peer, 100);
        // Tick at t=105 (< 10s suspect threshold)
        let actions = mgr.tick(105);
        assert!(actions.is_empty());
    }

    #[test]
    fn select_gossip_peers_returns_streaming_peers() {
        let (mgr, _) = make_manager(SyncRole::Replicator);
        let peer1 = SyncPeerId([3u8; 32]);
        let peer2 = SyncPeerId([4u8; 32]);
        mgr.subscribe_peer(peer1).unwrap();
        mgr.subscribe_peer(peer2).unwrap();
        // Transition both to Streaming
        for peer in &[peer1, peer2] {
            mgr.transition_peer(
                *peer,
                SyncLifecycle::Authenticating,
                TransitionTrigger::TlsHandshakeComplete,
            )
            .unwrap();
            mgr.transition_peer(
                *peer,
                SyncLifecycle::Streaming,
                TransitionTrigger::SignatureValid,
            )
            .unwrap();
        }
        let selected = mgr.select_gossip_peers(5);
        assert!(selected.contains(&peer1));
        assert!(selected.contains(&peer2));
    }

    #[test]
    fn peer_states_returns_all_peers() {
        let (mgr, _) = make_manager(SyncRole::Replicator);
        let peer1 = SyncPeerId([3u8; 32]);
        let peer2 = SyncPeerId([4u8; 32]);
        mgr.subscribe_peer(peer1).unwrap();
        mgr.subscribe_peer(peer2).unwrap();
        let states = mgr.peer_states();
        assert_eq!(states.len(), 2);
    }
}
