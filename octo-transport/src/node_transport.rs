use std::sync::Arc;

use futures::future::join_all;

use crate::receiver::{NetworkReceiver, ReceiveContext};
use crate::sender::{NetworkSender, SendContext, TransportError};

/// Declarative transport stack that fans out or fails over to multiple senders,
/// and dispatches inbound payloads to registered receivers.
///
/// This is the consumer-facing API for any code — sync engines, agent
/// runtimes, marketplace services — that needs to send and receive data
/// through the network.
pub struct NodeTransport {
    senders: Vec<Arc<dyn NetworkSender>>,
    receivers: std::sync::Mutex<Vec<Arc<dyn NetworkReceiver>>>,
}

impl NodeTransport {
    pub fn new(senders: Vec<Arc<dyn NetworkSender>>) -> Self {
        Self {
            senders,
            receivers: std::sync::Mutex::new(Vec::new()),
        }
    }

    /// Register a handler for inbound payloads.
    /// Handlers are called in registration order by `dispatch()`.
    /// Safe to call concurrently — receivers are protected by Mutex.
    pub fn register_receiver(&self, receiver: Arc<dyn NetworkReceiver>) {
        self.receivers.lock().unwrap().push(receiver);
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

    /// Dispatch an inbound payload to all registered receivers.
    /// Calls `on_receive()` on each receiver in registration order.
    /// Returns first error (fail-fast) or Ok if all succeed.
    pub async fn dispatch(
        &self,
        payload: &[u8],
        ctx: &ReceiveContext,
    ) -> Result<(), TransportError> {
        let receivers: Vec<_> = self.receivers.lock().unwrap().clone();
        for receiver in &receivers {
            receiver.on_receive(payload, ctx).await?;
        }
        Ok(())
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

    // === Payload transport regression tests ===

    use std::sync::Mutex;

    /// CapturingSender records payloads received by send().
    struct CapturingSender {
        captured: Arc<Mutex<Vec<Vec<u8>>>>,
    }

    impl CapturingSender {
        fn new(captured: Arc<Mutex<Vec<Vec<u8>>>>) -> Self {
            Self { captured }
        }
    }

    #[async_trait]
    impl NetworkSender for CapturingSender {
        async fn send(&self, payload: &[u8], _ctx: &SendContext) -> Result<(), TransportError> {
            self.captured.lock().unwrap().push(payload.to_vec());
            Ok(())
        }
        fn name(&self) -> &str {
            "capturing"
        }
        fn is_healthy(&self) -> bool {
            true
        }
    }

    #[tokio::test]
    async fn send_best_passes_payload_to_sender() {
        let captured = Arc::new(Mutex::new(Vec::new()));
        let t = NodeTransport::new(vec![Arc::new(CapturingSender::new(captured.clone()))]);
        let payload = b"test payload for send_best";
        t.send_best(payload, &ctx()).await.unwrap();
        let payloads = captured.lock().unwrap();
        assert_eq!(payloads.len(), 1);
        assert_eq!(payloads[0], b"test payload for send_best");
    }

    #[tokio::test]
    async fn broadcast_passes_payload_to_all_senders() {
        let captured = Arc::new(Mutex::new(Vec::new()));
        let t = NodeTransport::new(vec![
            Arc::new(CapturingSender::new(captured.clone())),
            Arc::new(CapturingSender::new(captured.clone())),
        ]);
        let payload = b"broadcast payload";
        let count = t.broadcast(payload, &ctx()).await;
        assert_eq!(count, 2);
        let payloads = captured.lock().unwrap();
        assert_eq!(payloads.len(), 2);
        assert_eq!(payloads[0], b"broadcast payload");
        assert_eq!(payloads[1], b"broadcast payload");
    }

    #[tokio::test]
    async fn failover_preserves_payload() {
        let captured = Arc::new(Mutex::new(Vec::new()));
        let t = NodeTransport::new(vec![
            Arc::new(MockSender::failing("fail")),
            Arc::new(CapturingSender::new(captured.clone())),
        ]);
        let payload = b"failover payload";
        t.send_best(payload, &ctx()).await.unwrap();
        let payloads = captured.lock().unwrap();
        assert_eq!(payloads.len(), 1);
        assert_eq!(payloads[0], b"failover payload");
    }

    // === Receiver dispatch tests ===

    use crate::receiver::{NetworkReceiver, ReceiveContext};

    struct MockReceiver {
        name: String,
        captured: Arc<Mutex<Vec<Vec<u8>>>>,
        should_fail: bool,
    }

    impl MockReceiver {
        fn new(name: &str) -> Self {
            Self {
                name: name.to_string(),
                captured: Arc::new(Mutex::new(Vec::new())),
                should_fail: false,
            }
        }

        fn failing(name: &str) -> Self {
            Self {
                name: name.to_string(),
                captured: Arc::new(Mutex::new(Vec::new())),
                should_fail: true,
            }
        }

        fn captured(&self) -> Vec<Vec<u8>> {
            self.captured.lock().unwrap().clone()
        }
    }

    #[async_trait]
    impl NetworkReceiver for MockReceiver {
        async fn on_receive(
            &self,
            payload: &[u8],
            _ctx: &ReceiveContext,
        ) -> Result<(), TransportError> {
            if self.should_fail {
                Err(TransportError::AdapterFailure(self.name.clone()))
            } else {
                self.captured.lock().unwrap().push(payload.to_vec());
                Ok(())
            }
        }

        fn name(&self) -> &str {
            &self.name
        }
    }

    fn recv_ctx() -> ReceiveContext {
        ReceiveContext {
            source_transport: "test".to_string(),
            mission_id: [0u8; 32],
            sender_id: None,
        }
    }

    #[tokio::test]
    async fn dispatch_empty_receivers() {
        let t = NodeTransport::new(vec![]);
        assert!(t.dispatch(b"data", &recv_ctx()).await.is_ok());
    }

    #[tokio::test]
    async fn dispatch_single_receiver() {
        let receiver = Arc::new(MockReceiver::new("rx1"));
        let rx_clone = Arc::clone(&receiver);
        let t = NodeTransport::new(vec![]);
        t.register_receiver(receiver);
        t.dispatch(b"hello", &recv_ctx()).await.unwrap();
        assert_eq!(rx_clone.captured(), vec![b"hello".to_vec()]);
    }

    #[tokio::test]
    async fn dispatch_multiple_receivers() {
        let rx1 = Arc::new(MockReceiver::new("rx1"));
        let rx2 = Arc::new(MockReceiver::new("rx2"));
        let rx1_clone = Arc::clone(&rx1);
        let rx2_clone = Arc::clone(&rx2);
        let t = NodeTransport::new(vec![]);
        t.register_receiver(rx1);
        t.register_receiver(rx2);
        t.dispatch(b"data", &recv_ctx()).await.unwrap();
        assert_eq!(rx1_clone.captured(), vec![b"data".to_vec()]);
        assert_eq!(rx2_clone.captured(), vec![b"data".to_vec()]);
    }

    #[tokio::test]
    async fn dispatch_fail_fast_on_first_error() {
        let rx1 = Arc::new(MockReceiver::failing("rx1"));
        let rx2 = Arc::new(MockReceiver::new("rx2"));
        let rx2_clone = Arc::clone(&rx2);
        let t = NodeTransport::new(vec![]);
        t.register_receiver(rx1);
        t.register_receiver(rx2);
        let result = t.dispatch(b"data", &recv_ctx()).await;
        assert!(matches!(result, Err(TransportError::AdapterFailure(_))));
        // rx2 should NOT have been called (fail-fast)
        assert_eq!(rx2_clone.captured(), Vec::<Vec<u8>>::new());
    }

    #[tokio::test]
    async fn dispatch_preserves_payload() {
        let receiver = Arc::new(MockReceiver::new("rx1"));
        let rx_clone = Arc::clone(&receiver);
        let t = NodeTransport::new(vec![]);
        t.register_receiver(receiver);
        let payload = b"exact payload bytes";
        t.dispatch(payload, &recv_ctx()).await.unwrap();
        assert_eq!(rx_clone.captured(), vec![payload.to_vec()]);
    }
}
