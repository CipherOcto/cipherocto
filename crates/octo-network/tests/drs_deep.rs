//! Deep coverage tests for DRS — route computation, signing, mission routing,
//! relay constraints, partition resilience, stealth config, trust edge cases, cache eviction.

use octo_network::drs::cache::RouteCache;
use octo_network::drs::domain::{RouteDomain, RouteScopeFlag};
use octo_network::drs::mission_routing::{
    relay_satisfies_constraints, BandwidthClass, GeoRegion, MissionRouteConstraints,
    PartitionMetrics, PartitionState, StealthConfig,
};
use octo_network::drs::route::{compare_routes, DeterministicRoute};
use octo_network::drs::scoring::{compute_route_score, ScoringWeights};
use octo_network::drs::trust::{compute_trust_score, TrustScore};

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

// ── Route computation ──

#[test]
fn test_compute_route_id_deterministic() {
    let route = make_route(0xAA, 500, 100, 50, 200, 100, 100);
    let id1 = route.compute_route_id();
    let id2 = route.compute_route_id();
    assert_eq!(id1, id2);
    assert_ne!(id1, [0u8; 32]);
}

#[test]
fn test_compute_route_id_different_for_different_routes() {
    let r1 = make_route(0xAA, 500, 100, 50, 200, 100, 100);
    let mut r2 = r1.clone();
    r2.route_epoch = 200;
    assert_ne!(r1.compute_route_id(), r2.compute_route_id());
}

#[test]
fn test_to_signing_bytes_deterministic() {
    let route = make_route(0xAA, 500, 100, 50, 200, 100, 100);
    let b1 = route.to_signing_bytes();
    let b2 = route.to_signing_bytes();
    assert_eq!(b1, b2);
    assert!(!b1.is_empty());
}

#[test]
fn test_to_signing_bytes_excludes_signature() {
    let mut route = make_route(0xAA, 500, 100, 50, 200, 100, 100);
    let b1 = route.to_signing_bytes();
    route.signature = [0xFF; 64];
    let b2 = route.to_signing_bytes();
    assert_eq!(b1, b2);
}

#[test]
fn test_verify_signature_invalid_key() {
    let route = make_route(0xAA, 500, 100, 50, 200, 100, 100);
    // Zero signature with random key should fail
    assert!(!route.verify_signature(&[0x42; 32]));
}

// ── Compare routes ──

#[test]
fn test_compare_routes_higher_score_wins() {
    let r1 = make_route(0x01, 900, 100, 50, 200, 100, 100);
    let r2 = make_route(0x02, 500, 100, 50, 200, 100, 100);
    let weights = ScoringWeights::balanced();
    let s1 = compute_route_score(&r1, &weights).unwrap();
    let s2 = compute_route_score(&r2, &weights).unwrap();

    let ord = compare_routes(&r1, &r2, s1, s2);
    // r1 has higher score → sorts first (Less)
    assert_eq!(ord, std::cmp::Ordering::Less);
}

#[test]
fn test_compare_routes_same_score_earlier_epoch_wins() {
    let r1 = make_route(0x01, 500, 100, 50, 200, 100, 50);
    let r2 = make_route(0x02, 500, 100, 50, 200, 100, 100);
    let ord = compare_routes(&r1, &r2, 1000, 1000);
    // Same score, earlier epoch wins
    assert_eq!(ord, std::cmp::Ordering::Less);
}

// ── Route cache eviction ──

#[test]
fn test_cache_evict_expired() {
    let mut cache = RouteCache::new(100);

    let r1 = make_route(0x01, 500, 100, 50, 200, 100, 1);
    let r2 = make_route(0x02, 500, 100, 50, 200, 100, 1);

    cache.insert(r1, 500, 1).unwrap();
    cache.insert(r2, 500, 1).unwrap();
    assert_eq!(cache.len(), 2);

    let removed = cache.evict_expired(100, 50); // max_age=50, current=100, cached_at=1 → 99 > 50
    assert_eq!(removed, 2);
    assert!(cache.is_empty());
}

#[test]
fn test_cache_update_existing() {
    let mut cache = RouteCache::new(100);

    let r1 = make_route(0x01, 500, 100, 50, 200, 100, 1);
    let is_new = cache.insert(r1.clone(), 500, 1).unwrap();
    assert!(is_new);

    let is_new = cache.insert(r1, 600, 2).unwrap(); // update
    assert!(!is_new);
    assert_eq!(cache.len(), 1);
}

// ── Mission routing constraints ──

#[test]
fn test_relay_satisfies_constraints_basic() {
    let constraints = MissionRouteConstraints {
        allowed_regions: vec![GeoRegion::Global],
        min_trust_score: 100,
        min_bandwidth: BandwidthClass::Low,
        stealth_mode: false,
        max_hops: 5,
        mission_id: [0u8; 32],
    };

    assert!(relay_satisfies_constraints(
        500,
        GeoRegion::NorthAmerica,
        BandwidthClass::Medium,
        500,
        &constraints,
        None,
    ));
}

#[test]
fn test_relay_satisfies_constraints_trust_gate() {
    let constraints = MissionRouteConstraints {
        allowed_regions: vec![GeoRegion::Global],
        min_trust_score: 1000,
        min_bandwidth: BandwidthClass::Low,
        stealth_mode: false,
        max_hops: 5,
        mission_id: [0u8; 32],
    };

    assert!(!relay_satisfies_constraints(
        500,
        GeoRegion::NorthAmerica,
        BandwidthClass::Medium,
        500,
        &constraints,
        None,
    ));
}

#[test]
fn test_relay_satisfies_constraints_geo_isolation() {
    let constraints = MissionRouteConstraints {
        allowed_regions: vec![GeoRegion::Europe],
        min_trust_score: 0,
        min_bandwidth: BandwidthClass::Low,
        stealth_mode: false,
        max_hops: 5,
        mission_id: [0u8; 32],
    };

    assert!(relay_satisfies_constraints(
        500,
        GeoRegion::Europe,
        BandwidthClass::Medium,
        500,
        &constraints,
        None,
    ));
    assert!(!relay_satisfies_constraints(
        500,
        GeoRegion::NorthAmerica,
        BandwidthClass::Medium,
        500,
        &constraints,
        None,
    ));
}

#[test]
fn test_relay_satisfies_constraints_bandwidth_gate() {
    let constraints = MissionRouteConstraints {
        allowed_regions: vec![GeoRegion::Global],
        min_trust_score: 0,
        min_bandwidth: BandwidthClass::High,
        stealth_mode: false,
        max_hops: 5,
        mission_id: [0u8; 32],
    };

    assert!(!relay_satisfies_constraints(
        500,
        GeoRegion::NorthAmerica,
        BandwidthClass::Low,
        500,
        &constraints,
        None,
    ));
}

#[test]
fn test_relay_satisfies_constraints_stealth_mode() {
    let constraints = MissionRouteConstraints {
        allowed_regions: vec![GeoRegion::Global],
        min_trust_score: 0,
        min_bandwidth: BandwidthClass::Low,
        stealth_mode: true,
        max_hops: 5,
        mission_id: [0u8; 32],
    };

    let stealth = StealthConfig {
        min_censorship_resistance: 800,
        randomize_hops: false,
        blocked_asn_prefixes: vec![],
        cover_traffic_ratio: 0,
    };

    // Relay with low censorship resistance fails
    assert!(!relay_satisfies_constraints(
        500,
        GeoRegion::NorthAmerica,
        BandwidthClass::Medium,
        300,
        &constraints,
        Some(&stealth),
    ));

    // Relay with high censorship resistance passes
    assert!(relay_satisfies_constraints(
        500,
        GeoRegion::NorthAmerica,
        BandwidthClass::Medium,
        900,
        &constraints,
        Some(&stealth),
    ));

    // No stealth config provided → fails
    assert!(!relay_satisfies_constraints(
        500,
        GeoRegion::NorthAmerica,
        BandwidthClass::Medium,
        900,
        &constraints,
        None,
    ));
}

// ── MissionRouteConstraints default ──

#[test]
fn test_mission_route_constraints_default() {
    let c = MissionRouteConstraints::default();
    assert_eq!(c.allowed_regions, vec![GeoRegion::Global]);
    assert_eq!(c.min_trust_score, 0);
    assert_eq!(c.max_hops, 5);
    assert!(!c.stealth_mode);
}

// ── StealthConfig default ──

#[test]
fn test_stealth_config_default() {
    let s = StealthConfig::default();
    assert_eq!(s.min_censorship_resistance, 500);
    assert!(!s.randomize_hops);
    assert!(s.blocked_asn_prefixes.is_empty());
}

// ── Partition resilience ──

#[test]
fn test_partition_state_computation() {
    assert_eq!(
        PartitionMetrics::compute_state(10, 10),
        PartitionState::Healthy
    );
    assert_eq!(
        PartitionMetrics::compute_state(10, 9),
        PartitionState::Healthy
    ); // 10% down
    assert_eq!(
        PartitionMetrics::compute_state(10, 8),
        PartitionState::Degraded
    ); // 20% down
    assert_eq!(
        PartitionMetrics::compute_state(10, 7),
        PartitionState::Degraded
    ); // 30% down
    assert_eq!(
        PartitionMetrics::compute_state(10, 5),
        PartitionState::Partitioned
    ); // 50% down
    assert_eq!(
        PartitionMetrics::compute_state(10, 0),
        PartitionState::Partitioned
    );
    assert_eq!(
        PartitionMetrics::compute_state(0, 0),
        PartitionState::Healthy
    ); // edge: zero total
}

// ── GeoRegion enum ──

#[test]
fn test_geo_region_variants() {
    assert_eq!(GeoRegion::NorthAmerica as u16, 0x0001);
    assert_eq!(GeoRegion::Europe as u16, 0x0002);
    assert_eq!(GeoRegion::Asia as u16, 0x0003);
    assert_eq!(GeoRegion::SouthAmerica as u16, 0x0004);
    assert_eq!(GeoRegion::Africa as u16, 0x0005);
    assert_eq!(GeoRegion::Oceania as u16, 0x0006);
    assert_eq!(GeoRegion::MiddleEast as u16, 0x0007);
    assert_eq!(GeoRegion::Global as u16, 0x0008);
}

// ── BandwidthClass ──

#[test]
fn test_bandwidth_class_ordering() {
    assert!(BandwidthClass::Low < BandwidthClass::Medium);
    assert!(BandwidthClass::Medium < BandwidthClass::High);
}

// ── Trust score edge cases ──

#[test]
fn test_trust_score_zero_median_stake() {
    let factors = TrustScore {
        historical_uptime: 100_000,
        proof_of_relay: 500,
        stake_weight: 1_000_000,
        mission_trust: 50_000,
        consensus_participation: 25_000,
    };
    // With median_stake=0, stake is uncapped: 1_000_000 / 1000 = 1000
    let score = compute_trust_score(&factors, 0).unwrap();
    // uptime=100000, relay=500*1000=500000, stake=1000, mission=50000, consensus=25000
    // total = 676000
    assert_eq!(score, 676_000);
}

#[test]
fn test_trust_score_factor_out_of_range() {
    let factors = TrustScore {
        historical_uptime: 2_000_000, // over 1M
        proof_of_relay: 0,
        stake_weight: 0,
        mission_trust: 0,
        consensus_participation: 0,
    };
    assert!(compute_trust_score(&factors, 1000).is_err());
}

#[test]
fn test_trust_score_mission_trust_out_of_range() {
    let factors = TrustScore {
        historical_uptime: 0,
        proof_of_relay: 0,
        stake_weight: 0,
        mission_trust: 2_000_000,
        consensus_participation: 0,
    };
    assert!(compute_trust_score(&factors, 1000).is_err());
}

// ── RouteScopeFlag ──

#[test]
fn test_route_scope_flag_to_u64() {
    assert_eq!(RouteScopeFlag::Global.to_u64(), 0x0001);
    assert_eq!(RouteScopeFlag::Consensus.to_u64(), 0x0020);
}

// ── RouteDomain ──

#[test]
fn test_route_domain_hash_different_networks() {
    let d1 = RouteDomain::global(1);
    let d2 = RouteDomain::global(2);
    assert_ne!(d1.domain_hash(), d2.domain_hash());
}
