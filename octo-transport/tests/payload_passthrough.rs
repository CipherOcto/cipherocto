//! L4: Full chain payload regression tests
//!
//! Verifies payload bytes survive the full chain end-to-end:
//!     `NodeTransport` → `PlatformAdapterBridge` → adapter
//!
//! Plan reference: `docs/plans/2026-06-28-payload-transport-regression-tests.md` (L4)

use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use octo_network::dot::adapters::{
    CapabilityReport, DeliveryReceipt, PlatformAdapter, RawPlatformMessage,
};
use octo_network::dot::domain::PlatformType;
use octo_network::dot::envelope::DeterministicEnvelope;
use octo_network::dot::error::PlatformAdapterError;
use octo_network::dot::BroadcastDomainId;

use octo_transport::adapter_bridge::PlatformAdapterBridge;
use octo_transport::node_transport::NodeTransport;
use octo_transport::sender::{NetworkSender, SendContext};

/// In-memory adapter that records every payload it receives.
///
/// Used to verify the full chain passes bytes through unmodified.
struct PayloadCaptureAdapter {
    platform: PlatformType,
    captured: Arc<Mutex<Vec<Vec<u8>>>>,
}

impl PayloadCaptureAdapter {
    fn new(platform: PlatformType) -> Self {
        Self {
            platform,
            captured: Arc::new(Mutex::new(Vec::new())),
        }
    }

    fn captured_handle(&self) -> Arc<Mutex<Vec<Vec<u8>>>> {
        self.captured.clone()
    }
}

#[async_trait]
impl PlatformAdapter for PayloadCaptureAdapter {
    async fn send_message(
        &self,
        _domain: &BroadcastDomainId,
        _envelope: &DeterministicEnvelope,
        payload: &[u8],
    ) -> Result<DeliveryReceipt, PlatformAdapterError> {
        self.captured.lock().unwrap().push(payload.to_vec());
        Ok(DeliveryReceipt {
            platform_message_id: "capture-001".to_string(),
            delivered_at: 0,
        })
    }

    async fn receive_messages(
        &self,
        _: &BroadcastDomainId,
    ) -> Result<Vec<RawPlatformMessage>, PlatformAdapterError> {
        Ok(vec![])
    }

    fn canonicalize(
        &self,
        _: &RawPlatformMessage,
    ) -> Result<DeterministicEnvelope, PlatformAdapterError> {
        Ok(DeterministicEnvelope::default())
    }

    fn capabilities(&self) -> CapabilityReport {
        CapabilityReport {
            max_payload_bytes: 65_536,
            supports_raw_binary: true,
            ..Default::default()
        }
    }

    fn domain_id(&self, _: &str) -> BroadcastDomainId {
        BroadcastDomainId::new(self.platform, "test.example.com")
    }

    fn platform_type(&self) -> PlatformType {
        self.platform
    }
}

fn test_ctx() -> SendContext {
    SendContext {
        mission_id: [1u8; 32],
        priority: 128,
        source_peer: [0xAAu8; 32],
        origin_gateway: [0xBBu8; 32],
    }
}

#[allow(clippy::type_complexity)]
fn make_bridge(
    adapter: Arc<PayloadCaptureAdapter>,
) -> (Arc<dyn NetworkSender>, Arc<Mutex<Vec<Vec<u8>>>>) {
    let captured = adapter.captured_handle();
    let platform = adapter.platform;
    let bridge: Arc<dyn NetworkSender> = Arc::new(PlatformAdapterBridge::new(
        adapter as Arc<dyn PlatformAdapter>,
        BroadcastDomainId::new(platform, "test.example.com"),
    ));
    (bridge, captured)
}

/// L4: full_chain_payload_integrity
///
/// Send a payload through `NodeTransport::broadcast` → `PlatformAdapterBridge` →
/// `PayloadCaptureAdapter` and verify the captured bytes are identical to the
/// input.
#[tokio::test]
async fn full_chain_payload_integrity() {
    let adapter = Arc::new(PayloadCaptureAdapter::new(PlatformType::Webhook));
    let (bridge, captured) = make_bridge(adapter);
    let node = NodeTransport::new(vec![bridge]);

    let payload: &[u8] = b"payload through full chain: NodeTransport -> Bridge -> Adapter";
    let success_count = node.broadcast(payload, &test_ctx()).await;

    assert_eq!(success_count, 1, "broadcast should report 1 success");

    let captured = captured.lock().unwrap();
    assert_eq!(captured.len(), 1, "exactly one payload should be captured");
    assert_eq!(
        captured[0], payload,
        "captured bytes must equal the input payload"
    );
}

/// L4: payload_roundtrip_through_mock
///
/// Send a payload through `NodeTransport::send_best` and verify the captured
/// bytes round-trip exactly through the full chain.
#[tokio::test]
async fn payload_roundtrip_through_mock() {
    let adapter = Arc::new(PayloadCaptureAdapter::new(PlatformType::Telegram));
    let (bridge, captured) = make_bridge(adapter);
    let node = NodeTransport::new(vec![bridge]);

    let payload = vec![0xCA, 0xFE, 0xBA, 0xBE, 0xDE, 0xAD, 0xBE, 0xEF];
    let result = node.send_best(&payload, &test_ctx()).await;

    assert!(result.is_ok(), "send_best should succeed: {result:?}");

    let captured = captured.lock().unwrap();
    assert_eq!(captured.len(), 1, "exactly one payload should be captured");
    assert_eq!(
        captured[0], payload,
        "captured bytes must round-trip exactly through the full chain"
    );
}
