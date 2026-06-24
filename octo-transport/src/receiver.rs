use async_trait::async_trait;

use crate::sender::TransportError;

/// Context for a received payload.
pub struct ReceiveContext {
    /// The source transport name.
    pub source_transport: String,
    /// The mission ID.
    pub mission_id: [u8; 32],
    /// The sender's peer ID (if authenticated).
    pub sender_id: Option<[u8; 32]>,
}

/// General-purpose inbound transport handler.
///
/// Handlers register with `DotGateway` or `NodeTransport` to receive
/// dispatched payloads. Each handler processes payloads matching its
/// domain or mission scope.
#[async_trait]
pub trait NetworkReceiver: Send + Sync {
    /// Handle an incoming payload from a transport.
    async fn on_receive(
        &self,
        payload: &[u8],
        context: &ReceiveContext,
    ) -> Result<(), TransportError>;

    /// Return the handler name for diagnostics.
    fn name(&self) -> &str;
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use async_trait::async_trait;

    use crate::receiver::{NetworkReceiver, ReceiveContext};
    use crate::sender::TransportError;

    struct MockReceiver {
        name: String,
    }

    #[async_trait]
    impl NetworkReceiver for MockReceiver {
        async fn on_receive(
            &self,
            _payload: &[u8],
            _ctx: &ReceiveContext,
        ) -> Result<(), TransportError> {
            Ok(())
        }

        fn name(&self) -> &str {
            &self.name
        }
    }

    #[tokio::test]
    async fn receiver_handle_success() {
        let r = MockReceiver {
            name: "test-rx".to_string(),
        };
        let ctx = ReceiveContext {
            source_transport: "quic".to_string(),
            mission_id: [1u8; 32],
            sender_id: Some([2u8; 32]),
        };
        assert!(r.on_receive(b"data", &ctx).await.is_ok());
    }

    #[test]
    fn receiver_name() {
        let r = MockReceiver {
            name: "test-rx".to_string(),
        };
        assert_eq!(r.name(), "test-rx");
    }

    #[tokio::test]
    async fn receiver_as_trait_object() {
        let r: Arc<dyn NetworkReceiver> = Arc::new(MockReceiver {
            name: "trait-obj".to_string(),
        });
        assert_eq!(r.name(), "trait-obj");
        let ctx = ReceiveContext {
            source_transport: "webhook".to_string(),
            mission_id: [0u8; 32],
            sender_id: None,
        };
        assert!(r.on_receive(b"payload", &ctx).await.is_ok());
    }
}
