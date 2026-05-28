//! Gossip Domain identification (RFC-0852 §2)

use serde::{Deserialize, Serialize};

/// Gossip scope — determines propagation domain.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[repr(u16)]
pub enum GossipScope {
    GLOBAL = 0x0001,
    REGIONAL = 0x0002,
    MISSION = 0x0003,
    PRIVATE = 0x0004,
    LOCAL = 0x0005,
    CONSENSUS = 0x0006,
}

impl GossipScope {
    /// Parse from u16 value.
    pub fn from_u16(val: u16) -> Option<Self> {
        match val {
            0x0001 => Some(Self::GLOBAL),
            0x0002 => Some(Self::REGIONAL),
            0x0003 => Some(Self::MISSION),
            0x0004 => Some(Self::PRIVATE),
            0x0005 => Some(Self::LOCAL),
            0x0006 => Some(Self::CONSENSUS),
            _ => None,
        }
    }

    /// Default TTL for this scope.
    pub fn default_ttl(&self) -> u16 {
        match self {
            Self::GLOBAL => 20,
            Self::REGIONAL => 10,
            Self::MISSION => 5,
            Self::PRIVATE => 3,
            Self::LOCAL => 3,
            Self::CONSENSUS => 10,
        }
    }
}

/// Identifies a gossip propagation domain.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[repr(C)]
pub struct GossipDomainId {
    /// Network identifier
    pub network_id: u32,
    /// Mission identifier (zero for non-mission scopes)
    pub mission_id: [u8; 32],
    /// GossipScope enum value
    pub scope: u16,
}

impl GossipDomainId {
    /// Create a new domain ID.
    pub fn new(network_id: u32, mission_id: [u8; 32], scope: GossipScope) -> Self {
        Self {
            network_id,
            mission_id,
            scope: scope as u16,
        }
    }

    /// Serialize to canonical bytes (38 bytes).
    pub fn to_canonical_bytes(&self) -> [u8; 38] {
        let mut buf = [0u8; 38];
        buf[0..4].copy_from_slice(&self.network_id.to_be_bytes());
        buf[4..36].copy_from_slice(&self.mission_id);
        buf[36..38].copy_from_slice(&self.scope.to_be_bytes());
        buf
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_gossip_scope_from_u16() {
        assert_eq!(GossipScope::from_u16(0x0001), Some(GossipScope::GLOBAL));
        assert_eq!(GossipScope::from_u16(0x0006), Some(GossipScope::CONSENSUS));
        assert_eq!(GossipScope::from_u16(0xFFFF), None);
    }

    #[test]
    fn test_default_ttl() {
        assert_eq!(GossipScope::GLOBAL.default_ttl(), 20);
        assert_eq!(GossipScope::MISSION.default_ttl(), 5);
        assert_eq!(GossipScope::PRIVATE.default_ttl(), 3);
    }

    #[test]
    fn test_domain_id_canonical_bytes() {
        let id = GossipDomainId::new(1, [0xAA; 32], GossipScope::GLOBAL);
        let bytes = id.to_canonical_bytes();
        assert_eq!(bytes.len(), 38);
        assert_eq!(&bytes[0..4], &1u32.to_be_bytes());
        assert_eq!(&bytes[4..36], &[0xAA; 32]);
        assert_eq!(&bytes[36..38], &0x0001u16.to_be_bytes());
    }

    #[test]
    fn test_domain_id_equality() {
        let a = GossipDomainId::new(1, [0u8; 32], GossipScope::MISSION);
        let b = GossipDomainId::new(1, [0u8; 32], GossipScope::MISSION);
        let c = GossipDomainId::new(2, [0u8; 32], GossipScope::MISSION);
        assert_eq!(a, b);
        assert_ne!(a, c);
    }
}
