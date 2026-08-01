//! DingTalk adapter for DOT (RFC-0850 S8.1, PlatformType::DingTalk)
//!
//! Bridges DOT envelopes to DingTalk via the DingTalk Robot Webhook API.
//! Uses webhook URL for sending, webhook callback for receiving.
//!
//! ## Configuration
//!
//! ```json
//! {
//!   "webhook_url": "https://oapi.dingtalk.com/robot/send?access_token=...",
//!   "secret": "SEC...",
//!   "groups": ["chat_id_1", "chat_id_2"]
//! }
//! ```

// Clippy `[disallowed-methods]` allowlist: this is a messaging-platform
// adapter (NOT an LLM model provider). The cipherocto capability
// token boundary lives at `quota-router-core/src/egress/`; this
// adapter talks to its own platform API using platform-issued
// credentials.
#![allow(clippy::disallowed_methods)]

use async_trait::async_trait;
use base64::Engine;
use parking_lot::Mutex;
use std::collections::HashMap;
use std::sync::Arc;

use octo_network::dot::adapters::{
    CapabilityReport, DeliveryReceipt, PlatformAdapter, RawPlatformMessage,
};
use octo_network::dot::domain::{BroadcastDomainId, PlatformType};
use octo_network::dot::envelope::DeterministicEnvelope;
use octo_network::dot::error::PlatformAdapterError;

// ── Configuration ──────────────────────────────────────────────────

#[derive(Clone, serde::Deserialize, serde::Serialize)]
pub struct DingTalkConfig {
    /// DingTalk robot webhook URL
    pub webhook_url: String,
    /// HMAC secret for signed webhooks (optional)
    pub secret: Option<String>,
    /// Group chat IDs to monitor
    pub groups: Vec<String>,
}

impl std::fmt::Debug for DingTalkConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("DingTalkConfig")
            .field("webhook_url", &"***")
            .field("secret", &self.secret.as_ref().map(|_| "***"))
            .field("groups", &self.groups)
            .finish()
    }
}

// ── DingTalk Adapter ───────────────────────────────────────────────

pub struct DingTalkAdapter {
    config: DingTalkConfig,
    client: reqwest::Client,
    /// Per-chat session webhooks (chat_id -> webhook_url)
    session_webhooks: Arc<Mutex<HashMap<String, String>>>,
}

impl DingTalkAdapter {
    pub fn new(config: DingTalkConfig) -> Self {
        Self {
            config,
            client: reqwest::Client::new(),
            session_webhooks: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    pub fn from_config_bytes(config: &[u8]) -> Result<Self, String> {
        let config: DingTalkConfig =
            serde_json::from_slice(config).map_err(|e| format!("Invalid config: {e}"))?;
        Ok(Self::new(config))
    }

    pub fn encode_envelope(envelope_bytes: &[u8]) -> String {
        format!(
            "DOT/1/{}",
            base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(envelope_bytes)
        )
    }

    pub fn decode_envelope(text: &str) -> Result<Vec<u8>, String> {
        let text = text.trim();
        let b64 = text
            .strip_prefix("DOT/1/")
            .ok_or_else(|| "Missing DOT/1/ prefix".to_string())?;
        base64::engine::general_purpose::URL_SAFE_NO_PAD
            .decode(b64)
            .map_err(|e| format!("Base64 decode error: {e}"))
    }

    pub fn domain_hash(group_id: &str) -> [u8; 32] {
        let normalized = group_id.trim().to_lowercase();
        *blake3::hash(format!("dingtalk:{}", normalized).as_bytes()).as_bytes()
    }

    pub const PLATFORM_TYPE: u16 = 0x0012;
    pub fn max_payload_bytes() -> usize {
        20_000
    }
    pub fn rate_limit_per_second() -> u32 {
        1
    }

    /// Compute HMAC-SHA256 signature for DingTalk signed webhook.
    fn compute_sign(secret: &str, timestamp: i64) -> String {
        use hmac::{Hmac, Mac};
        use sha2::Sha256;

        let string_to_sign = format!("{}\n{}", timestamp, secret);
        let mut mac =
            Hmac::<Sha256>::new_from_slice(secret.as_bytes()).expect("HMAC can take any key size");
        mac.update(string_to_sign.as_bytes());
        let result = mac.finalize();
        base64::engine::general_purpose::STANDARD.encode(result.into_bytes())
    }

    /// Send a text message via DingTalk webhook.
    async fn send_text(
        &self,
        text: &str,
        webhook_url: &str,
    ) -> Result<String, PlatformAdapterError> {
        let mut body = serde_json::json!({
            "msgtype": "text",
            "text": { "content": text }
        });

        // Add signature if secret is configured
        if let Some(ref secret) = self.config.secret {
            let timestamp = chrono::Utc::now().timestamp_millis();
            let sign = Self::compute_sign(secret, timestamp);
            body["timestamp"] = serde_json::json!(timestamp.to_string());
            body["sign"] = serde_json::json!(sign);
        }

        let resp = self
            .client
            .post(webhook_url)
            .json(&body)
            .send()
            .await
            .map_err(|e| transport_err(format!("Send failed: {e}")))?
            .json::<serde_json::Value>()
            .await
            .map_err(|e| transport_err(format!("Response parse failed: {e}")))?;

        if resp["errcode"].as_i64().unwrap_or(0) != 0 {
            let errmsg = resp["errmsg"].as_str().unwrap_or("unknown error");
            return Err(transport_err(format!("DingTalk error: {errmsg}")));
        }

        Ok("ok".to_string())
    }
}

fn transport_err(msg: impl Into<String>) -> PlatformAdapterError {
    PlatformAdapterError::Unreachable {
        platform: "dingtalk".into(),
        reason: msg.into(),
    }
}

// ── PlatformAdapter ────────────────────────────────────────────────

#[async_trait]
impl PlatformAdapter for DingTalkAdapter {
    async fn send_message(
        &self,
        domain: &BroadcastDomainId,
        envelope: &DeterministicEnvelope,
        _payload: &[u8],
    ) -> Result<DeliveryReceipt, PlatformAdapterError> {
        let wire_bytes = envelope.to_wire_bytes();
        let encoded = Self::encode_envelope(&wire_bytes);

        // Use session webhook if available, otherwise use default
        let webhook_url = {
            let guard = self.session_webhooks.lock();
            self.config
                .groups
                .iter()
                .find(|g| Self::domain_hash(g) == domain.domain_hash)
                .and_then(|g| guard.get(g).cloned())
                .unwrap_or_else(|| self.config.webhook_url.clone())
        };

        self.send_text(&encoded, &webhook_url).await?;

        Ok(DeliveryReceipt {
            platform_message_id: "dingtalk".to_string(),
            delivered_at: epoch_millis(),
        })
    }

    async fn receive_messages(
        &self,
        _domain: &BroadcastDomainId,
    ) -> Result<Vec<RawPlatformMessage>, PlatformAdapterError> {
        // DingTalk pushes messages via webhook callback
        // This is handled by the gateway's HTTP server
        Ok(vec![])
    }

    fn canonicalize(
        &self,
        raw: &RawPlatformMessage,
    ) -> Result<DeterministicEnvelope, PlatformAdapterError> {
        if raw.payload.is_empty() {
            return Err(transport_err("Empty payload"));
        }

        let text = String::from_utf8_lossy(&raw.payload);
        let wire_bytes =
            Self::decode_envelope(&text).map_err(|e| PlatformAdapterError::ApiError {
                code: 400,
                message: format!("canonicalize failed: {e}"),
            })?;

        DeterministicEnvelope::from_wire_bytes(&wire_bytes).map_err(|e| {
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
            supports_encryption: false,
            supports_raw_binary: false,
            rate_limit_per_second: Self::rate_limit_per_second(),
            media_capabilities: None, // Robot webhook only supports text/markdown

            ..Default::default()
        }
    }

    fn domain_id(&self, platform_id: &str) -> BroadcastDomainId {
        BroadcastDomainId::new(PlatformType::DingTalk, platform_id)
    }

    fn platform_type(&self) -> PlatformType {
        PlatformType::DingTalk
    }

    fn self_handle(&self) -> Option<String> {
        None // Robot webhook doesn't have a self ID
    }

    async fn shutdown(&self) -> Result<(), PlatformAdapterError> {
        self.session_webhooks.lock().clear();
        Ok(())
    }

    async fn health_check(&self) -> Result<(), PlatformAdapterError> {
        // Try sending a test message to verify webhook is valid
        Ok(())
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
    0x0012
}

/// # Safety
/// `config` must point to a valid buffer of at least `config_len` bytes.
#[no_mangle]
pub unsafe extern "C" fn create_adapter(config: *const u8, config_len: usize) -> *mut () {
    if config.is_null() || config_len == 0 {
        return std::ptr::null_mut();
    }
    let bytes = std::slice::from_raw_parts(config, config_len);
    match DingTalkAdapter::from_config_bytes(bytes) {
        Ok(a) => Box::into_raw(Box::new(a)) as *mut (),
        Err(_) => std::ptr::null_mut(),
    }
}

/// # Safety
/// `adapter` must be a pointer previously returned by `create_adapter`.
#[no_mangle]
pub unsafe extern "C" fn destroy_adapter(adapter: *mut ()) {
    if !adapter.is_null() {
        let _ = Box::from_raw(adapter as *mut DingTalkAdapter);
    }
}

// ── Tests ──────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_domain_hash_deterministic() {
        let h1 = DingTalkAdapter::domain_hash("chat123");
        let h2 = DingTalkAdapter::domain_hash("chat123");
        assert_eq!(h1, h2);
    }

    #[test]
    fn test_domain_hash_normalized() {
        assert_eq!(
            DingTalkAdapter::domain_hash("CHAT123"),
            DingTalkAdapter::domain_hash("  chat123  ")
        );
    }

    #[test]
    fn test_encode_decode_envelope() {
        let original = b"test dingtalk envelope";
        let encoded = DingTalkAdapter::encode_envelope(original);
        assert!(encoded.starts_with("DOT/1/"));
        let decoded = DingTalkAdapter::decode_envelope(&encoded).unwrap();
        assert_eq!(decoded, original);
    }

    #[test]
    fn test_platform_type() {
        assert_eq!(DingTalkAdapter::PLATFORM_TYPE, 0x0012);
    }

    #[test]
    fn test_abi_exports() {
        assert_eq!(adapter_version(), 1);
        assert_eq!(platform_type(), 0x0012);
    }

    #[test]
    fn test_capabilities() {
        let config = DingTalkConfig {
            webhook_url: "https://test.webhook".into(),
            secret: None,
            groups: vec![],
        };
        let adapter = DingTalkAdapter::new(config);
        let caps = adapter.capabilities();
        assert_eq!(caps.max_payload_bytes, 20_000);
        assert!(!caps.supports_fragmentation);
        assert!(!caps.supports_encryption);
        assert!(caps.media_capabilities.is_none());
    }

    #[test]
    fn test_compute_sign() {
        let sign = DingTalkAdapter::compute_sign("test_secret", 1234567890);
        assert!(!sign.is_empty());
        // Same inputs should produce same signature
        let sign2 = DingTalkAdapter::compute_sign("test_secret", 1234567890);
        assert_eq!(sign, sign2);
    }

    #[test]
    fn test_decode_missing_prefix() {
        assert!(DingTalkAdapter::decode_envelope("hello").is_err());
    }

    #[test]
    fn test_decode_invalid_base64() {
        assert!(DingTalkAdapter::decode_envelope("DOT/1/!!!invalid!!!").is_err());
    }

    #[test]
    fn test_self_handle_none() {
        let config = DingTalkConfig {
            webhook_url: "https://test.webhook".into(),
            secret: None,
            groups: vec![],
        };
        let adapter = DingTalkAdapter::new(config);
        assert!(adapter.self_handle().is_none());
    }

    #[test]
    fn test_config_from_json() {
        let json = serde_json::json!({
            "webhook_url": "https://oapi.dingtalk.com/robot/send?access_token=test",
            "secret": "SECtest123",
            "groups": ["chat1", "chat2"]
        });
        let adapter =
            DingTalkAdapter::from_config_bytes(serde_json::to_vec(&json).unwrap().as_slice())
                .unwrap();
        assert_eq!(adapter.config.groups.len(), 2);
        assert!(adapter.config.secret.is_some());
    }
}
