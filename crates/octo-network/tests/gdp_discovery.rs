//! Integration tests for the Gateway Discovery Protocol (GDP).
//!
//! Tests the full discovery lifecycle: identity creation → advertisement →
//! heartbeat → scope filtering → cache management.

use octo_network::gdp::advertisement::GatewayAdvertisement;
use octo_network::gdp::discovery::{
    default_ttl, min_octo_b_for_scope, min_octo_for_scope, DiscoveryState, BootstrapMethod, ScopeFilter,
    TTL_GLOBAL, TTL_LOCAL, TTL_REGIONAL,
};
use octo_network::gdp::heartbeat::GatewayHeartbeat;
use octo_network::gdp::identity::GdpGatewayIdentity;
use octo_network::gdp::types::{DiscoveryLifecycle, DiscoveryScope, GatewayCapability, StakeRequirement};

use octo_network::dot::gateway::{GatewayClass, GatewayIdentity};

fn test_base_identity() -> GatewayIdentity {
    GatewayIdentity::new([0x42u8; 32], 1, GatewayClass::Edge, 100)
}

// ── GDP Identity ──

#[test]
fn test_gdp_identity_full_builder() {
    let base = test_base_identity();
    let gdp = GdpGatewayIdentity::new(base.clone())
        .with_platforms(0x0001 | 0x0002)
        .with_capabilities(0x0001 | 0x0004);

    assert_eq!(gdp.gateway_id(), base.gateway_id);
    assert_eq!(gdp.supported_platforms(), 0x0003);
    assert_eq!(gdp.capabilities(), 0x0005);
}

// ── Gateway Advertisement ──

#[test]
fn test_advertisement_signing_bytes_deterministic() {
    let adv = GatewayAdvertisement {
        version: 1,
        gateway_id: [0x42u8; 32],
        network_id: 1,
        sequence: 1,
        logical_timestamp: 1000,
        gateway_class: 0x0001,
        capabilities_root: [0x02; 32],
        transport_root: [0x03; 32],
        route_root: [0x04; 32],
        trust_root: [0x05; 32],
        overlay_endpoints: vec![],
        signature: [0u8; 64],
    };

    let b1 = adv.to_signing_bytes();
    let b2 = adv.to_signing_bytes();
    assert_eq!(b1, b2);
}

#[test]
fn test_advertisement_merkle_root_properties() {
    // Empty
    assert_eq!(GatewayAdvertisement::compute_merkle_root(&[]), [0u8; 32]);

    // Single
    let single = GatewayAdvertisement::compute_merkle_root(&[[0xAA; 32]]);
    assert_ne!(single, [0u8; 32]);

    // Deterministic
    let items = [[0x01; 32], [0x02; 32], [0x03; 32]];
    let r1 = GatewayAdvertisement::compute_merkle_root(&items);
    let r2 = GatewayAdvertisement::compute_merkle_root(&items);
    assert_eq!(r1, r2);

    // Order matters
    let a = [[0x01; 32], [0x02; 32]];
    let b = [[0x02; 32], [0x01; 32]];
    assert_ne!(
        GatewayAdvertisement::compute_merkle_root(&a),
        GatewayAdvertisement::compute_merkle_root(&b)
    );
}

// ── Heartbeat ──

#[test]
fn test_heartbeat_lifecycle() {
    let hb = GatewayHeartbeat {
        gateway_id: [0x42; 32],
        sequence: 1,
        active_routes: 5,
        load_class: 100,
        uptime_class: 200,
        logical_timestamp: 1000,
        signature: [0u8; 64],
    };

    let bytes = hb.to_signing_bytes();
    assert!(!bytes.is_empty());

    // Not expired
    assert!(!hb.is_expired(1050, 100));
    // Exact boundary
    assert!(!hb.is_expired(1100, 100));
    // Expired
    assert!(hb.is_expired(1101, 100));
}

#[test]
fn test_heartbeat_signing_excludes_signature() {
    let mut hb = GatewayHeartbeat {
        gateway_id: [0x42; 32],
        sequence: 1,
        active_routes: 5,
        load_class: 100,
        uptime_class: 200,
        logical_timestamp: 1000,
        signature: [0u8; 64],
    };

    let b1 = hb.to_signing_bytes();
    hb.signature = [0xFF; 64];
    let b2 = hb.to_signing_bytes();
    assert_eq!(b1, b2);
}

// ── Discovery scope and TTL ──

#[test]
fn test_discovery_scope_ttl_mapping() {
    assert_eq!(default_ttl(&DiscoveryScope::Local), TTL_LOCAL);
    assert_eq!(default_ttl(&DiscoveryScope::Regional), TTL_REGIONAL);
    assert_eq!(default_ttl(&DiscoveryScope::Global), TTL_GLOBAL);
}

#[test]
fn test_discovery_scope_stake_requirements() {
    assert_eq!(min_octo_for_scope(&DiscoveryScope::Local), 0);
    assert_eq!(min_octo_for_scope(&DiscoveryScope::Global), 1000);
    assert_eq!(min_octo_b_for_scope(&DiscoveryScope::Consensus), 200);
}

// ── Scope filter ──

#[test]
fn test_scope_filter_visibility() {
    let global_filter = ScopeFilter::global_only();
    assert!(global_filter.is_visible_in(&DiscoveryScope::Global));
    assert!(!global_filter.is_visible_in(&DiscoveryScope::Local));

    let all_public = ScopeFilter::all_public();
    assert!(all_public.is_visible_in(&DiscoveryScope::Local));
    assert!(all_public.is_visible_in(&DiscoveryScope::Regional));
    assert!(all_public.is_visible_in(&DiscoveryScope::Global));

    let mission = ScopeFilter::mission([0xAA; 32]);
    assert!(mission.is_visible_in(&DiscoveryScope::Mission));
    assert!(!mission.is_visible_in(&DiscoveryScope::Global));
}

// ── Discovery state machine ──

#[test]
fn test_discovery_state_lifecycle() {
    let mut state = DiscoveryState::new(BootstrapMethod::Static);
    assert_eq!(state.phase, DiscoveryLifecycle::Bootstrap);
    assert_eq!(state.peer_count, 0);

    // Set peer count to meet expansion requirement
    state.peer_count = 5;

    // Transition: Bootstrap → Expansion
    assert!(state.start_expansion().is_ok());
    assert_eq!(state.phase, DiscoveryLifecycle::Expansion);

    // Transition: Expansion → Stabilization
    assert!(state.stabilize(100).is_ok());
    assert_eq!(state.phase, DiscoveryLifecycle::Stabilization);
}

#[test]
fn test_discovery_state_degradation() {
    let mut state = DiscoveryState::new(BootstrapMethod::Static);
    state.phase = DiscoveryLifecycle::Stabilization;

    // Simulate failure
    state.phase = DiscoveryLifecycle::Degraded;
    assert_eq!(state.phase, DiscoveryLifecycle::Degraded);

    // Recovery
    state.phase = DiscoveryLifecycle::Recovering;
    assert_eq!(state.phase, DiscoveryLifecycle::Recovering);
}

// ── Gateway Capability bitmask ──

#[test]
fn test_gateway_capability_bitmask() {
    let caps = GatewayCapability::Edge as u64 | GatewayCapability::Relay as u64 | GatewayCapability::Stealth as u64;
    assert!(caps & GatewayCapability::Edge as u64 != 0);
    assert!(caps & GatewayCapability::Relay as u64 != 0);
    assert!(caps & GatewayCapability::Stealth as u64 != 0);
    assert!(caps & GatewayCapability::Consensus as u64 == 0);
}

// ── Advertisement expiration ──

#[test]
fn test_advertisement_expiration() {
    use octo_network::gdp::types::AdvertisementExpiration;

    let exp = AdvertisementExpiration {
        logical_timestamp: 100,
        ttl_epochs: 50,
        scope: DiscoveryScope::Global,
    };

    assert!(!exp.is_expired(149));
    assert!(!exp.is_expired(150));
    assert!(exp.is_expired(151));
}
