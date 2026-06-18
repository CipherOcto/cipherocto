//! Integration tests for DGP gossip propagation.

mod common;

use common::mock_network::MockNetwork;
use octo_network::dgp::domain::{GossipDomainId, GossipScope};
use octo_network::dgp::object::{GossipObjectType, FLAG_FLOOD, FLAG_INCREMENTAL};
use octo_network::dgp::{sort_canonical, DedupSet, FloodMode, GossipObject, IncrementalMode};
use octo_network::dot::adapters::PlatformAdapter;

fn make_gossip_obj(hash_byte: u8, ts: u64, flags: u64) -> GossipObject {
    GossipObject {
        object_type: GossipObjectType::Envelope as u16,
        object_hash: [hash_byte; 32],
        object_size: 100,
        domain_id: GossipDomainId::new(1, [0u8; 32], GossipScope::GLOBAL),
        logical_timestamp: ts,
        origin_gateway: [0x01; 32],
        ttl_hops: 20,
        propagation_flags: flags,
        payload_root: [0u8; 32],
        signature: [0u8; 64],
    }
}

#[test]
fn test_gossip_flood_eligible() {
    let obj_a = make_gossip_obj(0xAA, 1000, FLAG_FLOOD);
    let obj_b = make_gossip_obj(0xBB, 2000, FLAG_FLOOD);
    let obj_c = make_gossip_obj(0xCC, 3000, FLAG_INCREMENTAL);

    let all = vec![obj_a, obj_b, obj_c];
    let eligible = FloodMode::eligible(&all);
    assert_eq!(eligible.len(), 2);
}

#[test]
fn test_gossip_deduplication() {
    let mut dedup = DedupSet::new(1000);
    let obj = make_gossip_obj(0xAA, 1000, FLAG_FLOOD);

    assert!(dedup.insert_if_new(obj.object_hash));
    assert!(!dedup.insert_if_new(obj.object_hash));
    assert_eq!(dedup.len(), 1);
}

#[test]
fn test_gossip_canonical_ordering() {
    let obj_c = make_gossip_obj(0xCC, 3000, FLAG_FLOOD);
    let obj_a = make_gossip_obj(0xAA, 1000, FLAG_FLOOD);
    let obj_b = make_gossip_obj(0xBB, 2000, FLAG_FLOOD);

    let mut objects = vec![obj_c, obj_a, obj_b];
    sort_canonical(&mut objects);

    assert_eq!(objects[0].object_hash[0], 0xAA);
    assert_eq!(objects[1].object_hash[0], 0xBB);
    assert_eq!(objects[2].object_hash[0], 0xCC);
}

#[test]
fn test_gossip_ttl_expiry() {
    let mut obj = make_gossip_obj(0xAA, 1000, FLAG_FLOOD);
    obj.ttl_hops = 1;
    assert!(obj.ttl_hops > 0);

    obj.decrement_ttl();
    assert_eq!(obj.ttl_hops, 0);

    let objects = vec![obj];
    let eligible = FloodMode::eligible(&objects);
    assert!(eligible.is_empty());
}

#[test]
fn test_incremental_mode_unseen_only() {
    let obj_a = make_gossip_obj(0xAA, 1000, FLAG_INCREMENTAL);
    let obj_b = make_gossip_obj(0xBB, 2000, FLAG_INCREMENTAL);
    let obj_c = make_gossip_obj(0xCC, 3000, FLAG_FLOOD);

    let all = vec![obj_a, obj_b, obj_c];
    let eligible = IncrementalMode::eligible(&all);
    assert_eq!(eligible.len(), 2);

    let peer_seen = vec![[0xAA; 32]];
    let unseen = IncrementalMode::unseen_objects(&all, &peer_seen);
    assert_eq!(unseen.len(), 2);
}

#[tokio::test]
async fn test_multi_node_broadcast() {
    let net = MockNetwork::new(5);
    let envelope = MockNetwork::make_envelope([0xDD; 32], 1, [0x01; 32], 5000);
    net.broadcast(0, &envelope).await;

    assert_eq!(net.pending_count().await, 4);
    net.deliver_all().await;
    assert_eq!(net.pending_count().await, 0);

    for i in 1..5 {
        let domain = net.gateways[i].adapter.domain_id("test");
        let msgs = net.gateways[i]
            .adapter
            .receive_messages(&domain)
            .await
            .unwrap();
        assert_eq!(msgs.len(), 1, "node {} should receive 1 message", i);
    }
}
