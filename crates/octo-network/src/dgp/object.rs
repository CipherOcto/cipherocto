//! Gossip Object (RFC-0852 §3)

use serde::{Deserialize, Serialize};

use super::domain::GossipDomainId;
use super::error::DgpError;

/// Gossip object types.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[repr(u16)]
pub enum GossipObjectType {
    Envelope = 0x0001,
    RouteUpdate = 0x0002,
    ConsensusFragment = 0x0003,
    MissionState = 0x0004,
    VectorCommitment = 0x0005,
    ZkProof = 0x0006,
    DiscoveryAdvertisement = 0x0007,
    SnapshotFragment = 0x0008,
}

impl GossipObjectType {
    /// Parse from u16.
    pub fn from_u16(val: u16) -> Option<Self> {
        match val {
            0x0001 => Some(Self::Envelope),
            0x0002 => Some(Self::RouteUpdate),
            0x0003 => Some(Self::ConsensusFragment),
            0x0004 => Some(Self::MissionState),
            0x0005 => Some(Self::VectorCommitment),
            0x0006 => Some(Self::ZkProof),
            0x0007 => Some(Self::DiscoveryAdvertisement),
            0x0008 => Some(Self::SnapshotFragment),
            _ => None,
        }
    }
}

/// Propagation flags bitmask.
pub const FLAG_FLOOD: u64 = 0x0001;
pub const FLAG_INCREMENTAL: u64 = 0x0002;
pub const FLAG_ANTI_ENTROPY: u64 = 0x0004;
pub const FLAG_DIRECTED: u64 = 0x0008;
pub const FLAG_RELIABLE: u64 = 0x0010;
pub const FLAG_COMPRESSED: u64 = 0x0020;

/// Gossip priority for scheduling.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[repr(u8)]
pub enum GossipPriority {
    Critical = 0,
    Consensus = 1,
    Mission = 2,
    Standard = 3,
    Bulk = 4,
    Archive = 5,
}

/// Canonical gossip object (RFC-0852 §3).
#[derive(Debug, Clone)]
#[repr(C)]
pub struct GossipObject {
    /// Object type
    pub object_type: u16,
    /// BLAKE3-256 of canonical serialization
    pub object_hash: [u8; 32],
    /// Serialized size in bytes
    pub object_size: u32,
    /// Propagation domain
    pub domain_id: GossipDomainId,
    /// Logical timestamp for ordering
    pub logical_timestamp: u64,
    /// Originating gateway
    pub origin_gateway: [u8; 32],
    /// Remaining hop count
    pub ttl_hops: u16,
    /// Propagation flags bitmask
    pub propagation_flags: u64,
    /// Merkle root of payload
    pub payload_root: [u8; 32],
    /// Ed25519 signature
    pub signature: [u8; 64],
}

impl GossipObject {
    /// Derive the object hash from canonical serialization.
    pub fn derive_object_hash(&self) -> [u8; 32] {
        let mut hasher = blake3::Hasher::new();
        hasher.update(&self.object_type.to_be_bytes());
        hasher.update(&self.object_size.to_be_bytes());
        hasher.update(&self.domain_id.to_canonical_bytes());
        hasher.update(&self.logical_timestamp.to_be_bytes());
        hasher.update(&self.origin_gateway);
        hasher.update(&self.ttl_hops.to_be_bytes());
        hasher.update(&self.propagation_flags.to_be_bytes());
        hasher.update(&self.payload_root);
        *hasher.finalize().as_bytes()
    }

    /// Verify the object hash matches the derived value.
    pub fn verify_hash(&self) -> bool {
        let derived = self.derive_object_hash();
        self.object_hash == derived
    }

    /// Check if a specific propagation flag is set.
    pub fn has_flag(&self, flag: u64) -> bool {
        (self.propagation_flags & flag) == flag
    }

    /// Decrement TTL. Returns false if expired.
    pub fn decrement_ttl(&mut self) -> bool {
        if self.ttl_hops == 0 {
            return false;
        }
        self.ttl_hops -= 1;
        self.ttl_hops > 0
    }

    /// Canonical ordering key: (domain_id_bytes, logical_timestamp, object_hash).
    pub fn ordering_key(&self) -> (Vec<u8>, u64, [u8; 32]) {
        (
            self.domain_id.to_canonical_bytes().to_vec(),
            self.logical_timestamp,
            self.object_hash,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dgp::domain::GossipScope;

    fn test_object() -> GossipObject {
        GossipObject {
            object_type: GossipObjectType::Envelope as u16,
            object_hash: [0u8; 32],
            object_size: 100,
            domain_id: GossipDomainId::new(1, [0u8; 32], GossipScope::GLOBAL),
            logical_timestamp: 1000,
            origin_gateway: [1u8; 32],
            ttl_hops: 20,
            propagation_flags: FLAG_FLOOD | FLAG_INCREMENTAL,
            payload_root: blake3::hash(b"payload").into(),
            signature: [0u8; 64],
        }
    }

    #[test]
    fn test_object_type_from_u16() {
        assert_eq!(
            GossipObjectType::from_u16(0x0001),
            Some(GossipObjectType::Envelope)
        );
        assert_eq!(
            GossipObjectType::from_u16(0x0008),
            Some(GossipObjectType::SnapshotFragment)
        );
        assert_eq!(GossipObjectType::from_u16(0x0099), None);
    }

    #[test]
    fn test_propagation_flags() {
        let obj = test_object();
        assert!(obj.has_flag(FLAG_FLOOD));
        assert!(obj.has_flag(FLAG_INCREMENTAL));
        assert!(!obj.has_flag(FLAG_DIRECTED));
        assert!(!obj.has_flag(FLAG_COMPRESSED));
    }

    #[test]
    fn test_ttl_decrement() {
        let mut obj = test_object();
        assert!(obj.decrement_ttl());
        assert_eq!(obj.ttl_hops, 19);
    }

    #[test]
    fn test_ttl_expiration() {
        let mut obj = test_object();
        obj.ttl_hops = 1;
        assert!(!obj.decrement_ttl());
        assert_eq!(obj.ttl_hops, 0);
    }

    #[test]
    fn test_ordering_key() {
        let a = test_object();
        let mut b = test_object();
        b.logical_timestamp = 2000;
        assert!(a.ordering_key() < b.ordering_key());
    }

    #[test]
    fn test_derive_object_hash() {
        let mut obj = test_object();
        let hash1 = obj.derive_object_hash();
        obj.object_hash = hash1;
        assert!(obj.verify_hash());
    }
}
