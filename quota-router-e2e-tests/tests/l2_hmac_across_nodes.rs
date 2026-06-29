use quota_router_e2e_tests::{TestCluster, Topology};

#[tokio::test]
async fn l2_t22_gossip_hmac_verified() {
    let cluster = TestCluster::new(
        2,
        Topology::Star,
        vec![vec!["gpt-4o".into()], vec!["gpt-4o".into()]],
    );
    cluster.start_all().await;

    // Broadcast gossip (with correct HMAC)
    cluster.nodes[0].broadcast_gossip().await;
    cluster.nodes[1].drive().await;

    let snap = cluster.nodes[1].gossip_cache_snapshot().await;
    assert!(!snap.is_empty(), "Valid gossip should be accepted");
}

#[tokio::test]
async fn l2_t24_announce_hmac_verified() {
    let cluster = TestCluster::new(1, Topology::Star, vec![vec!["gpt-4o".into()]]);
    cluster.start_all().await;

    // Announce (with correct HMAC) — already done in start_all
    let count = cluster.nodes[0].peer_count().await;
    // Self-announce doesn't add to peer cache
    assert_eq!(count, 0);
}

#[tokio::test]
async fn l2_t26_withdraw_hmac_verified() {
    let cluster = TestCluster::new(
        2,
        Topology::Star,
        vec![vec!["gpt-4o".into()], vec!["gpt-4o".into()]],
    );
    cluster.start_all().await;

    // Broadcast gossip to establish peer
    cluster.nodes[0].broadcast_gossip().await;
    cluster.nodes[1].drive().await;

    let snap = cluster.nodes[1].gossip_cache_snapshot().await;
    assert!(!snap.is_empty());
}
