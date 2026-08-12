//! L2 multi-hop forwarding tests (RFC-0870 T2..T9).
//!
//! These tests drive the FULL production code path: real `QuotaRouterNode::route`,
//! real `QuotaRouterHandler::on_receive`, HMAC verification, peer-trust gates,
//! TTL handling, and `PendingRequests` resolution. The only seam we replace
//! is the wire (`NetworkSender`) — every other code path runs exactly as in
//! production.
//!
//! For a multi-hop forward to fire, two nodes must share at least one model so
//! the announce handler's model-overlap gate accepts each side. Node 0 routes
//! for a model *only* node 1 has. Node 0 must learn node 1's capacity through
//! the gossip cache (after announce merge), then `select_destinations` picks
//! node 1, and `route()` actually puts a frame on the wire.

use std::time::Duration;

use quota_router_core::node::provider::RouterNodeId;
use quota_router_core::node::RouterNodeError;
use quota_router_integration_tests::{make_request, TestCluster};

/// T2 — single_hop_forwarding
/// Origin has no model X; exactly one peer has it. After gossip converges
/// the originator's `select_destinations` resolves to the peer; `route()`
/// emits a real forward request; the peer's handler dispatches locally
/// and the originator receives the response.
#[tokio::test]
#[ignore = "blocked on 0870-c (NodeEnvelope dispatch compat — mission filed missions/open/0870-c-envelope-dispatch-compat.md)"]
async fn l2_t2_single_hop_forwarding() {
    // Shared model `gpt-3.5-turbo` lets both nodes accept each other's
    // announces. Origin routes for `gpt-4o`, which only node 1 has.
    let cluster = TestCluster::new(
        2,
        vec![
            vec!["gpt-3.5-turbo".into()],
            vec!["gpt-3.5-turbo".into(), "gpt-4o".into()],
        ],
    );
    cluster.start_all().await;
    cluster.wait_converged(Duration::from_millis(100)).await;

    // Distinguish node 1's response from a phantom local one.
    cluster.nodes[1]
        .provider
        .set_response("gpt-4o", b"from-node-1".to_vec());

    let ctx = make_request("gpt-4o");
    let result = cluster.nodes[0].route(&ctx, b"my-payload").await;
    assert!(
        result.is_ok(),
        "forwarding to peer with gpt-4o should succeed: {:?}",
        result
    );
    assert_eq!(result.unwrap(), b"from-node-1".to_vec());

    // The forwarded payload must have actually reached node 1's
    // production LocalProvider (`MockLocalProvider::completion`).
    let captured = cluster.nodes[1].provider.captured();
    assert!(
        captured.iter().any(|(_, p)| p == b"my-payload"),
        "node 1's LocalProvider should have seen the forwarded payload, got: {:?}",
        captured
    );
}

/// T3 — policy_cheapest
/// With two remote peers both offering `gpt-4o`, the `Cheapest` policy
/// (lower price first) must pick the cheaper peer. We seed the gossip
/// cache directly with two `ProviderCapacity` records that differ only
/// in pricing, then assert that the cheaper provider is the one whose
/// `set_response` bytes the originator gets back.
#[tokio::test]
#[ignore = "blocked on 0870-c (NodeEnvelope dispatch compat — mission filed missions/open/0870-c-envelope-dispatch-compat.md)"]
async fn l2_t3_policy_cheapest() {
    use quota_router_core::node::provider::{
        ModelPricing, ProviderCapacity, ProviderHealth, ProviderId, RouterNodeId,
    };

    let cluster = TestCluster::new(
        2,
        vec![
            vec!["gpt-3.5-turbo".into()],
            vec!["gpt-3.5-turbo".into(), "gpt-4o".into()],
        ],
    );
    cluster.start_all().await;

    // Inject two peer capacity records for the origin node. The cheaper
    // option uses `node 1` (which can actually answer); the expensive
    // option is a synthetic phantom node that no real node represents.
    {
        let node = &*cluster.nodes[0].node;
        // Cheap option: back-reference node 1 so the forward wire actually
        // succeeds.
        node.gossip_cache.lock().unwrap().merge(
            RouterNodeId([2u8; 32]),
            vec![ProviderCapacity {
                provider_id: ProviderId([2u8; 32]),
                provider_name: "cheaper".into(),
                router_node_id: RouterNodeId([2u8; 32]),
                models: vec!["gpt-4o".into()],
                requests_remaining: 100,
                pricing: vec![ModelPricing {
                    model: "gpt-4o".into(),
                    price_per_1k_tokens: 1,
                }],
                status: ProviderHealth::Healthy,
                latency_ms: 200,
                success_rate_bps: 9500,
                last_updated: 0,
            }],
        );
        // Expensive option: phantom peer
        node.gossip_cache.lock().unwrap().merge(
            RouterNodeId([99u8; 32]),
            vec![ProviderCapacity {
                provider_id: ProviderId([99u8; 32]),
                provider_name: "expensive".into(),
                router_node_id: RouterNodeId([99u8; 32]),
                models: vec!["gpt-4o".into()],
                requests_remaining: 100,
                pricing: vec![ModelPricing {
                    model: "gpt-4o".into(),
                    price_per_1k_tokens: 100,
                }],
                status: ProviderHealth::Healthy,
                latency_ms: 200,
                success_rate_bps: 9500,
                last_updated: 0,
            }],
        );
    }

    cluster.nodes[1]
        .provider
        .set_response("gpt-4o", b"cheap-reply".to_vec());

    let mut ctx = make_request("gpt-4o");
    ctx.policy_override = Some(quota_router_core::node::request::RoutingPolicy::Cheapest);
    let result = cluster.nodes[0].route(&ctx, b"hello").await;
    assert!(result.is_ok(), "cheapest peer should answer: {:?}", result);
    assert_eq!(result.unwrap(), b"cheap-reply");
}

/// T4 — policy_fastest
/// Mirror of T3 but using `RoutingPolicy::Fastest` with `latency_ms` as
/// the differentiator.
#[tokio::test]
#[ignore = "blocked on 0870-c (NodeEnvelope dispatch compat — mission filed missions/open/0870-c-envelope-dispatch-compat.md)"]
async fn l2_t4_policy_fastest() {
    use quota_router_core::node::provider::{
        ModelPricing, ProviderCapacity, ProviderHealth, ProviderId, RouterNodeId,
    };

    let cluster = TestCluster::new(
        2,
        vec![
            vec!["gpt-3.5-turbo".into()],
            vec!["gpt-3.5-turbo".into(), "gpt-4o".into()],
        ],
    );
    cluster.start_all().await;

    {
        let node = &*cluster.nodes[0].node;
        node.gossip_cache.lock().unwrap().merge(
            RouterNodeId([2u8; 32]),
            vec![ProviderCapacity {
                provider_id: ProviderId([2u8; 32]),
                provider_name: "fast".into(),
                router_node_id: RouterNodeId([2u8; 32]),
                models: vec!["gpt-4o".into()],
                requests_remaining: 100,
                pricing: vec![ModelPricing {
                    model: "gpt-4o".into(),
                    price_per_1k_tokens: 5,
                }],
                status: ProviderHealth::Healthy,
                latency_ms: 50,
                success_rate_bps: 9500,
                last_updated: 0,
            }],
        );
        node.gossip_cache.lock().unwrap().merge(
            RouterNodeId([98u8; 32]),
            vec![ProviderCapacity {
                provider_id: ProviderId([98u8; 32]),
                provider_name: "slow".into(),
                router_node_id: RouterNodeId([98u8; 32]),
                models: vec!["gpt-4o".into()],
                requests_remaining: 100,
                pricing: vec![ModelPricing {
                    model: "gpt-4o".into(),
                    price_per_1k_tokens: 5,
                }],
                status: ProviderHealth::Healthy,
                latency_ms: 900,
                success_rate_bps: 9500,
                last_updated: 0,
            }],
        );
    }

    cluster.nodes[1]
        .provider
        .set_response("gpt-4o", b"fast-reply".to_vec());

    let mut ctx = make_request("gpt-4o");
    ctx.policy_override = Some(quota_router_core::node::request::RoutingPolicy::Fastest);
    let result = cluster.nodes[0].route(&ctx, b"hello").await;
    assert!(result.is_ok(), "fastest peer should answer: {:?}", result);
    assert_eq!(result.unwrap(), b"fast-reply");
}

/// T8 — forward_timeout
/// The destination never sends a response; `route()` must surface
/// `ForwardTimeout` rather than hanging forever. We achieve this by
/// evicting the peer's inbox from the shared peer_map AFTER gossip has
/// converged. The originator still picks the peer as the destination
/// (its gossip cache is unchanged) but the InProcessSender finds no
/// recipient on the wire and the originator's oneshot is never fulfilled.
#[tokio::test]
#[ignore = "blocked on 0870-c (NodeEnvelope dispatch compat — mission filed missions/open/0870-c-envelope-dispatch-compat.md)"]
async fn l2_t8_forward_timeout() {
    let mut cluster = TestCluster::new(
        2,
        vec![
            vec!["gpt-3.5-turbo".into()],
            vec!["gpt-3.5-turbo".into(), "gpt-4o".into()],
        ],
    );
    cluster.start_all().await;
    cluster.wait_converged(Duration::from_millis(100)).await;

    // Tighten the originator's forward timeout so the test is fast.
    {
        cluster.node_mut(0).await.config.forwarding.forward_timeout = Duration::from_millis(50);
    }

    // Sever node 1 from the in-process mesh so the forward goes into
    // the void. The originator's gossip cache is untouched, so its
    // select_destinations still picks node 1.
    cluster.sever_peer(RouterNodeId([2u8; 32]));

    let ctx = make_request("gpt-4o");
    let result = tokio::time::timeout(
        Duration::from_secs(2),
        cluster.nodes[0].route(&ctx, b"hello"),
    )
    .await;
    let inner = result.expect("route() must not hang past 2s");
    assert!(
        matches!(inner, Err(RouterNodeError::ForwardTimeout)),
        "expected ForwardTimeout, got {:?}",
        inner
    );
}

/// T9 — max_concurrent_forwards
/// With `max_concurrent_forwards = 1`, a second concurrent `route()` from
/// the same origin must be either rate-limited or fast-failed rather than
/// opening a second forward socket. We exercise this by issuing two
/// concurrent routes.
#[tokio::test]
#[ignore = "blocked on 0870-c (NodeEnvelope dispatch compat — mission filed missions/open/0870-c-envelope-dispatch-compat.md)"]
async fn l2_t9_max_concurrent_forwards() {
    let mut cluster = TestCluster::new(
        2,
        vec![
            vec!["gpt-3.5-turbo".into()],
            vec!["gpt-3.5-turbo".into(), "gpt-4o".into()],
        ],
    );
    cluster.start_all().await;
    cluster.wait_converged(Duration::from_millis(100)).await;

    // Set the max concurrent forwards to 1.
    {
        cluster
            .node_mut(0)
            .await
            .config
            .forwarding
            .max_concurrent_forwards = 1;
    }

    let ctx = make_request("gpt-4o");
    // Fire two concurrent routes; one must succeed, the other must fail
    // (rate-limited or capacity-exceeded). We tolerate either because the
    // production code path for concurrent gating is what we're verifying.
    let r1 = cluster.nodes[0].route(&ctx, b"hello-1");
    let r2 = cluster.nodes[0].route(&ctx, b"hello-2");
    let (a, b) = tokio::join!(r1, r2);
    let oks = [&a, &b].iter().filter(|r| r.is_ok()).count();
    let errs = [&a, &b]
        .iter()
        .filter(|r| matches!(r, Err(RouterNodeError::RateLimited)))
        .count();
    assert!(
        oks >= 1,
        "at least one route should succeed: {:?} {:?}",
        a,
        b
    );
    assert!(
        errs >= 1,
        "with max_concurrent_forwards=1, the other route should be rate-limited: {:?} {:?}",
        a,
        b
    );
}
