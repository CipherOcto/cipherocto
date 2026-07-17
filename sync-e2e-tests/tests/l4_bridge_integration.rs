//! L4: Bridge integration tests — DRS/DOM/ORR through the full transport chain.
//!
//! Exercises: bridge → NodeTransport → RecordingSender → verify payload bytes.

use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use parking_lot::Mutex as PMutex;

use octo_network::dom::OverlayIntent;
use octo_network::dot::gateway::{GatewayClass, GatewayIdentity};
use octo_network::drs::DeterministicRoute;
use octo_network::gdp::identity::GdpGatewayIdentity;
use octo_network::gdp::overlay_endpoint::OverlayEndpoint;
use octo_network::gdp::types::GatewayCapability;
use octo_network::orr::PeeledLayer;
use octo_network::sync::TransportBroadcaster;
use octo_transport::broadcaster::NodeTransportBroadcaster;
use octo_transport::discovery::TransportDiscovery;
use octo_transport::dom_bridge::DomTransportBridge;
use octo_transport::drs_bridge::DrsTransportBridge;
use octo_transport::node_transport::NodeTransport;
use octo_transport::orr_bridge::OrrTransportBridge;
use octo_transport::sender::{NetworkSender, SendContext, TransportError};

// ─── Shared test infrastructure ─────────────────────────────────────

struct RecordingSender {
    name: String,
    payloads: PMutex<Vec<Vec<u8>>>,
}

impl RecordingSender {
    fn new(name: &str) -> Self {
        Self {
            name: name.to_string(),
            payloads: PMutex::new(Vec::new()),
        }
    }

    fn last_payload(&self) -> Option<Vec<u8>> {
        self.payloads.lock().last().cloned()
    }

    fn payload_count(&self) -> usize {
        self.payloads.lock().len()
    }
}

#[async_trait]
impl NetworkSender for RecordingSender {
    async fn send(&self, payload: &[u8], _ctx: &SendContext) -> Result<(), TransportError> {
        self.payloads.lock().push(payload.to_vec());
        Ok(())
    }

    fn name(&self) -> &str {
        &self.name
    }

    fn is_healthy(&self) -> bool {
        true
    }
}

fn make_identity() -> GdpGatewayIdentity {
    GdpGatewayIdentity::new(GatewayIdentity::new(
        [0x42u8; 32],
        1,
        GatewayClass::Edge,
        100,
    ))
}

fn make_discovery() -> Arc<Mutex<TransportDiscovery>> {
    Arc::new(Mutex::new(TransportDiscovery::new(
        make_identity(),
        [0xABu8; 32],
        100,
    )))
}

fn make_senders() -> (Vec<Arc<dyn NetworkSender>>, Vec<Arc<RecordingSender>>) {
    let r1 = Arc::new(RecordingSender::new("webhook"));
    let r2 = Arc::new(RecordingSender::new("quic"));
    let senders: Vec<Arc<dyn NetworkSender>> = vec![r1.clone(), r2.clone()];
    (senders, vec![r1, r2])
}

fn make_ctx(mission: u8) -> SendContext {
    SendContext {
        mission_id: [mission; 32],
        priority: 0,
        source_peer: [0x01; 32],
        origin_gateway: [0x02; 32],
    }
}

fn make_route() -> DeterministicRoute {
    DeterministicRoute {
        route_id: [0xAA; 32],
        source_gateway: [0x01; 32],
        destination_gateway: [0x02; 32],
        next_hop: [0x03; 32],
        transport_vector_root: [0u8; 32],
        trust_score: 500,
        bandwidth_class: 100,
        latency_class: 50,
        censorship_resistance_class: 200,
        route_cost: 1000,
        route_epoch: 100,
        valid_until_epoch: 0,
        ttl_hops: 10,
        signature: [0u8; 64],
    }
}

fn make_intent() -> OverlayIntent {
    OverlayIntent {
        intent_id: [0x01; 32],
        intent_type: 0x0001,
        mission_id: [0xAB; 32],
        sender_id: [0x02; 32],
        sequence: 1,
        logical_timestamp: 1000,
        expiration: 2000,
        payload_root: [0x03; 32],
        economic_weight: 5000,
        execution_class: 0x0002,
        signature: [0x04; 64],
    }
}

fn make_peeled() -> PeeledLayer {
    PeeledLayer {
        next_gateway: [0x05; 32],
        transport: octo_network::orr::TransportVector {
            transport_type: 0x0009,
            domain_id: [0u8; 32],
            priority: 100,
            bandwidth_class: 0,
            censorship_score: 0,
        },
        inner_payload: b"encrypted-hop-data".to_vec(),
        hop_index: 1,
    }
}

// ─── DRS Bridge Tests ───────────────────────────────────────────────

#[tokio::test]
async fn drs_resolve_and_send_delivers_to_recording_sender() {
    let (senders, records) = make_senders();
    let transport = Arc::new(NodeTransport::new(senders));
    let bridge = DrsTransportBridge::new(transport, make_discovery());

    let route = make_route();
    let ctx = make_ctx(0xAA);
    let payload = b"drs-route-payload";

    bridge
        .resolve_and_send(&route, payload, &ctx)
        .await
        .unwrap();

    // send_best picks first healthy sender — verify it received the exact payload
    let received = records[0].last_payload().unwrap();
    assert_eq!(received, payload);
}

#[tokio::test]
async fn drs_broadcast_reaches_all_senders() {
    let (senders, records) = make_senders();
    let transport = Arc::new(NodeTransport::new(senders));
    let bridge = DrsTransportBridge::new(transport, make_discovery());

    let ctx = make_ctx(0xBB);
    let count = bridge.broadcast(b"drs-broadcast", &ctx).await;

    assert_eq!(count, 2);
    assert_eq!(records[0].last_payload().unwrap(), b"drs-broadcast");
    assert_eq!(records[1].last_payload().unwrap(), b"drs-broadcast");
}

#[test]
fn drs_transport_available_queries_discovery() {
    let (senders, _) = make_senders();
    let discovery = make_discovery();
    let bridge = DrsTransportBridge::new(Arc::new(NodeTransport::new(senders)), discovery.clone());

    // Empty discovery — no peers known
    assert!(!bridge.transport_available(0x0009));

    // Register a peer that supports webhook transport
    {
        let disc = discovery.lock().unwrap();
        let entry = octo_network::gdp::cache::GatewayCacheEntry {
            advertisement_hash: [0x55; 32],
            first_seen: 1000,
            last_seen: 1000,
            trust_score: 500,
            identity: GatewayIdentity {
                gateway_id: [0x77; 32],
                public_key: [0x77; 32],
                network_id: 1,
                gateway_class: GatewayClass::Edge,
                creation_epoch: 1000,
                supported_platforms: 0,
                capabilities: 0,
            },
            capabilities: vec![GatewayCapability::Relay],
            endpoints: vec![OverlayEndpoint {
                transport_type: 0x0009,
                endpoint_hash: [0u8; 32],
                priority: 100,
                bandwidth_class: 0,
                flags: 0,
            }],
        };
        disc.cache_insert(entry, 1000);
    }

    assert!(bridge.transport_available(0x0009));
    assert!(!bridge.transport_available(0x000B));
}

#[tokio::test]
async fn drs_failover_skips_unhealthy_sender() {
    struct UnhealthySender;

    #[async_trait]
    impl NetworkSender for UnhealthySender {
        async fn send(&self, _: &[u8], _: &SendContext) -> Result<(), TransportError> {
            Err(TransportError::Unhealthy)
        }
        fn name(&self) -> &str {
            "unhealthy"
        }
        fn is_healthy(&self) -> bool {
            false
        }
    }

    let healthy = Arc::new(RecordingSender::new("backup"));
    let transport = Arc::new(NodeTransport::new(vec![
        Arc::new(UnhealthySender) as Arc<dyn NetworkSender>,
        healthy.clone() as Arc<dyn NetworkSender>,
    ]));
    let bridge = DrsTransportBridge::new(transport, make_discovery());
    let ctx = make_ctx(0xCC);

    let result = bridge
        .resolve_and_send(&make_route(), b"failover-data", &ctx)
        .await;
    assert!(result.is_ok());
    assert_eq!(healthy.last_payload().unwrap(), b"failover-data");
}

// ─── DOM Bridge Tests ───────────────────────────────────────────────

#[tokio::test]
async fn dom_broadcast_intent_delivers_to_recording_sender() {
    let (senders, records) = make_senders();
    let transport = Arc::new(NodeTransport::new(senders));
    let broadcaster = Arc::new(NodeTransportBroadcaster::new(transport));
    let bridge = DomTransportBridge::new(broadcaster);

    let intent = make_intent();
    let mission_id = [0xAB; 32];

    bridge.broadcast_intent(&intent, &mission_id).await.unwrap();

    // Verify payload arrives at recording senders via broadcast
    let received0 = records[0].last_payload().unwrap();
    let received1 = records[1].last_payload().unwrap();

    // Both senders should get the same payload
    assert_eq!(received0, received1);

    // Payload starts with the DGP object type header 0x0009
    let object_type = u16::from_le_bytes([received0[0], received0[1]]);
    assert_eq!(object_type, 0x0009);
}

#[tokio::test]
async fn dom_intent_object_bytes_matches_signing_bytes() {
    let intent = make_intent();
    let bytes = DomTransportBridge::intent_object_bytes(&intent);
    let signing = intent.to_signing_bytes();

    // Object bytes = 2-byte header + signing bytes
    assert_eq!(bytes.len(), 2 + signing.len());
    assert_eq!(&bytes[2..], &signing);
}

#[tokio::test]
async fn dom_broadcast_propagation_failure() {
    struct FailBroadcaster;

    #[async_trait]
    impl TransportBroadcaster for FailBroadcaster {
        async fn broadcast(&self, _: &[u8], _: &[u8; 32]) -> Result<(), std::io::Error> {
            Err(std::io::Error::new(
                std::io::ErrorKind::ConnectionRefused,
                "mock",
            ))
        }
    }

    let bridge = DomTransportBridge::new(Arc::new(FailBroadcaster));
    let result = bridge.broadcast_intent(&make_intent(), &[0xAB; 32]).await;
    assert!(result.is_err());
}

#[tokio::test]
async fn dom_multiple_intents_sequential_broadcast() {
    let (senders, records) = make_senders();
    let transport = Arc::new(NodeTransport::new(senders));
    let broadcaster = Arc::new(NodeTransportBroadcaster::new(transport));
    let bridge = DomTransportBridge::new(broadcaster);

    let mission_id = [0xCD; 32];

    for seq in 1..=5u64 {
        let mut intent = make_intent();
        intent.sequence = seq;
        intent.logical_timestamp = 1000 + seq;
        bridge.broadcast_intent(&intent, &mission_id).await.unwrap();
    }

    // Each sender should have 5 payloads
    assert_eq!(records[0].payload_count(), 5);
    assert_eq!(records[1].payload_count(), 5);

    // Each payload should start with 0x0009
    for record in &records {
        for payload in record.payloads.lock().iter() {
            let ot = u16::from_le_bytes([payload[0], payload[1]]);
            assert_eq!(ot, 0x0009);
        }
    }
}

// ─── ORR Bridge Tests ───────────────────────────────────────────────

#[tokio::test]
async fn orr_forward_hop_delivers_inner_payload() {
    let (senders, records) = make_senders();
    let transport = Arc::new(NodeTransport::new(senders));
    let bridge = OrrTransportBridge::new(transport, make_discovery());

    let peeled = make_peeled();
    let mission_id = [0xDE; 32];

    bridge.forward_hop(&peeled, &mission_id).await.unwrap();

    // send_best picks first sender
    let received = records[0].last_payload().unwrap();
    assert_eq!(received, b"encrypted-hop-data");
}

#[tokio::test]
async fn orr_forward_hop_sets_next_gateway_as_source() {
    let (senders, _) = make_senders();
    let transport = Arc::new(NodeTransport::new(senders));
    let bridge = OrrTransportBridge::new(transport, make_discovery());

    let mut peeled = make_peeled();
    peeled.next_gateway = [0xAA; 32];
    let mission_id = [0xBF; 32];

    let result = bridge.forward_hop(&peeled, &mission_id).await;
    assert!(result.is_ok());
}

#[tokio::test]
async fn orr_forward_hop_empty_payload() {
    let (senders, records) = make_senders();
    let transport = Arc::new(NodeTransport::new(senders));
    let bridge = OrrTransportBridge::new(transport, make_discovery());

    let mut peeled = make_peeled();
    peeled.inner_payload = vec![];
    peeled.hop_index = 0;

    bridge.forward_hop(&peeled, &[0xCE; 32]).await.unwrap();

    // Empty payload still gets sent
    assert_eq!(records[0].payload_count(), 1);
    assert!(records[0].last_payload().unwrap().is_empty());
}

#[tokio::test]
async fn orr_transport_supported_queries_discovery() {
    let (senders, _) = make_senders();
    let discovery = make_discovery();
    let bridge = OrrTransportBridge::new(Arc::new(NodeTransport::new(senders)), discovery.clone());

    assert!(!bridge.transport_supported(0x0009));

    // Register peer with WebRTC transport
    {
        let disc = discovery.lock().unwrap();
        let entry = octo_network::gdp::cache::GatewayCacheEntry {
            advertisement_hash: [0x88; 32],
            first_seen: 500,
            last_seen: 500,
            trust_score: 800,
            identity: GatewayIdentity {
                gateway_id: [0x99; 32],
                public_key: [0x99; 32],
                network_id: 1,
                gateway_class: GatewayClass::Edge,
                creation_epoch: 500,
                supported_platforms: 0,
                capabilities: 0,
            },
            capabilities: vec![GatewayCapability::Relay],
            endpoints: vec![OverlayEndpoint {
                transport_type: 0x000D, // WebRTC
                endpoint_hash: [0u8; 32],
                priority: 200,
                bandwidth_class: 0,
                flags: 0,
            }],
        };
        disc.cache_insert(entry, 500);
    }

    assert!(bridge.transport_supported(0x000D));
    assert!(!bridge.transport_supported(0x000B));
}

#[tokio::test]
async fn orr_forward_hop_all_senders_unhealthy() {
    struct UnhealthySender;

    #[async_trait]
    impl NetworkSender for UnhealthySender {
        async fn send(&self, _: &[u8], _: &SendContext) -> Result<(), TransportError> {
            Err(TransportError::Unhealthy)
        }
        fn name(&self) -> &str {
            "dead"
        }
        fn is_healthy(&self) -> bool {
            false
        }
    }

    let transport = Arc::new(NodeTransport::new(vec![
        Arc::new(UnhealthySender) as Arc<dyn NetworkSender>
    ]));
    let bridge = OrrTransportBridge::new(transport, make_discovery());

    let result = bridge.forward_hop(&make_peeled(), &[0xFF; 32]).await;
    assert!(result.is_err());
}
