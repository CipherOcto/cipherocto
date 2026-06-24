//! DGP ↔ sync engine integration.
//!
//! Provides [`SyncDgpHandler`] (implements [`SyncHandler`] for the sync engine)
//! and [`SyncNetworkBridge`] (routes inbound DGP objects and packages outbound
//! sync envelopes).
//!
//! # Usage
//!
//! ```text
//! // 1. Create the handler (receives sync events from DGP)
//! let handler = SyncDgpHandler::new(session_manager.clone());
//!
//! // 2. Create the bridge (routes DGP ↔ sync)
//! let bridge = SyncNetworkBridge::new(mission_id, handler);
//!
//! // 3. Inbound: when a DGP SnapshotFragment arrives
//! bridge.on_dgp_object(subtype, peer_id, payload);
//!
//! // 4. Outbound: when the sync engine wants to send
//! let fragment = bridge.prepare_outbound(subtype, peer_id, payload);
//! // ... wrap fragment into GossipObject and broadcast via DGP
//! ```

use std::sync::Arc;

use octo_sync::dgp_bridge::SyncHandler;
use octo_sync::envelope::WalTailChunk;
use octo_sync::identity::SyncPeerId;
use octo_sync::session::SyncSessionManager;

use crate::dgp::domain::{GossipDomainId, GossipScope};

/// DGP object type for sync snapshots (matches `GossipObjectType::SnapshotFragment = 0x0008`).
pub const SYNC_SNAPSHOT_OBJECT_TYPE: u16 = 0x0008;

/// The sync engine's DGP handler.
///
/// Implements [`SyncHandler`] so that DGP-delivered `SnapshotFragment`
/// envelopes are routed to the sync engine. Each callback receives the
/// raw payload bytes — the sync engine is responsible for decoding.
///
/// # Thread Safety
///
/// All methods are `&self` (the handler is shared via `Arc`). The underlying
/// `SyncSessionManager` uses `parking_lot::Mutex` internally.
pub struct SyncDgpHandler {
    session: Arc<SyncSessionManager>,
    /// Inbound summary responses received from peers (for testing/metrics).
    inbound_summaries: parking_lot::Mutex<Vec<([u8; 32], Vec<u8>)>>,
    /// Inbound segment responses received from peers.
    inbound_segments: parking_lot::Mutex<Vec<([u8; 32], Vec<u8>)>>,
    /// Inbound WAL tail responses received from peers.
    inbound_wal_tails: parking_lot::Mutex<Vec<([u8; 32], Vec<u8>)>>,
}

impl SyncDgpHandler {
    /// Create a new handler wrapping the given session manager.
    pub fn new(session: Arc<SyncSessionManager>) -> Self {
        Self {
            session,
            inbound_summaries: parking_lot::Mutex::new(Vec::new()),
            inbound_segments: parking_lot::Mutex::new(Vec::new()),
            inbound_wal_tails: parking_lot::Mutex::new(Vec::new()),
        }
    }

    /// Return a reference to the underlying session manager.
    pub fn session(&self) -> &Arc<SyncSessionManager> {
        &self.session
    }

    /// Drain all inbound events (for testing/metrics).
    #[allow(clippy::type_complexity)]
    pub fn drain_inbound(
        &self,
    ) -> (
        Vec<([u8; 32], Vec<u8>)>,
        Vec<([u8; 32], Vec<u8>)>,
        Vec<([u8; 32], Vec<u8>)>,
    ) {
        let summaries = self.inbound_summaries.lock().drain(..).collect();
        let segments = self.inbound_segments.lock().drain(..).collect();
        let wal_tails = self.inbound_wal_tails.lock().drain(..).collect();
        (summaries, segments, wal_tails)
    }
}

impl SyncHandler for SyncDgpHandler {
    fn on_summary(&self, peer_id: [u8; 32], payload: Vec<u8>) {
        // Decode the SummaryResponse and store for the sync engine to compare.
        // The sync engine uses summaries for anti-entropy: comparing remote
        // summaries against local state to determine which segments to request.
        match octo_sync::envelope::SummaryResponse::decode(&payload) {
            Ok(response) => {
                // Log for diagnostics; store raw bytes for drain_inbound.
                tracing::debug!(
                    peer = ?peer_id,
                    writer_lsn = response.writer_lsn,
                    summary_count = response.summaries.len(),
                    "decoded SummaryResponse"
                );
                self.inbound_summaries.lock().push((peer_id, payload));
            }
            Err(e) => {
                // Decode failure — store raw bytes anyway for diagnostics.
                tracing::warn!(
                    peer = ?peer_id,
                    error = %e,
                    "failed to decode SummaryResponse, storing raw bytes"
                );
                self.inbound_summaries.lock().push((peer_id, payload));
            }
        }
    }

    fn on_segment(&self, peer_id: [u8; 32], payload: Vec<u8>) {
        // Store the raw segment payload for the sync engine to process.
        // The sync engine's SegmentIndexer handles segment validation
        // (BLAKE3 root check, CRC32, LZ4 decompression) and database writes.
        tracing::debug!(
            peer = ?peer_id,
            payload_len = payload.len(),
            "received segment response"
        );
        self.inbound_segments.lock().push((peer_id, payload));
    }

    fn on_wal_tail(&self, peer_id: [u8; 32], payload: Vec<u8>) {
        // Decode the WalTailChunk and apply entries to the local database.
        // This is the core sync path: remote WAL entries → local database.
        match WalTailChunk::decode(&payload) {
            Ok(chunk) => {
                let sync_peer = SyncPeerId(peer_id);
                match self.session.apply_wal_tail(sync_peer, &chunk) {
                    Ok(applied) => {
                        tracing::debug!(
                            peer = ?peer_id,
                            from_lsn = chunk.from_lsn,
                            to_lsn = chunk.to_lsn,
                            entries = chunk.entries.len(),
                            applied,
                            "applied WAL tail chunk"
                        );
                    }
                    Err(e) => {
                        tracing::warn!(
                            peer = ?peer_id,
                            error = %e,
                            "failed to apply WAL tail chunk"
                        );
                        // Store raw bytes as fallback for drain_inbound.
                        self.inbound_wal_tails.lock().push((peer_id, payload));
                    }
                }
            }
            Err(e) => {
                tracing::warn!(
                    peer = ?peer_id,
                    error = %e,
                    "failed to decode WalTailChunk"
                );
                // Store raw bytes for diagnostics.
                self.inbound_wal_tails.lock().push((peer_id, payload));
            }
        }
    }
}

/// The sync ↔ DGP network bridge.
///
/// Provides the integration point between the DGP transport layer and the
/// sync engine. Manages:
/// - Inbound routing: DGP objects → sync engine
/// - Outbound packaging: sync engine → DGP `GossipObject`
/// - Domain ID computation for sync-specific gossip domains
pub struct SyncNetworkBridge {
    /// The DGP handler that receives inbound events.
    handler: Arc<SyncDgpHandler>,
    /// The mission ID for domain routing.
    mission_id: [u8; 32],
    /// Logical timestamp counter for outbound objects.
    timestamp: parking_lot::Mutex<u64>,
}

impl SyncNetworkBridge {
    /// Create a new bridge.
    pub fn new(mission_id: [u8; 32], handler: Arc<SyncDgpHandler>) -> Self {
        Self {
            handler,
            mission_id,
            timestamp: parking_lot::Mutex::new(0),
        }
    }

    /// Handle an inbound DGP SnapshotFragment object.
    ///
    /// Routes to the sync engine's handler based on the envelope subtype.
    /// Subtypes 0xA0-0xC2 are in the Sync envelope range per RFC-0862.
    pub fn on_dgp_object(
        &self,
        subtype: u8,
        peer_id: [u8; 32],
        payload: Vec<u8>,
    ) -> Result<(), octo_sync::error::SyncError> {
        let fragment = octo_sync::dgp_bridge::GossipSnapshotFragment::new(
            subtype,
            peer_id,
            self.mission_id,
            payload,
        );
        self.handler.session().adapter().current_lsn()?; // validate adapter is alive

        let bridge = octo_sync::dgp_bridge::DgpSyncBridge::new(
            self.mission_id,
            self.handler.clone() as Arc<dyn SyncHandler>,
        );
        bridge.dispatch(&fragment)
    }

    /// Prepare an outbound sync envelope for DGP broadcast.
    ///
    /// Returns the fragment metadata needed to construct a `GossipObject`:
    /// - `object_type`: Always `0x0008` (SnapshotFragment)
    /// - `domain_id`: Computed from mission_id
    /// - `payload`: The raw sync envelope bytes
    /// - `logical_timestamp`: Monotonically increasing
    pub fn prepare_outbound(
        &self,
        subtype: u8,
        peer_id: [u8; 32],
        payload: Vec<u8>,
    ) -> SyncOutboundEnvelope {
        let ts = {
            let mut t = self.timestamp.lock();
            *t += 1;
            *t
        };

        SyncOutboundEnvelope {
            object_type: SYNC_SNAPSHOT_OBJECT_TYPE,
            subtype,
            domain_id: GossipDomainId::new(1, self.mission_id, GossipScope::MISSION),
            logical_timestamp: ts,
            peer_id,
            payload,
        }
    }

    /// Return a reference to the handler.
    pub fn handler(&self) -> &Arc<SyncDgpHandler> {
        &self.handler
    }
}

/// An outbound sync envelope ready for DGP broadcast.
///
/// The caller wraps this into a `GossipObject` and sends it via the DGP layer.
#[derive(Debug, Clone)]
pub struct SyncOutboundEnvelope {
    /// The DGP object type (always `0x0008`).
    pub object_type: u16,
    /// The sync envelope subtype.
    pub subtype: u8,
    /// The DGP domain for this sync object.
    pub domain_id: GossipDomainId,
    /// Monotonically increasing logical timestamp.
    pub logical_timestamp: u64,
    /// The target peer (for directed mode) or broadcast origin.
    pub peer_id: [u8; 32],
    /// The raw payload bytes.
    pub payload: Vec<u8>,
}

/// Compute the DGP domain ID for a sync mission.
///
/// Sync objects use `GossipScope::MISSION` so they are scoped to the
/// mission's gossip domain. The network_id is hardcoded to 1 (CipherOcto
/// mainnet); testnet/devnet override this.
pub fn compute_sync_domain_id(mission_id: &[u8; 32], network_id: u32) -> GossipDomainId {
    GossipDomainId::new(network_id, *mission_id, GossipScope::MISSION)
}

#[cfg(test)]
mod tests {
    use super::*;
    use octo_sync::config::{SyncConfig, SyncRole};
    use octo_sync::test_util::MockAdapter;

    fn make_handler_and_bridge() -> (Arc<SyncDgpHandler>, SyncNetworkBridge) {
        let mut mission_id = [0u8; 32];
        mission_id[0] = 0xCD;
        let config = SyncConfig::new(mission_id, SyncRole::Replicator, vec![0x10; 32]);
        let adapter = Arc::new(MockAdapter::new(mission_id, [0x11; 32]));
        let session = SyncSessionManager::new(
            adapter as Arc<dyn octo_sync::adapter::DatabaseSyncAdapter>,
            config,
            &[0x42u8; 32],
        )
        .unwrap();
        let handler = Arc::new(SyncDgpHandler::new(Arc::new(session)));
        let bridge = SyncNetworkBridge::new(mission_id, handler.clone());
        (handler, bridge)
    }

    #[test]
    fn bridge_routes_summary_response() {
        let (handler, bridge) = make_handler_and_bridge();
        bridge.on_dgp_object(0xA1, [2u8; 32], vec![0xAA]).unwrap();
        let (summaries, segments, wal_tails) = handler.drain_inbound();
        assert_eq!(summaries.len(), 1);
        assert_eq!(summaries[0].0, [2u8; 32]);
        assert_eq!(summaries[0].1, vec![0xAA]);
        assert!(segments.is_empty());
        assert!(wal_tails.is_empty());
    }

    #[test]
    fn bridge_routes_segment_response() {
        let (handler, bridge) = make_handler_and_bridge();
        bridge
            .on_dgp_object(0xA3, [3u8; 32], vec![0xBB, 0xCC])
            .unwrap();
        let (summaries, segments, wal_tails) = handler.drain_inbound();
        assert!(summaries.is_empty());
        assert_eq!(segments.len(), 1);
        assert_eq!(segments[0].1, vec![0xBB, 0xCC]);
        assert!(wal_tails.is_empty());
    }

    #[test]
    fn bridge_routes_wal_tail_response() {
        let (handler, bridge) = make_handler_and_bridge();
        bridge.on_dgp_object(0xB1, [4u8; 32], vec![0xDD]).unwrap();
        let (summaries, segments, wal_tails) = handler.drain_inbound();
        assert!(summaries.is_empty());
        assert!(segments.is_empty());
        assert_eq!(wal_tails.len(), 1);
    }

    #[test]
    fn bridge_rejects_unknown_subtype() {
        let (_, bridge) = make_handler_and_bridge();
        let err = bridge.on_dgp_object(0x99, [2u8; 32], vec![]).unwrap_err();
        assert!(matches!(
            err,
            octo_sync::error::SyncError::UnknownEnvelopeSubtype(0x99)
        ));
    }

    #[test]
    fn prepare_outbound_increments_timestamp() {
        let (_, bridge) = make_handler_and_bridge();
        let e1 = bridge.prepare_outbound(0xA1, [1u8; 32], vec![]);
        let e2 = bridge.prepare_outbound(0xA1, [1u8; 32], vec![]);
        assert_eq!(e1.object_type, SYNC_SNAPSHOT_OBJECT_TYPE);
        assert_eq!(e1.logical_timestamp, 1);
        assert_eq!(e2.logical_timestamp, 2);
    }

    #[test]
    fn compute_sync_domain_id_deterministic() {
        let mission = [0xAB; 32];
        let d1 = compute_sync_domain_id(&mission, 1);
        let d2 = compute_sync_domain_id(&mission, 1);
        assert_eq!(d1, d2);
        assert_eq!(d1.scope, GossipScope::MISSION);
    }

    #[test]
    fn compute_sync_domain_id_different_networks() {
        let mission = [0xAB; 32];
        let d1 = compute_sync_domain_id(&mission, 1);
        let d2 = compute_sync_domain_id(&mission, 2);
        assert_ne!(d1, d2);
    }
}
