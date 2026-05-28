//! Gateway Advertisement (RFC-0851 §4)

use crate::gdp::overlay_endpoint::OverlayEndpoint;
use serde::{Deserialize, Serialize};

/// Gateway Advertisement (GADV)
///
/// Advertises gateway capabilities, transport endpoints, and routes.
/// All Merkle roots use BLAKE3-256.
#[derive(Debug, Clone)]
pub struct GatewayAdvertisement {
    /// Protocol version
    pub version: u16,
    /// Gateway identifier (32 bytes)
    pub gateway_id: [u8; 32],
    /// Network identifier
    pub network_id: u32,
    /// Strictly monotonic sequence per gateway
    pub sequence: u64,
    /// Logical timestamp
    pub logical_timestamp: u64,
    /// Gateway class (enum value)
    pub gateway_class: u16,
    /// Merkle root of capabilities
    pub capabilities_root: [u8; 32],
    /// Merkle root of transport endpoints
    pub transport_root: [u8; 32],
    /// Merkle root of route vectors
    pub route_root: [u8; 32],
    /// Trust score commitment (RFC-0860)
    pub trust_root: [u8; 32],
    /// Overlay endpoints
    pub overlay_endpoints: Vec<OverlayEndpoint>,
    /// Ed25519 signature (64 bytes)
    pub signature: [u8; 64],
}

impl GatewayAdvertisement {
    /// Compute the Merkle root of a set of items using BLAKE3-256.
    ///
    /// Empty sets return [0x00; 32].
    pub fn compute_merkle_root(items: &[[u8; 32]]) -> [u8; 32] {
        if items.is_empty() {
            return [0u8; 32];
        }
        let mut level = items.to_vec();
        while level.len() > 1 {
            let mut next = Vec::new();
            for chunk in level.chunks(2) {
                let mut hasher = blake3::Hasher::new();
                hasher.update(&chunk[0]);
                if chunk.len() > 1 {
                    hasher.update(&chunk[1]);
                } else {
                    // Duplicate last leaf for odd count
                    hasher.update(&chunk[0]);
                }
                next.push(*hasher.finalize().as_bytes());
            }
            level = next;
        }
        level[0]
    }

    /// Compute signing bytes for signature verification.
    /// Excludes the signature field itself.
    pub fn to_signing_bytes(&self) -> Vec<u8> {
        let mut bytes = Vec::new();
        bytes.extend_from_slice(&self.version.to_be_bytes());
        bytes.extend_from_slice(&self.gateway_id);
        bytes.extend_from_slice(&self.network_id.to_be_bytes());
        bytes.extend_from_slice(&self.sequence.to_be_bytes());
        bytes.extend_from_slice(&self.logical_timestamp.to_be_bytes());
        bytes.extend_from_slice(&self.gateway_class.to_be_bytes());
        bytes.extend_from_slice(&self.capabilities_root);
        bytes.extend_from_slice(&self.transport_root);
        bytes.extend_from_slice(&self.route_root);
        bytes.extend_from_slice(&self.trust_root);
        bytes
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_merkle_root_empty() {
        let root = GatewayAdvertisement::compute_merkle_root(&[]);
        assert_eq!(root, [0u8; 32]);
    }

    #[test]
    fn test_merkle_root_single() {
        let items = [[1u8; 32]];
        let root = GatewayAdvertisement::compute_merkle_root(&items);
        assert_ne!(root, [0u8; 32]);
    }

    #[test]
    fn test_merkle_root_deterministic() {
        let items = [[1u8; 32], [2u8; 32], [3u8; 32]];
        let r1 = GatewayAdvertisement::compute_merkle_root(&items);
        let r2 = GatewayAdvertisement::compute_merkle_root(&items);
        assert_eq!(r1, r2);
    }

    #[test]
    fn test_merkle_root_different_order() {
        let items_a = [[1u8; 32], [2u8; 32]];
        let items_b = [[2u8; 32], [1u8; 32]];
        let r1 = GatewayAdvertisement::compute_merkle_root(&items_a);
        let r2 = GatewayAdvertisement::compute_merkle_root(&items_b);
        assert_ne!(r1, r2);
    }

    #[test]
    fn test_merkle_root_odd_count() {
        let items = [[1u8; 32], [2u8; 32], [3u8; 32]];
        let root = GatewayAdvertisement::compute_merkle_root(&items);
        // Should not panic and produce valid root
        assert_ne!(root, [0u8; 32]);
    }

    #[test]
    fn test_signing_bytes_deterministic() {
        let adv = GatewayAdvertisement {
            version: 1,
            gateway_id: [1u8; 32],
            network_id: 1,
            sequence: 1,
            logical_timestamp: 1000,
            gateway_class: 0x0001,
            capabilities_root: [2u8; 32],
            transport_root: [3u8; 32],
            route_root: [4u8; 32],
            trust_root: [5u8; 32],
            overlay_endpoints: vec![],
            signature: [0u8; 64],
        };
        let b1 = adv.to_signing_bytes();
        let b2 = adv.to_signing_bytes();
        assert_eq!(b1, b2);
    }

    #[test]
    fn test_signing_bytes_excludes_signature() {
        let mut adv = GatewayAdvertisement {
            version: 1,
            gateway_id: [1u8; 32],
            network_id: 1,
            sequence: 1,
            logical_timestamp: 1000,
            gateway_class: 0x0001,
            capabilities_root: [2u8; 32],
            transport_root: [3u8; 32],
            route_root: [4u8; 32],
            trust_root: [5u8; 32],
            overlay_endpoints: vec![],
            signature: [0u8; 64],
        };
        let b1 = adv.to_signing_bytes();
        adv.signature = [0xFFu8; 64];
        let b2 = adv.to_signing_bytes();
        assert_eq!(b1, b2);
    }
}
