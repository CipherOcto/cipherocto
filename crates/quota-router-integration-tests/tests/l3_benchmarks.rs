use std::time::{Duration, Instant};

use quota_router_core::node::provider::{NetworkId, ProviderAuth, ProviderConfig, RouterNodeId};
use quota_router_core::node::request::RequestContext;
use quota_router_core::node::QuotaRouterNode;

fn make_request(model: &str) -> RequestContext {
    RequestContext {
        model: model.to_string(),
        preferred_provider: None,
        model_group: None,
        input_tokens: None,
        max_output_tokens: None,
        tags: None,
        max_price_per_1k_tokens: None,
        max_latency_ms: None,
        policy_override: None,
        consumer_id: [0u8; 32],
        priority: 0,
        deadline: None,
    }
}

fn build_node(provider_models: Vec<&str>) -> std::sync::Arc<QuotaRouterNode> {
    let mut builder = QuotaRouterNode::builder()
        .node_id(RouterNodeId([1u8; 32]))
        .network_id(NetworkId([2u8; 32]));

    for model in provider_models {
        builder = builder.provider(ProviderConfig {
            name: model.to_string(),
            endpoint: "http://localhost".into(),
            auth: ProviderAuth::Local,
            models: vec![model.to_string()],
        });
    }

    builder.build().unwrap()
}

#[tokio::test]
async fn l3_b1_local_dispatch_latency() {
    let node = build_node(vec!["gpt-4o"]);
    let ctx = make_request("gpt-4o");

    let iterations = 1000;
    let start = Instant::now();
    for _ in 0..iterations {
        let _ = node.route(&ctx, b"test").await;
    }
    let elapsed = start.elapsed();
    let per_op = elapsed / iterations;

    eprintln!(
        "B1 local_dispatch: {} iterations in {:?} ({:?}/op)",
        iterations, elapsed, per_op
    );

    // Should complete well under 5ms per dispatch
    assert!(
        per_op < Duration::from_millis(5),
        "local dispatch too slow: {:?}/op",
        per_op
    );
}

#[tokio::test]
async fn l3_b5_concurrent_routing_throughput() {
    let node = std::sync::Arc::new(tokio::sync::Mutex::new(build_node(vec!["gpt-4o"])));
    let ctx = make_request("gpt-4o");

    let iterations = 100;
    let start = Instant::now();
    let mut handles = vec![];
    for _ in 0..iterations {
        let node = node.clone();
        let ctx = ctx.clone();
        handles.push(tokio::spawn(async move {
            let node = node.lock().await;
            let _ = node.route(&ctx, b"test").await;
        }));
    }
    for h in handles {
        h.await.unwrap();
    }
    let elapsed = start.elapsed();
    let throughput = iterations as f64 / elapsed.as_secs_f64();

    eprintln!(
        "B5 concurrent_routing: {} requests in {:?} ({:.0} req/s)",
        iterations, elapsed, throughput
    );

    // Should handle at least 100 req/s
    assert!(
        throughput > 100.0,
        "throughput too low: {:.0} req/s",
        throughput
    );
}

#[test]
fn l3_b6_select_destinations_benchmark() {
    use quota_router_core::node::provider::{
        ModelPricing, ProviderCapacity, ProviderHealth, ProviderId,
    };
    use quota_router_core::node::request::RoutingPolicy;
    use quota_router_core::node::scorer::select_destinations;

    let mut providers = Vec::new();
    for i in 0..100 {
        providers.push(ProviderCapacity {
            provider_id: ProviderId([i as u8; 32]),
            provider_name: format!("provider-{}", i),
            router_node_id: RouterNodeId([0u8; 32]),
            models: vec!["gpt-4o".into()],
            requests_remaining: 100,
            pricing: vec![ModelPricing {
                model: "gpt-4o".into(),
                price_per_1k_tokens: (i as u64) + 1,
            }],
            status: ProviderHealth::Healthy,
            latency_ms: (i as u32) + 10,
            success_rate_bps: 9000 + (i as u16),
            last_updated: 0,
        });
    }

    let ctx = make_request("gpt-4o");
    let iterations = 1000;
    let start = Instant::now();
    for _ in 0..iterations {
        let _ = select_destinations(&ctx, &providers, &[], &RoutingPolicy::Balanced);
    }
    let elapsed = start.elapsed();
    let per_call = elapsed / iterations;

    eprintln!(
        "B6 select_destinations (100 providers): {} calls in {:?} ({:?}/call)",
        iterations, elapsed, per_call
    );

    // Should be well under 1ms per call
    assert!(
        per_call < Duration::from_millis(1),
        "scoring too slow: {:?}/call",
        per_call
    );
}
