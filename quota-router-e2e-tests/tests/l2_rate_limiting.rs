use quota_router_e2e_tests::{make_request, TestCluster};

/// T28 — rate_limit_local_dispatch
/// Consumer sends enough requests to exceed the per-consumer rate limit.
/// Default RateLimiter has max_sustained=100, max_burst=500. Sending 600
/// requests in a tight loop should trigger rate limiting after the burst
/// is exhausted (bucket doesn't refill fast enough in a tight loop).
#[tokio::test]
async fn l2_t28_rate_limit_local_dispatch() {
    let cluster = TestCluster::new(1, vec![vec!["gpt-4o".into()]]);
    cluster.start_all().await;

    let ctx = make_request("gpt-4o");

    let mut allowed = 0;
    let mut rate_limited = 0;
    // Send 800 requests — burst is 500, so after 500 the rate limiter
    // should start rejecting. The tight loop means no time for refill.
    // Using 800 (not 600) gives a 300-request margin for CI timing variance.
    for _ in 0..800 {
        match cluster.nodes[0].route(&ctx, b"hello").await {
            Ok(_) => allowed += 1,
            Err(quota_router::RouterNodeError::RateLimited) => rate_limited += 1,
            Err(e) => panic!("unexpected error: {:?}", e),
        }
    }

    assert!(
        allowed >= 1,
        "at least some requests should succeed within burst"
    );
    assert!(
        rate_limited >= 1,
        "rate limiting should trigger after burst exhausted: allowed={}, rate_limited={}",
        allowed,
        rate_limited
    );
}

/// T29 — rate_limit_forwarded_requests
/// Consumer sends forwarded requests that exceed the per-consumer limit.
#[tokio::test]
async fn l2_t29_rate_limit_forwarded_requests() {
    let cluster = TestCluster::new(
        2,
        vec![
            vec!["gpt-3.5-turbo".into()],
            vec!["gpt-3.5-turbo".into(), "gpt-4o".into()],
        ],
    );
    cluster.start_all().await;

    let ctx = make_request("gpt-4o");
    let mut allowed = 0;
    let mut rate_limited = 0;
    let mut other_errors = 0;
    for _ in 0..800 {
        match cluster.nodes[0].route(&ctx, b"hello").await {
            Ok(_) => allowed += 1,
            Err(quota_router::RouterNodeError::RateLimited) => rate_limited += 1,
            Err(_) => other_errors += 1,
        }
    }

    // Log non-rate-limit errors for debugging
    if other_errors > 0 {
        eprintln!(
            "T29: {} allowed, {} rate_limited, {} other errors",
            allowed, rate_limited, other_errors
        );
    }

    assert!(allowed >= 1, "at least one forward should succeed");
    assert!(
        rate_limited >= 1,
        "rate limiting should trigger on forwarded requests: allowed={}, rate_limited={}, other={}",
        allowed,
        rate_limited,
        other_errors
    );
}
