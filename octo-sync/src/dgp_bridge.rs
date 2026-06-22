//! DGP (Deterministic Gossip Protocol) sync bridge (per RFC-0862 Phase 3, mission 0862f).
//!
//! v1 implementation: minimal stub. The DGP `SnapshotFragment` object type
//! (RFC-0852 §3, type 0x0008) is the carrier for SyncSummary, SyncSegment,
//! and WalTailChunk envelopes. This module provides a thin bridge that
//! routes DGP-delivered fragments to the appropriate Sync handler.
//!
//! The full Phase 3 implementation (DRS-based peer selection, PoRelay trust
//! scoring, multi-reader topology) is in mission 0862f; this stub provides
//! the type definitions and a basic dispatch function.

use crate::envelope::WalTailChunk;
use crate::error::SyncError;
use crate::segment::SyncSegment;
use crate::summary::SyncSummary;

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

/// The DGP sync bridge. Routes fragments to the appropriate handler.
pub struct DgpSyncBridge {
    /// The local mission_id.
    mission_id: [u8; 32],
}

impl DgpSyncBridge {
    /// Create a new `DgpSyncBridge`.
    pub fn new(mission_id: [u8; 32]) -> Self {
        Self { mission_id }
    }

    /// Dispatch a DGP-delivered SnapshotFragment to the appropriate handler.
    ///
    /// Returns `Ok(())` if the fragment is for a different mission (silently
    /// ignored), or `Err(SyncError::UnknownEnvelopeSubtype)` if the subtype
    /// is not in the Sync range (0xA0-0xC2).
    ///
    /// v1 stub: the actual deserialization to SyncSummary/SyncSegment/WalTailChunk
    /// is performed by the appropriate mission module; this function just
    /// dispatches by subtype.
    pub fn dispatch(&self, fragment: &GossipSnapshotFragment) -> Result<(), SyncError> {
        // Ignore fragments for other missions
        if fragment.mission_id != self.mission_id {
            return Ok(());
        }
        // Dispatch by subtype
        match fragment.subtype {
            0xA1 => {
                // SummaryResponse
                // v1 stub: the cipherocto sync engine deserializes the payload
                // to a SyncSummary and processes it.
                let _: Option<SyncSummary> = None;
                Ok(())
            }
            0xA3 => {
                // SegmentResponse
                let _: Option<SyncSegment> = None;
                Ok(())
            }
            0xB1 => {
                // WalTailResponse
                let _: Option<WalTailChunk> = None;
                Ok(())
            }
            other => Err(SyncError::UnknownEnvelopeSubtype(other)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dispatch_unknown_subtype_errors() {
        let bridge = DgpSyncBridge::new([1u8; 32]);
        let frag = GossipSnapshotFragment::new(0x99, [2u8; 32], [1u8; 32], vec![]);
        let err = bridge.dispatch(&frag).unwrap_err();
        assert_eq!(err, SyncError::UnknownEnvelopeSubtype(0x99));
    }

    #[test]
    fn dispatch_other_mission_is_no_op() {
        let bridge = DgpSyncBridge::new([1u8; 32]);
        let frag = GossipSnapshotFragment::new(0xB1, [2u8; 32], [9u8; 32], vec![]);
        bridge.dispatch(&frag).unwrap(); // different mission, no error
    }

    #[test]
    fn dispatch_summary_response_ok() {
        let bridge = DgpSyncBridge::new([1u8; 32]);
        let frag = GossipSnapshotFragment::new(0xA1, [2u8; 32], [1u8; 32], vec![]);
        bridge.dispatch(&frag).unwrap();
    }
}
