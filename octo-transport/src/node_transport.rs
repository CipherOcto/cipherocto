use std::sync::Arc;

use futures::future::join_all;

use crate::sender::{NetworkSender, SendContext, TransportError};

/// Declarative transport stack that fans out or fails over to multiple senders.
///
/// This is the consumer-facing API for any code — sync engines, agent
/// runtimes, marketplace services — that needs to send data through
/// the network.
pub struct NodeTransport {
    senders: Vec<Arc<dyn NetworkSender>>,
}

impl NodeTransport {
    pub fn new(senders: Vec<Arc<dyn NetworkSender>>) -> Self {
        Self { senders }
    }

    /// Broadcast to all healthy transports concurrently.
    /// Returns count of successful sends.
    pub async fn broadcast(&self, payload: &[u8], ctx: &SendContext) -> usize {
        let futures: Vec<_> = self
            .senders
            .iter()
            .filter(|s| s.is_healthy())
            .map(|s| s.send(payload, ctx))
            .collect();

        let results = join_all(futures).await;
        results.into_iter().filter(|r| r.is_ok()).count()
    }

    /// Send to the best available transport (failover).
    /// Tries transports in order, skips unhealthy, returns first success.
    pub async fn send_best(&self, payload: &[u8], ctx: &SendContext) -> Result<(), TransportError> {
        let mut last_err = None;
        for sender in &self.senders {
            if !sender.is_healthy() {
                continue;
            }
            match sender.send(payload, ctx).await {
                Ok(()) => return Ok(()),
                Err(e) => {
                    last_err = Some(e);
                }
            }
        }
        if last_err.is_some() {
            Err(TransportError::AllTransportsFailed)
        } else {
            Err(TransportError::Unhealthy)
        }
    }

    /// Return list of healthy transport names.
    pub fn healthy_transports(&self) -> Vec<String> {
        self.senders
            .iter()
            .filter(|s| s.is_healthy())
            .map(|s| s.name().to_string())
            .collect()
    }

    /// Return count of total transports.
    pub fn transport_count(&self) -> usize {
        self.senders.len()
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use async_trait::async_trait;

    use crate::node_transport::NodeTransport;
    use crate::sender::{NetworkSender, SendContext, TransportError};

    struct MockSender {
        name: String,
        healthy: bool,
        should_fail: bool,
    }

    impl MockSender {
        fn new(name: &str) -> Self {
            Self {
                name: name.to_string(),
                healthy: true,
                should_fail: false,
            }
        }

        fn unhealthy(name: &str) -> Self {
            Self {
                name: name.to_string(),
                healthy: false,
                should_fail: false,
            }
        }

        fn failing(name: &str) -> Self {
            Self {
                name: name.to_string(),
                healthy: true,
                should_fail: true,
            }
        }
    }

    #[async_trait]
    impl NetworkSender for MockSender {
        async fn send(&self, _payload: &[u8], _ctx: &SendContext) -> Result<(), TransportError> {
            if self.should_fail {
                Err(TransportError::AdapterFailure(self.name.clone()))
            } else {
                Ok(())
            }
        }

        fn name(&self) -> &str {
            &self.name
        }

        fn is_healthy(&self) -> bool {
            self.healthy
        }
    }

    fn ctx() -> SendContext {
        SendContext {
            mission_id: [0u8; 32],
            domain: None,
            priority: 0,
            source_peer: [0u8; 32],
            origin_gateway: [0u8; 32],
        }
    }

    fn senders(list: Vec<MockSender>) -> Vec<Arc<dyn NetworkSender>> {
        list.into_iter()
            .map(|s| Arc::new(s) as Arc<dyn NetworkSender>)
            .collect()
    }

    #[tokio::test]
    async fn broadcast_all_healthy() {
        let t = NodeTransport::new(senders(vec![
            MockSender::new("a"),
            MockSender::new("b"),
            MockSender::new("c"),
        ]));
        assert_eq!(t.broadcast(b"data", &ctx()).await, 3);
    }

    #[tokio::test]
    async fn broadcast_skips_unhealthy() {
        let t = NodeTransport::new(senders(vec![
            MockSender::new("a"),
            MockSender::unhealthy("b"),
            MockSender::new("c"),
        ]));
        assert_eq!(t.broadcast(b"data", &ctx()).await, 2);
    }

    #[tokio::test]
    async fn broadcast_all_unhealthy() {
        let t = NodeTransport::new(senders(vec![
            MockSender::unhealthy("a"),
            MockSender::unhealthy("b"),
        ]));
        assert_eq!(t.broadcast(b"data", &ctx()).await, 0);
    }

    #[tokio::test]
    async fn broadcast_skips_failing() {
        let t = NodeTransport::new(senders(vec![
            MockSender::new("a"),
            MockSender::failing("b"),
            MockSender::new("c"),
        ]));
        assert_eq!(t.broadcast(b"data", &ctx()).await, 2);
    }

    #[tokio::test]
    async fn send_best_first_success() {
        let t = NodeTransport::new(senders(vec![MockSender::new("a"), MockSender::new("b")]));
        assert!(t.send_best(b"data", &ctx()).await.is_ok());
    }

    #[tokio::test]
    async fn send_best_failover() {
        let t = NodeTransport::new(senders(vec![
            MockSender::failing("a"),
            MockSender::new("b"),
        ]));
        assert!(t.send_best(b"data", &ctx()).await.is_ok());
    }

    #[tokio::test]
    async fn send_best_all_fail() {
        let t = NodeTransport::new(senders(vec![
            MockSender::failing("a"),
            MockSender::failing("b"),
        ]));
        let result = t.send_best(b"data", &ctx()).await;
        assert!(matches!(result, Err(TransportError::AllTransportsFailed)));
    }

    #[tokio::test]
    async fn send_best_skips_unhealthy() {
        let t = NodeTransport::new(senders(vec![
            MockSender::unhealthy("a"),
            MockSender::new("b"),
        ]));
        assert!(t.send_best(b"data", &ctx()).await.is_ok());
    }

    #[tokio::test]
    async fn send_best_all_unhealthy() {
        let t = NodeTransport::new(senders(vec![
            MockSender::unhealthy("a"),
            MockSender::unhealthy("b"),
        ]));
        assert!(matches!(
            t.send_best(b"data", &ctx()).await,
            Err(TransportError::Unhealthy)
        ));
    }

    #[test]
    fn healthy_transports() {
        let t = NodeTransport::new(senders(vec![
            MockSender::new("a"),
            MockSender::unhealthy("b"),
            MockSender::new("c"),
        ]));
        assert_eq!(t.healthy_transports(), vec!["a", "c"]);
    }

    #[test]
    fn transport_count() {
        let t = NodeTransport::new(senders(vec![MockSender::new("a"), MockSender::new("b")]));
        assert_eq!(t.transport_count(), 2);
    }

    #[test]
    fn transport_count_empty() {
        let t = NodeTransport::new(vec![]);
        assert_eq!(t.transport_count(), 0);
    }

    #[tokio::test]
    async fn broadcast_empty_senders() {
        let t = NodeTransport::new(vec![]);
        assert_eq!(t.broadcast(b"data", &ctx()).await, 0);
    }
}
