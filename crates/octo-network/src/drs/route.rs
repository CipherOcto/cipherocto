//! Deterministic Route (RFC-0856 Section 4)

use serde::{Deserialize, Serialize};

/// DRS protocol version constant (RFC-0856 Section 4.2)
pub const DRS_PROTOCOL_VERSION: u8 = 1;

/// Deterministic route — in-memory representation (RFC-0856 Section 4).
///
/// All fields use fixed-size types for deterministic serialization.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[repr(C)]
pub struct DeterministicRoute {
    /// Globally unique route identifier
    pub route_id: [u8; 32],
    /// Source gateway identifier
    pub source_gateway: [u8; 32],
    /// Destination gateway identifier
    pub destination_gateway: [u8; 32],
    /// Next hop gateway identifier
    pub next_hop: [u8; 32],
    /// Merkle root of transport vectors
    pub transport_vector_root: [u8; 32],
    /// Trust score (0-1_000_000)
    pub trust_score: u64,
    /// Bandwidth class (0-65535)
    pub bandwidth_class: u16,
    /// Latency class (0-65535)
    pub latency_class: u16,
    /// Censorship resistance class (0-65535)
    pub censorship_resistance_class: u16,
    /// Route cost (OCTO-B per hop, microtokens)
    pub route_cost: u64,
    /// Route epoch (consensus-derived)
    pub route_epoch: u64,
    /// Epoch until which this route is valid (0 = no expiry)
    pub valid_until_epoch: u64,
    /// Maximum hop count
    pub ttl_hops: u16,
    /// Ed25519 signature over canonical route bytes (RFC-0856 Section 4.1)
    #[serde(with = "serde_bytes")]
    pub signature: [u8; 64],
}

/// Transport vector — describes one transport path (RFC-0856 Section 5.2).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[repr(C)]
pub struct TransportVector {
    /// Transport type identifier
    pub transport_type: u16,
    /// Transport class
    pub transport_class: u16,
    /// Reliability score (0-1_000_000)
    pub reliability_score: u32,
    /// Censorship resistance score (0-1_000_000)
    pub censorship_score: u32,
    /// Cost class
    pub cost_class: u32,
}

impl DeterministicRoute {
    /// Compute the route identifier (RFC-0856 Section 4.2).
    ///
    /// `route_id = BLAKE3-256(version || source_gateway || destination_gateway
    ///             || next_hop || transport_vector_root || route_epoch)`
    pub fn compute_route_id(&self) -> [u8; 32] {
        let mut hasher = blake3::Hasher::new();
        hasher.update(&[DRS_PROTOCOL_VERSION]);
        hasher.update(&self.source_gateway);
        hasher.update(&self.destination_gateway);
        hasher.update(&self.next_hop);
        hasher.update(&self.transport_vector_root);
        hasher.update(&self.route_epoch.to_be_bytes());
        *hasher.finalize().as_bytes()
    }

    /// Compute canonical bytes for signing (excludes signature field).
    pub fn to_signing_bytes(&self) -> Vec<u8> {
        let mut buf = Vec::new();
        buf.extend_from_slice(&self.route_id);
        buf.extend_from_slice(&self.source_gateway);
        buf.extend_from_slice(&self.destination_gateway);
        buf.extend_from_slice(&self.next_hop);
        buf.extend_from_slice(&self.transport_vector_root);
        buf.extend_from_slice(&self.trust_score.to_be_bytes());
        buf.extend_from_slice(&self.bandwidth_class.to_be_bytes());
        buf.extend_from_slice(&self.latency_class.to_be_bytes());
        buf.extend_from_slice(&self.censorship_resistance_class.to_be_bytes());
        buf.extend_from_slice(&self.route_cost.to_be_bytes());
        buf.extend_from_slice(&self.route_epoch.to_be_bytes());
        buf.extend_from_slice(&self.valid_until_epoch.to_be_bytes());
        buf.extend_from_slice(&self.ttl_hops.to_be_bytes());
        buf
    }

    /// Verify the route signature against a public key.
    pub fn verify_signature(&self, public_key: &[u8; 32]) -> bool {
        use ed25519_dalek::{Signature, Verifier, VerifyingKey};
        let vk = match VerifyingKey::from_bytes(public_key) {
            Ok(vk) => vk,
            Err(_) => return false,
        };
        let sig = Signature::from_bytes(&self.signature);
        vk.verify(&self.to_signing_bytes(), &sig).is_ok()
    }

    /// Check if route is expired.
    /// A route is expired if its `valid_until_epoch` is non-zero and has passed.
    pub fn is_expired(&self, current_epoch: u64) -> bool {
        self.valid_until_epoch != 0 && self.valid_until_epoch < current_epoch
    }
}

/// Compare two routes for canonical ordering.
/// Order: score DESC, epoch ASC, route_id ASC
pub fn compare_routes(
    a: &DeterministicRoute,
    b: &DeterministicRoute,
    score_a: u64,
    score_b: u64,
) -> std::cmp::Ordering {
    score_b
        .cmp(&score_a) // DESC: higher score sorts first
        .then_with(|| a.route_epoch.cmp(&b.route_epoch))
        .then_with(|| a.route_id.cmp(&b.route_id))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_route(id: u8, epoch: u64) -> DeterministicRoute {
        DeterministicRoute {
            route_id: [id; 32],
            source_gateway: [0x01; 32],
            destination_gateway: [0x02; 32],
            next_hop: [0x03; 32],
            transport_vector_root: [0u8; 32],
            trust_score: 500,
            bandwidth_class: 100,
            latency_class: 50,
            censorship_resistance_class: 200,
            route_cost: 1000,
            route_epoch: epoch,
            valid_until_epoch: 0,
            ttl_hops: 10,
            signature: [0u8; 64],
        }
    }

    #[test]
    fn test_route_id_deterministic() {
        let r = make_route(1, 100);
        let c1 = r.compute_route_id();
        let c2 = r.compute_route_id();
        assert_eq!(c1, c2);
    }

    #[test]
    fn test_route_id_includes_version() {
        let r = make_route(1, 100);
        let id = r.compute_route_id();
        // Verify it produces a valid 32-byte hash
        assert_ne!(id, [0u8; 32]);
    }

    #[test]
    fn test_route_is_expired() {
        let mut r = make_route(1, 100);
        // valid_until_epoch = 0 means no expiry
        assert!(!r.is_expired(50));
        assert!(!r.is_expired(100));
        assert!(!r.is_expired(101));

        // Set valid_until_epoch
        r.valid_until_epoch = 200;
        assert!(!r.is_expired(50));
        assert!(!r.is_expired(200));
        assert!(r.is_expired(201));
    }

    #[test]
    fn test_compare_routes_score() {
        let a = make_route(1, 100);
        let b = make_route(2, 100);
        // Higher score wins (DESC): a(1000) sorts before b(500) → Less
        assert_eq!(compare_routes(&a, &b, 1000, 500), std::cmp::Ordering::Less);
        // Reverse: b(500) sorts after a(1000) → Greater
        assert_eq!(
            compare_routes(&a, &b, 500, 1000),
            std::cmp::Ordering::Greater
        );
    }

    #[test]
    fn test_compare_routes_epoch_tiebreak() {
        let a = make_route(1, 100);
        let b = make_route(2, 200);
        // Same score, lower epoch wins
        assert_eq!(compare_routes(&a, &b, 500, 500), std::cmp::Ordering::Less);
    }

    #[test]
    fn test_compare_routes_id_tiebreak() {
        let a = make_route(1, 100);
        let b = make_route(2, 100);
        // Same score, same epoch, lower id wins
        assert_eq!(compare_routes(&a, &b, 500, 500), std::cmp::Ordering::Less);
    }
}
