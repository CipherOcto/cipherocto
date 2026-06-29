//! Integration tests for failure scenarios.

mod common;

use common::mock_adapter::{FailureMode, MockPlatformAdapter};
use common::mock_network::MockNetwork;
use octo_network::dgp::dedup::GossipReplayCache;
use octo_network::dot::adapters::PlatformAdapter;
use octo_network::dot::domain::PlatformType;

#[tokio::test]
async fn test_replay_attack_rejected() {
    let mut cache = GossipReplayCache::new(10000, 1_000_000);
    let envelope_id = [0xAA; 32];

    assert!(cache.check_and_insert(envelope_id, 100).unwrap());
    assert!(!cache.check_and_insert(envelope_id, 101).unwrap());
}

#[tokio::test]
async fn test_replay_cache_eviction() {
    let mut cache = GossipReplayCache::new(3, 1000);

    cache.check_and_insert([0x01; 32], 100).unwrap();
    cache.check_and_insert([0x02; 32], 200).unwrap();
    cache.check_and_insert([0x03; 32], 300).unwrap();
    assert_eq!(cache.len(), 3);

    cache.check_and_insert([0x04; 32], 400).unwrap();
    assert_eq!(cache.len(), 3);
}

#[tokio::test]
async fn test_drop_all_failure_mode() {
    let adapter =
        MockPlatformAdapter::new(PlatformType::NativeP2P).with_failure_mode(FailureMode::DropAll);

    let envelope = MockNetwork::make_envelope([0xAA; 32], 1, [0x01; 32], 1000);
    let domain = adapter.domain_id("test");
    let result = adapter.send_message(&domain, &envelope, b"test").await;

    assert!(result.is_err());
    assert_eq!(adapter.outbound_count().await, 0);
}

#[tokio::test]
async fn test_duplicate_failure_mode() {
    let adapter = MockPlatformAdapter::new(PlatformType::NativeP2P)
        .with_failure_mode(FailureMode::Duplicate(2));

    let envelope = MockNetwork::make_envelope([0xBB; 32], 1, [0x02; 32], 2000);
    let domain = adapter.domain_id("test");
    adapter.send_message(&domain, &envelope, b"test").await.unwrap();

    assert_eq!(adapter.outbound_count().await, 3);
}

#[tokio::test]
async fn test_reorder_failure_mode() {
    let adapter =
        MockPlatformAdapter::new(PlatformType::NativeP2P).with_failure_mode(FailureMode::Reorder);

    adapter.inject_message(vec![1, 2, 3]).await;
    adapter.inject_message(vec![4, 5, 6]).await;

    let domain = adapter.domain_id("test");
    let messages = adapter.receive_messages(&domain).await.unwrap();
    assert_eq!(messages.len(), 2);
    assert_eq!(messages[0].payload, vec![4, 5, 6]);
    assert_eq!(messages[1].payload, vec![1, 2, 3]);
}

#[tokio::test]
async fn test_multi_node_convergence() {
    let net = MockNetwork::new(3);

    let obj_a = MockNetwork::make_envelope([0x01; 32], 1, [0x01; 32], 1000);
    let obj_b = MockNetwork::make_envelope([0x02; 32], 1, [0x02; 32], 2000);
    let obj_c = MockNetwork::make_envelope([0x03; 32], 1, [0x03; 32], 3000);

    net.broadcast(0, &obj_a).await;
    net.broadcast(1, &obj_b).await;
    net.broadcast(2, &obj_c).await;

    net.deliver_all().await;

    for i in 0..3 {
        let domain = net.gateways[i].adapter.domain_id("test");
        let msgs = net.gateways[i]
            .adapter
            .receive_messages(&domain)
            .await
            .unwrap();
        assert_eq!(msgs.len(), 2, "node {} should receive 2 messages", i);
    }
}

#[tokio::test]
async fn test_empty_network() {
    let net = MockNetwork::new(0);
    assert_eq!(net.gateways.len(), 0);
    assert_eq!(net.pending_count().await, 0);
}
