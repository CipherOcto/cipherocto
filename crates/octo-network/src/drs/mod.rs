//! Deterministic Route Selection (DRS) -- RFC-0856
//!
//! Selects optimal routes across deterministic overlay networks
//! using trust-weighted scoring and deterministic eviction.

pub mod cache;
pub mod domain;
pub mod error;
pub mod route;
pub mod scoring;
pub mod trust;

pub use cache::{CachedRoute, RouteCache};
pub use domain::{RouteDomain, RouteScopeFlag};
pub use error::DrsError;
pub use route::{compare_routes, DeterministicRoute, TransportVector};
pub use scoring::{compute_route_score, ScoringWeights};
pub use trust::{compute_trust_score, TrustScore};

/// DRS protocol version
pub const DRS_PROTOCOL_VERSION: u8 = 1;

/// Maximum routes per cache
pub const MAX_ROUTES_PER_CACHE: u32 = 10_000;

/// Sentinel value for `valid_until_epoch` meaning the route never expires.
/// RFC-0856: routes with valid_until_epoch == 0 are treated as persistent
/// until explicitly revoked or evicted.
pub const ROUTE_NO_EXPIRY: u64 = 0;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_drs_constants() {
        assert_eq!(DRS_PROTOCOL_VERSION, 1u8);
        assert_eq!(MAX_ROUTES_PER_CACHE, 10_000);
    }

    #[test]
    fn test_full_route_pipeline() {
        // Create a route
        let route = DeterministicRoute {
            route_id: [0xAA; 32],
            source_gateway: [0x01; 32],
            destination_gateway: [0x02; 32],
            next_hop: [0x03; 32],
            transport_vector_root: [0u8; 32],
            trust_score: 500,
            bandwidth_class: 100,
            latency_class: 50,
            censorship_resistance_class: 200,
            route_cost: 1000,
            route_epoch: 100,
            valid_until_epoch: 0,
            ttl_hops: 10,
            signature: [0u8; 64],
        };

        // Score it
        let weights = ScoringWeights::balanced();
        let score = compute_route_score(&route, &weights).unwrap();
        assert!(score > 0);

        // Cache it
        let mut cache = RouteCache::new(1000);
        cache.insert(route.clone(), score, 100).unwrap();
        assert_eq!(cache.len(), 1);

        // Retrieve and verify
        let cached = cache.get(&[0xAA; 32], 100).unwrap();
        assert_eq!(cached.score, score);
    }

    #[test]
    fn test_trust_score_integration() {
        let factors = TrustScore {
            historical_uptime: 100_000,
            proof_of_relay: 500,
            stake_weight: 500_000,
            mission_trust: 50_000,
            consensus_participation: 25_000,
        };
        let trust = compute_trust_score(&factors, 1000).unwrap();
        assert!(trust > 0);
        assert!(trust <= 1_000_000);
    }

    #[test]
    fn test_route_domain_and_scope() {
        let domain = RouteDomain::mission(1, [0xBB; 32]);
        assert_eq!(domain.scope_flags, RouteScopeFlag::Mission as u64);
        assert!(RouteScopeFlag::Mission.is_set(domain.scope_flags));
        assert!(!RouteScopeFlag::Global.is_set(domain.scope_flags));
    }
}
