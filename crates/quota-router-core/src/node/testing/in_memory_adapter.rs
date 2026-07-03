//! In-memory `PlatformAdapter` for Layer 2 tests.
//!
//! Routes messages through the full production path:
//! `send()` → `PlatformAdapterBridge::send()` → `adapter.send_message()`
//! → mpsc inbox → `adapter.receive_messages()` → `canonicalize()`
//! → `NodeTransport::dispatch()` → handler.

use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use octo_network::dot::adapters::{
    CapabilityReport, DeliveryReceipt, PlatformAdapter, RawPlatformMessage,
};
use octo_network::dot::domain::{BroadcastDomainId, PlatformType};
use octo_network::dot::envelope::{DeterministicEnvelope, ENVELOPE_WIRE_LEN};
use octo_network::dot::error::PlatformAdapterError;

pub type PeerInboxMap = Arc<
    Mutex<
        BTreeMap<[u8; 32], tokio::sync::mpsc::Sender<(Vec<u8>, Vec<u8>)>>,
    >,
>;

#[allow(clippy::type_complexity)]
pub struct InMemoryChannelAdapter {
    peer_inboxes: PeerInboxMap,
    self_id: [u8; 32],
    rx: Arc<tokio::sync::Mutex<tokio::sync::mpsc::Receiver<(Vec<u8>, Vec<u8>)>>>,
    platform_type: PlatformType,
    platform_id: String,
}

impl InMemoryChannelAdapter {
    pub fn new(
        peer_inboxes: PeerInboxMap,
        self_id: [u8; 32],
        platform_type: PlatformType,
        platform_id: &str,
    ) -> Self {
        let (tx, rx) = tokio::sync::mpsc::channel(256);
        peer_inboxes
            .lock()
            .unwrap()
            .insert(self_id, tx);
        Self {
            peer_inboxes,
            self_id,
            rx: Arc::new(tokio::sync::Mutex::new(rx)),
            platform_type,
            platform_id: platform_id.to_string(),
        }
    }
}

#[async_trait]
impl PlatformAdapter for InMemoryChannelAdapter {
    async fn send_message(
        &self,
        _domain: &BroadcastDomainId,
        envelope: &DeterministicEnvelope,
        payload: &[u8],
    ) -> Result<DeliveryReceipt, PlatformAdapterError> {
        let envelope_bytes = envelope.to_wire_bytes();
        let inboxes = self.peer_inboxes.lock().unwrap();
        for (id, tx) in inboxes.iter() {
            if *id != self.self_id {
                let _ = tx.try_send((envelope_bytes.clone(), payload.to_vec()));
            }
        }
        Ok(DeliveryReceipt {
            platform_message_id: format!("mem-{}", self.platform_id),
            delivered_at: 0,
        })
    }

    async fn receive_messages(
        &self,
        _domain: &BroadcastDomainId,
    ) -> Result<Vec<RawPlatformMessage>, PlatformAdapterError> {
        let mut rx = self.rx.lock().await;
        let mut messages = Vec::new();
        while let Ok((envelope_bytes, payload_bytes)) = rx.try_recv() {
            let mut combined =
                Vec::with_capacity(envelope_bytes.len() + payload_bytes.len());
            combined.extend_from_slice(&envelope_bytes);
            combined.extend_from_slice(&payload_bytes);
            messages.push(RawPlatformMessage {
                platform_id: self.platform_id.clone(),
                payload: combined,
                metadata: BTreeMap::new(),
            });
        }
        Ok(messages)
    }

    fn canonicalize(
        &self,
        raw: &RawPlatformMessage,
    ) -> Result<DeterministicEnvelope, PlatformAdapterError> {
        if raw.payload.len() < ENVELOPE_WIRE_LEN {
            return Err(PlatformAdapterError::ApiError {
                code: 400,
                message: format!(
                    "frame too short: {} bytes, need {}",
                    raw.payload.len(),
                    ENVELOPE_WIRE_LEN
                ),
            });
        }
        DeterministicEnvelope::from_wire_bytes(&raw.payload[..ENVELOPE_WIRE_LEN]).map_err(|e| {
            PlatformAdapterError::ApiError {
                code: 400,
                message: format!("envelope parse error: {e}"),
            }
        })
    }

    fn capabilities(&self) -> CapabilityReport {
        CapabilityReport {
            max_payload_bytes: 16 * 1024 * 1024,
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

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn send_broadcasts_to_peers() {
        let inboxes: PeerInboxMap = Arc::new(Mutex::new(BTreeMap::new()));
        let adapter_a = InMemoryChannelAdapter::new(
            inboxes.clone(),
            [1u8; 32],
            PlatformType::NativeP2P,
            "node-a",
        );
        let adapter_b = InMemoryChannelAdapter::new(
            inboxes.clone(),
            [2u8; 32],
            PlatformType::NativeP2P,
            "node-b",
        );

        let domain = BroadcastDomainId::new(PlatformType::NativeP2P, "test");
        let envelope = DeterministicEnvelope::default();
        adapter_a
            .send_message(&domain, &envelope, b"ping")
            .await
            .unwrap();

        let msgs = adapter_b.receive_messages(&domain).await.unwrap();
        assert_eq!(msgs.len(), 1);

        let parsed = adapter_b.canonicalize(&msgs[0]).unwrap();
        assert_eq!(parsed.envelope_id, envelope.envelope_id);
    }

    #[tokio::test]
    async fn no_self_delivery() {
        let inboxes: PeerInboxMap = Arc::new(Mutex::new(BTreeMap::new()));
        let adapter = InMemoryChannelAdapter::new(
            inboxes,
            [1u8; 32],
            PlatformType::NativeP2P,
            "lonely",
        );

        let domain = BroadcastDomainId::new(PlatformType::NativeP2P, "test");
        let envelope = DeterministicEnvelope::default();
        adapter
            .send_message(&domain, &envelope, b"echo")
            .await
            .unwrap();

        let msgs = adapter.receive_messages(&domain).await.unwrap();
        assert!(msgs.is_empty(), "should not receive own messages");
    }

    #[test]
    fn canonicalize_short_frame_errors() {
        let inboxes: PeerInboxMap = Arc::new(Mutex::new(BTreeMap::new()));
        let adapter = InMemoryChannelAdapter::new(
            inboxes,
            [1u8; 32],
            PlatformType::NativeP2P,
            "t",
        );
        let raw = RawPlatformMessage {
            platform_id: "t".into(),
            payload: vec![0u8; 10],
            metadata: BTreeMap::new(),
        };
        assert!(adapter.canonicalize(&raw).is_err());
    }
}
