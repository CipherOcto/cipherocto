//! Mission Gossip (RFC-0855 §9)
//!
//! Mission-scoped gossip domains with 7 propagation classes,
//! DGP priority mapping, and mission-scoped isolation.

use serde::{Deserialize, Serialize};

use super::mission_id::MissionId;

/// Mission gossip scope (RFC-0855 §9.1).
///
/// Each MON creates an isolated gossip domain. Messages within one
/// mission's gossip domain MUST NOT leak to other missions.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[repr(C)]
pub struct MissionGossipScope {
    /// Mission identifier
    pub mission_id: MissionId,
    /// Scope flags (bitmask)
    pub scope_flags: u64,
}

/// Scope flag constants for MissionGossipScope.
pub const SCOPE_FLAG_ENCRYPTED: u64 = 0x0001;
pub const SCOPE_FLAG_PRIORITY: u64 = 0x0002;
pub const SCOPE_FLAG_RELIABLE: u64 = 0x0004;

impl MissionGossipScope {
    /// Create a new mission gossip scope.
    pub fn new(mission_id: MissionId, scope_flags: u64) -> Self {
        Self {
            mission_id,
            scope_flags,
        }
    }

    /// Check if a specific scope flag is set.
    pub fn has_flag(&self, flag: u64) -> bool {
        (self.scope_flags & flag) == flag
    }
}

/// Mission propagation classes (RFC-0855 §9.2).
///
/// Maps to DGP GossipPriority for scheduling.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[repr(u16)]
pub enum MissionPropagationClass {
    /// Critical alerts, compromise detection — maps to DGP Critical
    Emergency = 0x0001,
    /// Validator data, attestations — maps to DGP Consensus
    Consensus = 0x0002,
    /// Commands, state updates — maps to DGP Mission
    Coordination = 0x0003,
    /// Compute payloads, inference requests — maps to DGP Bulk
    Execution = 0x0004,
    /// Model exchange, swarm coordination — maps to DGP Standard
    Ai = 0x0005,
    /// General mission communication — maps to DGP Standard
    Standard = 0x0006,
    /// Historical replication — maps to DGP Archive
    Archive = 0x0007,
}

impl MissionPropagationClass {
    /// Parse from u16 value.
    pub fn from_u16(val: u16) -> Option<Self> {
        match val {
            0x0001 => Some(Self::Emergency),
            0x0002 => Some(Self::Consensus),
            0x0003 => Some(Self::Coordination),
            0x0004 => Some(Self::Execution),
            0x0005 => Some(Self::Ai),
            0x0006 => Some(Self::Standard),
            0x0007 => Some(Self::Archive),
            _ => None,
        }
    }

    /// Map to DGP GossipPriority (RFC-0855 §9.2 mapping table).
    pub fn to_gossip_priority(&self) -> crate::dgp::GossipPriority {
        match self {
            Self::Emergency => crate::dgp::GossipPriority::Critical,
            Self::Consensus => crate::dgp::GossipPriority::Consensus,
            Self::Coordination => crate::dgp::GossipPriority::Mission,
            Self::Execution => crate::dgp::GossipPriority::Bulk,
            Self::Ai => crate::dgp::GossipPriority::Standard,
            Self::Standard => crate::dgp::GossipPriority::Standard,
            Self::Archive => crate::dgp::GossipPriority::Archive,
        }
    }
}

/// Mission gossip message — wraps a payload with mission-scoped metadata.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct MissionGossipMessage {
    /// Mission this message belongs to
    pub mission_id: MissionId,
    /// Propagation class for scheduling
    pub propagation_class: MissionPropagationClass,
    /// Payload hash (BLAKE3-256)
    pub payload_hash: [u8; 32],
    /// Logical timestamp for ordering
    pub logical_timestamp: u64,
    /// Sender gateway ID
    pub sender_gateway: [u8; 32],
    /// Ed25519 signature
    #[serde(with = "serde_bytes")]
    pub signature: [u8; 64],
}

impl MissionGossipMessage {
    /// Check if this message belongs to a specific mission.
    pub fn belongs_to_mission(&self, mission_id: &MissionId) -> bool {
        self.mission_id == *mission_id
    }

    /// Compute signing bytes.
    pub fn to_signing_bytes(&self) -> Vec<u8> {
        let mut bytes = Vec::new();
        bytes.extend_from_slice(&self.mission_id.to_canonical_bytes());
        bytes.extend_from_slice(&(self.propagation_class as u16).to_be_bytes());
        bytes.extend_from_slice(&self.payload_hash);
        bytes.extend_from_slice(&self.logical_timestamp.to_be_bytes());
        bytes.extend_from_slice(&self.sender_gateway);
        bytes
    }
}

/// Verify that a gossip message is scoped to the correct mission.
///
/// Returns true if the message's mission_id matches the expected mission,
/// preventing cross-mission gossip leakage.
pub fn verify_mission_scope(
    message: &MissionGossipMessage,
    expected_mission_id: &MissionId,
) -> bool {
    message.belongs_to_mission(expected_mission_id)
}

/// In-memory snapshot of the gossip protocol state for a mission scope.
///
/// Phase 12 G15 per RFC-0011-t §Substrate Mapping Table. Operates on
/// the in-memory snapshot of the gossip state; live gossip adapter
/// OUT OF SCOPE for Phase 12. Per-extension impl crates (Layer D)
/// provide real gossip adapters in follow-on missions. Scalar
/// field-order determinism preserved per RFC-0011-h §Output Envelope
/// determinism (no HashMap/BTreeMap iteration; zero collection
/// dependencies — counters are plain `u64` fields).
#[derive(Clone, Debug)]
pub struct Gossip {
    mission_id: MissionId,
    peers_reachable: u64,
    messages_sent: u64,
    messages_received: u64,
    messages_dropped: u64,
    anti_entropy_rounds: u64,
    last_sync_epoch: u64,
}

/// Aggregate gossip stats output (read-only projection).
///
/// Phase 12 G15 per RFC-0011-t §Substrate Mapping Table. Returned by
/// `Gossip::stats()` for substrate-faithful projection to the CLI
/// dispatch layer.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct GossipStats {
    pub mission_id_hex: String,
    pub peers_reachable: u64,
    pub messages_sent: u64,
    pub messages_received: u64,
    pub messages_dropped: u64,
    pub anti_entropy_rounds: u64,
    pub last_sync_epoch: u64,
}

impl Gossip {
    /// Construct a new `Gossip` snapshot from explicit counters.
    ///
    /// `anti_entropy_rounds` is plumbed through the substrate API
    /// but caller-stubbed at 0 in this phase per RFC-0011-t
    /// §Substrate-faithfulness. The real anti-entropy counter
    /// (RFC-0855 §8.2) is OUT OF SCOPE for Phase 12 and lands
    /// in a follow-on Layer D adapter mission; until then
    /// CLI callers MUST pass `0` for this slot so the substrate
    /// contract remains substrate-faithful (no premature counter
    /// semantics committed in Layer B).
    pub fn new(
        mission_id: MissionId,
        peers_reachable: u64,
        messages_sent: u64,
        messages_received: u64,
        messages_dropped: u64,
        anti_entropy_rounds: u64,
        last_sync_epoch: u64,
    ) -> Self {
        Self {
            mission_id,
            peers_reachable,
            messages_sent,
            messages_received,
            messages_dropped,
            anti_entropy_rounds,
            last_sync_epoch,
        }
    }

    /// Read the gossip stats as a substrate-faithful projection.
    pub fn stats(&self) -> GossipStats {
        GossipStats {
            mission_id_hex: hex::encode(self.mission_id.to_canonical_bytes()),
            peers_reachable: self.peers_reachable,
            messages_sent: self.messages_sent,
            messages_received: self.messages_received,
            messages_dropped: self.messages_dropped,
            anti_entropy_rounds: self.anti_entropy_rounds,
            last_sync_epoch: self.last_sync_epoch,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_mission_id() -> MissionId {
        MissionId::new(1, &[0xAA; 32], 100, &[0xBB; 32], 1)
    }

    #[test]
    fn test_propagation_class_from_u16() {
        assert_eq!(
            MissionPropagationClass::from_u16(0x0001),
            Some(MissionPropagationClass::Emergency)
        );
        assert_eq!(
            MissionPropagationClass::from_u16(0x0007),
            Some(MissionPropagationClass::Archive)
        );
        assert_eq!(MissionPropagationClass::from_u16(0x0099), None);
    }

    #[test]
    fn test_propagation_class_to_gossip_priority() {
        assert_eq!(
            MissionPropagationClass::Emergency.to_gossip_priority(),
            crate::dgp::GossipPriority::Critical
        );
        assert_eq!(
            MissionPropagationClass::Consensus.to_gossip_priority(),
            crate::dgp::GossipPriority::Consensus
        );
        assert_eq!(
            MissionPropagationClass::Coordination.to_gossip_priority(),
            crate::dgp::GossipPriority::Mission
        );
        assert_eq!(
            MissionPropagationClass::Execution.to_gossip_priority(),
            crate::dgp::GossipPriority::Bulk
        );
        assert_eq!(
            MissionPropagationClass::Archive.to_gossip_priority(),
            crate::dgp::GossipPriority::Archive
        );
    }

    #[test]
    fn test_propagation_class_ordering() {
        assert!(MissionPropagationClass::Emergency < MissionPropagationClass::Consensus);
        assert!(MissionPropagationClass::Consensus < MissionPropagationClass::Coordination);
        assert!(MissionPropagationClass::Standard < MissionPropagationClass::Archive);
    }

    #[test]
    fn test_mission_gossip_scope_flags() {
        let scope = MissionGossipScope::new(test_mission_id(), SCOPE_FLAG_ENCRYPTED);
        assert!(scope.has_flag(SCOPE_FLAG_ENCRYPTED));
        assert!(!scope.has_flag(SCOPE_FLAG_PRIORITY));
    }

    #[test]
    fn test_mission_gossip_scope_combined_flags() {
        let scope = MissionGossipScope::new(
            test_mission_id(),
            SCOPE_FLAG_ENCRYPTED | SCOPE_FLAG_RELIABLE,
        );
        assert!(scope.has_flag(SCOPE_FLAG_ENCRYPTED));
        assert!(scope.has_flag(SCOPE_FLAG_RELIABLE));
        assert!(!scope.has_flag(SCOPE_FLAG_PRIORITY));
    }

    #[test]
    fn test_gossip_message_belongs_to_mission() {
        let msg = MissionGossipMessage {
            mission_id: test_mission_id(),
            propagation_class: MissionPropagationClass::Standard,
            payload_hash: [0xBB; 32],
            logical_timestamp: 1000,
            sender_gateway: [0xCC; 32],
            signature: [0u8; 64],
        };
        assert!(msg.belongs_to_mission(&test_mission_id()));
        assert!(!msg.belongs_to_mission(&MissionId::new(2, &[0xDD; 32], 200, &[0xEE; 32], 1)));
    }

    #[test]
    fn test_gossip_message_signing_bytes_deterministic() {
        let msg = MissionGossipMessage {
            mission_id: test_mission_id(),
            propagation_class: MissionPropagationClass::Standard,
            payload_hash: [0xBB; 32],
            logical_timestamp: 1000,
            sender_gateway: [0xCC; 32],
            signature: [0u8; 64],
        };
        let b1 = msg.to_signing_bytes();
        let b2 = msg.to_signing_bytes();
        assert_eq!(b1, b2);
    }

    #[test]
    fn test_verify_mission_scope_correct() {
        let msg = MissionGossipMessage {
            mission_id: test_mission_id(),
            propagation_class: MissionPropagationClass::Standard,
            payload_hash: [0xBB; 32],
            logical_timestamp: 1000,
            sender_gateway: [0xCC; 32],
            signature: [0u8; 64],
        };
        assert!(verify_mission_scope(&msg, &test_mission_id()));
    }

    #[test]
    fn test_verify_mission_scope_wrong_mission() {
        let msg = MissionGossipMessage {
            mission_id: test_mission_id(),
            propagation_class: MissionPropagationClass::Standard,
            payload_hash: [0xBB; 32],
            logical_timestamp: 1000,
            sender_gateway: [0xCC; 32],
            signature: [0u8; 64],
        };
        let other = MissionId::new(99, &[0xFF; 32], 300, &[0xFE; 32], 1);
        assert!(!verify_mission_scope(&msg, &other));
    }

    #[test]
    fn test_all_propagation_classes_parse() {
        for i in 1..=7u16 {
            assert!(
                MissionPropagationClass::from_u16(i).is_some(),
                "Failed to parse class {i}"
            );
        }
        assert!(MissionPropagationClass::from_u16(8).is_none());
    }

    fn phase12_test_mission_id() -> MissionId {
        MissionId::new(12, &[0xCC; 32], 1200, &[0xDD; 32], 1)
    }

    #[test]
    fn tv_phase12_substrate_1_gossip_new_constructs_with_counters() {
        let mid = phase12_test_mission_id();
        let g = Gossip::new(mid, 7, 100, 95, 5, 3, 1200);
        let expected_mid = phase12_test_mission_id();
        assert_eq!(g.peers_reachable, 7);
        assert_eq!(g.messages_sent, 100);
        assert_eq!(g.messages_received, 95);
        assert_eq!(g.messages_dropped, 5);
        assert_eq!(g.anti_entropy_rounds, 3);
        assert_eq!(g.last_sync_epoch, 1200);
        assert_eq!(g.mission_id, expected_mid);
    }

    #[test]
    fn tv_phase12_substrate_2_gossip_stats_projects_counters() {
        let mid = phase12_test_mission_id();
        let g = Gossip::new(mid, 7, 100, 95, 5, 3, 1200);
        let stats = g.stats();
        assert_eq!(stats.peers_reachable, 7);
        assert_eq!(stats.messages_sent, 100);
        assert_eq!(stats.messages_received, 95);
        assert_eq!(stats.messages_dropped, 5);
        assert_eq!(stats.anti_entropy_rounds, 3);
        assert_eq!(stats.last_sync_epoch, 1200);
        // mission_id_hex is hex-encoded canonical bytes (76 chars for 38-byte
        // MissionId canonical form per RFC-0855 §MissionId canonical encoding).
        assert_eq!(stats.mission_id_hex.len(), 76);
        assert!(stats.mission_id_hex.chars().all(|c| c.is_ascii_hexdigit()));
        assert!(stats
            .mission_id_hex
            .chars()
            .all(|c| !c.is_ascii_uppercase()));
    }

    #[test]
    fn tv_phase12_substrate_3_gossip_stats_deterministic_across_calls() {
        let mid = phase12_test_mission_id();
        let g = Gossip::new(mid, 7, 100, 95, 5, 3, 1200);
        let s1 = g.stats();
        let s2 = g.stats();
        assert_eq!(s1, s2);
    }

    #[test]
    fn tv_phase12_substrate_4_gossip_stats_distinct_per_mission() {
        let mid_a = MissionId::new(12, &[0xAA; 32], 1200, &[0xBB; 32], 1);
        let mid_b = MissionId::new(13, &[0xAA; 32], 1200, &[0xBB; 32], 1);
        let g_a = Gossip::new(mid_a, 7, 100, 95, 5, 3, 1200);
        let g_b = Gossip::new(mid_b, 7, 100, 95, 5, 3, 1200);
        let s_a = g_a.stats();
        let s_b = g_b.stats();
        assert_ne!(s_a.mission_id_hex, s_b.mission_id_hex);
        // All counter fields identical (only mission differs).
        assert_eq!(s_a.peers_reachable, s_b.peers_reachable);
        assert_eq!(s_a.messages_sent, s_b.messages_sent);
    }
}
