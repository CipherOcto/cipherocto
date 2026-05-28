//! Mission Identity (RFC-0855 §2.1)

use serde::{Deserialize, Serialize};

/// Globally unique mission identifier.
///
/// `mission_hash = BLAKE3-256(creator_peer_id || creation_epoch || genesis_nonce)`
///
/// Determinism: given identical genesis inputs, all nodes compute identical MissionId.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[repr(C)]
pub struct MissionId {
    /// Network identifier
    pub network_id: u32,
    /// BLAKE3-256 of mission genesis material
    pub mission_hash: [u8; 32],
    /// Protocol version
    pub version: u16,
}

impl MissionId {
    /// Size of serialized MissionId: 4 + 32 + 2 = 38 bytes
    pub const SIZE: usize = 38;

    /// Create a new MissionId by deriving mission_hash from genesis material.
    pub fn new(
        network_id: u32,
        creator_peer_id: &[u8; 32],
        creation_epoch: u64,
        genesis_nonce: &[u8; 32],
        version: u16,
    ) -> Self {
        let mission_hash = Self::derive_hash(creator_peer_id, creation_epoch, genesis_nonce);
        Self {
            network_id,
            mission_hash,
            version,
        }
    }

    /// Derive mission_hash deterministically from genesis material.
    pub fn derive_hash(
        creator_peer_id: &[u8; 32],
        creation_epoch: u64,
        genesis_nonce: &[u8; 32],
    ) -> [u8; 32] {
        let mut hasher = blake3::Hasher::new();
        hasher.update(creator_peer_id);
        hasher.update(&creation_epoch.to_be_bytes());
        hasher.update(genesis_nonce);
        *hasher.finalize().as_bytes()
    }

    /// Serialize to canonical bytes (big-endian network_id || mission_hash || version).
    pub fn to_canonical_bytes(&self) -> [u8; Self::SIZE] {
        let mut buf = [0u8; Self::SIZE];
        buf[0..4].copy_from_slice(&self.network_id.to_be_bytes());
        buf[4..36].copy_from_slice(&self.mission_hash);
        buf[36..38].copy_from_slice(&self.version.to_be_bytes());
        buf
    }

    /// Deserialize from canonical bytes.
    pub fn from_canonical_bytes(bytes: &[u8]) -> Result<Self, crate::mon::error::MonError> {
        if bytes.len() < Self::SIZE {
            return Err(crate::mon::error::MonError::InvalidMissionId {
                mission_hash: [0u8; 32],
            });
        }
        let network_id = u32::from_be_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]);
        let mut mission_hash = [0u8; 32];
        mission_hash.copy_from_slice(&bytes[4..36]);
        let version = u16::from_be_bytes([bytes[36], bytes[37]]);
        Ok(Self {
            network_id,
            mission_hash,
            version,
        })
    }
}

/// Mission type identifier (RFC-0855 §2.3)
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[repr(u16)]
pub enum MissionType {
    AiSwarm = 0x0001,
    ComputeCluster = 0x0002,
    Enterprise = 0x0003,
    Governance = 0x0004,
    Tactical = 0x0005,
    Research = 0x0006,
    ProofSwarm = 0x0007,
    DataFederation = 0x0008,
    Custom = 0xFFFF,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_mission_id_deterministic() {
        let peer = [1u8; 32];
        let nonce = [2u8; 32];
        let id1 = MissionId::new(1, &peer, 100, &nonce, 1);
        let id2 = MissionId::new(1, &peer, 100, &nonce, 1);
        assert_eq!(id1, id2);
    }

    #[test]
    fn test_mission_id_different_inputs() {
        let peer1 = [1u8; 32];
        let peer2 = [2u8; 32];
        let nonce = [3u8; 32];
        let id1 = MissionId::new(1, &peer1, 100, &nonce, 1);
        let id2 = MissionId::new(1, &peer2, 100, &nonce, 1);
        assert_ne!(id1.mission_hash, id2.mission_hash);
    }

    #[test]
    fn test_mission_id_serialization_roundtrip() {
        let peer = [42u8; 32];
        let nonce = [99u8; 32];
        let id = MissionId::new(7, &peer, 500, &nonce, 2);
        let bytes = id.to_canonical_bytes();
        let recovered = MissionId::from_canonical_bytes(&bytes).unwrap();
        assert_eq!(id, recovered);
    }

    #[test]
    fn test_mission_id_from_bytes_too_short() {
        let result = MissionId::from_canonical_bytes(&[0u8; 10]);
        assert!(result.is_err());
    }

    #[test]
    fn test_mission_type_repr() {
        assert_eq!(MissionType::AiSwarm as u16, 0x0001);
        assert_eq!(MissionType::Custom as u16, 0xFFFF);
    }
}
