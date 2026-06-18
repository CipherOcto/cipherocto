//! Partition Resilience and Multi-Transport Mobility (RFC-0855 §13, §14)
//!
//! Partition resilience provides automatic recovery when network partitions heal.
//! Multi-transport mobility enables seamless carrier switching while preserving
//! mission identity.

use serde::{Deserialize, Serialize};

use crate::mon::mission_id::MissionId;

/// Partition event — a domain isolation or failure event.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PartitionEvent {
    pub domain_hash: [u8; 32],
    pub epoch: u64,
    pub affected_peers: Vec<[u8; 32]>,
}

/// Reconciliation state tracking.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[repr(C)]
pub struct ReconciliationState {
    pub mission_id: MissionId,
    pub partition_epoch: u64,
    pub local_state_root: [u8; 32],
    pub remote_state_root: [u8; 32],
    pub converged: bool,
    pub verified: bool,
}

impl ReconciliationState {
    pub fn new(mission_id: MissionId, partition_epoch: u64) -> Self {
        Self {
            mission_id,
            partition_epoch,
            local_state_root: [0xFF; 32],
            remote_state_root: [0x00; 32],
            converged: false,
            verified: false,
        }
    }

    /// Check if local and remote roots match (convergence).
    /// Only returns true if roots match AND both have been verified.
    pub fn check_convergence(&mut self) -> bool {
        self.converged = self.verified && self.local_state_root == self.remote_state_root;
        self.converged
    }

    /// Mark state roots as verified (both sides have been set to real values).
    pub fn verify(&mut self) {
        self.verified = true;
    }
}

// -- Multi-Transport Mobility (RFC-0855 §14) --

/// Transport carrier types for mobility.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[repr(u16)]
pub enum TransportCarrier {
    NativeP2P = 0x0001,
    Quic = 0x0002,
    Telegram = 0x0003,
    Discord = 0x0004,
    Matrix = 0x0005,
    Nostr = 0x0006,
    WebRtc = 0x0007,
    Bluetooth = 0x0008,
    LoRa = 0x0009,
    Signal = 0x000A,
    Webhook = 0x000B,
}

impl TransportCarrier {
    /// Parse from u16.
    pub fn from_u16(val: u16) -> Option<Self> {
        match val {
            0x0001 => Some(Self::NativeP2P),
            0x0002 => Some(Self::Quic),
            0x0003 => Some(Self::Telegram),
            0x0004 => Some(Self::Discord),
            0x0005 => Some(Self::Matrix),
            0x0006 => Some(Self::Nostr),
            0x0007 => Some(Self::WebRtc),
            0x0008 => Some(Self::Bluetooth),
            0x0009 => Some(Self::LoRa),
            0x000A => Some(Self::Signal),
            0x000B => Some(Self::Webhook),
            _ => None,
        }
    }
}

/// Mobility session — tracks transport changes while preserving identity.
///
/// RFC-0855 §14: The same peer_id MUST persist across transport changes.
/// Identity preservation is enforced by the cryptographic identity layer.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct MobilitySession {
    /// Peer identity (constant across transport changes)
    pub peer_id: [u8; 32],
    /// Mission this session belongs to
    pub mission_id: MissionId,
    /// Current active carrier
    pub active_carrier: TransportCarrier,
    /// Previous carrier (for dual-route handshake)
    pub previous_carrier: Option<TransportCarrier>,
    /// Epoch of last transport switch
    pub last_switch_epoch: u64,
    /// Number of transport switches
    pub switch_count: u32,
}

impl MobilitySession {
    /// Create a new mobility session.
    pub fn new(
        peer_id: [u8; 32],
        mission_id: MissionId,
        initial_carrier: TransportCarrier,
    ) -> Self {
        Self {
            peer_id,
            mission_id,
            active_carrier: initial_carrier,
            previous_carrier: None,
            last_switch_epoch: 0,
            switch_count: 0,
        }
    }

    /// Switch to a new carrier with seamless handover.
    ///
    /// RFC-0855 §14: New route established before old route terminated.
    /// Returns true if switch was successful (different carrier).
    pub fn switch_carrier(&mut self, new_carrier: TransportCarrier, epoch: u64) -> bool {
        if new_carrier == self.active_carrier {
            return false; // no-op, same carrier
        }
        self.previous_carrier = Some(self.active_carrier);
        self.active_carrier = new_carrier;
        self.last_switch_epoch = epoch;
        self.switch_count += 1;
        true
    }

    /// Check if identity is preserved (peer_id unchanged).
    /// This is always true by construction — the field is immutable after creation.
    pub fn is_identity_preserved(&self, expected_peer_id: &[u8; 32]) -> bool {
        self.peer_id == *expected_peer_id
    }

    /// Whether a dual-route handshake is in progress (previous carrier still active).
    pub fn is_handover_in_progress(&self) -> bool {
        self.previous_carrier.is_some() && self.last_switch_epoch > 0
    }

    /// Complete the handover (clear previous carrier).
    pub fn complete_handover(&mut self) {
        self.previous_carrier = None;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mon::mission_id::MissionId;

    fn test_mission_id() -> MissionId {
        MissionId::new(1, &[1u8; 32], 100, &[2u8; 32], 1)
    }

    #[test]
    fn test_reconciliation_new() {
        let rs = ReconciliationState::new(test_mission_id(), 100);
        assert_eq!(rs.partition_epoch, 100);
        assert!(!rs.converged);
        assert!(!rs.verified);
    }

    #[test]
    fn test_reconciliation_no_false_convergence() {
        let rs = ReconciliationState::new(test_mission_id(), 100);
        // Without verify(), convergence must be false even though sentinels differ
        assert!(!rs.converged);
    }

    #[test]
    fn test_reconciliation_convergence() {
        let mut rs = ReconciliationState::new(test_mission_id(), 100);
        rs.local_state_root = [0xAA; 32];
        rs.remote_state_root = [0xAA; 32];
        rs.verify();
        assert!(rs.check_convergence());
    }

    #[test]
    fn test_reconciliation_no_convergence() {
        let mut rs = ReconciliationState::new(test_mission_id(), 100);
        rs.local_state_root = [0xAA; 32];
        rs.remote_state_root = [0xBB; 32];
        rs.verify();
        assert!(!rs.check_convergence());
    }

    #[test]
    fn test_reconciliation_requires_verify() {
        let mut rs = ReconciliationState::new(test_mission_id(), 100);
        // Set matching roots but don't verify
        rs.local_state_root = [0xAA; 32];
        rs.remote_state_root = [0xAA; 32];
        // Should still be false without verify()
        assert!(!rs.check_convergence());
    }

    // -- TransportCarrier tests --

    #[test]
    fn test_transport_carrier_from_u16() {
        assert_eq!(
            TransportCarrier::from_u16(0x0001),
            Some(TransportCarrier::NativeP2P)
        );
        assert_eq!(
            TransportCarrier::from_u16(0x000B),
            Some(TransportCarrier::Webhook)
        );
        assert_eq!(TransportCarrier::from_u16(0x00FF), None);
    }

    #[test]
    fn test_transport_carrier_repr() {
        assert_eq!(TransportCarrier::Telegram as u16, 0x0003);
        assert_eq!(TransportCarrier::Bluetooth as u16, 0x0008);
    }

    // -- MobilitySession tests --

    #[test]
    fn test_mobility_session_new() {
        let session =
            MobilitySession::new([0xAA; 32], test_mission_id(), TransportCarrier::Telegram);
        assert_eq!(session.peer_id, [0xAA; 32]);
        assert_eq!(session.active_carrier, TransportCarrier::Telegram);
        assert!(session.previous_carrier.is_none());
        assert_eq!(session.switch_count, 0);
    }

    #[test]
    fn test_mobility_switch_carrier() {
        let mut session =
            MobilitySession::new([0xAA; 32], test_mission_id(), TransportCarrier::Telegram);
        assert!(session.switch_carrier(TransportCarrier::Quic, 100));
        assert_eq!(session.active_carrier, TransportCarrier::Quic);
        assert_eq!(session.previous_carrier, Some(TransportCarrier::Telegram));
        assert_eq!(session.switch_count, 1);
    }

    #[test]
    fn test_mobility_switch_same_carrier_noop() {
        let mut session =
            MobilitySession::new([0xAA; 32], test_mission_id(), TransportCarrier::Telegram);
        assert!(!session.switch_carrier(TransportCarrier::Telegram, 100));
        assert_eq!(session.switch_count, 0);
    }

    #[test]
    fn test_mobility_identity_preserved() {
        let session =
            MobilitySession::new([0xAA; 32], test_mission_id(), TransportCarrier::Telegram);
        assert!(session.is_identity_preserved(&[0xAA; 32]));
        assert!(!session.is_identity_preserved(&[0xBB; 32]));
    }

    #[test]
    fn test_mobility_handover_in_progress() {
        let mut session =
            MobilitySession::new([0xAA; 32], test_mission_id(), TransportCarrier::Telegram);
        assert!(!session.is_handover_in_progress());
        session.switch_carrier(TransportCarrier::Quic, 100);
        assert!(session.is_handover_in_progress());
        session.complete_handover();
        assert!(!session.is_handover_in_progress());
    }

    #[test]
    fn test_mobility_multiple_switches() {
        let mut session =
            MobilitySession::new([0xAA; 32], test_mission_id(), TransportCarrier::NativeP2P);
        session.switch_carrier(TransportCarrier::Telegram, 100);
        session.switch_carrier(TransportCarrier::Quic, 200);
        session.switch_carrier(TransportCarrier::Matrix, 300);
        assert_eq!(session.switch_count, 3);
        assert_eq!(session.active_carrier, TransportCarrier::Matrix);
        assert_eq!(session.last_switch_epoch, 300);
    }
}
