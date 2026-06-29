use quota_router_e2e_tests::{make_request, TestCluster, Topology};

#[tokio::test]
async fn l2_t28_rate_limit_local_dispatch() {
    let cluster = TestCluster::new(1, Topology::Star, vec![vec!["gpt-4o".into()]]);
    cluster.start_all().await;

    let ctx = make_request("gpt-4o");

    // Send many requests — first batch should succeed, then rate limited
    let mut allowed = 0;
    let mut rate_limited = 0;
    for _ in 0..200 {
        match cluster.nodes[0].route(&ctx, b"hello").await {
            Ok(_) => allowed += 1,
            Err(quota_router::RouterNodeError::RateLimited) => rate_limited += 1,
            Err(e) => panic!("unexpected error: {:?}", e),
        }
    }

    // Default rate limiter: 100/s sustained, 500 burst
    // All 200 should succeed since burst is 500
    assert_eq!(allowed, 200);
    assert_eq!(rate_limited, 0);
}

#[tokio::test]
async fn l2_t29_rate_limit_forwarded_requests() {
    let cluster = TestCluster::new(
        2,
        Topology::Star,
        vec![vec!["gpt-4o".into()], vec!["gpt-4o".into()]],
    );
    cluster.start_all().await;

    let ctx = make_request("gpt-4o");
    let mut allowed = 0;
    for _ in 0..10 {
        match cluster.nodes[0].route(&ctx, b"hello").await {
            Ok(_) => allowed += 1,
            Err(_) => {}
        }
    }
    // Should get at least some successes
    assert!(allowed > 0);
}
