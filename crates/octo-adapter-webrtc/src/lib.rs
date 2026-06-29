//! WebRTC data channel adapter for DOT (RFC-0850 §8.1, PlatformType::WebRTC)
//!
//! Stub implementation for WebRTC data channel transport. Full implementation
//! requires the `webrtc-rs` crate for peer connection management.
//!
//! ## Configuration
//!
//! ```json
//! {
//!   "signaling_url": "https://signal.example.com/offer",
//!   "ice_servers": ["stun:stun.l.google.com:19302"],
//!   "peer_id": "peer-abc123"
//! }
//! ```

use async_trait::async_trait;
use base64::Engine;
use serde::Deserialize;
use tokio::sync::Mutex;

use octo_network::dot::adapters::{
    backoff::RetryConfig, CapabilityReport, DeliveryReceipt, PlatformAdapter, RawPlatformMessage,
};
use octo_network::dot::domain::{BroadcastDomainId, PlatformType};
use octo_network::dot::envelope::DeterministicEnvelope;
use octo_network::dot::error::PlatformAdapterError;

// ── Configuration ──────────────────────────────────────────────────

#[derive(Clone, Debug, Deserialize, serde::Serialize)]
pub struct WebRTCConfig {
    /// Signaling server URL for peer discovery
    pub signaling_url: String,
    /// ICE/STUN/TURN server URLs
    #[serde(default)]
    pub ice_servers: Vec<String>,
    /// Peer identifier
    pub peer_id: String,
}

// ── Adapter ────────────────────────────────────────────────────────

pub struct WebRTCAdapter {
    config: WebRTCConfig,
    /// Buffered messages for stub send/receive
    pending: Mutex<Vec<RawPlatformMessage>>,
}

impl WebRTCAdapter {
    pub fn new(config: WebRTCConfig) -> Self {
        Self {
            config,
            pending: Mutex::new(Vec::new()),
        }
    }

    pub fn from_config_bytes(config: &[u8]) -> Result<Self, String> {
        let config: WebRTCConfig =
            serde_json::from_slice(config).map_err(|e| format!("Invalid config: {}", e))?;
        Ok(Self::new(config))
    }

    /// Domain hash: `BLAKE3-256("webrtc:{peer_id}")`
    pub fn domain_hash(peer_id: &str) -> [u8; 32] {
        let normalized = peer_id.trim().to_lowercase();
        *blake3::hash(format!("webrtc:{}", normalized).as_bytes()).as_bytes()
    }

    pub const PLATFORM_TYPE: u16 = 0x000D;
    pub fn max_payload_bytes() -> usize {
        262_144
    }
    pub fn rate_limit_per_second() -> u32 {
        1000
    }

    /// Encode an envelope as base64 with DOT/1/ prefix.
    pub fn encode_envelope(envelope_bytes: &[u8]) -> String {
        format!(
            "DOT/1/{}",
            base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(envelope_bytes)
        )
    }

    /// Decode a DOT/1/-prefixed base64 envelope.
    pub fn decode_envelope(text: &str) -> Result<Vec<u8>, String> {
        let text = text.trim();
        let b64 = text
            .strip_prefix("DOT/1/")
            .ok_or_else(|| "Missing DOT/1/ prefix".to_string())?;
        base64::engine::general_purpose::URL_SAFE_NO_PAD
            .decode(b64)
            .map_err(|e| format!("Base64 decode error: {}", e))
    }
}

// ── PlatformAdapter ────────────────────────────────────────────────

fn transport_err(msg: impl Into<String>) -> PlatformAdapterError {
    PlatformAdapterError::Unreachable {
        platform: "webrtc".into(),
        reason: msg.into(),
    }
}

#[async_trait]
impl PlatformAdapter for WebRTCAdapter {
    async fn send_message(
        &self,
        _domain: &BroadcastDomainId,
        envelope: &DeterministicEnvelope,
        _payload: &[u8],
    ) -> Result<DeliveryReceipt, PlatformAdapterError> {
        let wire_bytes = envelope.to_wire_bytes();

        // Stub: buffer message for signaling server delivery
        let encoded = Self::encode_envelope(&wire_bytes);
        let mut metadata = std::collections::BTreeMap::new();
        metadata.insert("peer_id".into(), self.config.peer_id.clone());
        metadata.insert("signaling_url".into(), self.config.signaling_url.clone());

        let _peer_id = self
            .config
            .ice_servers
            .first()
            .map(|s| s.as_str())
            .unwrap_or("default");

        let msg = RawPlatformMessage {
            platform_id: format!("rtc-{}", epoch_millis()),
            payload: encoded.into_bytes(),
            metadata,
        };

        let mut pending = self.pending.lock().await;
        pending.push(msg);

        let retry = RetryConfig::default();
        let _ = retry; // Stub: no actual retries needed

        Ok(DeliveryReceipt {
            platform_message_id: format!("rtc-{}", epoch_millis()),
            delivered_at: epoch_millis(),
        })
    }

    async fn receive_messages(
        &self,
        _domain: &BroadcastDomainId,
    ) -> Result<Vec<RawPlatformMessage>, PlatformAdapterError> {
        // Stub: return buffered messages (in production, would come from data channel)
        let mut pending = self.pending.lock().await;
        Ok(std::mem::take(&mut *pending))
    }

    fn canonicalize(
        &self,
        raw: &RawPlatformMessage,
    ) -> Result<DeterministicEnvelope, PlatformAdapterError> {
        if raw.payload.is_empty() {
            return Err(transport_err("Empty payload"));
        }
        // Payload may be DOT/1/-encoded text or raw wire bytes
        if let Ok(text) = std::str::from_utf8(&raw.payload) {
            if let Ok(decoded) = Self::decode_envelope(text) {
                return DeterministicEnvelope::from_wire_bytes(&decoded).map_err(|e| {
                    PlatformAdapterError::ApiError {
                        code: 400,
                        message: format!("canonicalize failed: {e}"),
                    }
                });
            }
        }
        DeterministicEnvelope::from_wire_bytes(&raw.payload).map_err(|e| {
            PlatformAdapterError::ApiError {
                code: 400,
                message: format!("canonicalize failed: {e}"),
            }
        })
    }

    fn capabilities(&self) -> CapabilityReport {
        CapabilityReport {
            max_payload_bytes: Self::max_payload_bytes(),
            supports_fragmentation: false,
            supports_encryption: true, // WebRTC data channels are encrypted via DTLS
            supports_raw_binary: true,
            rate_limit_per_second: Self::rate_limit_per_second(),
            media_capabilities: None,

            ..Default::default()
        }
    }

    fn domain_id(&self, platform_id: &str) -> BroadcastDomainId {
        BroadcastDomainId::new(PlatformType::WebRTC, platform_id)
    }

    fn platform_type(&self) -> PlatformType {
        PlatformType::WebRTC
    }

    async fn shutdown(&self) -> Result<(), PlatformAdapterError> {
        Ok(())
    }
    async fn health_check(&self) -> Result<(), PlatformAdapterError> {
        // Stub: check signaling URL reachability
        let timeout = std::time::Duration::from_secs(5);
        let client = reqwest::Client::new();
        match tokio::time::timeout(timeout, client.head(&self.config.signaling_url).send()).await {
            Ok(Ok(_)) => Ok(()),
            Ok(Err(e)) => Err(transport_err(format!("Health check failed: {e}"))),
            Err(_) => Err(transport_err("Health check timed out")),
        }
    }
}

fn epoch_millis() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

// ── Plugin ABI ─────────────────────────────────────────────────────

#[no_mangle]
pub extern "C" fn adapter_version() -> u32 {
    1
}

#[no_mangle]
pub extern "C" fn platform_type() -> u16 {
    0x000D
}

#[no_mangle]
/// # Safety
/// `config` must point to a valid buffer of at least `len` bytes.
pub unsafe extern "C" fn create_adapter(config: *const u8, config_len: usize) -> *mut () {
    if config.is_null() || config_len == 0 {
        return std::ptr::null_mut();
    }
    let bytes = std::slice::from_raw_parts(config, config_len);
    match WebRTCAdapter::from_config_bytes(bytes) {
        Ok(a) => Box::into_raw(Box::new(a)) as *mut (),
        Err(_) => std::ptr::null_mut(),
    }
}

#[no_mangle]
/// # Safety
/// `ptr` must be a pointer previously returned by `create_adapter`.
pub unsafe extern "C" fn destroy_adapter(adapter: *mut ()) {
    if !adapter.is_null() {
        let _ = Box::from_raw(adapter as *mut WebRTCAdapter);
    }
}

// ── Tests ──────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_domain_hash_deterministic() {
        let h1 = WebRTCAdapter::domain_hash("peer-abc123");
        let h2 = WebRTCAdapter::domain_hash("peer-abc123");
        assert_eq!(h1, h2);
    }

    #[test]
    fn test_domain_hash_normalized() {
        assert_eq!(
            WebRTCAdapter::domain_hash("PEER-ABC123"),
            WebRTCAdapter::domain_hash("  peer-abc123  ")
        );
    }

    #[test]
    fn test_encode_decode_envelope() {
        let original = b"test webrtc envelope";
        let encoded = WebRTCAdapter::encode_envelope(original);
        assert!(encoded.starts_with("DOT/1/"));
        let decoded = WebRTCAdapter::decode_envelope(&encoded).unwrap();
        assert_eq!(decoded, original);
    }

    #[test]
    fn test_platform_type() {
        assert_eq!(WebRTCAdapter::PLATFORM_TYPE, 0x000D);
    }

    #[test]
    fn test_abi_exports() {
        assert_eq!(adapter_version(), 1);
        assert_eq!(platform_type(), 0x000D);
    }

    #[test]
    fn test_config_from_json() {
        let json = serde_json::json!({
            "signaling_url": "https://signal.example.com/offer",
            "ice_servers": ["stun:stun.l.google.com:19302"],
            "peer_id": "peer-abc123"
        });
        let a = WebRTCAdapter::from_config_bytes(serde_json::to_vec(&json).unwrap().as_slice())
            .unwrap();
        assert_eq!(a.config.signaling_url, "https://signal.example.com/offer");
        assert_eq!(a.config.ice_servers.len(), 1);
        assert_eq!(a.config.peer_id, "peer-abc123");
    }

    #[test]
    fn test_capabilities() {
        let a = WebRTCAdapter::new(WebRTCConfig {
            signaling_url: "https://signal.example.com".into(),
            ice_servers: vec![],
            peer_id: "peer-1".into(),
        });
        let c = a.capabilities();
        assert_eq!(c.max_payload_bytes, 262_144);
        assert!(!c.supports_fragmentation);
        assert!(c.supports_encryption);
        assert_eq!(c.rate_limit_per_second, 1000);
    }
}
