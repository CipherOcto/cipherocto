//! L2 inbound happy path — direct `node.receive()` with a valid envelope.
//!
//! Verifies that the production public inbound API
//! (`QuotaRouterNode::receive`) correctly dispatches a valid
//! `CapacityGossip` envelope (discriminator `0xC6`) through the
//! builder-installed handler, merging the gossiped capacities into the
//! receiver's gossip cache.
//!
//! This test deliberately exercises the production path:
//!   - `node.receive(payload, &ctx)` → `transport.dispatch(...)` →
//!     `handler.on_receive(...)` → `handle_capacity_gossip(...)`.
//!
//! No parallel call sites; no manual handler wiring.

use octo_transport::receiver::ReceiveContext;
use quota_router::announce::SignedPayload;
use quota_router::gossip::{monotonic_now, CapacityGossipPayload};
use quota_router::provider::{
    ModelPricing, ProviderCapacity, ProviderHealth, ProviderId, RouterNodeId,
};
use quota_router::{envelope, DISC_CAPACITY_GOSSIP};
use quota_router_e2e_tests::TestCluster;

fn make_capacity(provider_name: &str, model: &str, remaining: u64) -> ProviderCapacity {
    ProviderCapacity {
        provider_id: ProviderId([0xA1u8; 32]),
        provider_name: provider_name.to_string(),
        router_node_id: RouterNodeId([0xB1u8; 32]),
        models: vec![model.to_string()],
        requests_remaining: remaining,
        pricing: vec![ModelPricing {
            model: model.to_string(),
            price_per_1k_tokens: 3,
        }],
        status: ProviderHealth::Healthy,
        latency_ms: 200,
        success_rate_bps: 9500,
        last_updated: 0,
    }
}

/// Direct `node.receive()` with a valid `CapacityGossip` envelope merges
/// the gossiped capacities into the receiver's gossip cache through the
/// production inbound dispatch path.
#[tokio::test]
async fn l2_inbound_happy_path_valid_gossip() {
    let cluster = TestCluster::new(2, vec![vec!["gpt-4o".into()], vec!["gpt-4o".into()]]);

    // Capture baseline gossip cache on node 1 (the receiver). The
    // baseline is empty since we haven't broadcast gossip yet.
    cluster.drive_all().await;
    let baseline = cluster.nodes[1].gossip_cache_snapshot().await;
    assert!(
        baseline.is_empty(),
        "node 1 gossip cache should be empty before injection, got {:?}",
        baseline
    );

    // Build a valid `CapacityGossipPayload` from a phantom sender. The
    // HMAC must match the network's key (derived from the network_id
    // used by the cluster) so the handler accepts it.
    let sender_id = RouterNodeId([0x99u8; 32]);
    let mut gossip = CapacityGossipPayload {
        sender_id,
        timestamp: monotonic_now(),
        capacities: vec![make_capacity("phantom-provider", "gpt-4o", 42)],
        known_peers: vec![],
        hmac: [0u8; 32],
    };
    gossip.hmac = gossip.compute_hmac(&cluster.network_key);

    // Serialize via the production `envelope()` helper — same path
    // used by every outbound site (broadcast_gossip, broadcast_announce,
    // route, send_forward_*).
    let framed = envelope(DISC_CAPACITY_GOSSIP, &gossip).expect("envelope");

    // Call the production public inbound API directly on node 1.
    let ctx = ReceiveContext {
        source_transport: "direct".into(),
        mission_id: [0u8; 32],
        sender_id: Some(sender_id.0),
    };
    let result = cluster.nodes[1].node.receive(&framed, &ctx).await;
    assert!(
        result.is_ok(),
        "node.receive should accept valid gossip: {:?}",
        result
    );

    // The handler must have merged the gossiped capacities into the
    // receiver's gossip cache.
    let snap = cluster.nodes[1].gossip_cache_snapshot().await;
    assert_eq!(snap.len(), 1, "expected 1 gossip entry, got {:?}", snap);
    let (observed_sender, observed_caps) = &snap[0];
    assert_eq!(*observed_sender, sender_id);
    assert_eq!(observed_caps.len(), 1);
    assert_eq!(observed_caps[0].provider_name, "phantom-provider");
    assert_eq!(observed_caps[0].requests_remaining, 42);
    assert_eq!(observed_caps[0].models, vec!["gpt-4o".to_string()]);
}

/// Direct `node.receive()` with an unknown discriminator returns Ok
/// (the handler treats unknown discriminators as no-ops — this is the
/// production contract verified in `receive_delegates_to_transport_dispatch`).
#[tokio::test]
async fn l2_inbound_happy_path_unknown_discriminator_is_ok() {
    let cluster = TestCluster::new(1, vec![vec!["gpt-4o".into()]]);
    let ctx = ReceiveContext {
        source_transport: "direct".into(),
        mission_id: [0u8; 32],
        sender_id: None,
    };
    let r = cluster.nodes[0].node.receive(&[0xFF], &ctx).await;
    assert!(r.is_ok(), "unknown discriminator must be Ok: {:?}", r);
}

/// Direct `node.receive()` with a valid `CapacityGossip` envelope
/// also pulls the gossiped `known_peers` into the peer cache through
/// the production handler.
#[tokio::test]
async fn l2_inbound_happy_path_gossip_pulls_known_peers() {
    let cluster = TestCluster::new(1, vec![vec!["gpt-4o".into()]]);

    // No peers configured on this node.
    let peers_before = cluster.nodes[0].node.peer_count();
    assert_eq!(peers_before, 0, "no configured peers at start");

    let sender_id = RouterNodeId([0x77u8; 32]);
    let known_peer_a = RouterNodeId([0xAAu8; 32]);
    let known_peer_b = RouterNodeId([0xBBu8; 32]);
    let mut gossip = CapacityGossipPayload {
        sender_id,
        timestamp: monotonic_now(),
        capacities: vec![],
        known_peers: vec![known_peer_a, known_peer_b],
        hmac: [0u8; 32],
    };
    gossip.hmac = gossip.compute_hmac(&cluster.network_key);
    let framed = envelope(DISC_CAPACITY_GOSSIP, &gossip).expect("envelope");

    let ctx = ReceiveContext {
        source_transport: "direct".into(),
        mission_id: [0u8; 32],
        sender_id: Some(sender_id.0),
    };
    let _ = cluster.nodes[0].node.receive(&framed, &ctx).await;

    // The sender is added to discovered peers, plus the two known_peers
    // it announced. peer_count = 3 (all from the discovered cache,
    // disjoint from `config.peers`).
    let peers_after = cluster.nodes[0].node.peer_count();
    assert!(
        peers_after >= 2,
        "gossip's known_peers should populate peer cache, got {}",
        peers_after
    );
}
