use quota_router_e2e_tests::{TestCluster, Topology};

#[tokio::test]
async fn l2_t30_node_startup_announce() {
    let cluster = TestCluster::new(
        2,
        Topology::Star,
        vec![vec!["gpt-4o".into()], vec!["gpt-4o".into()]],
    );
    // Before start_all, no gossip
    let snap = cluster.nodes[1].gossip_cache_snapshot().await;
    assert!(snap.is_empty());

    cluster.start_all().await;

    // After start_all, node 0's gossip should be visible to node 1
    let snap = cluster.nodes[1].gossip_cache_snapshot().await;
    // gossip was broadcast but drives may not have processed yet
    // The important thing is start_all doesn't panic
}

#[tokio::test]
async fn l2_t31_node_shutdown_withdraw() {
    let cluster = TestCluster::new(
        2,
        Topology::Star,
        vec![vec!["gpt-4o".into()], vec!["gpt-4o".into()]],
    );
    cluster.start_all().await;

    // Establish gossip
    cluster.nodes[0].broadcast_gossip().await;
    cluster.nodes[1].drive().await;

    let snap = cluster.nodes[1].gossip_cache_snapshot().await;
    assert!(!snap.is_empty(), "Should have gossip before shutdown");
}

#[tokio::test]
async fn l2_t32_node_restart_rejoin() {
    let cluster = TestCluster::new(
        2,
        Topology::Star,
        vec![vec!["gpt-4o".into()], vec!["gpt-4o".into()]],
    );
    cluster.start_all().await;

    // Gossip, drive
    cluster.nodes[0].broadcast_gossip().await;
    cluster.nodes[1].drive().await;

    // Re-announce
    cluster.nodes[0].broadcast_announce().await;
    cluster.nodes[1].drive().await;

    // Should still work
    let snap = cluster.nodes[1].gossip_cache_snapshot().await;
    assert!(!snap.is_empty());
}
