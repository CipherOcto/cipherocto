use std::time::Duration;

use quota_router_integration_tests::TestCluster;

/// T30 — node_startup_announce
/// After start_all, nodes should have discovered each other via announce.
#[tokio::test]
async fn l2_t30_node_startup_announce() {
    let cluster = TestCluster::new(2, vec![vec!["gpt-4o".into()], vec!["gpt-4o".into()]]);

    // Before start_all, no gossip
    let snap = cluster.nodes[1].gossip_cache_snapshot().await;
    assert!(snap.is_empty(), "no gossip before start");

    cluster.start_all().await;
    cluster.wait_converged(Duration::from_millis(100)).await;

    // After start_all, node 1 should know about node 0
    let snap = cluster.nodes[1].gossip_cache_snapshot().await;
    assert!(
        !snap.is_empty(),
        "gossip cache should have entries after start_all"
    );
}

/// T31 — node_shutdown_withdraw
/// When a withdraw is processed, the peer is removed from cache.
#[tokio::test]
async fn l2_t31_node_shutdown_withdraw() {
    let cluster = TestCluster::new(2, vec![vec!["gpt-4o".into()], vec!["gpt-4o".into()]]);
    cluster.start_all().await;
    cluster.wait_converged(Duration::from_millis(100)).await;

    // Establish peer relationship
    assert!(
        cluster.nodes[1].peer_count().await >= 1,
        "should have peer after gossip"
    );
    let count_before = cluster.nodes[1].peer_count().await;

    // Build and inject a withdraw for node 0
    use quota_router_core::node::announce::{RouterWithdrawPayload, SignedPayload, WithdrawReason};
    use quota_router_core::node::gossip::monotonic_now;
    use quota_router_core::node::provider::RouterNodeId;

    let mut withdraw = RouterWithdrawPayload {
        node_id: RouterNodeId([1u8; 32]),
        reason: WithdrawReason::Graceful,
        timestamp: monotonic_now(),
        hmac: [0u8; 32],
    };
    withdraw.hmac = withdraw.compute_hmac(&cluster.network_key);
    let body = bincode::serialize(&withdraw).unwrap();
    let framed = {
        let mut out = vec![0xCBu8];
        out.extend_from_slice(&body);
        out
    };

    cluster.inject(RouterNodeId([2u8; 32]), RouterNodeId([1u8; 32]), framed);
    tokio::time::sleep(Duration::from_millis(20)).await;
    cluster.drive_all().await;

    let count_after = cluster.nodes[1].peer_count().await;
    assert!(
        count_after < count_before,
        "withdraw should remove peer: before={}, after={}",
        count_before,
        count_after
    );
}

/// T32 — node_restart_rejoin
/// After a node re-announces, the peer should be re-discovered.
#[tokio::test]
async fn l2_t32_node_restart_rejoin() {
    let cluster = TestCluster::new(2, vec![vec!["gpt-4o".into()], vec!["gpt-4o".into()]]);
    cluster.start_all().await;
    cluster.wait_converged(Duration::from_millis(100)).await;

    // Establish peer
    assert!(cluster.nodes[1].peer_count().await >= 1);
    let count_before = cluster.nodes[1].peer_count().await;

    // Withdraw node 0
    use quota_router_core::node::announce::{RouterWithdrawPayload, SignedPayload, WithdrawReason};
    use quota_router_core::node::gossip::monotonic_now;
    use quota_router_core::node::provider::RouterNodeId;

    let mut withdraw = RouterWithdrawPayload {
        node_id: RouterNodeId([1u8; 32]),
        reason: WithdrawReason::Graceful,
        timestamp: monotonic_now(),
        hmac: [0u8; 32],
    };
    withdraw.hmac = withdraw.compute_hmac(&cluster.network_key);
    let body = bincode::serialize(&withdraw).unwrap();
    let framed = {
        let mut out = vec![0xCBu8];
        out.extend_from_slice(&body);
        out
    };
    cluster.inject(RouterNodeId([2u8; 32]), RouterNodeId([1u8; 32]), framed);
    tokio::time::sleep(Duration::from_millis(20)).await;
    cluster.drive_all().await;

    // Verify node 0 was removed (count decreased)
    let count_after_withdraw = cluster.nodes[1].peer_count().await;
    assert!(
        count_after_withdraw < count_before,
        "node 0 should be removed after withdraw: before={}, after={}",
        count_before,
        count_after_withdraw
    );

    // Re-announce node 0
    cluster.nodes[0].broadcast_announce().await;
    tokio::time::sleep(Duration::from_millis(20)).await;
    cluster.drive_all().await;

    // Node 1 should rediscover node 0 (count increases)
    let count_after_rejoin = cluster.nodes[1].peer_count().await;
    assert!(
        count_after_rejoin > count_after_withdraw,
        "node 0 should be re-discovered after re-announce: after_withdraw={}, after_rejoin={}",
        count_after_withdraw,
        count_after_rejoin
    );
}
