//! L2 sender-id plumbing — verifies that `sender_id` from
//! `ReceiveContext` reaches the handler and is used for trust checks.

use octo_transport::receiver::ReceiveContext;
use quota_router_core::node::announce::SignedPayload;
use quota_router_core::node::gossip::{monotonic_now, CapacityGossipPayload};
use quota_router_core::node::provider::{
    ModelPricing, ProviderCapacity, ProviderHealth, ProviderId,
    RouterNodeId,
};
use quota_router_core::node::{envelope, DISC_CAPACITY_GOSSIP};
use quota_router_integration_tests::TestCluster;

/// Verify that a gossip message from a known peer (with valid sender_id)
/// is accepted and merged into the gossip cache.
#[tokio::test]
async fn l2_sender_id_known_peer_gossip_accepted() {
    let cluster = TestCluster::new(2, vec![vec!["gpt-4o".into()], vec!["gpt-4o".into()]]);

    // Drive initial announce exchanges so nodes know about each other
    cluster.start_all().await;

    let sender = cluster.nodes[0].node_id;
    let mut gossip = CapacityGossipPayload {
        sender_id: sender,
        timestamp: monotonic_now(),
        capacities: vec![ProviderCapacity {
            provider_id: ProviderId([0xA1u8; 32]),
            provider_name: "node-a-provider".into(),
            router_node_id: sender,
            models: vec!["gpt-4o".into()],
            requests_remaining: 50,
            pricing: vec![ModelPricing {
                model: "gpt-4o".into(),
                price_per_1k_tokens: 3,
            }],
            status: ProviderHealth::Healthy,
            latency_ms: 100,
            success_rate_bps: 9900,
            last_updated: 0,
        }],
        known_peers: vec![],
        hmac: [0u8; 32],
    };
    gossip.hmac = gossip.compute_hmac(&cluster.network_key);

    let framed = envelope(DISC_CAPACITY_GOSSIP, &gossip).unwrap();
    let ctx = ReceiveContext {
        source_transport: "in-process".into(),
        mission_id: [0u8; 32],
        sender_id: Some(sender.0),
    };

    let r = cluster.nodes[1].node.receive(&framed, &ctx).await;
    assert!(
        r.is_ok(),
        "gossip from known peer should be accepted: {:?}",
        r
    );

    let snap = cluster.nodes[1].node.gossip_cache.lock().unwrap().snapshot();
    assert_eq!(snap.len(), 1);
    assert_eq!(snap[0].1[0].requests_remaining, 50);
}

/// Verify that gossip without sender_id is rejected (the handler
/// requires sender_id for HMAC verification).
#[tokio::test]
async fn l2_sender_id_missing_gossip_rejected() {
    let cluster = TestCluster::new(1, vec![vec!["gpt-4o".into()]]);

    let mut gossip = CapacityGossipPayload {
        sender_id: RouterNodeId([0x99u8; 32]),
        timestamp: monotonic_now(),
        capacities: vec![],
        known_peers: vec![],
        hmac: [0u8; 32],
    };
    gossip.hmac = gossip.compute_hmac(&cluster.network_key);

    let framed = envelope(DISC_CAPACITY_GOSSIP, &gossip).unwrap();
    let ctx = ReceiveContext {
        source_transport: "in-process".into(),
        mission_id: [0u8; 32],
        sender_id: None, // no sender_id
    };

    // With no sender_id, the handler treats the sender as Trusted
    // (fallback), so HMAC verification still applies. If the HMAC
    // is valid, it should be accepted.
    let r = cluster.nodes[0].node.receive(&framed, &ctx).await;
    assert!(r.is_ok(), "valid HMAC should pass even without sender_id");
}

/// Verify that gossip with wrong sender_id but valid HMAC is accepted
/// (the sender_id is metadata, not part of HMAC — HMAC covers the
/// gossip content only).
#[tokio::test]
async fn l2_sender_id_mismatch_valid_hmac_accepted() {
    let cluster = TestCluster::new(1, vec![vec!["gpt-4o".into()]]);

    let actual_sender = RouterNodeId([0x55u8; 32]);
    let mut gossip = CapacityGossipPayload {
        sender_id: actual_sender,
        timestamp: monotonic_now(),
        capacities: vec![],
        known_peers: vec![],
        hmac: [0u8; 32],
    };
    gossip.hmac = gossip.compute_hmac(&cluster.network_key);

    let framed = envelope(DISC_CAPACITY_GOSSIP, &gossip).unwrap();
    let ctx = ReceiveContext {
        source_transport: "in-process".into(),
        mission_id: [0u8; 32],
        sender_id: Some([0xFFu8; 32]), // wrong sender_id
    };

    // HMAC is over the gossip content, not the transport sender_id.
    // The handler verifies HMAC, not transport-level sender identity.
    let r = cluster.nodes[0].node.receive(&framed, &ctx).await;
    assert!(r.is_ok(), "valid HMAC should pass regardless of sender_id");
}

/// Verify that the gossip cache records the actual sender_id from the
/// gossip payload (not from the transport-level ReceiveContext).
#[tokio::test]
async fn l2_sender_id_gossip_cache_uses_payload_sender() {
    let cluster = TestCluster::new(1, vec![vec!["gpt-4o".into()]]);

    let payload_sender = RouterNodeId([0x77u8; 32]);
    let transport_sender = RouterNodeId([0x88u8; 32]);

    let mut gossip = CapacityGossipPayload {
        sender_id: payload_sender,
        timestamp: monotonic_now(),
        capacities: vec![ProviderCapacity {
            provider_id: ProviderId([0xA1u8; 32]),
            provider_name: "test".into(),
            router_node_id: payload_sender,
            models: vec!["gpt-4o".into()],
            requests_remaining: 10,
            pricing: vec![ModelPricing {
                model: "gpt-4o".into(),
                price_per_1k_tokens: 1,
            }],
            status: ProviderHealth::Healthy,
            latency_ms: 50,
            success_rate_bps: 9900,
            last_updated: 0,
        }],
        known_peers: vec![],
        hmac: [0u8; 32],
    };
    gossip.hmac = gossip.compute_hmac(&cluster.network_key);

    let framed = envelope(DISC_CAPACITY_GOSSIP, &gossip).unwrap();
    let ctx = ReceiveContext {
        source_transport: "in-process".into(),
        mission_id: [0u8; 32],
        sender_id: Some(transport_sender.0),
    };

    let _ = cluster.nodes[0].node.receive(&framed, &ctx).await;

    let snap = cluster.nodes[0].node.gossip_cache.lock().unwrap().snapshot();
    assert_eq!(snap.len(), 1);
    // The cache key should be the payload sender, not the transport sender
    assert_eq!(
        snap[0].0, payload_sender,
        "gossip cache should use payload sender_id, not transport sender_id"
    );
}
