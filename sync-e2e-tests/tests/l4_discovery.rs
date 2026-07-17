//! L4: Transport discovery integration tests.
//!
//! Verifies that TransportDiscovery correctly builds advertisements,
//! manages peer cache, and provides transport-type queries.

use octo_network::dot::gateway::{GatewayClass, GatewayIdentity};
use octo_network::gdp::identity::GdpGatewayIdentity;
use octo_network::gdp::overlay_endpoint::OverlayEndpoint;
use octo_network::gdp::types::GatewayCapability;
use octo_transport::discovery::TransportDiscovery;
use octo_transport::node_transport::NodeTransport;
use octo_transport::sender::{NetworkSender, SendContext, TransportError};

use async_trait::async_trait;
use std::sync::Arc;

struct MockSender {
    name: String,
    healthy: bool,
}

#[async_trait]
impl NetworkSender for MockSender {
    async fn send(&self, _p: &[u8], _c: &SendContext) -> Result<(), TransportError> {
        Ok(())
    }
    fn name(&self) -> &str {
        &self.name
    }
    fn is_healthy(&self) -> bool {
        self.healthy
    }
}

fn make_identity(node_id: [u8; 32]) -> GdpGatewayIdentity {
    let base = GatewayIdentity::new(node_id, 1, GatewayClass::Edge, 1);
    GdpGatewayIdentity::new(base)
}

#[test]
fn l4_build_advertisement_from_transport() {
    let disc = TransportDiscovery::new(make_identity([0x42u8; 32]), [0xABu8; 32], 100);
    let transport = NodeTransport::new(vec![
        Arc::new(MockSender {
            name: "webhook".into(),
            healthy: true,
        }) as Arc<dyn NetworkSender>,
        Arc::new(MockSender {
            name: "quic".into(),
            healthy: true,
        }) as Arc<dyn NetworkSender>,
    ]);

    let adv = disc.build_advertisement(&transport, 1, 1000);
    assert_eq!(adv.version, 1);
    assert_eq!(adv.overlay_endpoints.len(), 2);
    assert_eq!(adv.network_id, 1);
    assert_eq!(adv.sequence, 1);
}

#[test]
fn l4_build_advertisement_from_identity_only() {
    let disc = TransportDiscovery::new(make_identity([0x99u8; 32]), [0xCDu8; 32], 100);
    let adv = disc.build_advertisement_from_identity(5000);
    assert_eq!(adv.version, 1);
    assert_eq!(adv.gateway_id, disc.identity().gateway_id());
    assert!(adv.overlay_endpoints.is_empty());
    assert_eq!(adv.sequence, 1);
}

#[test]
fn l4_cache_insert_and_lookup() {
    let disc = TransportDiscovery::new(make_identity([0x42u8; 32]), [0xABu8; 32], 100);
    let entry = octo_network::gdp::cache::GatewayCacheEntry {
        advertisement_hash: [0x55u8; 32],
        first_seen: 1000,
        last_seen: 1000,
        trust_score: 500,
        identity: GatewayIdentity {
            gateway_id: [0x77u8; 32],
            public_key: [0x77u8; 32],
            network_id: 1,
            gateway_class: GatewayClass::Edge,
            creation_epoch: 1000,
            supported_platforms: 0,
            capabilities: 0,
        },
        capabilities: vec![GatewayCapability::Relay],
        endpoints: vec![OverlayEndpoint {
            transport_type: 10,
            endpoint_hash: [0u8; 32],
            priority: 100,
            bandwidth_class: 0,
            flags: 0,
        }],
    };
    disc.cache_insert(entry, 1000);

    assert_eq!(disc.peer_count(), 1);
    assert!(disc.peer_supports_transport(&[0x77u8; 32], 10));
    assert!(!disc.peer_supports_transport(&[0x77u8; 32], 20));
}

#[test]
fn l4_cache_entries_returns_snapshot() {
    let disc = TransportDiscovery::new(make_identity([0x42u8; 32]), [0xABu8; 32], 100);

    for i in 0..3u8 {
        let entry = octo_network::gdp::cache::GatewayCacheEntry {
            advertisement_hash: [i; 32],
            first_seen: 1000,
            last_seen: 1000,
            trust_score: 500,
            identity: GatewayIdentity {
                gateway_id: [i + 0x10; 32],
                public_key: [i + 0x10; 32],
                network_id: 1,
                gateway_class: GatewayClass::Edge,
                creation_epoch: 1000,
                supported_platforms: 0,
                capabilities: 0,
            },
            capabilities: vec![],
            endpoints: vec![],
        };
        disc.cache_insert(entry, 1000);
    }

    let entries = disc.cache_entries();
    assert_eq!(entries.len(), 3);
}

#[test]
fn l4_sequence_increments_across_builds() {
    let disc = TransportDiscovery::new(make_identity([0x42u8; 32]), [0xABu8; 32], 100);
    let transport = NodeTransport::new(vec![Arc::new(MockSender {
        name: "webhook".into(),
        healthy: true,
    }) as Arc<dyn NetworkSender>]);

    let a1 = disc.build_advertisement(&transport, 1, 1000);
    let a2 = disc.build_advertisement(&transport, 1, 1001);
    let a3 = disc.build_advertisement_from_identity(1002);
    assert_eq!(a1.sequence, 1);
    assert_eq!(a2.sequence, 2);
    assert_eq!(a3.sequence, 3);
}

#[test]
fn l4_peers_with_transport_type() {
    let disc = TransportDiscovery::new(make_identity([0x42u8; 32]), [0xABu8; 32], 100);

    let entry_a = octo_network::gdp::cache::GatewayCacheEntry {
        advertisement_hash: [1u8; 32],
        first_seen: 1000,
        last_seen: 1000,
        trust_score: 500,
        identity: GatewayIdentity {
            gateway_id: [0xAAu8; 32],
            public_key: [0xAAu8; 32],
            network_id: 1,
            gateway_class: GatewayClass::Edge,
            creation_epoch: 1000,
            supported_platforms: 0,
            capabilities: 0,
        },
        capabilities: vec![],
        endpoints: vec![OverlayEndpoint {
            transport_type: 5,
            endpoint_hash: [0u8; 32],
            priority: 100,
            bandwidth_class: 0,
            flags: 0,
        }],
    };
    let entry_b = octo_network::gdp::cache::GatewayCacheEntry {
        advertisement_hash: [2u8; 32],
        first_seen: 1000,
        last_seen: 1000,
        trust_score: 500,
        identity: GatewayIdentity {
            gateway_id: [0xBBu8; 32],
            public_key: [0xBBu8; 32],
            network_id: 1,
            gateway_class: GatewayClass::Edge,
            creation_epoch: 1000,
            supported_platforms: 0,
            capabilities: 0,
        },
        capabilities: vec![],
        endpoints: vec![OverlayEndpoint {
            transport_type: 10,
            endpoint_hash: [0u8; 32],
            priority: 100,
            bandwidth_class: 0,
            flags: 0,
        }],
    };
    disc.cache_insert(entry_a, 1000);
    disc.cache_insert(entry_b, 1000);

    let t5_peers = disc.peers_with_transport(5);
    assert_eq!(t5_peers.len(), 1);
    assert_eq!(t5_peers[0], [0xAAu8; 32]);

    let t10_peers = disc.peers_with_transport(10);
    assert_eq!(t10_peers.len(), 1);
    assert_eq!(t10_peers[0], [0xBBu8; 32]);

    let t99_peers = disc.peers_with_transport(99);
    assert!(t99_peers.is_empty());
}
