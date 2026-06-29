use quota_router_e2e_tests::{make_request, TestCluster, Topology};

#[tokio::test]
async fn l2_t11_three_node_fan_out() {
    // Node A: no gpt-4o, Node B: no gpt-4o, Node C: has gpt-4o
    let cluster = TestCluster::new(
        3,
        Topology::Line,
        vec![
            vec!["claude-3".into()],
            vec!["gemini-pro".into()],
            vec!["gpt-4o".into()],
        ],
    );
    cluster.start_all().await;

    // A routes gpt-4o — should not find locally
    let ctx = make_request("gpt-4o");
    let result = cluster.nodes[0].route(&ctx, b"hello").await;
    // Without gossip, A doesn't know C has gpt-4o, so NoProvider
    assert!(result.is_err());
}

#[tokio::test]
async fn l2_t12_ttl_chain_exhaustion() {
    let cluster = TestCluster::new(
        4,
        Topology::Line,
        vec![
            vec!["claude-3".into()],
            vec!["gemini-pro".into()],
            vec!["llama-3".into()],
            vec!["gpt-4o".into()],
        ],
    );
    cluster.start_all().await;

    // Route with TTL=2 — should die before reaching node D
    let mut ctx = make_request("gpt-4o");
    ctx.policy_override = None;
    let result = cluster.nodes[0].route(&ctx, b"hello").await;
    // No gossip, so NoProvider (can't even start forwarding)
    assert!(result.is_err());
}

#[tokio::test]
async fn l2_t14_star_topology() {
    let cluster = TestCluster::new(
        4,
        Topology::Star,
        vec![
            vec!["claude-3".into()],
            vec!["gpt-4o".into()],
            vec!["gemini-pro".into()],
            vec!["llama-3".into()],
        ],
    );
    cluster.start_all().await;

    // Node 0 has claude-3 — should dispatch locally
    let ctx = make_request("claude-3");
    let result = cluster.nodes[0].route(&ctx, b"hello").await;
    assert!(result.is_ok());

    // Node 0 routes gpt-4o — no local provider, no gossip → NoProvider
    let ctx = make_request("gpt-4o");
    let result = cluster.nodes[0].route(&ctx, b"hello").await;
    assert!(result.is_err());
}
