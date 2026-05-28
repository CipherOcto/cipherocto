//! Deterministic Route (RFC-0856 §4)

use serde::{Deserialize, Serialize};

/// Deterministic route — in-memory representation (RFC-0856 §4).
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
    /// Maximum hop count
    pub ttl_hops: u16,
}

/// Transport vector — describes one transport path (RFC-0856 §5.2).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[repr(C)]
pub struct TransportVector {
    /// Transport type identifier
    pub transport_type: u16,
    /// Domain identifier hash
    pub domain_id: [u8; 32],
    /// Priority (lower = preferred)
    pub priority: u8,
    /// Bandwidth classification
    pub bandwidth_class: u8,
    /// Censorship resistance score
    pub censorship_score: u8,
}

impl DeterministicRoute {
    /// Compute the route commitment hash.
    /// commitment = BLAKE3-256(route_id || gateway_sequence_hash || weights_hash || epoch)
    pub fn compute_commitment(&self, weights_hash: &[u8; 32]) -> [u8; 32] {
        let mut hasher = blake3::Hasher::new();
        hasher.update(&self.route_id);
        hasher.update(&self.source_gateway);
        hasher.update(&self.destination_gateway);
        hasher.update(&self.next_hop);
        hasher.update(weights_hash);
        hasher.update(&self.route_epoch.to_be_bytes());
        *hasher.finalize().as_bytes()
    }

    /// Check if route is expired.
    pub fn is_expired(&self, current_epoch: u64) -> bool {
        self.route_epoch < current_epoch
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
    score_a
        .cmp(&score_b)
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
            ttl_hops: 10,
        }
    }

    #[test]
    fn test_route_commitment_deterministic() {
        let r = make_route(1, 100);
        let weights_hash = [0xAA; 32];
        let c1 = r.compute_commitment(&weights_hash);
        let c2 = r.compute_commitment(&weights_hash);
        assert_eq!(c1, c2);
    }

    #[test]
    fn test_route_commitment_different_weights() {
        let r = make_route(1, 100);
        let c1 = r.compute_commitment(&[0xAA; 32]);
        let c2 = r.compute_commitment(&[0xBB; 32]);
        assert_ne!(c1, c2);
    }

    #[test]
    fn test_route_is_expired() {
        let r = make_route(1, 100);
        assert!(!r.is_expired(50));
        assert!(!r.is_expired(100));
        assert!(r.is_expired(101));
    }

    #[test]
    fn test_compare_routes_score() {
        let a = make_route(1, 100);
        let b = make_route(2, 100);
        // Higher score wins
        assert_eq!(
            compare_routes(&a, &b, 1000, 500),
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
