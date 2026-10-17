use quota_router_integration_tests::TestCluster;

#[tokio::test]

async fn l2_t15_gossip_propagation() {
    let cluster = TestCluster::new(2, vec![vec!["gpt-4o".into()], vec!["gpt-4o".into()]]);
    cluster.start_all().await;

    // Node 0 broadcasts gossip
    cluster.nodes[0].broadcast_gossip().await;
    // Give time for message delivery
    tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    // Drive node 1 to process the gossip
    cluster.nodes[1].drive().await;

    let snap = cluster.nodes[1].gossip_cache_snapshot().await;
    assert!(
        !snap.is_empty(),
        "Node 1 should have gossip from Node 0, peers={:?}",
        cluster.nodes.len()
    );
}

#[tokio::test]

async fn l2_t17_three_node_gossip_convergence() {
    let cluster = TestCluster::new(
        3,
        vec![
            vec!["gpt-4o".into()],
            vec!["claude-3".into()],
            vec!["gemini-pro".into()],
        ],
    );
    cluster.start_all().await;

    // All nodes broadcast gossip
    cluster.broadcast_all_gossip().await;
    tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    cluster.drive_all().await;
    tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    cluster.drive_all().await;

    // Each node should know about others' providers
    for i in 0..3 {
        let snap = cluster.nodes[i].gossip_cache_snapshot().await;
        assert!(!snap.is_empty(), "Node {} should have gossip from peers", i);
    }
}

#[tokio::test]

async fn l2_t18_gossip_capacity_update() {
    let cluster = TestCluster::new(2, vec![vec!["gpt-4o".into()], vec!["gpt-4o".into()]]);
    cluster.start_all().await;

    // Initial gossip
    cluster.nodes[0].broadcast_gossip().await;
    tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    cluster.nodes[1].drive().await;

    let snap1 = cluster.nodes[1].gossip_cache_snapshot().await;
    let initial_count = snap1.len();

    // Second gossip
    cluster.nodes[0].broadcast_gossip().await;
    tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    cluster.nodes[1].drive().await;

    let snap2 = cluster.nodes[1].gossip_cache_snapshot().await;
    assert!(snap2.len() >= initial_count);
}
