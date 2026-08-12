use std::time::Duration;

use quota_router_core::node::provider::{
    ModelPricing, ProviderCapacity, ProviderHealth, ProviderId, RouterNodeId,
};
use quota_router_integration_tests::{make_request, TestCluster};

/// T11 — three_node_fan_out
/// Node A has no gpt-4o, Node C has gpt-4o. After gossip converges,
/// A should forward to C and C's provider should be called.
#[tokio::test]
#[ignore = "blocked on 0870-c (NodeEnvelope dispatch compat — mission filed missions/open/0870-c-envelope-dispatch-compat.md)"]
async fn l2_t11_three_node_fan_out() {
    let cluster = TestCluster::new(
        3,
        vec![
            vec!["gpt-3.5-turbo".into()],
            vec!["gpt-3.5-turbo".into()],
            vec!["gpt-3.5-turbo".into(), "gpt-4o".into()],
        ],
    );
    cluster.start_all().await;
    cluster.wait_converged(Duration::from_millis(100)).await;

    // Seed node 0's gossip cache with node 2's gpt-4o capability
    {
        let node = &*cluster.nodes[0].node;
        node.gossip_cache.lock().unwrap().merge(
            RouterNodeId([3u8; 32]),
            vec![ProviderCapacity {
                provider_id: ProviderId([3u8; 32]),
                provider_name: "far".into(),
                router_node_id: RouterNodeId([3u8; 32]),
                models: vec!["gpt-4o".into()],
                requests_remaining: 100,
                pricing: vec![ModelPricing {
                    model: "gpt-4o".into(),
                    price_per_1k_tokens: 5,
                }],
                status: ProviderHealth::Healthy,
                latency_ms: 200,
                success_rate_bps: 9500,
                last_updated: 0,
            }],
        );
    }

    cluster.nodes[2]
        .provider
        .set_response("gpt-4o", b"from-node-2".to_vec());

    let ctx = make_request("gpt-4o");
    let _result = cluster.nodes[0].route(&ctx, b"fan-out-payload").await;

    // Verify the provider was called on node 2 (the one with gpt-4o).
    // Due to broadcast fan-out, both node 1 and node 2 receive the forward.
    // Node 1 rejects (no gpt-4o), node 2 responds. The oneshot resolves
    // with whichever arrives first — we just verify node 2's provider
    // was actually invoked.
    let captured = cluster.nodes[2].provider.captured();
    assert!(
        captured
            .iter()
            .any(|(m, p)| m == "gpt-4o" && p == b"fan-out-payload"),
        "node 2's provider should have received the forwarded payload, got: {:?}",
        captured
    );
}

/// T12 — ttl_chain_exhaustion
/// With TTL=0, the first hop rejects with TtlExpired.
#[tokio::test]
#[ignore = "blocked on 0870-c (NodeEnvelope dispatch compat — mission filed missions/open/0870-c-envelope-dispatch-compat.md)"]
async fn l2_t12_ttl_chain_exhaustion() {
    let mut cluster = TestCluster::new(
        2,
        vec![
            vec!["gpt-3.5-turbo".into()],
            vec!["gpt-3.5-turbo".into(), "gpt-4o".into()],
        ],
    );
    cluster.start_all().await;
    cluster.wait_converged(Duration::from_millis(100)).await;

    // Seed node 0's gossip with node 1's gpt-4o
    {
        let node = &*cluster.nodes[0].node;
        node.gossip_cache.lock().unwrap().merge(
            RouterNodeId([2u8; 32]),
            vec![ProviderCapacity {
                provider_id: ProviderId([2u8; 32]),
                provider_name: "peer1".into(),
                router_node_id: RouterNodeId([2u8; 32]),
                models: vec!["gpt-4o".into()],
                requests_remaining: 100,
                pricing: vec![ModelPricing {
                    model: "gpt-4o".into(),
                    price_per_1k_tokens: 5,
                }],
                status: ProviderHealth::Healthy,
                latency_ms: 200,
                success_rate_bps: 9500,
                last_updated: 0,
            }],
        );
    }

    // Set TTL=0 — the forward arrives at node 1 with ttl=0, which rejects
    cluster.node_mut(0).await.config.forwarding.max_ttl = 0;
    cluster.node_mut(0).await.config.forwarding.forward_timeout = Duration::from_millis(200);

    let ctx = make_request("gpt-4o");
    let result = cluster.nodes[0].route(&ctx, b"hello").await;
    match result {
        Err(quota_router_core::node::RouterNodeError::ForwardRejected(_)) => {}
        other => panic!("expected ForwardRejected (TTL=0), got {:?}", other),
    }
}

/// T14 — multi_provider_dispatch
/// Node 0 has no gpt-4o. Nodes 1 and 2 both have it. Node 0 should
/// route to the best available provider.
#[tokio::test]
#[ignore = "blocked on 0870-c (NodeEnvelope dispatch compat — mission filed missions/open/0870-c-envelope-dispatch-compat.md)"]
async fn l2_t14_star_topology() {
    let cluster = TestCluster::new(
        3,
        vec![
            vec!["gpt-3.5-turbo".into()],
            vec!["gpt-3.5-turbo".into(), "gpt-4o".into()],
            vec!["gpt-3.5-turbo".into(), "gpt-4o".into()],
        ],
    );
    cluster.start_all().await;
    cluster.wait_converged(Duration::from_millis(100)).await;

    // Seed node 0's gossip with both peers' gpt-4o
    {
        let node = &*cluster.nodes[0].node;
        node.gossip_cache.lock().unwrap().merge(
            RouterNodeId([2u8; 32]),
            vec![ProviderCapacity {
                provider_id: ProviderId([2u8; 32]),
                provider_name: "peer1".into(),
                router_node_id: RouterNodeId([2u8; 32]),
                models: vec!["gpt-4o".into()],
                requests_remaining: 100,
                pricing: vec![ModelPricing {
                    model: "gpt-4o".into(),
                    price_per_1k_tokens: 3,
                }],
                status: ProviderHealth::Healthy,
                latency_ms: 100,
                success_rate_bps: 9500,
                last_updated: 0,
            }],
        );
        node.gossip_cache.lock().unwrap().merge(
            RouterNodeId([3u8; 32]),
            vec![ProviderCapacity {
                provider_id: ProviderId([3u8; 32]),
                provider_name: "peer2".into(),
                router_node_id: RouterNodeId([3u8; 32]),
                models: vec!["gpt-4o".into()],
                requests_remaining: 100,
                pricing: vec![ModelPricing {
                    model: "gpt-4o".into(),
                    price_per_1k_tokens: 10,
                }],
                status: ProviderHealth::Healthy,
                latency_ms: 300,
                success_rate_bps: 9500,
                last_updated: 0,
            }],
        );
    }

    cluster.nodes[1]
        .provider
        .set_response("gpt-4o", b"from-peer1".to_vec());

    // Node 0 has gpt-3.5-turbo locally — should dispatch without forwarding
    let ctx = make_request("gpt-3.5-turbo");
    let result = cluster.nodes[0].route(&ctx, b"hello").await;
    assert!(result.is_ok(), "local gpt-3.5-turbo dispatch should work");

    // Node 0 routes gpt-4o — should forward to a peer
    let ctx = make_request("gpt-4o");
    let result = cluster.nodes[0].route(&ctx, b"hello").await;
    assert!(
        result.is_ok(),
        "gpt-4o should be forwarded to a peer: {:?}",
        result
    );
}
