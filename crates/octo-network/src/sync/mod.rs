//! Stoolap Data Sync integration (RFC-0862).
//!
//! Bridges `octo-sync` (leaf workspace) with `octo-network`'s DGP layer.
//! Routes `SnapshotFragment` objects (object_type = 0x0008) to the sync
//! engine and sends outbound sync envelopes via DGP.
//!
//! # Architecture
//!
//! ```text
//! DGP (RFC-0852)
//!   └── object_type = 0x0008 SnapshotFragment
//!        └── SyncNode::on_snapshot_fragment
//!             └── DgpSyncBridge::dispatch
//!                  └── SyncHandler (sync engine impl)
//!
//! Outbound:
//!   SyncSessionManager::on_commit
//!     └── SyncNode::send_sync_envelope
//!          └── DGP GossipObject (object_type = 0x0008)
//! ```

pub mod dgp_integration;

use octo_sync::dgp_bridge::{DgpSyncBridge, GossipSnapshotFragment, SyncHandler};
use octo_sync::session::SyncSessionManager;

pub use dgp_integration::{SyncDgpHandler, SyncNetworkBridge};

/// DGP object type for sync snapshots (GossipObjectType::SnapshotFragment = 0x0008).
pub const SYNC_SNAPSHOT_OBJECT_TYPE: u16 = 0x0008;

/// The sync node: wraps `SyncSessionManager` and provides DGP integration.
///
/// This is the entry point for wiring the sync protocol into the network layer.
/// It does NOT own the network transport — the caller is responsible for
/// delivering DGP objects to [`SyncNode::on_snapshot_fragment`] and
/// sending outbound envelopes returned by [`SyncNode::prepare_sync_envelope`].
pub struct SyncNode<H: SyncHandler> {
    /// The sync session manager.
    session: SyncSessionManager,
    /// The DGP bridge for dispatching inbound fragments.
    bridge: DgpSyncBridge<H>,
    /// Mission ID for DGP domain routing.
    mission_id: [u8; 32],
}

impl<H: SyncHandler> SyncNode<H> {
    /// Create a new `SyncNode` from a session manager and handler.
    pub fn new(session: SyncSessionManager, handler: std::sync::Arc<H>) -> Self {
        let mission_id = session.config().mission_id;
        let bridge = DgpSyncBridge::new(mission_id, handler);
        Self {
            session,
            bridge,
            mission_id,
        }
    }

    /// Handle an incoming DGP SnapshotFragment.
    ///
    /// Dispatches to the appropriate handler method based on the envelope subtype.
    /// Fragments for other missions are silently ignored (per RFC-0852 §7).
    pub fn on_snapshot_fragment(
        &self,
        fragment: &GossipSnapshotFragment,
    ) -> Result<(), octo_sync::error::SyncError> {
        self.bridge.dispatch(fragment)
    }

    /// Return a reference to the underlying session manager.
    pub fn session(&self) -> &SyncSessionManager {
        &self.session
    }

    /// Return the mission ID.
    pub fn mission_id(&self) -> &[u8; 32] {
        &self.mission_id
    }

    /// Prepare an outbound sync envelope for DGP broadcast.
    ///
    /// Given a subtype and payload, wraps them into a `GossipSnapshotFragment`
    /// that the caller can send via DGP. The caller is responsible for:
    /// 1. Wrapping into a `GossipObject` with `object_type = 0x0008`
    /// 2. Computing `domain_id` from the mission_id
    /// 3. Signing and broadcasting via the DGP layer
    pub fn prepare_sync_envelope(
        &self,
        subtype: u8,
        peer_id: [u8; 32],
        payload: Vec<u8>,
    ) -> GossipSnapshotFragment {
        GossipSnapshotFragment::new(subtype, peer_id, self.mission_id, payload)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use octo_sync::config::{SyncConfig, SyncRole};
    use octo_sync::test_util::MockAdapter;
    use std::sync::Arc;

    struct TestSyncHandler;

    impl SyncHandler for TestSyncHandler {
        fn on_summary(&self, _peer_id: [u8; 32], _payload: Vec<u8>) {}
        fn on_segment(&self, _peer_id: [u8; 32], _payload: Vec<u8>) {}
        fn on_wal_tail(&self, _peer_id: [u8; 32], _payload: Vec<u8>) {}
    }

    fn make_node() -> SyncNode<TestSyncHandler> {
        let mut mission_id = [0u8; 32];
        mission_id[0] = 0xAB;
        let config = SyncConfig::new(mission_id, SyncRole::Replicator, vec![0x01; 32]);
        let adapter = Arc::new(MockAdapter::new(mission_id, [0x02; 32]));
        let session =
            SyncSessionManager::new(adapter as Arc<dyn octo_sync::adapter::DatabaseSyncAdapter>, config, &[0x42u8; 32])
                .unwrap();
        SyncNode::new(session, Arc::new(TestSyncHandler))
    }

    #[test]
    fn sync_node_creation() {
        let node = make_node();
        assert_eq!(node.mission_id()[0], 0xAB);
    }

    #[test]
    fn dispatch_matching_mission() {
        let node = make_node();
        let frag = GossipSnapshotFragment::new(0xA1, [2u8; 32], *node.mission_id(), vec![1, 2, 3]);
        // Should not error (handler silently accepts)
        node.on_snapshot_fragment(&frag).unwrap();
    }

    #[test]
    fn dispatch_wrong_mission_silently_dropped() {
        let node = make_node();
        let frag = GossipSnapshotFragment::new(0xA1, [2u8; 32], [99u8; 32], vec![]);
        node.on_snapshot_fragment(&frag).unwrap();
    }

    #[test]
    fn prepare_sync_envelope() {
        let node = make_node();
        let peer_id = [3u8; 32];
        let frag = node.prepare_sync_envelope(0xB1, peer_id, vec![0xAA]);
        assert_eq!(frag.object_type, 0x0008);
        assert_eq!(frag.subtype, 0xB1);
        assert_eq!(frag.peer_id, peer_id);
        assert_eq!(frag.mission_id, *node.mission_id());
        assert_eq!(frag.payload, vec![0xAA]);
    }
}
