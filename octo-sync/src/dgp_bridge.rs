//! DGP (Deterministic Gossip Protocol) sync bridge (per RFC-0862 Phase 3, mission 0862f).
//!
//! Routes DGP-delivered `SnapshotFragment` envelopes to the cipherocto sync
//! engine's [`SyncHandler`] implementation. The bridge does NOT decode the
//! payload — the handler does (the cipherocto sync engine is the owner of
//! the wire encoding). The bridge just dispatches by envelope subtype.
//!
//! # Production architecture
//!
//! ```text
//! DGP (RFC-0852)
//!   └── object_type = 0x0008 SnapshotFragment
//!        └── subtype 0xA0-0xC2 (Sync envelopes)
//!             └── DgpSyncBridge::dispatch
//!                  ├── 0xA1 SummaryResponse   → handler.on_summary(peer, payload)
//!                  ├── 0xA3 SegmentResponse   → handler.on_segment(peer, payload)
//!                  └── 0xB1 WalTailResponse  → handler.on_wal_tail(peer, payload)
//! ```
//!
//! The handler is an `Arc<dyn SyncHandler>` so the cipherocto sync engine can
//! provide a single concrete impl that handles all three subtypes.

use std::sync::Arc;

use crate::error::SyncError;

/// A DGP-delivered `SnapshotFragment` (RFC-0852 §3, object_type = 0x0008).
#[derive(Debug, Clone)]
pub struct GossipSnapshotFragment {
    /// The DGP object_type discriminator (always 0x0008 for Sync fragments).
    pub object_type: u16,
    /// The envelope subtype within the Sync range (0xA0-0xC2).
    pub subtype: u8,
    /// The peer that sent this fragment.
    pub peer_id: [u8; 32],
    /// The mission_id this fragment belongs to.
    pub mission_id: [u8; 32],
    /// The encoded payload (one of SyncSummary, SyncSegment, or WalTailChunk).
    pub payload: Vec<u8>,
}

impl GossipSnapshotFragment {
    /// Create a new `GossipSnapshotFragment`.
    pub fn new(subtype: u8, peer_id: [u8; 32], mission_id: [u8; 32], payload: Vec<u8>) -> Self {
        Self {
            object_type: 0x0008,
            subtype,
            peer_id,
            mission_id,
            payload,
        }
    }
}

/// The handler that the cipherocto sync engine implements to process
/// DGP-delivered Sync envelopes.
///
/// This trait is the integration boundary between the wire layer (DGP
/// bridge) and the sync engine. The cipherocto sync engine provides a
/// concrete impl; the DGP bridge calls into it.
pub trait SyncHandler: Send + Sync + 'static {
    /// Handle a `SummaryResponse` envelope (subtype 0xA1).
    ///
    /// `peer_id` is the sending peer. `payload` is the raw `SyncSummary`
    /// bytes (the wire encoding is the cipherocto sync engine's choice;
    /// the bridge does not parse it).
    fn on_summary(&self, peer_id: [u8; 32], payload: Vec<u8>);

    /// Handle a `SegmentResponse` envelope (subtype 0xA3).
    fn on_segment(&self, peer_id: [u8; 32], payload: Vec<u8>);

    /// Handle a `WalTailResponse` envelope (subtype 0xB1).
    fn on_wal_tail(&self, peer_id: [u8; 32], payload: Vec<u8>);
}

/// The DGP sync bridge. Routes fragments to the appropriate handler.
pub struct DgpSyncBridge<H: SyncHandler + ?Sized> {
    /// The local mission_id.
    mission_id: [u8; 32],
    /// The handler to dispatch to.
    handler: Arc<H>,
}

impl<H: SyncHandler + ?Sized> DgpSyncBridge<H> {
    /// Create a new `DgpSyncBridge` for the given mission and handler.
    pub fn new(mission_id: [u8; 32], handler: Arc<H>) -> Self {
        Self {
            mission_id,
            handler,
        }
    }

    /// Dispatch a DGP-delivered SnapshotFragment to the appropriate handler.
    ///
    /// Returns:
    /// - `Ok(())` if the fragment is for a different mission (silently
    ///   ignored, per RFC-0852 §7 anti-entropy semantics)
    /// - `Err(SyncError::UnknownEnvelopeSubtype)` if the subtype is not in
    ///   the Sync range (0xA0-0xC2) — fired by the envelope validator
    ///   per RFC-0862 §Error Handling
    pub fn dispatch(&self, fragment: &GossipSnapshotFragment) -> Result<(), SyncError> {
        // Ignore fragments for other missions (silently drop, no error)
        if fragment.mission_id != self.mission_id {
            return Ok(());
        }
        // Dispatch by subtype. The bridge does NOT decode the payload —
        // the handler is responsible for parsing.
        match fragment.subtype {
            0xA1 => {
                self.handler
                    .on_summary(fragment.peer_id, fragment.payload.clone());
                Ok(())
            }
            0xA3 => {
                self.handler
                    .on_segment(fragment.peer_id, fragment.payload.clone());
                Ok(())
            }
            0xB1 => {
                self.handler
                    .on_wal_tail(fragment.peer_id, fragment.payload.clone());
                Ok(())
            }
            other => Err(SyncError::UnknownEnvelopeSubtype(other)),
        }
    }

    /// Return the local mission_id.
    pub fn mission_id(&self) -> &[u8; 32] {
        &self.mission_id
    }

    /// Return a reference to the handler.
    pub fn handler(&self) -> &Arc<H> {
        &self.handler
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    /// A test handler that records all calls.
    struct TestHandler {
        summaries: Mutex<Vec<([u8; 32], Vec<u8>)>>,
        segments: Mutex<Vec<([u8; 32], Vec<u8>)>>,
        wal_tails: Mutex<Vec<([u8; 32], Vec<u8>)>>,
    }

    impl TestHandler {
        fn new() -> Self {
            Self {
                summaries: Mutex::new(Vec::new()),
                segments: Mutex::new(Vec::new()),
                wal_tails: Mutex::new(Vec::new()),
            }
        }
    }

    impl SyncHandler for TestHandler {
        fn on_summary(&self, peer_id: [u8; 32], payload: Vec<u8>) {
            self.summaries.lock().unwrap().push((peer_id, payload));
        }
        fn on_segment(&self, peer_id: [u8; 32], payload: Vec<u8>) {
            self.segments.lock().unwrap().push((peer_id, payload));
        }
        fn on_wal_tail(&self, peer_id: [u8; 32], payload: Vec<u8>) {
            self.wal_tails.lock().unwrap().push((peer_id, payload));
        }
    }

    #[test]
    fn dispatch_unknown_subtype_errors() {
        let handler = Arc::new(TestHandler::new());
        let bridge = DgpSyncBridge::new([1u8; 32], handler);
        let frag = GossipSnapshotFragment::new(0x99, [2u8; 32], [1u8; 32], vec![]);
        let err = bridge.dispatch(&frag).unwrap_err();
        assert_eq!(err, SyncError::UnknownEnvelopeSubtype(0x99));
    }

    #[test]
    fn dispatch_other_mission_is_silently_dropped() {
        let handler = Arc::new(TestHandler::new());
        let bridge = DgpSyncBridge::new([1u8; 32], handler);
        let frag = GossipSnapshotFragment::new(0xB1, [2u8; 32], [9u8; 32], vec![1, 2, 3]);
        bridge.dispatch(&frag).unwrap();
        // Handler was NOT called (different mission, silent drop)
        let h = bridge.handler();
        assert_eq!(h.wal_tails.lock().unwrap().len(), 0);
    }

    #[test]
    fn dispatch_summary_response_calls_handler() {
        let handler = Arc::new(TestHandler::new());
        let bridge = DgpSyncBridge::new([1u8; 32], handler.clone());
        let frag = GossipSnapshotFragment::new(0xA1, [2u8; 32], [1u8; 32], vec![0xAA, 0xBB]);
        bridge.dispatch(&frag).unwrap();
        let h = bridge.handler();
        assert_eq!(h.summaries.lock().unwrap().len(), 1);
        assert_eq!(h.summaries.lock().unwrap()[0].0, [2u8; 32]);
        assert_eq!(h.summaries.lock().unwrap()[0].1, vec![0xAA, 0xBB]);
    }

    #[test]
    fn dispatch_segment_response_calls_handler() {
        let handler = Arc::new(TestHandler::new());
        let bridge = DgpSyncBridge::new([1u8; 32], handler.clone());
        let frag = GossipSnapshotFragment::new(0xA3, [2u8; 32], [1u8; 32], vec![0xCC]);
        bridge.dispatch(&frag).unwrap();
        let h = bridge.handler();
        assert_eq!(h.segments.lock().unwrap().len(), 1);
    }

    #[test]
    fn dispatch_wal_tail_response_calls_handler() {
        let handler = Arc::new(TestHandler::new());
        let bridge = DgpSyncBridge::new([1u8; 32], handler.clone());
        let frag = GossipSnapshotFragment::new(0xB1, [2u8; 32], [1u8; 32], vec![0xDD, 0xEE, 0xFF]);
        bridge.dispatch(&frag).unwrap();
        let h = bridge.handler();
        assert_eq!(h.wal_tails.lock().unwrap().len(), 1);
        assert_eq!(h.wal_tails.lock().unwrap()[0].1, vec![0xDD, 0xEE, 0xFF]);
    }

    #[test]
    fn mission_id_getter() {
        let handler = Arc::new(TestHandler::new());
        let bridge = DgpSyncBridge::new([42u8; 32], handler);
        assert_eq!(bridge.mission_id(), &[42u8; 32]);
    }
}
