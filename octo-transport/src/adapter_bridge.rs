use std::sync::Arc;

use async_trait::async_trait;
use octo_network::dot::adapters::PlatformAdapter;
use octo_network::dot::envelope::{DeterministicEnvelope, MessageType};
use octo_network::dot::error::PlatformAdapterError;
use octo_network::dot::BroadcastDomainId;

use crate::sender::{NetworkSender, SendContext, TransportError};

/// Bridges a `PlatformAdapter` to the `NetworkSender` trait.
///
/// Wraps any platform-specific adapter (Telegram, Discord, QUIC, etc.)
/// and presents it as a general-purpose `NetworkSender` that can deliver
/// payloads by constructing `DeterministicEnvelope`s.
pub struct PlatformAdapterBridge {
    adapter: Arc<dyn PlatformAdapter>,
    domain: BroadcastDomainId,
}

impl PlatformAdapterBridge {
    /// Create a new bridge for the given adapter and domain.
    pub fn new(adapter: Arc<dyn PlatformAdapter>, domain: BroadcastDomainId) -> Self {
        Self { adapter, domain }
    }

    fn build_envelope(payload: &[u8], ctx: &SendContext) -> DeterministicEnvelope {
        let mut envelope = DeterministicEnvelope {
            version: 1,
            network_id: 1,
            message_type: MessageType::Message as u16,
            envelope_id: [0u8; 32],
            mission_id: ctx.mission_id,
            source_peer: ctx.source_peer,
            origin_gateway: ctx.origin_gateway,
            logical_timestamp: 0,
            ttl_hops: 10,
            payload_hash: *blake3::hash(payload).as_bytes(),
            route_trace_root: [0u8; 32],
            flags: 0,
            signature: [0u8; 64],
        };
        envelope.envelope_id = envelope.derive_envelope_id();
        envelope
    }
}

fn adapter_error_to_transport(e: PlatformAdapterError) -> TransportError {
    TransportError::AdapterFailure(format!("{e}"))
}

#[async_trait]
impl NetworkSender for PlatformAdapterBridge {
    async fn send(&self, payload: &[u8], ctx: &SendContext) -> Result<(), TransportError> {
        self.adapter
            .health_check()
            .await
            .map_err(|_e| TransportError::Unhealthy)?;
        let envelope = Self::build_envelope(payload, ctx);
        self.adapter
            .send_envelope(&self.domain, &envelope)
            .await
            .map_err(adapter_error_to_transport)?;
        Ok(())
    }

    fn name(&self) -> &str {
        self.adapter.platform_type().name()
    }

    fn is_healthy(&self) -> bool {
        true
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use async_trait::async_trait;
    use octo_network::dot::adapters::{
        CapabilityReport, DeliveryReceipt, PlatformAdapter, RawPlatformMessage,
    };
    use octo_network::dot::envelope::DeterministicEnvelope;
    use octo_network::dot::error::PlatformAdapterError;
    use octo_network::dot::{BroadcastDomainId, PlatformType};

    use crate::adapter_bridge::PlatformAdapterBridge;
    use crate::sender::{NetworkSender, SendContext, TransportError};

    /// Mock adapter that always succeeds.
    struct MockAdapter {
        platform_type: PlatformType,
    }

    impl MockAdapter {
        fn new(pt: PlatformType) -> Self {
            Self { platform_type: pt }
        }
    }

    #[async_trait]
    impl PlatformAdapter for MockAdapter {
        async fn send_envelope(
            &self,
            _domain: &BroadcastDomainId,
            _envelope: &DeterministicEnvelope,
        ) -> Result<DeliveryReceipt, PlatformAdapterError> {
            Ok(DeliveryReceipt {
                platform_message_id: "mock-001".to_string(),
                delivered_at: 1000,
            })
        }

        async fn receive_messages(
            &self,
            _domain: &BroadcastDomainId,
        ) -> Result<Vec<RawPlatformMessage>, PlatformAdapterError> {
            Ok(vec![])
        }

        fn canonicalize(
            &self,
            _raw: &RawPlatformMessage,
        ) -> Result<DeterministicEnvelope, PlatformAdapterError> {
            Ok(DeterministicEnvelope::default())
        }

        fn capabilities(&self) -> CapabilityReport {
            CapabilityReport {
                max_payload_bytes: 4096,
                supports_fragmentation: false,
                supports_encryption: false,
                supports_raw_binary: true,
                rate_limit_per_second: 100,
                media_capabilities: None,
            }
        }

        fn domain_id(&self, platform_id: &str) -> BroadcastDomainId {
            BroadcastDomainId::new(self.platform_type, platform_id)
        }

        fn platform_type(&self) -> PlatformType {
            self.platform_type
        }
    }

    /// Mock adapter that always fails.
    struct FailingMockAdapter;

    #[async_trait]
    impl PlatformAdapter for FailingMockAdapter {
        async fn send_envelope(
            &self,
            _domain: &BroadcastDomainId,
            _envelope: &DeterministicEnvelope,
        ) -> Result<DeliveryReceipt, PlatformAdapterError> {
            Err(PlatformAdapterError::Unreachable {
                platform: "mock-fail".to_string(),
                reason: "simulated failure".to_string(),
            })
        }

        async fn receive_messages(
            &self,
            _domain: &BroadcastDomainId,
        ) -> Result<Vec<RawPlatformMessage>, PlatformAdapterError> {
            Ok(vec![])
        }

        fn canonicalize(
            &self,
            _raw: &RawPlatformMessage,
        ) -> Result<DeterministicEnvelope, PlatformAdapterError> {
            Ok(DeterministicEnvelope::default())
        }

        fn capabilities(&self) -> CapabilityReport {
            CapabilityReport {
                max_payload_bytes: 4096,
                supports_fragmentation: false,
                supports_encryption: false,
                supports_raw_binary: true,
                rate_limit_per_second: 100,
                media_capabilities: None,
            }
        }

        fn domain_id(&self, platform_id: &str) -> BroadcastDomainId {
            BroadcastDomainId::new(PlatformType::Webhook, platform_id)
        }

        fn platform_type(&self) -> PlatformType {
            PlatformType::Webhook
        }
    }

    fn test_domain() -> BroadcastDomainId {
        BroadcastDomainId::new(PlatformType::Webhook, "test.example.com")
    }

    fn test_ctx() -> SendContext {
        SendContext {
            mission_id: [1u8; 32],
            domain: Some(test_domain()),
            priority: 128,
            source_peer: [0xAAu8; 32],
            origin_gateway: [0xBBu8; 32],
        }
    }

    // === NetworkSender trait tests ===

    #[tokio::test]
    async fn bridge_send_success() {
        let adapter: Arc<dyn PlatformAdapter> = Arc::new(MockAdapter::new(PlatformType::Telegram));
        let bridge = PlatformAdapterBridge::new(adapter, test_domain());

        assert!(bridge.is_healthy());
        assert_eq!(bridge.name(), "telegram");

        let result = bridge.send(b"hello world", &test_ctx()).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn bridge_send_failure() {
        let adapter: Arc<dyn PlatformAdapter> = Arc::new(FailingMockAdapter);
        let bridge = PlatformAdapterBridge::new(adapter, test_domain());

        let result = bridge.send(b"payload", &test_ctx()).await;
        assert!(result.is_err());
        match result.unwrap_err() {
            TransportError::AdapterFailure(msg) => {
                assert!(msg.contains("mock-fail"));
            }
            other => panic!("expected AdapterFailure, got: {other:?}"),
        }
    }

    #[tokio::test]
    async fn bridge_name_matches_platform_type() {
        let pairs = vec![
            (PlatformType::Discord, "discord"),
            (PlatformType::Matrix, "matrix"),
            (PlatformType::Nostr, "nostr"),
            (PlatformType::Signal, "signal"),
            (PlatformType::IRC, "irc"),
            (PlatformType::Slack, "slack"),
            (PlatformType::WhatsApp, "whatsapp"),
            (PlatformType::Webhook, "webhook"),
            (PlatformType::NativeP2P, "native-p2p"),
            (PlatformType::Bluetooth, "bluetooth"),
            (PlatformType::LoRa, "lora"),
            (PlatformType::WebRTC, "webrtc"),
            (PlatformType::Bluesky, "bluesky"),
            (PlatformType::Twitter, "twitter"),
            (PlatformType::Reddit, "reddit"),
            (PlatformType::WeChat, "wechat"),
            (PlatformType::DingTalk, "dingtalk"),
            (PlatformType::Lark, "lark"),
            (PlatformType::QQ, "qq"),
            (PlatformType::Quic, "quic"),
        ];

        for (pt, expected_name) in pairs {
            let adapter: Arc<dyn PlatformAdapter> = Arc::new(MockAdapter::new(pt));
            let bridge = PlatformAdapterBridge::new(adapter, test_domain());
            assert_eq!(bridge.name(), expected_name, "wrong name for {pt:?}");
        }
    }

    // === SendContext tests ===

    #[test]
    fn send_context_construction() {
        let ctx = SendContext {
            mission_id: [42u8; 32],
            domain: None,
            priority: 255,
            source_peer: [0x11u8; 32],
            origin_gateway: [0x22u8; 32],
        };
        assert_eq!(ctx.mission_id, [42u8; 32]);
        assert!(ctx.domain.is_none());
        assert_eq!(ctx.priority, 255);
        assert_eq!(ctx.source_peer, [0x11u8; 32]);
        assert_eq!(ctx.origin_gateway, [0x22u8; 32]);
    }

    // === TransportError tests ===

    #[test]
    fn transport_error_display() {
        let e1 = TransportError::AdapterFailure("test".to_string());
        assert!(format!("{e1}").contains("test"));

        let e2 = TransportError::AllTransportsFailed;
        assert!(format!("{e2}").contains("all transports failed"));

        let e3 = TransportError::EnvelopeConstruction("bad payload".to_string());
        assert!(format!("{e3}").contains("bad payload"));

        let e4 = TransportError::Unhealthy;
        assert!(format!("{e4}").contains("unhealthy"));
    }

    // === Envelope construction tests ===

    #[tokio::test]
    async fn bridge_constructs_valid_envelope() {
        let adapter: Arc<dyn PlatformAdapter> = Arc::new(MockAdapter::new(PlatformType::Webhook));
        let bridge = PlatformAdapterBridge::new(adapter, test_domain());

        let ctx = SendContext {
            mission_id: [0xABu8; 32],
            domain: Some(test_domain()),
            priority: 100,
            source_peer: [0xCCu8; 32],
            origin_gateway: [0xDDu8; 32],
        };

        let result = bridge.send(b"test payload", &ctx).await;
        assert!(result.is_ok());
    }

    // === Integration: NetworkSender as trait object ===

    #[tokio::test]
    async fn bridge_as_trait_object() {
        let adapter: Arc<dyn PlatformAdapter> = Arc::new(MockAdapter::new(PlatformType::Telegram));
        let bridge: Arc<dyn NetworkSender> =
            Arc::new(PlatformAdapterBridge::new(adapter, test_domain()));

        assert!(bridge.is_healthy());
        assert_eq!(bridge.name(), "telegram");
        assert!(bridge.send(b"data", &test_ctx()).await.is_ok());
    }
}
