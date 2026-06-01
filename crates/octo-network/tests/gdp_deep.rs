//! Deep coverage tests for GDP — discovery gossip, overlay endpoint,
//! discovery state machine edge cases, bootstrap methods.

use octo_network::gdp::discovery::{
    default_ttl, min_octo_b_for_scope, min_octo_for_scope, BootstrapMethod, DiscoveryState, ScopeFilter,
};
use octo_network::gdp::discovery_gossip::{
    is_discovery_advertisement, lifecycle_to_gossip_mode, mode_to_flag, scope_to_gossip_domain,
    scope_to_gossip_scope, scope_ttl, wrap_advertisement, DiscoveryGossipMode,
};
use octo_network::gdp::overlay_endpoint::OverlayEndpoint;
use octo_network::gdp::types::{DiscoveryLifecycle, DiscoveryScope, GatewayCapability};

// ── Discovery Gossip: scope mapping ──

#[test]
fn test_scope_to_gossip_scope_all() {
    assert_eq!(scope_to_gossip_scope(DiscoveryScope::Local), octo_network::dgp::domain::GossipScope::LOCAL);
    assert_eq!(scope_to_gossip_scope(DiscoveryScope::Regional), octo_network::dgp::domain::GossipScope::REGIONAL);
    assert_eq!(scope_to_gossip_scope(DiscoveryScope::Mission), octo_network::dgp::domain::GossipScope::MISSION);
    assert_eq!(scope_to_gossip_scope(DiscoveryScope::Global), octo_network::dgp::domain::GossipScope::GLOBAL);
    assert_eq!(scope_to_gossip_scope(DiscoveryScope::Private), octo_network::dgp::domain::GossipScope::PRIVATE);
    assert_eq!(scope_to_gossip_scope(DiscoveryScope::Consensus), octo_network::dgp::domain::GossipScope::CONSENSUS);
}

#[test]
fn test_scope_to_gossip_domain_mission_preserves_id() {
    let mission_id = [0xAA; 32];
    let domain = scope_to_gossip_domain(DiscoveryScope::Mission, 1, mission_id);
    assert_eq!(domain.mission_id, mission_id);
}

#[test]
fn test_scope_to_gossip_domain_non_mission_zeroes_mission() {
    let domain = scope_to_gossip_domain(DiscoveryScope::Global, 1, [0xAA; 32]);
    assert_eq!(domain.mission_id, [0u8; 32]);
}

#[test]
fn test_scope_ttl_all() {
    assert_eq!(scope_ttl(DiscoveryScope::Local), 3);
    assert_eq!(scope_ttl(DiscoveryScope::Regional), 10);
    assert_eq!(scope_ttl(DiscoveryScope::Mission), 5);
    assert_eq!(scope_ttl(DiscoveryScope::Global), 20);
    assert_eq!(scope_ttl(DiscoveryScope::Private), 3);
    assert_eq!(scope_ttl(DiscoveryScope::Consensus), 10);
}

#[test]
fn test_lifecycle_to_gossip_mode_all() {
    assert_eq!(lifecycle_to_gossip_mode(DiscoveryLifecycle::Bootstrap), DiscoveryGossipMode::Flood);
    assert_eq!(lifecycle_to_gossip_mode(DiscoveryLifecycle::Expansion), DiscoveryGossipMode::Incremental);
    assert_eq!(lifecycle_to_gossip_mode(DiscoveryLifecycle::Stabilization), DiscoveryGossipMode::Incremental);
    assert_eq!(lifecycle_to_gossip_mode(DiscoveryLifecycle::Degraded), DiscoveryGossipMode::AntiEntropy);
    assert_eq!(lifecycle_to_gossip_mode(DiscoveryLifecycle::Recovering), DiscoveryGossipMode::Flood);
}

#[test]
fn test_mode_to_flag_all() {
    assert_eq!(mode_to_flag(DiscoveryGossipMode::Flood), octo_network::dgp::object::FLAG_FLOOD);
    assert_eq!(mode_to_flag(DiscoveryGossipMode::Incremental), octo_network::dgp::object::FLAG_INCREMENTAL);
    assert_eq!(mode_to_flag(DiscoveryGossipMode::AntiEntropy), octo_network::dgp::object::FLAG_ANTI_ENTROPY);
    assert_eq!(mode_to_flag(DiscoveryGossipMode::Directed), octo_network::dgp::object::FLAG_DIRECTED);
}

// ── Discovery Gossip: wrap advertisement ──

#[test]
fn test_wrap_advertisement_global() {
    let obj = wrap_advertisement(
        [0x42; 32],
        DiscoveryScope::Global,
        1,
        [0u8; 32],
        1000,
        [0xBB; 32],
        DiscoveryGossipMode::Flood,
    );

    assert_eq!(obj.object_type, octo_network::dgp::object::GossipObjectType::DiscoveryAdvertisement as u16);
    assert_eq!(obj.origin_gateway, [0x42; 32]);
    assert_eq!(obj.logical_timestamp, 1000);
    assert!(obj.propagation_flags & octo_network::dgp::object::FLAG_FLOOD != 0);
    assert!(is_discovery_advertisement(&obj));
}

#[test]
fn test_wrap_advertisement_mission_scope() {
    let obj = wrap_advertisement(
        [0x42; 32],
        DiscoveryScope::Mission,
        1,
        [0xAA; 32],
        1000,
        [0xBB; 32],
        DiscoveryGossipMode::Directed,
    );
    assert_eq!(obj.domain_id.mission_id, [0xAA; 32]);
}

// ── is_discovery_advertisement ──

#[test]
fn test_is_discovery_advertisement_false_for_other() {
    let mut obj = wrap_advertisement(
        [0x42; 32], DiscoveryScope::Global, 1, [0u8; 32], 1000, [0xBB; 32], DiscoveryGossipMode::Flood,
    );
    assert!(is_discovery_advertisement(&obj));

    obj.object_type = octo_network::dgp::object::GossipObjectType::Envelope as u16;
    assert!(!is_discovery_advertisement(&obj));
}

// ── Overlay Endpoint ──

#[test]
fn test_overlay_endpoint_new() {
    let ep = OverlayEndpoint::new(0x0001, [0xAA; 32]);
    assert_eq!(ep.transport_type, 0x0001);
    assert_eq!(ep.endpoint_hash, [0xAA; 32]);
    assert_eq!(ep.priority, 100);
    assert_eq!(ep.bandwidth_class, 0);
    assert_eq!(ep.flags, 0);
}

// ── Discovery state machine edge cases ──

#[test]
fn test_discovery_start_expansion_wrong_phase() {
    let mut state = DiscoveryState::new(BootstrapMethod::Static);
    state.phase = DiscoveryLifecycle::Expansion;
    state.peer_count = 10;
    assert!(state.start_expansion().is_err());
}

#[test]
fn test_discovery_start_expansion_too_few_peers() {
    let mut state = DiscoveryState::new(BootstrapMethod::Static);
    state.peer_count = 3;
    assert!(state.start_expansion().is_err());
}

#[test]
fn test_discovery_stabilize_wrong_phase() {
    let mut state = DiscoveryState::new(BootstrapMethod::Static);
    assert!(state.stabilize(100).is_err());
}

#[test]
fn test_discovery_full_lifecycle() {
    let mut state = DiscoveryState::new(BootstrapMethod::QrBlob);
    assert_eq!(state.phase, DiscoveryLifecycle::Bootstrap);
    assert_eq!(state.bootstrap_method, Some(BootstrapMethod::QrBlob));

    state.peer_count = 5;
    state.start_expansion().unwrap();
    assert_eq!(state.phase, DiscoveryLifecycle::Expansion);

    state.stabilize(500).unwrap();
    assert_eq!(state.phase, DiscoveryLifecycle::Stabilization);
    assert_eq!(state.stabilized_at, 500);
}

// ── Bootstrap methods ──

#[test]
fn test_bootstrap_method_variants() {
    let methods = [
        BootstrapMethod::Static,
        BootstrapMethod::QrBlob,
        BootstrapMethod::LanBroadcast,
        BootstrapMethod::DotDomain,
        BootstrapMethod::TrustedPeers,
        BootstrapMethod::MissionInvite,
    ];
    for (i, m) in methods.iter().enumerate() {
        for (j, n) in methods.iter().enumerate() {
            if i != j {
                assert_ne!(*m as u16, *n as u16);
            }
        }
    }
}

// ── Default TTL and stake functions ──

#[test]
fn test_default_ttl_all_scopes() {
    assert_eq!(default_ttl(&DiscoveryScope::Local), 3);
    assert_eq!(default_ttl(&DiscoveryScope::Regional), 10);
    assert_eq!(default_ttl(&DiscoveryScope::Mission), 5);
    assert_eq!(default_ttl(&DiscoveryScope::Global), 20);
    assert_eq!(default_ttl(&DiscoveryScope::Private), 3);
    assert_eq!(default_ttl(&DiscoveryScope::Consensus), 10);
}

#[test]
fn test_min_octo_all_scopes() {
    assert_eq!(min_octo_for_scope(&DiscoveryScope::Local), 0);
    assert_eq!(min_octo_for_scope(&DiscoveryScope::Regional), 500);
    assert_eq!(min_octo_for_scope(&DiscoveryScope::Mission), 1000);
    assert_eq!(min_octo_for_scope(&DiscoveryScope::Global), 1000);
    assert_eq!(min_octo_for_scope(&DiscoveryScope::Private), 0);
    assert_eq!(min_octo_for_scope(&DiscoveryScope::Consensus), 1000);
}

#[test]
fn test_min_octo_b_all_scopes() {
    assert_eq!(min_octo_b_for_scope(&DiscoveryScope::Local), 0);
    assert_eq!(min_octo_b_for_scope(&DiscoveryScope::Regional), 50);
    assert_eq!(min_octo_b_for_scope(&DiscoveryScope::Mission), 100);
    assert_eq!(min_octo_b_for_scope(&DiscoveryScope::Global), 100);
    assert_eq!(min_octo_b_for_scope(&DiscoveryScope::Private), 0);
    assert_eq!(min_octo_b_for_scope(&DiscoveryScope::Consensus), 200);
}

// ── Scope filter ──

#[test]
fn test_scope_filter_mission_with_id() {
    let filter = ScopeFilter::mission([0xAA; 32]);
    assert_eq!(filter.mission_id, Some([0xAA; 32]));
    assert!(filter.is_visible_in(&DiscoveryScope::Mission));
    assert!(!filter.is_visible_in(&DiscoveryScope::Global));
}

// ── Gateway Capability ──

#[test]
fn test_gateway_capability_all_bits_unique() {
    let caps = [
        GatewayCapability::Edge, GatewayCapability::Relay, GatewayCapability::Consensus,
        GatewayCapability::Archive, GatewayCapability::Stealth, GatewayCapability::Translation,
        GatewayCapability::Storage, GatewayCapability::OnionRelay, GatewayCapability::AIExecution,
        GatewayCapability::VectorIndex, GatewayCapability::ZkVerification, GatewayCapability::MissionCoordinator,
    ];
    for i in 0..caps.len() {
        for j in (i+1)..caps.len() {
            assert_ne!(caps[i] as u64, caps[j] as u64);
        }
    }
}
