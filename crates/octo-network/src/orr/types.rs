//! Core ORR types (RFC-0858 §2)

use serde::{Deserialize, Serialize};

/// Route flags bitmask
pub const ROUTE_FLAG_MISSION_SCOPED: u64 = 0x0001;
pub const ROUTE_FLAG_COVER: u64 = 0x0002;
pub const ROUTE_FLAG_HIGH_LATENCY: u64 = 0x0004;
pub const ROUTE_FLAG_STEALTH: u64 = 0x0008;

/// OnionRoute — top-level route descriptor (RFC-0858 §2.1)
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[repr(C)]
pub struct OnionRoute {
    /// Unique route identifier (BLAKE3-256 of route construction inputs)
    pub route_id: [u8; 32],
    /// Mission identifier (zero if not mission-scoped)
    pub mission_id: [u8; 32],
    /// Network epoch when route was constructed
    pub route_epoch: u64,
    /// Number of hops in the route
    pub hop_count: u16,
    /// Entry gateway identifier
    pub entry_gateway: [u8; 32],
    /// Exit gateway identifier
    pub exit_gateway: [u8; 32],
    /// Merkle root of layered route data
    pub layered_route_root: [u8; 32],
    /// Route construction timestamp (logical, not wall-clock)
    pub construction_timestamp: u64,
    /// Route flags (bitmask)
    pub flags: u64,
}

impl OnionRoute {
    /// Derive route_id from construction inputs.
    /// route_id = BLAKE3-256(mission_id || route_epoch || hop_count || entry_gateway || exit_gateway || construction_timestamp)
    pub fn derive_route_id(
        mission_id: &[u8; 32],
        route_epoch: u64,
        hop_count: u16,
        entry_gateway: &[u8; 32],
        exit_gateway: &[u8; 32],
        construction_timestamp: u64,
    ) -> [u8; 32] {
        let mut hasher = blake3::Hasher::new();
        hasher.update(mission_id);
        hasher.update(&route_epoch.to_be_bytes());
        hasher.update(&hop_count.to_be_bytes());
        hasher.update(entry_gateway);
        hasher.update(exit_gateway);
        hasher.update(&construction_timestamp.to_be_bytes());
        *hasher.finalize().as_bytes()
    }

    /// Compute layered_route_root from hop hashes.
    pub fn compute_layered_route_root(hop_hashes: &[[u8; 32]]) -> [u8; 32] {
        let mut hasher = blake3::Hasher::new();
        for hash in hop_hashes {
            hasher.update(hash);
        }
        *hasher.finalize().as_bytes()
    }
}

/// OnionHop — per-hop encrypted routing instructions (RFC-0858 §2.2)
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct OnionHop {
    /// Hop index in the route (0 = entry, hop_count-1 = exit)
    pub hop_index: u16,
    /// Relay gateway identifier
    pub relay_gateway: [u8; 32],
    /// Merkle root of remaining route's transport vectors
    pub transport_vector_root: [u8; 32],
    /// Encrypted next-hop instructions (128 bytes = 96 plaintext + 16 MAC + 16 padding)
    #[serde(with = "serde_bytes")]
    pub encrypted_next_hop: [u8; 128],
    /// Encrypted payload fragment (peeled at this hop)
    pub encrypted_payload_fragment: Vec<u8>,
    /// Hop-level MAC for integrity (BLAKE3-256)
    pub hop_mac: [u8; 32],
    /// Ephemeral public key for this hop's key derivation (X25519)
    pub ephemeral_public_key: [u8; 32],
}

/// TransportVector — transport selection for each hop (RFC-0858 §5.2)
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[repr(C)]
pub struct TransportVector {
    /// Transport type (per RFC-0850 platform types)
    pub transport_type: u16,
    /// Broadcast domain for this hop (BLAKE3-256 hash, 32 bytes)
    pub domain_id: [u8; 32],
    /// Priority within transport class
    pub priority: u16,
    /// Bandwidth class (0-255)
    pub bandwidth_class: u8,
    /// Censorship resistance score (0-255)
    pub censorship_score: u8,
}

/// RouteCommitment — cryptographic commitment to route structure (RFC-0858 §2.6)
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[repr(C)]
pub struct RouteCommitment {
    /// BLAKE3-256 of relay sequence
    pub relay_hash: [u8; 32],
    /// BLAKE3-256 of transport vectors
    pub transport_hash: [u8; 32],
    /// BLAKE3-256 of diversity scores
    pub diversity_hash: [u8; 32],
    /// Network epoch
    pub epoch: u64,
    /// Final commitment = BLAKE3-256(relay_hash || transport_hash || diversity_hash || epoch)
    pub commitment: [u8; 32],
}

impl RouteCommitment {
    /// Compute the route commitment deterministically.
    pub fn compute(
        relay_hash: [u8; 32],
        transport_hash: [u8; 32],
        diversity_hash: [u8; 32],
        epoch: u64,
    ) -> Self {
        let mut hasher = blake3::Hasher::new();
        hasher.update(&relay_hash);
        hasher.update(&transport_hash);
        hasher.update(&diversity_hash);
        hasher.update(&epoch.to_be_bytes());
        let commitment = *hasher.finalize().as_bytes();
        Self {
            relay_hash,
            transport_hash,
            diversity_hash,
            epoch,
            commitment,
        }
    }
}

/// CoverPolicy — cover traffic generation policy (RFC-0858 §2.5)
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[repr(u16)]
pub enum CoverPolicy {
    /// Constant rate: minimum envelopes per time period
    Constant { min_rate: u32 } = 0x0001,
    /// Proportional: cover traffic as ratio of real traffic (basis points, 100 = 1%)
    Proportional { ratio: u16 } = 0x0002,
    /// Burst matching: cover traffic during detected bursts
    Burst { sensitivity: u8 } = 0x0003,
    /// Disabled: no cover traffic (testing only)
    Disabled = 0x0004,
}

/// CoverEnvelope — cover traffic envelope (RFC-0858 §6.1)
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CoverEnvelope {
    /// Same structure as OnionRoute
    pub route: OnionRoute,
    /// Same layered encryption as real traffic
    pub layered_payload: Vec<u8>,
    // Note: cover flag is encrypted in the innermost layer, visible only to the destination.
    // Cover envelopes MUST be indistinguishable from real envelopes at every relay.
}

/// OnionDomain — mission-scoped anonymity domain (RFC-0858 §7.1)
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[repr(C)]
pub struct OnionDomain {
    /// Mission identifier
    pub mission_id: [u8; 32],
    /// Domain key (used for domain-scoped route construction)
    pub domain_key: [u8; 32],
    /// Minimum relay trust score for domain participation
    pub min_trust_score: u32,
    /// Required transport diversity (minimum distinct transport types)
    pub min_transport_diversity: u8,
    /// Cover traffic policy
    pub cover_policy: CoverPolicy,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_onion_route_derive_id_deterministic() {
        let id1 = OnionRoute::derive_route_id(&[1u8; 32], 100, 3, &[2u8; 32], &[3u8; 32], 500);
        let id2 = OnionRoute::derive_route_id(&[1u8; 32], 100, 3, &[2u8; 32], &[3u8; 32], 500);
        assert_eq!(id1, id2);
    }

    #[test]
    fn test_onion_route_derive_id_different_inputs() {
        let id1 = OnionRoute::derive_route_id(&[1u8; 32], 100, 3, &[2u8; 32], &[3u8; 32], 500);
        let id2 = OnionRoute::derive_route_id(&[1u8; 32], 100, 4, &[2u8; 32], &[3u8; 32], 500);
        assert_ne!(id1, id2);
    }

    #[test]
    fn test_layered_route_root_deterministic() {
        let hashes = vec![[0xAA; 32], [0xBB; 32], [0xCC; 32]];
        let root1 = OnionRoute::compute_layered_route_root(&hashes);
        let root2 = OnionRoute::compute_layered_route_root(&hashes);
        assert_eq!(root1, root2);
    }

    #[test]
    fn test_route_commitment_deterministic() {
        let c1 = RouteCommitment::compute([0xAA; 32], [0xBB; 32], [0xCC; 32], 100);
        let c2 = RouteCommitment::compute([0xAA; 32], [0xBB; 32], [0xCC; 32], 100);
        assert_eq!(c1.commitment, c2.commitment);
    }

    #[test]
    fn test_route_commitment_different_epoch() {
        let c1 = RouteCommitment::compute([0xAA; 32], [0xBB; 32], [0xCC; 32], 100);
        let c2 = RouteCommitment::compute([0xAA; 32], [0xBB; 32], [0xCC; 32], 200);
        assert_ne!(c1.commitment, c2.commitment);
    }

    #[test]
    fn test_transport_vector_equality() {
        let v1 = TransportVector {
            transport_type: 1,
            domain_id: [0xAA; 32],
            priority: 10,
            bandwidth_class: 5,
            censorship_score: 200,
        };
        let v2 = v1.clone();
        assert_eq!(v1, v2);
    }

    #[test]
    fn test_onion_domain_creation() {
        let domain = OnionDomain {
            mission_id: [0x01; 32],
            domain_key: [0x02; 32],
            min_trust_score: 500,
            min_transport_diversity: 3,
            cover_policy: CoverPolicy::Proportional { ratio: 2000 },
        };
        assert_eq!(domain.min_trust_score, 500);
        assert_eq!(domain.min_transport_diversity, 3);
    }

    #[test]
    fn test_cover_envelope_creation() {
        let route = OnionRoute {
            route_id: [0u8; 32],
            mission_id: [0u8; 32],
            route_epoch: 0,
            hop_count: 3,
            entry_gateway: [1u8; 32],
            exit_gateway: [2u8; 32],
            layered_route_root: [0u8; 32],
            construction_timestamp: 0,
            flags: 0,
        };
        let cover = CoverEnvelope {
            route,
            layered_payload: vec![0u8; 128],
        };
        assert_eq!(cover.route.hop_count, 3);
        assert_eq!(cover.layered_payload.len(), 128);
    }

    #[test]
    fn test_onion_route_flags() {
        let route = OnionRoute {
            route_id: [0u8; 32],
            mission_id: [0u8; 32],
            route_epoch: 0,
            hop_count: 3,
            entry_gateway: [1u8; 32],
            exit_gateway: [2u8; 32],
            layered_route_root: [0u8; 32],
            construction_timestamp: 0,
            flags: ROUTE_FLAG_COVER | ROUTE_FLAG_HIGH_LATENCY,
        };
        assert!(route.flags & ROUTE_FLAG_COVER != 0);
        assert!(route.flags & ROUTE_FLAG_HIGH_LATENCY != 0);
        assert!(route.flags & ROUTE_FLAG_STEALTH == 0);
    }
}
