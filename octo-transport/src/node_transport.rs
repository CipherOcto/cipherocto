use std::sync::Arc;
use std::time::Duration;

use futures::future::join_all;
use tokio::sync::{oneshot, Mutex};

use crate::receiver::{NetworkReceiver, ReceiveContext};
use crate::request_response::PendingRequests;
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
    /// In-flight request/response registry keyed by RFC-0871
    /// `envelope_id`. Populated by `register_response_handler`; consumed
    /// by `dispatch_response` (or swept by `evict_expired_pending`).
    ///
    /// Mission 0870k-transport-request-response: generalizes the
    /// mesh-specific `PendingRequests` at
    /// `crates/quota-router-core/src/node/quota_router_node.rs:2312+`.
    pending: Arc<Mutex<PendingRequests>>,
}

impl NodeTransport {
    pub fn new(senders: Vec<Arc<dyn NetworkSender>>) -> Self {
        Self {
            senders,
            receivers: std::sync::Mutex::new(Vec::new()),
            pending: Arc::new(Mutex::new(PendingRequests::new())),
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

    /// Send a request via the first sender that does NOT return
    /// `TransportError::Unsupported`. Await the response until `timeout`
    /// elapses, matching it by RFC-0871 `envelope_id` via the
    /// `register_response_handler` / `dispatch_response` pair.
    ///
    /// Mission 0870k-transport-request-response: high-level API.
    /// Caller MUST compute `envelope_id` per RFC-0871 §Algorithms step 2
    /// BEFORE calling this method (the substrate only correlates; it
    /// does not interpret envelope semantics).
    ///
    /// Returns the reply payload bytes (caller decodes per the reply
    /// envelope's `payload_kind`). On timeout returns
    /// `Err(TransportError::AllTransportsFailed)`.
    pub async fn request_response(
        &self,
        payload: &[u8],
        envelope_id: [u8; 32],
        context: &SendContext,
        timeout: Duration,
    ) -> Result<Vec<u8>, TransportError> {
        // Register the response handler BEFORE sending so the reply
        // dispatch path can find it (avoid race where reply arrives
        // before handler is registered).
        let rx = {
            let mut pending = self.pending.lock().await;
            pending
                .register(envelope_id)
                .map_err(|_| TransportError::AllTransportsFailed)?
        };

        // Try senders in order; skip Unsupported. The returned `Vec<u8>`
        // from a successful `send_request` is the synchronous response
        // (substrate auto-delivers it to the registered handler).
        // Async senders return `Ok(())` (empty payload) and rely on
        // `dispatch_response` to deliver the reply later.
        let mut last_err: Option<TransportError> = None;
        let mut sent = false;
        for sender in &self.senders {
            if !sender.is_healthy() {
                continue;
            }
            match sender
                .send_request(payload, envelope_id, context, timeout)
                .await
            {
                Ok(payload) => {
                    sent = true;
                    // Sync reply: deliver via complete() so the
                    // awaiting caller gets the bytes without waiting
                    // for the full timeout. Empty payload means "async;
                    // will deliver via dispatch_response".
                    if !payload.is_empty() {
                        let mut pending = self.pending.lock().await;
                        // Best-effort: if cancel races us, ignore.
                        let _ = pending.complete(envelope_id, payload);
                    }
                    break;
                }
                Err(TransportError::Unsupported(_)) => continue,
                Err(e) => {
                    last_err = Some(e);
                }
            }
        }

        if !sent {
            // Remove the handler we registered — nobody will reply.
            // Use `cancel` (not `complete(empty)`) so the receiver
            // gets cancelled cleanly without seeing a fake payload.
            let mut pending = self.pending.lock().await;
            pending.cancel(envelope_id);
            return Err(last_err.unwrap_or(TransportError::Unsupported(
                "no sender implements send_request".to_owned(),
            )));
        }

        // Await the reply or the timeout.
        match tokio::time::timeout(timeout, rx).await {
            Ok(Ok(payload)) => Ok(payload),
            Ok(Err(_)) => Err(TransportError::AllTransportsFailed),
            Err(_elapsed) => {
                // Sweep the stale entry via cancel — receiver is
                // already cancelled by the timeout.
                let mut pending = self.pending.lock().await;
                pending.cancel(envelope_id);
                Err(TransportError::AllTransportsFailed)
            }
        }
    }

    /// Register an expectation for a response with the given RFC-0871
    /// `envelope_id`. Returns a `oneshot::Receiver<Vec<u8>>` that
    /// resolves when `dispatch_response` is called with the matching
    /// id, or the receiver is dropped (timeout / cancellation).
    ///
    /// Use this when the caller wants to register a handler without
    /// going through `request_response` (e.g. intermediate nodes
    /// forwarding a request and binding both the forward and the
    /// return path).
    pub async fn register_response_handler(
        &self,
        envelope_id: [u8; 32],
    ) -> Result<oneshot::Receiver<Vec<u8>>, TransportError> {
        let mut pending = self.pending.lock().await;
        pending
            .register(envelope_id)
            .map_err(|_| TransportError::AllTransportsFailed)
    }

    /// Called by the inbound dispatch path when a reply envelope
    /// arrives with `envelope_id` matching a registered handler.
    /// Delivers the payload to the awaiting caller.
    ///
    /// Public so test infrastructure can inject responses directly; in
    /// production the receive loop calls it after extracting
    /// `envelope.envelope_id` from the inbound envelope.
    ///
    /// If no handler is registered for the given id, the reply falls
    /// through to `NetworkReceiver::on_receive` (existing path) —
    /// fail-closed for unknown replies.
    pub async fn dispatch_response(
        &self,
        envelope_id: [u8; 32],
        payload: Vec<u8>,
    ) -> Result<(), TransportError> {
        let mut pending = self.pending.lock().await;
        pending
            .complete(envelope_id, payload)
            .map_err(|_| TransportError::AllTransportsFailed)
    }

    /// Sweep stale pending entries older than `timeout`. Returns count
    /// of entries evicted. Called by background sweeper tasks or on
    /// shutdown.
    pub async fn evict_expired_pending(&self, timeout: Duration) -> usize {
        use std::time::Instant;

        let mut pending = self.pending.lock().await;
        pending.evict_expired(Instant::now(), timeout)
    }

    /// Number of in-flight pending entries. For diagnostics/tests.
    pub async fn pending_count(&self) -> usize {
        let pending = self.pending.lock().await;
        pending.len()
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
    use std::time::Duration;

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

    // === Request/response tests (mission 0870k AC-7..AC-10) ===

    /// Mock sender that records `send_request` calls + returns a
    /// canned response.
    struct MockRequestSender {
        name: String,
        canned_response: Vec<u8>,
    }

    impl MockRequestSender {
        fn new(name: &str, response: &[u8]) -> Self {
            Self {
                name: name.to_string(),
                canned_response: response.to_vec(),
            }
        }
    }

    #[async_trait]
    impl NetworkSender for MockRequestSender {
        async fn send(&self, _payload: &[u8], _ctx: &SendContext) -> Result<(), TransportError> {
            Ok(())
        }

        async fn send_request(
            &self,
            _payload: &[u8],
            _envelope_id: [u8; 32],
            _context: &SendContext,
            _timeout: Duration,
        ) -> Result<Vec<u8>, TransportError> {
            Ok(self.canned_response.clone())
        }

        fn name(&self) -> &str {
            &self.name
        }

        fn is_healthy(&self) -> bool {
            true
        }
    }

    #[tokio::test]
    async fn request_response_unsupported_sender_returns_error() {
        // MockSender has the default `send_request = Err(Unsupported)` body.
        let t = NodeTransport::new(senders(vec![MockSender::new("unsupported-a")]));
        let result = t
            .request_response(b"req", [1u8; 32], &ctx(), Duration::from_millis(50))
            .await;
        assert!(matches!(result, Err(TransportError::Unsupported(_))));
        // Pending entry should be cleaned up after the cancel.
        assert_eq!(t.pending_count().await, 0);
    }

    #[tokio::test]
    async fn request_response_round_trip_via_mock_sender() {
        // Sender returns canned reply synchronously. Reply echoes the
        // request envelope_id (caller verifies by registering their
        // own handler bound to that id and confirming dispatch_response
        // matched).
        let envelope_id = [42u8; 32];
        let reply_payload = b"reply bytes";
        let t = NodeTransport::new(vec![Arc::new(MockRequestSender::new(
            "mocksendreq",
            reply_payload,
        ))]);

        let result = t
            .request_response(b"req", envelope_id, &ctx(), Duration::from_secs(1))
            .await
            .expect("request_response succeeds");
        assert_eq!(result, reply_payload);
        // Pending registry is empty after round-trip.
        assert_eq!(t.pending_count().await, 0);
    }

    #[tokio::test]
    async fn dispatch_response_unknown_envelope_id_returns_err() {
        let t = NodeTransport::new(vec![]);
        let result = t.dispatch_response([99u8; 32], b"orphan".to_vec()).await;
        assert!(matches!(result, Err(TransportError::AllTransportsFailed)));
        // No panic; pending registry stays empty.
        assert_eq!(t.pending_count().await, 0);
    }

    #[tokio::test]
    async fn register_response_handler_drop_on_caller_cancel() {
        let t = NodeTransport::new(vec![]);
        let envelope_id = [7u8; 32];
        let rx = t
            .register_response_handler(envelope_id)
            .await
            .expect("register");
        assert_eq!(t.pending_count().await, 1);
        drop(rx); // caller cancels
                  // dispatch_response now fails (ReceiverDropped) but entry is
                  // removed.
        let result = t.dispatch_response(envelope_id, b"too-late".to_vec()).await;
        assert!(matches!(result, Err(TransportError::AllTransportsFailed)));
        assert_eq!(t.pending_count().await, 0);
    }
}
