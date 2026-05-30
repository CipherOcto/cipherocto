//! Mission Topology (RFC-0855 §5)

use serde::{Deserialize, Serialize};

use crate::mon::governance::GovernanceModel;
use crate::mon::lifecycle::MissionState;
use crate::mon::mission_id::{MissionId, MissionType};

/// Topology models (RFC-0855 §5.1)
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[repr(u16)]
pub enum TopologyModel {
    Mesh = 0x0001,
    Hierarchical = 0x0002,
    Star = 0x0003,
    Swarm = 0x0004,
    Ring = 0x0005,
    Hybrid = 0x0006,
}

/// Minimum participants per topology model.
pub fn min_participants_for_topology(model: TopologyModel) -> u32 {
    match model {
        TopologyModel::Mesh => 2,
        TopologyModel::Hierarchical => 3,
        TopologyModel::Star => 2,
        TopologyModel::Swarm => 5,
        TopologyModel::Ring => 3,
        TopologyModel::Hybrid => 2,
    }
}

/// Topology commitment for deterministic replay (RFC-0855 §5.2).
#[derive(Clone, Debug, Serialize, Deserialize)]
#[repr(C)]
pub struct TopologyCommitment {
    pub mission_id: MissionId,
    pub model: TopologyModel,
    pub participant_root: [u8; 32],
    pub route_root: [u8; 32],
    pub epoch: u64,
    pub commitment: [u8; 32],
}

impl TopologyCommitment {
    /// Compute commitment = BLAKE3-256(participant_root || route_root || epoch)
    pub fn compute(
        mission_id: MissionId,
        model: TopologyModel,
        participant_root: [u8; 32],
        route_root: [u8; 32],
        epoch: u64,
    ) -> Self {
        let mut hasher = blake3::Hasher::new();
        hasher.update(&participant_root);
        hasher.update(&route_root);
        hasher.update(&epoch.to_be_bytes());
        let commitment = *hasher.finalize().as_bytes();
        Self {
            mission_id,
            model,
            participant_root,
            route_root,
            epoch,
            commitment,
        }
    }
}

/// Mission descriptor flags (RFC-0855 §2.2)
pub const MISSION_FLAG_STEALTH: u64 = 0x0001;
pub const MISSION_FLAG_AUTO_RECOVER: u64 = 0x0002;
pub const MISSION_FLAG_PROOF_REQUIRED: u64 = 0x0004;
pub const MISSION_FLAG_EPHEMERAL: u64 = 0x0008;

/// Mission descriptor (RFC-0855 §2.2)
#[derive(Clone, Debug, Serialize, Deserialize)]
#[repr(C)]
pub struct MissionDescriptor {
    pub mission_id: MissionId,
    pub descriptor_version: u64,
    pub mission_type: MissionType,
    pub creation_epoch: u64,
    pub governance_model: GovernanceModel,
    pub cryptographic_suite: u16,
    pub mission_root: [u8; 32],
    pub max_participants: u32,
    pub min_participants: u32,
    pub ttl_epochs: u64,
    pub flags: u64,
}

/// Mission state root (RFC-0855 §12.1)
#[derive(Clone, Debug, Serialize, Deserialize)]
#[repr(C)]
pub struct MissionStateRoot {
    pub mission_id: MissionId,
    pub state: MissionState,
    pub epoch: u64,
    pub state_root: [u8; 32],
    pub participant_root: [u8; 32],
    pub execution_root: [u8; 32],
    pub gossip_root: [u8; 32],
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mon::mission_id::MissionId;

    #[test]
    fn test_topology_model_repr() {
        assert_eq!(TopologyModel::Mesh as u16, 0x0001);
        assert_eq!(TopologyModel::Hybrid as u16, 0x0006);
    }

    #[test]
    fn test_min_participants() {
        assert_eq!(min_participants_for_topology(TopologyModel::Mesh), 2);
        assert_eq!(
            min_participants_for_topology(TopologyModel::Hierarchical),
            3
        );
        assert_eq!(min_participants_for_topology(TopologyModel::Swarm), 5);
    }

    #[test]
    fn test_topology_commitment_deterministic() {
        let peer = [1u8; 32];
        let nonce = [2u8; 32];
        let mid = MissionId::new(1, &peer, 100, &nonce, 1);
        let tc1 = TopologyCommitment::compute(mid, TopologyModel::Mesh, [0xAA; 32], [0xBB; 32], 50);
        let tc2 = TopologyCommitment::compute(mid, TopologyModel::Mesh, [0xAA; 32], [0xBB; 32], 50);
        assert_eq!(tc1.commitment, tc2.commitment);
    }

    #[test]
    fn test_topology_commitment_different_roots() {
        let peer = [1u8; 32];
        let nonce = [2u8; 32];
        let mid = MissionId::new(1, &peer, 100, &nonce, 1);
        let tc1 = TopologyCommitment::compute(mid, TopologyModel::Mesh, [0xAA; 32], [0xBB; 32], 50);
        let tc2 = TopologyCommitment::compute(mid, TopologyModel::Mesh, [0xCC; 32], [0xBB; 32], 50);
        assert_ne!(tc1.commitment, tc2.commitment);
    }

    #[test]
    fn test_mission_flags() {
        assert_eq!(MISSION_FLAG_STEALTH, 0x0001);
        assert_eq!(MISSION_FLAG_EPHEMERAL, 0x0008);
    }
}
