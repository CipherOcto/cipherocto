//! `PlatformAdapterPoller` — runtime adapter-poll → `NodeTransport::dispatch`
//! bridge.
//!
//! Pairs with `PlatformAdapterBridge` (which handles the outbound direction
//! via `NetworkSender`). Together they make a `PlatformAdapter` fully usable
//! from `NodeTransport`:
//!
//! - `PlatformAdapterBridge::send` → `NetworkSender::send` →
//!   `adapter.send_message(...)` (outbound)
//! - `PlatformAdapterPoller::run` → poll `adapter.receive_messages(...)` →
//!   `NodeTransport::dispatch(payload, ctx)` (inbound)
//!
//! Without the poller, the mesh can SEND through a `PlatformAdapter` but
//! cannot RECEIVE — a real gap in the production path. The poller closes
//! that gap and is the receiving side of the bridge.
//!
//! Wire-format contract (RFC-0850 §8.8 Raw mode):
//! - `RawPlatformMessage.payload` is `[DeterministicEnvelope wire bytes][mesh payload]`
//! - The poller parses the first `ENVELOPE_WIRE_LEN` bytes via `canonicalize()`
//! - The remainder is fed to `NodeTransport::dispatch` as the mesh payload
//! - `envelope.source_peer` is mapped to `ReceiveContext.sender_id` so the
//!   handler's HMAC trust check can resolve the sender's `PeerTrust`

use std::sync::Arc;

use octo_network::dot::adapters::PlatformAdapter;
use octo_network::dot::envelope::{DeterministicEnvelope, ENVELOPE_WIRE_LEN};
use octo_network::dot::BroadcastDomainId;

use crate::node_transport::NodeTransport;
use crate::receiver::ReceiveContext;

/// Runtime poller that drains `PlatformAdapter::receive_messages` and feeds
/// the inbound payloads into `NodeTransport::dispatch`.
pub struct PlatformAdapterPoller {
    adapter: Arc<dyn PlatformAdapter>,
    domain: BroadcastDomainId,
    transport: Arc<NodeTransport>,
}

impl PlatformAdapterPoller {
    pub fn new(
        adapter: Arc<dyn PlatformAdapter>,
        domain: BroadcastDomainId,
        transport: Arc<NodeTransport>,
    ) -> Self {
        Self {
            adapter,
            domain,
            transport,
        }
    }

    /// Run the poll loop. Returns when the adapter's inbound mpsc closes
    /// (typically after `adapter.shutdown()` is called).
    ///
    /// For each `RawPlatformMessage` returned by the adapter:
    ///   1. canonicalize() → `DeterministicEnvelope` (parses first
    ///      `ENVELOPE_WIRE_LEN` bytes of `raw.payload`)
    ///   2. extract `envelope.source_peer` → `ReceiveContext.sender_id`
    ///   3. slice `raw.payload[ENVELOPE_WIRE_LEN..]` as mesh payload
    ///   4. `transport.dispatch(payload, ctx)` → registered receivers
    pub async fn run(&self) {
        loop {
            let messages = match self.adapter.receive_messages(&self.domain).await {
                Ok(m) => m,
                Err(e) => {
                    tracing::warn!("PlatformAdapterPoller: receive_messages error: {e}");
                    tokio::task::yield_now().await;
                    continue;
                }
            };
            if messages.is_empty() {
                // No inbound frames; yield to let other tasks run. Production
                // deployments may add a configurable idle backoff here.
                tokio::task::yield_now().await;
                continue;
            }
            for raw in messages {
                self.dispatch_one(&raw).await;
            }
        }
    }

    async fn dispatch_one(&self, raw: &octo_network::dot::adapters::RawPlatformMessage) {
        let envelope: DeterministicEnvelope = match self.adapter.canonicalize(raw) {
            Ok(e) => e,
            Err(e) => {
                tracing::warn!("PlatformAdapterPoller: canonicalize error: {e}");
                return;
            }
        };
        let ctx = ReceiveContext {
            source_transport: self.adapter.platform_type().name().into(),
            mission_id: envelope.mission_id,
            sender_id: Some(envelope.source_peer),
        };
        let payload: &[u8] = if raw.payload.len() >= ENVELOPE_WIRE_LEN {
            &raw.payload[ENVELOPE_WIRE_LEN..]
        } else {
            &[]
        };
        if let Err(e) = self.transport.dispatch(payload, &ctx).await {
            tracing::warn!("PlatformAdapterPoller: dispatch error: {e}");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::receiver::{NetworkReceiver, ReceiveContext};
    use crate::sender::TransportError;
    use octo_network::dot::adapters::{
        CapabilityReport, DeliveryReceipt, PlatformAdapter, RawPlatformMessage,
    };
    use octo_network::dot::domain::{BroadcastDomainId, PlatformType};
    use octo_network::dot::envelope::{DeterministicEnvelope, ENVELOPE_WIRE_LEN};
    use octo_network::dot::error::PlatformAdapterError;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Mutex;

    /// Adapter that returns a pre-set Vec<RawPlatformMessage> on every poll,
    /// then signals exhaustion via a flag.
    struct FixedAdapter {
        platform_type: PlatformType,
        queue: Mutex<Vec<RawPlatformMessage>>,
        exhausted: std::sync::atomic::AtomicBool,
        poll_calls: AtomicUsize,
    }

    impl FixedAdapter {
        fn new(platform_type: PlatformType, queue: Vec<RawPlatformMessage>) -> Self {
            Self {
                platform_type,
                queue: Mutex::new(queue),
                exhausted: std::sync::atomic::AtomicBool::new(false),
                poll_calls: AtomicUsize::new(0),
            }
        }
    }

    #[async_trait::async_trait]
    impl PlatformAdapter for FixedAdapter {
        async fn send_message(
            &self,
            _domain: &BroadcastDomainId,
            _envelope: &DeterministicEnvelope,
            _payload: &[u8],
        ) -> Result<DeliveryReceipt, PlatformAdapterError> {
            Ok(DeliveryReceipt {
                platform_message_id: "fixed".into(),
                delivered_at: 0,
            })
        }
        async fn receive_messages(
            &self,
            _domain: &BroadcastDomainId,
        ) -> Result<Vec<RawPlatformMessage>, PlatformAdapterError> {
            self.poll_calls.fetch_add(1, Ordering::SeqCst);
            let mut q = self.queue.lock().unwrap();
            if q.is_empty() {
                self.exhausted.store(true, Ordering::SeqCst);
                Ok(vec![])
            } else {
                Ok(std::mem::take(&mut *q))
            }
        }
        fn canonicalize(
            &self,
            raw: &RawPlatformMessage,
        ) -> Result<DeterministicEnvelope, PlatformAdapterError> {
            if raw.payload.len() < ENVELOPE_WIRE_LEN {
                return Err(PlatformAdapterError::ApiError {
                    code: 400,
                    message: format!(
                        "envelope parse error: frame too short ({} bytes, need {})",
                        raw.payload.len(),
                        ENVELOPE_WIRE_LEN
                    ),
                });
            }
            DeterministicEnvelope::from_wire_bytes(&raw.payload[..ENVELOPE_WIRE_LEN]).map_err(|e| {
                PlatformAdapterError::ApiError {
                    code: 400,
                    message: format!("envelope parse error: {}", e),
                }
            })
        }
        fn capabilities(&self) -> CapabilityReport {
            CapabilityReport {
                max_payload_bytes: 1024,
                supports_raw_binary: true,
                ..Default::default()
            }
        }
        fn domain_id(&self, platform_id: &str) -> BroadcastDomainId {
            BroadcastDomainId::new(self.platform_type, platform_id)
        }
        fn platform_type(&self) -> PlatformType {
            self.platform_type
        }
    }

    /// Receiver that captures every payload + ctx it sees.
    struct CaptureReceiver {
        captured: Mutex<Vec<(Vec<u8>, ReceiveContext)>>,
    }
    impl CaptureReceiver {
        fn new() -> Self {
            Self {
                captured: Mutex::new(Vec::new()),
            }
        }
        fn snapshot(&self) -> Vec<(Vec<u8>, ReceiveContext)> {
            self.captured.lock().unwrap().clone()
        }
    }
    #[async_trait::async_trait]
    impl NetworkReceiver for CaptureReceiver {
        async fn on_receive(
            &self,
            payload: &[u8],
            ctx: &ReceiveContext,
        ) -> Result<(), TransportError> {
            self.captured
                .lock()
                .unwrap()
                .push((payload.to_vec(), ctx.clone()));
            Ok(())
        }
        fn name(&self) -> &str {
            "capture"
        }
    }

    fn make_envelope_payload() -> (DeterministicEnvelope, Vec<u8>, Vec<u8>) {
        let envelope = DeterministicEnvelope::default();
        let envelope_bytes = envelope.to_wire_bytes();
        let mesh_payload = b"hello from the poller".to_vec();
        // raw.payload is [envelope_bytes][mesh_payload]
        let mut raw_payload = envelope_bytes.clone();
        raw_payload.extend_from_slice(&mesh_payload);
        (envelope, mesh_payload, raw_payload)
    }

    #[tokio::test]
    async fn poller_dispatches_inbound_payload_to_registered_receiver() {
        let (envelope, mesh_payload, raw_payload) = make_envelope_payload();
        let raw = RawPlatformMessage {
            platform_id: "test-peer".into(),
            payload: raw_payload,
            metadata: Default::default(),
        };
        let adapter: Arc<dyn PlatformAdapter> =
            Arc::new(FixedAdapter::new(PlatformType::Tcp, vec![raw]));
        let capture = Arc::new(CaptureReceiver::new());
        let transport = Arc::new(NodeTransport::new(vec![]));
        transport.register_receiver(capture.clone());

        let domain = BroadcastDomainId::new(PlatformType::Tcp, "test.example");
        let poller = PlatformAdapterPoller::new(adapter, domain, transport);

        // Run poller in the background; let it consume the queue, then stop.
        let handle = tokio::spawn(async move { poller.run().await });
        // Wait up to 500ms for the queue to drain
        let mut elapsed = 0u64;
        while capture.snapshot().is_empty() && elapsed < 50 {
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
            elapsed += 1;
        }
        // Give a brief grace period then abort
        tokio::time::sleep(std::time::Duration::from_millis(20)).await;
        handle.abort();

        let captured = capture.snapshot();
        assert_eq!(captured.len(), 1, "exactly one inbound message");
        let (captured_payload, captured_ctx) = &captured[0];
        assert_eq!(captured_payload, &mesh_payload);
        assert_eq!(captured_ctx.source_transport, "tcp");
        assert_eq!(captured_ctx.sender_id, Some(envelope.source_peer));
    }

    #[tokio::test]
    async fn poller_skips_short_frames() {
        // raw.payload shorter than ENVELOPE_WIRE_LEN — should not panic.
        let raw = RawPlatformMessage {
            platform_id: "bad".into(),
            payload: vec![0u8; 10],
            metadata: Default::default(),
        };
        let adapter: Arc<dyn PlatformAdapter> =
            Arc::new(FixedAdapter::new(PlatformType::Tcp, vec![raw]));
        let capture = Arc::new(CaptureReceiver::new());
        let transport = Arc::new(NodeTransport::new(vec![]));
        transport.register_receiver(capture.clone());

        let poller = PlatformAdapterPoller::new(
            adapter,
            BroadcastDomainId::new(PlatformType::Tcp, "x"),
            transport,
        );
        let handle = tokio::spawn(async move { poller.run().await });
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        handle.abort();

        // The poller calls canonicalize which fails (frame too short) → skip.
        // No payload should reach the receiver.
        assert!(
            capture.snapshot().is_empty(),
            "short frames must not reach the receiver"
        );
    }
}
