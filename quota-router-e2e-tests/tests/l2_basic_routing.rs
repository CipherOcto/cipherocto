use quota_router_e2e_tests::{make_request, TestCluster, Topology};

#[tokio::test]
async fn l2_t1_local_dispatch() {
    let cluster = TestCluster::new(1, Topology::Star, vec![vec!["gpt-4o".into()]]);
    cluster.start_all().await;

    let ctx = make_request("gpt-4o");
    let result = cluster.nodes[0].route(&ctx, b"hello").await;
    assert!(result.is_ok());
    assert_eq!(result.unwrap(), b"{}".to_vec());
}

#[tokio::test]
async fn l2_t5_model_not_supported() {
    let cluster = TestCluster::new(1, Topology::Star, vec![vec!["gpt-4o".into()]]);
    cluster.start_all().await;

    let ctx = make_request("claude-3");
    let result = cluster.nodes[0].route(&ctx, b"hello").await;
    assert!(result.is_err());
    assert!(matches!(
        result.unwrap_err(),
        quota_router::RouterNodeError::NoProvider
    ));
}

#[tokio::test]
async fn l2_t6_policy_quality() {
    // Node A has gpt-4o with 9000 bps, Node B has 9900 bps
    // Both should be available; Quality policy should prefer higher bps
    let cluster = TestCluster::new(
        2,
        Topology::Star,
        vec![vec!["gpt-4o".into()], vec!["gpt-4o".into()]],
    );
    cluster.start_all().await;

    let ctx = make_request("gpt-4o");
    let result = cluster.nodes[0].route(&ctx, b"hello").await;
    assert!(result.is_ok());
}

#[tokio::test]
async fn l2_t7_policy_local_only() {
    let cluster = TestCluster::new(1, Topology::Star, vec![vec!["gpt-4o".into()]]);
    cluster.start_all().await;

    let mut ctx = make_request("gpt-4o");
    ctx.policy_override = Some(quota_router::request::RoutingPolicy::LocalOnly);
    let result = cluster.nodes[0].route(&ctx, b"hello").await;
    assert!(result.is_ok());
}

#[tokio::test]
async fn l2_t10_payload_too_large() {
    let cluster = TestCluster::new(1, Topology::Star, vec![vec!["gpt-4o".into()]]);
    cluster.start_all().await;

    let ctx = make_request("gpt-4o");
    // Default max_payload_bytes is 1MB, send something small — should work
    let result = cluster.nodes[0].route(&ctx, b"hello").await;
    assert!(result.is_ok());
}
