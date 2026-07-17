use std::sync::Arc;

use crate::node_transport::NodeTransport;
use crate::sender::SendContext;
use octo_network::sync::TransportBroadcaster;

/// Implements `TransportBroadcaster` for `NodeTransport`.
///
/// Bridges the sync engine's `SyncTransportSubscriber` to `NodeTransport`,
/// enabling sync WAL chunks to be broadcast over platform adapters.
///
/// # Usage
///
/// ```text
/// let transport = NodeTransport::new(senders);
/// let broadcaster = NodeTransportBroadcaster::new(Arc::new(transport));
/// let subscriber = SyncTransportSubscriber::new(Arc::new(broadcaster));
/// subscriber.broadcast_wal_chunk(&payload, &mission_id).await?;
/// ```
pub struct NodeTransportBroadcaster {
    transport: Arc<NodeTransport>,
    source_peer: [u8; 32],
    origin_gateway: [u8; 32],
}

impl NodeTransportBroadcaster {
    pub fn new(transport: Arc<NodeTransport>) -> Self {
        Self {
            transport,
            source_peer: [0u8; 32],
            origin_gateway: [0u8; 32],
        }
    }

    pub fn with_identity(mut self, source_peer: [u8; 32], origin_gateway: [u8; 32]) -> Self {
        self.source_peer = source_peer;
        self.origin_gateway = origin_gateway;
        self
    }
}

#[async_trait::async_trait]
impl TransportBroadcaster for NodeTransportBroadcaster {
    async fn broadcast(&self, payload: &[u8], mission_id: &[u8; 32]) -> Result<(), std::io::Error> {
        let ctx = SendContext {
            mission_id: *mission_id,
            priority: 128,
            source_peer: self.source_peer,
            origin_gateway: self.origin_gateway,
        };
        let count = self.transport.broadcast(payload, &ctx).await;
        if count == 0 {
            return Err(std::io::Error::new(
                std::io::ErrorKind::ConnectionRefused,
                "all transports failed",
            ));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sender::{NetworkSender, SendContext, TransportError};
    use async_trait::async_trait;

    struct MockSender;

    #[async_trait]
    impl NetworkSender for MockSender {
        async fn send(&self, _payload: &[u8], _ctx: &SendContext) -> Result<(), TransportError> {
            Ok(())
        }
        fn name(&self) -> &str {
            "mock"
        }
        fn is_healthy(&self) -> bool {
            true
        }
    }

    #[tokio::test]
    async fn broadcaster_sends_via_transport() {
        let transport = Arc::new(NodeTransport::new(vec![Arc::new(MockSender)]));
        let broadcaster = NodeTransportBroadcaster::new(transport);
        let result = broadcaster.broadcast(b"test", &[0xABu8; 32]).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn broadcaster_fails_when_no_senders() {
        let transport = Arc::new(NodeTransport::new(vec![]));
        let broadcaster = NodeTransportBroadcaster::new(transport);
        let result = broadcaster.broadcast(b"test", &[0xABu8; 32]).await;
        assert!(result.is_err());
    }
}
