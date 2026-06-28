//! Slack adapter for DOT (RFC-0850 §8.1, PlatformType::Slack)
//!
//! Uses Slack Web API for sending and Events API for receiving.
//!
//! ## Configuration
//!
//! ```json
//! {
//!   "bot_token": "xoxb-...",
//!   "channels": ["C0123456789"]
//! }
//! ```

use async_trait::async_trait;
use base64::Engine;
use std::collections::BTreeMap;
use tokio::sync::Mutex;

use octo_network::dot::adapters::{
    backoff::RetryConfig, CapabilityReport, DeliveryReceipt, PlatformAdapter, RawPlatformMessage,
};
use octo_network::dot::domain::{BroadcastDomainId, PlatformType};
use octo_network::dot::envelope::DeterministicEnvelope;
use octo_network::dot::error::PlatformAdapterError;

#[derive(Clone, Debug, serde::Deserialize, serde::Serialize)]
pub struct SlackConfig {
    pub bot_token: String,
    pub channels: Vec<String>,
}

pub struct SlackAdapter {
    config: SlackConfig,
    client: reqwest::Client,
    last_ts: Mutex<BTreeMap<String, String>>,
    /// Cached bot user ID from auth.test
    self_id: tokio::sync::Mutex<Option<String>>,
}

impl SlackAdapter {
    pub fn new(config: SlackConfig) -> Self {
        Self {
            config,
            client: reqwest::Client::new(),
            last_ts: Mutex::new(BTreeMap::new()),
            self_id: tokio::sync::Mutex::new(None),
        }
    }

    pub fn from_config_bytes(config: &[u8]) -> Result<Self, String> {
        let config: SlackConfig =
            serde_json::from_slice(config).map_err(|e| format!("Invalid config: {e}"))?;
        Ok(Self::new(config))
    }

    fn api_base() -> &'static str {
        "https://slack.com/api"
    }

    pub fn encode_envelope(bytes: &[u8]) -> String {
        format!(
            "DOT/1/{}",
            base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(bytes)
        )
    }

    pub fn decode_envelope(text: &str) -> Result<Vec<u8>, String> {
        let b64 = text
            .trim()
            .strip_prefix("DOT/1/")
            .ok_or("Missing DOT/1/ prefix")?;
        base64::engine::general_purpose::URL_SAFE_NO_PAD
            .decode(b64)
            .map_err(|e| format!("Base64: {e}"))
    }

    pub fn domain_hash(channel_id: &str) -> [u8; 32] {
        *blake3::hash(format!("slack:{}", channel_id.trim().to_lowercase()).as_bytes()).as_bytes()
    }

    pub const PLATFORM_TYPE: u16 = 0x0007;
    pub fn max_payload_bytes() -> usize {
        40_000
    }
    pub fn rate_limit_per_second() -> u32 {
        1
    }

    async fn send_message(&self, channel: &str, text: &str) -> Result<String, String> {
        let body = serde_json::json!({ "channel": channel, "text": text });
        let resp = self
            .client
            .post(format!("{}/chat.postMessage", Self::api_base()))
            .header("Authorization", format!("Bearer {}", self.config.bot_token))
            .json(&body)
            .send()
            .await
            .map_err(|e| format!("HTTP: {e}"))?;
        let data: serde_json::Value = resp.json().await.map_err(|e| format!("JSON: {e}"))?;
        if data["ok"] == true {
            Ok(data["ts"].as_str().unwrap_or("0").to_string())
        } else {
            Err(format!(
                "Slack error: {}",
                data["error"].as_str().unwrap_or("unknown")
            ))
        }
    }

    async fn get_messages(&self, channel: &str) -> Result<Vec<SlackMessage>, String> {
        let mut url = format!(
            "{}/conversations.history?channel={}&limit=20",
            Self::api_base(),
            channel
        );
        if let Some(ts) = self.last_ts.lock().await.get(channel) {
            url = format!("{}&oldest={}", url, ts);
        }
        let resp = self
            .client
            .get(&url)
            .header("Authorization", format!("Bearer {}", self.config.bot_token))
            .send()
            .await
            .map_err(|e| format!("HTTP: {e}"))?;
        let data: serde_json::Value = resp.json().await.map_err(|e| format!("JSON: {e}"))?;
        if data["ok"] != true {
            return Err(format!(
                "Slack error: {}",
                data["error"].as_str().unwrap_or("")
            ));
        }
        let mut messages = Vec::new();
        if let Some(msgs) = data["messages"].as_array() {
            for m in msgs {
                messages.push(SlackMessage {
                    ts: m["ts"].as_str().unwrap_or("0").to_string(),
                    text: m["text"].as_str().unwrap_or("").to_string(),
                    user: m["user"].as_str().unwrap_or("").to_string(),
                });
            }
        }
        Ok(messages)
    }
}

#[derive(Debug, Clone)]
struct SlackMessage {
    ts: String,
    text: String,
    user: String,
}

fn transport_err(msg: impl Into<String>) -> PlatformAdapterError {
    PlatformAdapterError::Unreachable {
        platform: "slack".into(),
        reason: msg.into(),
    }
}

#[async_trait]
impl PlatformAdapter for SlackAdapter {
    async fn send_envelope(
        &self,
        domain: &BroadcastDomainId,
        envelope: &DeterministicEnvelope,
    ) -> Result<DeliveryReceipt, PlatformAdapterError> {
        let wire_bytes = envelope.to_wire_bytes();
        let encoded = Self::encode_envelope(&wire_bytes);
        let channel = self
            .config
            .channels
            .iter()
            .find(|ch| Self::domain_hash(ch) == domain.domain_hash)
            .ok_or_else(|| transport_err("No channel for domain"))?;

        let retry = RetryConfig::default();
        for attempt in 0..=retry.max_retries {
            match self.send_message(channel, &encoded).await {
                Ok(ts) => {
                    return Ok(DeliveryReceipt {
                        platform_message_id: ts,
                        delivered_at: 0,
                    })
                }
                Err(e) => {
                    if e.contains("rate_limited") && retry.should_retry(attempt) {
                        tokio::time::sleep(retry.delay_for_attempt(attempt)).await;
                        continue;
                    }
                    return Err(transport_err(e));
                }
            }
        }
        Err(transport_err("Retries exhausted"))
    }

    async fn receive_messages(
        &self,
        _domain: &BroadcastDomainId,
    ) -> Result<Vec<RawPlatformMessage>, PlatformAdapterError> {
        let mut messages = Vec::new();
        for ch in &self.config.channels {
            if let Ok(slack_msgs) = self.get_messages(ch).await {
                for m in slack_msgs {
                    if let Ok(payload) = Self::decode_envelope(&m.text) {
                        let mut meta = BTreeMap::new();
                        meta.insert("channel".into(), ch.clone());
                        meta.insert("user".into(), m.user.clone());
                        meta.insert("ts".into(), m.ts.clone());
                        messages.push(RawPlatformMessage {
                            platform_id: m.ts.clone(),
                            payload,
                            metadata: meta,
                        });
                    }
                }
            }
        }
        Ok(messages)
    }

    fn canonicalize(
        &self,
        raw: &RawPlatformMessage,
    ) -> Result<DeterministicEnvelope, PlatformAdapterError> {
        if raw.payload.is_empty() {
            return Err(transport_err("Empty payload"));
        }
        DeterministicEnvelope::from_wire_bytes(&raw.payload).map_err(|e| {
            PlatformAdapterError::ApiError {
                code: 400,
                message: format!("canonicalize: {e}"),
            }
        })
    }

    fn capabilities(&self) -> CapabilityReport {
        CapabilityReport {
            max_payload_bytes: Self::max_payload_bytes(),
            supports_fragmentation: true,
            supports_encryption: false,
            supports_raw_binary: false,
            rate_limit_per_second: Self::rate_limit_per_second(),
            media_capabilities: None,

            ..Default::default()
        }
    }

    fn domain_id(&self, platform_id: &str) -> BroadcastDomainId {
        BroadcastDomainId::new(PlatformType::Slack, platform_id)
    }
    fn platform_type(&self) -> PlatformType {
        PlatformType::Slack
    }

    fn self_handle(&self) -> Option<String> {
        // Return cached value if available (sync access)
        // The actual resolution happens in health_check or first send
        self.self_id.try_lock().ok().and_then(|guard| guard.clone())
    }

    async fn shutdown(&self) -> Result<(), PlatformAdapterError> {
        *self.self_id.lock().await = None;
        Ok(())
    }

    async fn health_check(&self) -> Result<(), PlatformAdapterError> {
        let timeout = std::time::Duration::from_secs(5);
        let url = format!("{}/auth.test", Self::api_base());
        match tokio::time::timeout(
            timeout,
            self.client
                .get(&url)
                .header("Authorization", format!("Bearer {}", self.config.bot_token))
                .send(),
        )
        .await
        {
            Ok(Ok(resp)) => {
                // Cache the bot user ID from auth.test response
                if let Ok(data) = resp.json::<serde_json::Value>().await {
                    if let Some(user_id) = data["user_id"].as_str() {
                        let mut guard = self.self_id.lock().await;
                        *guard = Some(user_id.to_string());
                    }
                }
                Ok(())
            }
            Ok(Err(e)) => Err(transport_err(format!("Health: {e}"))),
            Err(_) => Err(transport_err("Health timeout")),
        }
    }
}

#[no_mangle]
pub extern "C" fn adapter_version() -> u32 {
    1
}
#[no_mangle]
pub extern "C" fn platform_type() -> u16 {
    0x0007
}
#[no_mangle]
/// # Safety
/// `config` must point to a valid buffer of at least `len` bytes.
pub unsafe extern "C" fn create_adapter(config: *const u8, len: usize) -> *mut () {
    if config.is_null() || len == 0 {
        return std::ptr::null_mut();
    }
    match SlackAdapter::from_config_bytes(std::slice::from_raw_parts(config, len)) {
        Ok(a) => Box::into_raw(Box::new(a)) as *mut (),
        Err(_) => std::ptr::null_mut(),
    }
}
#[no_mangle]
/// # Safety
/// `ptr` must be a pointer previously returned by `create_adapter`.
pub unsafe extern "C" fn destroy_adapter(ptr: *mut ()) {
    if !ptr.is_null() {
        let _ = Box::from_raw(ptr as *mut SlackAdapter);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn test_domain_hash() {
        assert_eq!(
            SlackAdapter::domain_hash("C123"),
            SlackAdapter::domain_hash("C123")
        );
        assert_ne!(
            SlackAdapter::domain_hash("C123"),
            SlackAdapter::domain_hash("C456")
        );
    }
    #[test]
    fn test_encode_decode() {
        let d = b"test";
        let e = SlackAdapter::encode_envelope(d);
        assert!(e.starts_with("DOT/1/"));
        assert_eq!(SlackAdapter::decode_envelope(&e).unwrap(), d);
    }
    #[test]
    fn test_platform_type() {
        assert_eq!(SlackAdapter::PLATFORM_TYPE, 0x0007);
    }
    #[test]
    fn test_abi() {
        assert_eq!(adapter_version(), 1);
        assert_eq!(platform_type(), 0x0007);
    }
    #[test]
    fn test_config() {
        let a = SlackAdapter::from_config_bytes(
            serde_json::to_vec(&serde_json::json!({"bot_token":"xoxb-test","channels":["C123"]}))
                .unwrap()
                .as_slice(),
        )
        .unwrap();
        assert_eq!(a.config.channels, vec!["C123"]);
    }
    #[test]
    fn test_capabilities() {
        let a = SlackAdapter::new(SlackConfig {
            bot_token: "".into(),
            channels: vec![],
        });
        assert_eq!(a.capabilities().max_payload_bytes, 40_000);
    }

    #[test]
    fn test_decode_missing_prefix() {
        assert!(SlackAdapter::decode_envelope("hello").is_err());
    }

    #[test]
    fn test_decode_invalid_base64() {
        assert!(SlackAdapter::decode_envelope("DOT/1/!!!invalid!!!").is_err());
    }

    #[test]
    fn test_domain_hash_deterministic() {
        let h1 = SlackAdapter::domain_hash("C123");
        let h2 = SlackAdapter::domain_hash("C123");
        assert_eq!(h1, h2);
    }

    #[test]
    fn test_domain_hash_normalized() {
        assert_eq!(
            SlackAdapter::domain_hash("C123"),
            SlackAdapter::domain_hash("c123")
        );
    }

    #[test]
    fn test_capabilities_fragmentation() {
        let a = SlackAdapter::new(SlackConfig {
            bot_token: "".into(),
            channels: vec![],
        });
        assert!(a.capabilities().supports_fragmentation);
        assert!(!a.capabilities().supports_encryption);
        assert_eq!(a.capabilities().rate_limit_per_second, 1);
    }

    #[test]
    fn test_self_handle_none_initially() {
        let a = SlackAdapter::new(SlackConfig {
            bot_token: "".into(),
            channels: vec![],
        });
        assert!(a.self_handle().is_none());
    }

    #[test]
    fn test_config_multiple_channels() {
        let a = SlackAdapter::from_config_bytes(
            serde_json::to_vec(&serde_json::json!({
                "bot_token": "xoxb-test",
                "channels": ["C111", "C222", "C333"]
            }))
            .unwrap()
            .as_slice(),
        )
        .unwrap();
        assert_eq!(a.config.channels.len(), 3);
    }
}
