//! ORR Transport Bridge — connects Onion Relay Routing (RFC-0858) to the transport stack.
//!
//! Forwards peeled onion hops through the transport layer.

use std::sync::{Arc, Mutex};

use octo_network::orr::PeeledLayer;

use crate::discovery::TransportDiscovery;
use crate::node_transport::NodeTransport;
use crate::sender::{SendContext, TransportError};

/// Bridges ORR hop forwarding to `NodeTransport` dispatch.
///
/// Given a `PeeledLayer` (result of onion peeling at a relay), resolves the
/// transport vector to a concrete sender and dispatches the inner payload.
pub struct OrrTransportBridge {
    transport: Arc<NodeTransport>,
    discovery: Arc<Mutex<TransportDiscovery>>,
}

impl OrrTransportBridge {
    /// Create a new ORR transport bridge.
    pub fn new(transport: Arc<NodeTransport>, discovery: Arc<Mutex<TransportDiscovery>>) -> Self {
        Self {
            transport,
            discovery,
        }
    }

    /// Forward a peeled onion hop to the next relay.
    ///
    /// Takes the `PeeledLayer` from onion peeling and sends the inner payload
    /// through the best available transport to the next hop gateway.
    pub async fn forward_hop(
        &self,
        peeled: &PeeledLayer,
        mission_id: &[u8; 32],
    ) -> Result<(), TransportError> {
        let ctx = SendContext {
            mission_id: *mission_id,
            priority: 1,
            source_peer: peeled.next_gateway,
            origin_gateway: [0u8; 32],
        };
        self.transport.send_best(&peeled.inner_payload, &ctx).await
    }

    /// Check if a specific transport type is supported for routing.
    pub fn transport_supported(&self, transport_type: u16) -> bool {
        let disc = self.discovery.lock().unwrap();
        !disc.peers_with_transport(transport_type).is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sender::NetworkSender;
    use async_trait::async_trait;
    use octo_network::dot::gateway::{GatewayClass, GatewayIdentity};
    use octo_network::gdp::identity::GdpGatewayIdentity;

    struct MockSender {
        name: String,
        healthy: bool,
    }

    #[async_trait]
    impl NetworkSender for MockSender {
        async fn send(&self, _payload: &[u8], _ctx: &SendContext) -> Result<(), TransportError> {
            Ok(())
        }
        fn name(&self) -> &str {
            &self.name
        }
        fn is_healthy(&self) -> bool {
            self.healthy
        }
    }

    fn make_bridge() -> OrrTransportBridge {
        let transport = Arc::new(NodeTransport::new(vec![Arc::new(MockSender {
            name: "webhook".into(),
            healthy: true,
        })
            as Arc<dyn NetworkSender>]));
        let identity = GdpGatewayIdentity::new(GatewayIdentity::new(
            [0x42u8; 32],
            1,
            GatewayClass::Edge,
            100,
        ));
        let discovery = Arc::new(Mutex::new(TransportDiscovery::new(
            identity,
            [0xABu8; 32],
            100,
        )));
        OrrTransportBridge::new(transport, discovery)
    }

    #[tokio::test]
    async fn forward_hop_succeeds() {
        let bridge = make_bridge();
        let peeled = PeeledLayer {
            next_gateway: [0x05; 32],
            transport: octo_network::orr::TransportVector {
                transport_type: 0x0009,
                domain_id: [0u8; 32],
                priority: 100,
                bandwidth_class: 0,
                censorship_score: 0,
            },
            inner_payload: vec![0x01, 0x02, 0x03],
            hop_index: 1,
        };
        let mission_id = [0xAB; 32];
        let result = bridge.forward_hop(&peeled, &mission_id).await;
        assert!(result.is_ok());
    }

    #[test]
    fn transport_supported_false_for_unknown() {
        let bridge = make_bridge();
        assert!(!bridge.transport_supported(0x000B));
    }

    #[tokio::test]
    async fn forward_hop_empty_payload() {
        let bridge = make_bridge();
        let peeled = PeeledLayer {
            next_gateway: [0x05; 32],
            transport: octo_network::orr::TransportVector {
                transport_type: 0x0009,
                domain_id: [0u8; 32],
                priority: 100,
                bandwidth_class: 0,
                censorship_score: 0,
            },
            inner_payload: vec![],
            hop_index: 0,
        };
        let mission_id = [0xAB; 32];
        let result = bridge.forward_hop(&peeled, &mission_id).await;
        assert!(result.is_ok());
    }
}
