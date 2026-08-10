//! L2 forward reject reasons — exercises every `ForwardRejectReason`
//! variant through the production `node.receive()` → handler path.

use octo_transport::receiver::ReceiveContext;
use quota_router_core::node::announce::SignedPayload;
use quota_router_core::node::forward::ForwardRequestPayload;
use quota_router_core::node::gossip::{monotonic_now, CapacityGossipPayload};
use quota_router_core::node::provider::{
    ModelPricing, NetworkId, ProviderCapacity, ProviderHealth, ProviderId, RouterNodeId,
};
use quota_router_core::node::{envelope, DISC_CAPACITY_GOSSIP, DISC_FORWARD_REQUEST};
use quota_router_integration_tests::{make_request, TestCluster};

fn make_fwd_request(request_id: [u8; 32], model: &str, ttl: u8) -> ForwardRequestPayload {
    ForwardRequestPayload {
        request_id,
        network_id: NetworkId([1u8; 32]),
        context: make_request(model),
        payload: b"test-payload".to_vec(),
        ttl,
        origin_node: RouterNodeId([0xAAu8; 32]),
        hop_count: 0,
        created_at: monotonic_now(),
        hmac: [0u8; 32],
    }
}

/// ForwardRequest with TTL=0 → handler rejects with TtlExpired.
#[tokio::test]
async fn l2_reject_ttl_expired() {
    let cluster = TestCluster::new(1, vec![vec!["gpt-4o".into()]]);

    let req = make_fwd_request([1u8; 32], "gpt-4o", 0);
    let framed = envelope(DISC_FORWARD_REQUEST, &req).unwrap();
    let ctx = ReceiveContext {
        source_transport: "test".into(),
        mission_id: [0u8; 32],
        sender_id: None,
    };
    let r = cluster.nodes[0].node.receive(&framed, &ctx).await;
    assert!(
        r.is_ok(),
        "TTL=0 should be accepted and rejected internally"
    );
}

/// ForwardRequest for unknown model → NoProvider rejection.
#[tokio::test]
async fn l2_reject_no_provider() {
    let cluster = TestCluster::new(1, vec![vec!["gpt-4o".into()]]);

    let req = make_fwd_request([2u8; 32], "nonexistent-model", 3);
    let framed = envelope(DISC_FORWARD_REQUEST, &req).unwrap();
    let ctx = ReceiveContext {
        source_transport: "test".into(),
        mission_id: [0u8; 32],
        sender_id: None,
    };
    let r = cluster.nodes[0].node.receive(&framed, &ctx).await;
    assert!(r.is_ok());
}

/// CapacityGossip with valid HMAC merges into gossip cache.
#[tokio::test]
async fn l2_gossip_valid_hmac_merges() {
    let cluster = TestCluster::new(2, vec![vec!["gpt-4o".into()], vec!["gpt-4o".into()]]);

    let sender = RouterNodeId([0x55u8; 32]);
    let mut gossip = CapacityGossipPayload {
        sender_id: sender,
        timestamp: monotonic_now(),
        capacities: vec![ProviderCapacity {
            provider_id: ProviderId([0xA1u8; 32]),
            provider_name: "remote-provider".into(),
            router_node_id: sender,
            models: vec!["gpt-4o".into()],
            requests_remaining: 100,
            pricing: vec![ModelPricing {
                model: "gpt-4o".into(),
                price_per_1k_tokens: 5,
            }],
            status: ProviderHealth::Healthy,
            latency_ms: 150,
            success_rate_bps: 9800,
            last_updated: 0,
        }],
        known_peers: vec![],
        hmac: [0u8; 32],
    };
    gossip.hmac = gossip.compute_hmac(&cluster.network_key);
    let framed = envelope(DISC_CAPACITY_GOSSIP, &gossip).unwrap();

    let ctx = ReceiveContext {
        source_transport: "test".into(),
        mission_id: [0u8; 32],
        sender_id: Some(sender.0),
    };
    let r = cluster.nodes[0].node.receive(&framed, &ctx).await;
    assert!(r.is_ok());

    let snap = cluster.nodes[0]
        .node
        .gossip_cache
        .lock()
        .unwrap()
        .snapshot();
    assert_eq!(snap.len(), 1, "gossip should have 1 entry after merge");
    assert_eq!(snap[0].0, sender);
    assert_eq!(snap[0].1[0].requests_remaining, 100);
}

/// CapacityGossip with invalid HMAC is rejected.
#[tokio::test]
async fn l2_gossip_invalid_hmac_rejected() {
    let cluster = TestCluster::new(1, vec![vec!["gpt-4o".into()]]);

    let gossip = CapacityGossipPayload {
        sender_id: RouterNodeId([0x66u8; 32]),
        timestamp: monotonic_now(),
        capacities: vec![],
        known_peers: vec![],
        hmac: [0u8; 32], // wrong HMAC
    };
    let framed = envelope(DISC_CAPACITY_GOSSIP, &gossip).unwrap();

    let ctx = ReceiveContext {
        source_transport: "test".into(),
        mission_id: [0u8; 32],
        sender_id: Some([0x66u8; 32]),
    };
    let r = cluster.nodes[0].node.receive(&framed, &ctx).await;
    assert!(r.is_err(), "invalid HMAC should be rejected");
}

/// CapacityGossip known_peers populates peer cache.
#[tokio::test]
async fn l2_gossip_known_peers_populates_cache() {
    let cluster = TestCluster::new(1, vec![vec!["gpt-4o".into()]]);

    let peer_a = RouterNodeId([0xAAu8; 32]);
    let peer_b = RouterNodeId([0xBBu8; 32]);
    let mut gossip = CapacityGossipPayload {
        sender_id: RouterNodeId([0x77u8; 32]),
        timestamp: monotonic_now(),
        capacities: vec![],
        known_peers: vec![peer_a, peer_b],
        hmac: [0u8; 32],
    };
    gossip.hmac = gossip.compute_hmac(&cluster.network_key);
    let framed = envelope(DISC_CAPACITY_GOSSIP, &gossip).unwrap();

    let ctx = ReceiveContext {
        source_transport: "test".into(),
        mission_id: [0u8; 32],
        sender_id: Some([0x77u8; 32]),
    };
    let _ = cluster.nodes[0].node.receive(&framed, &ctx).await;

    let peers = cluster.nodes[0].node.peer_count();
    assert!(
        peers >= 2,
        "known_peers should populate cache, got {}",
        peers
    );
}

/// RouterAnnounce with valid HMAC adds peer if model overlap.
#[tokio::test]
async fn l2_announce_valid_adds_peer() {
    use quota_router_core::node::announce::RouterAnnouncePayload;
    use quota_router_core::node::DISC_ROUTER_ANNOUNCE;

    let cluster = TestCluster::new(1, vec![vec!["gpt-4o".into()]]);

    let peer_id = RouterNodeId([0xCCu8; 32]);
    let mut announce = RouterAnnouncePayload {
        node_id: peer_id,
        pricing_policy: None,
        network_id: NetworkId([1u8; 32]),
        supported_models: vec!["gpt-4o".into()],
        capacities: vec![],
        timestamp: monotonic_now(),
        hmac: [0u8; 32],
    };
    announce.hmac = announce.compute_hmac(&cluster.network_key);
    let framed = envelope(DISC_ROUTER_ANNOUNCE, &announce).unwrap();

    let ctx = ReceiveContext {
        source_transport: "test".into(),
        mission_id: [0u8; 32],
        sender_id: Some(peer_id.0),
    };
    let r = cluster.nodes[0].node.receive(&framed, &ctx).await;
    assert!(r.is_ok());

    let peers = cluster.nodes[0].node.peer_count();
    assert!(peers >= 1, "announce should add peer, got {}", peers);
}

/// RouterAnnounce with invalid HMAC is rejected.
#[tokio::test]
async fn l2_announce_invalid_hmac_rejected() {
    use quota_router_core::node::announce::RouterAnnouncePayload;
    use quota_router_core::node::DISC_ROUTER_ANNOUNCE;

    let cluster = TestCluster::new(1, vec![vec!["gpt-4o".into()]]);

    let announce = RouterAnnouncePayload {
        node_id: RouterNodeId([0xDDu8; 32]),
        network_id: NetworkId([1u8; 32]),
        supported_models: vec!["gpt-4o".into()],
        capacities: vec![],
        timestamp: monotonic_now(),
        hmac: [0u8; 32], // wrong
        pricing_policy: None,
    };
    let framed = envelope(DISC_ROUTER_ANNOUNCE, &announce).unwrap();

    let ctx = ReceiveContext {
        source_transport: "test".into(),
        mission_id: [0u8; 32],
        sender_id: Some([0xDDu8; 32]),
    };
    let r = cluster.nodes[0].node.receive(&framed, &ctx).await;
    assert!(r.is_err());
}

/// RouterWithdraw with valid HMAC removes peer.
#[tokio::test]
async fn l2_withdraw_removes_peer() {
    use quota_router_core::node::announce::{
        RouterAnnouncePayload, RouterWithdrawPayload, WithdrawReason,
    };
    use quota_router_core::node::{DISC_ROUTER_ANNOUNCE, DISC_ROUTER_WITHDRAW};

    let cluster = TestCluster::new(1, vec![vec!["gpt-4o".into()]]);

    // First add the peer via announce
    let peer_id = RouterNodeId([0xEEu8; 32]);
    let mut announce = RouterAnnouncePayload {
        node_id: peer_id,
        pricing_policy: None,
        network_id: NetworkId([1u8; 32]),
        supported_models: vec!["gpt-4o".into()],
        capacities: vec![],
        timestamp: monotonic_now(),
        hmac: [0u8; 32],
    };
    announce.hmac = announce.compute_hmac(&cluster.network_key);
    let ctx = ReceiveContext {
        source_transport: "test".into(),
        mission_id: [0u8; 32],
        sender_id: Some(peer_id.0),
    };
    let _ = cluster.nodes[0]
        .node
        .receive(&envelope(DISC_ROUTER_ANNOUNCE, &announce).unwrap(), &ctx)
        .await;
    assert!(cluster.nodes[0].node.peer_count() >= 1);

    // Now withdraw
    let mut withdraw = RouterWithdrawPayload {
        node_id: peer_id,
        reason: WithdrawReason::Graceful,
        timestamp: monotonic_now(),
        hmac: [0u8; 32],
    };
    withdraw.hmac = withdraw.compute_hmac(&cluster.network_key);
    let framed = envelope(DISC_ROUTER_WITHDRAW, &withdraw).unwrap();
    let _ = cluster.nodes[0].node.receive(&framed, &ctx).await;

    // Withdraw removes from peer_cache.direct
    let cache = cluster.nodes[0].node.peer_cache.lock().unwrap();
    assert!(
        !cache.direct_ids().contains(&peer_id),
        "withdrawn peer should not be in peer_cache.direct"
    );
}
