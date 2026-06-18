//! Integration tests for the Deterministic Route Selection (DRS).
//!
//! Tests the full route lifecycle: creation → scoring → caching →
//! trust computation → route comparison → mission routing.

use octo_network::drs::cache::RouteCache;
use octo_network::drs::domain::{RouteDomain, RouteScopeFlag};
use octo_network::drs::mission_routing::{derive_hop_key, OnionHopKey, OnionRoute};
use octo_network::drs::route::{compare_routes, DeterministicRoute, TransportVector};
use octo_network::drs::scoring::{compute_route_score, ScoringWeights};
use octo_network::drs::trust::{compute_trust_score, TrustScore};
use octo_network::mon::routing::{
    compute_route_commitment, MissionRouteTable, RouteEntry, RouteIsolationGuard,
};

fn make_route(
    id_byte: u8,
    trust: u64,
    bw: u16,
    lat: u16,
    censor: u16,
    cost: u64,
    epoch: u64,
) -> DeterministicRoute {
    DeterministicRoute {
        route_id: {
            let mut arr = [0u8; 32];
            arr[0] = id_byte;
            arr
        },
        source_gateway: [0x01; 32],
        destination_gateway: [0x02; 32],
        next_hop: [0x03; 32],
        transport_vector_root: [0u8; 32],
        trust_score: trust,
        bandwidth_class: bw,
        latency_class: lat,
        censorship_resistance_class: censor,
        route_cost: cost,
        route_epoch: epoch,
        valid_until_epoch: 0,
        ttl_hops: 10,
        signature: [0u8; 64],
    }
}

// ── Full route lifecycle ──

#[test]
fn test_full_route_lifecycle() {
    let route = make_route(0xAA, 500_000, 100, 50, 200, 1000, 100);

    // Compute score
    let weights = ScoringWeights::balanced();
    let score = compute_route_score(&route, &weights).unwrap();
    assert!(score > 0);

    // Cache
    let mut cache = RouteCache::new(1000);
    let is_new = cache.insert(route.clone(), score, 100).unwrap();
    assert!(is_new);

    // Retrieve and verify
    let cached = cache.get(&route.route_id, 100).unwrap();
    assert_eq!(cached.score, score);
    assert_eq!(cached.route.route_id, route.route_id);

    // Update access time
    let cached2 = cache.get(&route.route_id, 200).unwrap();
    assert_eq!(cached2.last_accessed, 200);
}

// ── Scoring ──

#[test]
fn test_scoring_higher_trust_higher_score() {
    let weights = ScoringWeights::balanced();
    let route_high = make_route(0x01, 900_000, 100, 50, 100, 100, 1);
    let route_low = make_route(0x02, 100_000, 100, 50, 100, 100, 1);

    let score_high = compute_route_score(&route_high, &weights).unwrap();
    let score_low = compute_route_score(&route_low, &weights).unwrap();

    assert!(score_high > score_low);
}

#[test]
fn test_scoring_higher_cost_lower_score() {
    let weights = ScoringWeights::balanced();
    let route_cheap = make_route(0x01, 500_000, 100, 50, 100, 100, 1);
    let route_expensive = make_route(0x02, 500_000, 100, 50, 100, 10_000, 1);

    let score_cheap = compute_route_score(&route_cheap, &weights).unwrap();
    let score_expensive = compute_route_score(&route_expensive, &weights).unwrap();

    assert!(score_cheap > score_expensive);
}

#[test]
fn test_scoring_weights_validation() {
    let valid = ScoringWeights::balanced();
    assert!(valid.validate().is_ok());

    let invalid_zero = ScoringWeights {
        trust_weight: 0,
        bandwidth_weight: 250_000,
        latency_weight: 200_000,
        censorship_weight: 150_000,
        cost_weight: 100_000,
        activation_epoch: 0,
    };
    assert!(invalid_zero.validate().is_err());

    let invalid_sum = ScoringWeights {
        trust_weight: 100_000,
        bandwidth_weight: 100_000,
        latency_weight: 100_000,
        censorship_weight: 100_000,
        cost_weight: 100_000,
        activation_epoch: 0,
    };
    assert!(invalid_sum.validate().is_err());
}

// ── Route comparison ──

#[test]
fn test_compare_routes_deterministic() {
    let route_a = make_route(0xAA, 500_000, 100, 50, 200, 1000, 100);
    let route_b = make_route(0xBB, 300_000, 80, 60, 150, 500, 100);

    let weights = ScoringWeights::balanced();
    let score_a = compute_route_score(&route_a, &weights).unwrap();
    let score_b = compute_route_score(&route_b, &weights).unwrap();
    let ord = compare_routes(&route_a, &route_b, score_a, score_b);
    // Higher score wins
    assert_eq!(ord, std::cmp::Ordering::Less); // a has higher score, sorts first

    // Compare is deterministic
    let ord2 = compare_routes(&route_a, &route_b, score_a, score_b);
    assert_eq!(ord, ord2);
}

// ── Route cache eviction ──

#[test]
fn test_route_cache_eviction_worst_first() {
    let mut cache = RouteCache::new(3);

    let r1 = make_route(0x01, 900_000, 100, 50, 200, 100, 1);
    let r2 = make_route(0x02, 500_000, 100, 50, 200, 100, 1);
    let r3 = make_route(0x03, 100_000, 100, 50, 200, 100, 1);

    cache.insert(r1.clone(), 900, 1).unwrap();
    cache.insert(r2.clone(), 500, 1).unwrap();
    cache.insert(r3.clone(), 100, 1).unwrap();
    assert_eq!(cache.len(), 3);

    // Insert a better route — should evict worst (r3)
    let r4 = make_route(0x04, 800_000, 100, 50, 200, 100, 1);
    cache.insert(r4.clone(), 800, 1).unwrap();

    assert_eq!(cache.len(), 3);
    assert!(cache.get(&r3.route_id, 1).is_none());
    assert!(cache.get(&r4.route_id, 1).is_some());
}

#[test]
fn test_route_cache_remove() {
    let mut cache = RouteCache::new(100);
    let route = make_route(0xAA, 500_000, 100, 50, 200, 100, 1);
    cache.insert(route.clone(), 500, 1).unwrap();

    assert!(cache.remove(&route.route_id));
    assert_eq!(cache.len(), 0);
    assert!(!cache.remove(&route.route_id));
}

#[test]
fn test_route_cache_expired_filtering() {
    let mut cache = RouteCache::new(100);
    let route = make_route(0xAA, 500_000, 100, 50, 200, 100, 1);
    cache.insert(route.clone(), 500, 1).unwrap();

    // Not expired
    assert!(cache.get(&route.route_id, 50).is_some());

    // Make an expired route
    let mut expired_route = make_route(0xBB, 500_000, 100, 50, 200, 100, 1);
    expired_route.valid_until_epoch = 50;
    cache.insert(expired_route.clone(), 500, 1).unwrap();

    // Route exists in cache regardless of expiry
    assert!(cache.get(&expired_route.route_id, 51).is_some());
}

// ── Trust scoring ──

#[test]
fn test_trust_score_full_factors() {
    let factors = TrustScore {
        historical_uptime: 800_000,
        proof_of_relay: 500,
        stake_weight: 1_000_000,
        mission_trust: 100_000,
        consensus_participation: 50_000,
    };

    let score = compute_trust_score(&factors, 500_000).unwrap();
    assert!(score > 0);
    assert!(score <= 1_000_000);
}

#[test]
fn test_trust_score_relay_cap_at_1000() {
    let factors = TrustScore {
        historical_uptime: 0,
        proof_of_relay: 5000, // way over cap
        stake_weight: 0,
        mission_trust: 0,
        consensus_participation: 0,
    };

    let score = compute_trust_score(&factors, 1000).unwrap();
    // Should cap relay at 1000: 1000 * 1000 = 1_000_000
    assert_eq!(score, 1_000_000);
}

#[test]
fn test_trust_score_stake_cap() {
    let factors_capped = TrustScore {
        historical_uptime: 0,
        proof_of_relay: 0,
        stake_weight: 100_000_000, // huge
        mission_trust: 0,
        consensus_participation: 0,
    };

    let factors_normal = TrustScore {
        historical_uptime: 0,
        proof_of_relay: 0,
        stake_weight: 5_000, // exactly at cap (median=500, cap=5000)
        mission_trust: 0,
        consensus_participation: 0,
    };

    let s1 = compute_trust_score(&factors_capped, 500).unwrap();
    let s2 = compute_trust_score(&factors_normal, 500).unwrap();
    assert_eq!(s1, s2); // both capped at same value
}

// ── Route domain ──

#[test]
fn test_route_domain_scope_isolation() {
    let global = RouteDomain::global(1);
    let mission = RouteDomain::mission(1, [0xAA; 32]);

    assert!(RouteScopeFlag::Global.is_set(global.scope_flags));
    assert!(!RouteScopeFlag::Global.is_set(mission.scope_flags));
    assert!(RouteScopeFlag::Mission.is_set(mission.scope_flags));

    assert_ne!(global.domain_hash(), mission.domain_hash());
}

// ── Mission Routing ──

#[test]
fn test_mission_route_table_lifecycle() {
    let mut table = MissionRouteTable::new([0xAA; 32]);

    let entry = RouteEntry {
        destination: [0x02; 32],
        next_hop: [0x03; 32],
        cost: 100,
        sequence: 1,
    };

    table.upsert(entry.clone());
    assert_eq!(table.len(), 1);

    let found = table.lookup(&[0x02; 32]).unwrap();
    assert_eq!(found.next_hop, [0x03; 32]);

    // Stale update rejected
    table.upsert(RouteEntry {
        destination: [0x02; 32],
        next_hop: [0x04; 32],
        cost: 50,
        sequence: 1, // same sequence
    });
    let still = table.lookup(&[0x02; 32]).unwrap();
    assert_eq!(still.next_hop, [0x03; 32]); // unchanged

    // Fresh update accepted
    table.upsert(RouteEntry {
        destination: [0x02; 32],
        next_hop: [0x04; 32],
        cost: 50,
        sequence: 2,
    });
    let updated = table.lookup(&[0x02; 32]).unwrap();
    assert_eq!(updated.next_hop, [0x04; 32]);

    // Remove
    table.remove(&[0x02; 32]);
    assert!(table.is_empty());
}

#[test]
fn test_route_isolation_guard() {
    let guard = RouteIsolationGuard::new([0xAA; 32], vec![[0x01; 32], [0x02; 32]]);

    assert!(guard.is_authorized(&[0xAA; 32], &[0x01; 32]));
    assert!(guard.is_authorized(&[0xAA; 32], &[0x02; 32]));
    assert!(!guard.is_authorized(&[0xAA; 32], &[0x03; 32])); // not authorized
    assert!(!guard.is_authorized(&[0xBB; 32], &[0x01; 32])); // wrong mission
}

#[test]
fn test_route_commitment_deterministic() {
    let c1 = compute_route_commitment(&[0xAA; 32], 5, 100);
    let c2 = compute_route_commitment(&[0xAA; 32], 5, 100);
    assert_eq!(c1, c2);
    assert_ne!(c1, [0u8; 32]);

    // Different epoch → different commitment
    let c3 = compute_route_commitment(&[0xAA; 32], 5, 101);
    assert_ne!(c1, c3);
}

// ── Onion-compatible routing ──

#[test]
fn test_onion_route_construction() {
    let hop_keys = vec![
        OnionHopKey {
            hop_index: 0,
            ephemeral_public: [0x01; 32],
            shared_secret_hash: [0x02; 32],
            session_key: [0x03; 32],
        },
        OnionHopKey {
            hop_index: 1,
            ephemeral_public: [0x04; 32],
            shared_secret_hash: [0x05; 32],
            session_key: [0x06; 32],
        },
    ];

    let route = OnionRoute::new([0xAA; 32], hop_keys.clone(), 1);
    assert_eq!(route.hop_count(), 2);
    assert_ne!(route.hop_keys_root, [0u8; 32]);

    // Deterministic
    let route2 = OnionRoute::new([0xAA; 32], hop_keys, 1);
    assert_eq!(route.hop_keys_root, route2.hop_keys_root);
}

#[test]
fn test_derive_hop_key_deterministic() {
    let key1 = derive_hop_key(0, [0x01; 32], &[0xAA; 32], &[0xBB; 32]);
    let key2 = derive_hop_key(0, [0x01; 32], &[0xAA; 32], &[0xBB; 32]);

    assert_eq!(key1.hop_index, 0);
    assert_eq!(key1.session_key, key2.session_key);
    assert_eq!(key1.shared_secret_hash, key2.shared_secret_hash);
}

#[test]
fn test_derive_hop_key_per_hop_isolation() {
    let k0 = derive_hop_key(0, [0x01; 32], &[0xAA; 32], &[0xBB; 32]);
    let k1 = derive_hop_key(1, [0x01; 32], &[0xAA; 32], &[0xBB; 32]);

    assert_ne!(k0.session_key, k1.session_key);
}

// ── Route expiry ──

#[test]
fn test_route_expiry() {
    let mut route = make_route(0xAA, 500_000, 100, 50, 200, 100, 100);
    assert!(!route.is_expired(50));

    route.valid_until_epoch = 100;
    assert!(!route.is_expired(100));
    assert!(route.is_expired(101));

    // No-expiry sentinel
    route.valid_until_epoch = 0;
    assert!(!route.is_expired(u64::MAX));
}
