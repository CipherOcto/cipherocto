//! Discord Webhook adapter for DOT (RFC-0850 S8.1, PlatformType::Discord)
//!
//! This adapter exports C ABI functions (`adapter_version`, `platform_type`,
//! `create_adapter`, `destroy_adapter`) for dynamic loading as a cdylib plugin.
//!
//! **Design note:** The adapter is a standalone HTTP client and does NOT
//! implement the `PlatformAdapter` trait directly. A future `FfiAdapter`
//! wrapper in `octo-network` will bridge the C ABI to `PlatformAdapter` when
//! the adapter is loaded as a plugin. This separation keeps the adapter crate
//! free of `octo-network` dependencies.
//!
//! ## Configuration
//!
//! ```json
//! {
//!   "bot_token": "Bot xxx",
//!   "webhook_url": "https://discord.com/api/webhooks/xxx/yyy",
//!   "guild_id": "123456789",
//!   "channels": ["987654321"]
//! }
//! ```

use base64::Engine;
use serde::Deserialize;

/// Discord adapter configuration.
#[derive(Clone, Deserialize, serde::Serialize)]
pub struct DiscordConfig {
    /// Discord bot token (for receiving via Gateway)
    #[serde(skip_serializing)]
    pub bot_token: String,
    /// Webhook URL (for sending)
    pub webhook_url: String,
    /// Guild (server) ID
    pub guild_id: String,
    /// Channel IDs to monitor
    pub channels: Vec<String>,
}

impl std::fmt::Debug for DiscordConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let redacted = if self.bot_token.len() > 8 {
            format!("{}***", &self.bot_token[..8])
        } else {
            "***".to_string()
        };
        f.debug_struct("DiscordConfig")
            .field("bot_token", &redacted)
            .field("webhook_url", &"<redacted>")
            .field("guild_id", &self.guild_id)
            .field("channels", &self.channels)
            .finish()
    }
}

/// Discord adapter client.
pub struct DiscordAdapter {
    config: DiscordConfig,
    client: reqwest::Client,
}

impl DiscordAdapter {
    /// Create a new Discord adapter from config.
    pub fn new(config: DiscordConfig) -> Self {
        Self {
            config,
            client: reqwest::Client::new(),
        }
    }

    /// Create from JSON config bytes (used by plugin ABI).
    pub fn from_config_bytes(config: &[u8]) -> Result<Self, String> {
        let config: DiscordConfig =
            serde_json::from_slice(config).map_err(|e| format!("Invalid config: {}", e))?;
        Ok(Self::new(config))
    }

    /// Send a message via Discord webhook.
    pub async fn send_webhook_message(&self, content: &str) -> Result<DiscordMessage, String> {
        let body = serde_json::json!({ "content": content });

        let resp = self
            .client
            .post(&self.config.webhook_url)
            .json(&body)
            .send()
            .await
            .map_err(|e| format!("HTTP error: {}", e))?;

        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            return Err(format!("Discord API error {}: {}", status, body));
        }

        resp.json()
            .await
            .map_err(|e| format!("JSON parse error: {}", e))
    }

    /// Send a file attachment via Discord webhook.
    pub async fn send_webhook_file(
        &self,
        filename: &str,
        data: &[u8],
    ) -> Result<DiscordMessage, String> {
        let file_part = reqwest::multipart::Part::bytes(data.to_vec())
            .file_name(filename.to_string())
            .mime_str("application/octet-stream")
            .map_err(|e| format!("MIME error: {}", e))?;

        let form = reqwest::multipart::Form::new().part("file", file_part);

        let resp = self
            .client
            .post(&self.config.webhook_url)
            .multipart(form)
            .send()
            .await
            .map_err(|e| format!("HTTP error: {}", e))?;

        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            return Err(format!("Discord API error {}: {}", status, body));
        }

        resp.json()
            .await
            .map_err(|e| format!("JSON parse error: {}", e))
    }

    /// Get messages from a Discord channel (requires bot token).
    pub async fn get_channel_messages(
        &self,
        channel_id: &str,
        limit: u32,
    ) -> Result<Vec<DiscordMessage>, String> {
        let url = format!(
            "https://discord.com/api/v10/channels/{}/messages?limit={}",
            channel_id, limit
        );

        let resp = self
            .client
            .get(&url)
            .header("Authorization", format!("Bot {}", self.config.bot_token))
            .send()
            .await
            .map_err(|e| format!("HTTP error: {}", e))?;

        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            return Err(format!("Discord API error {}: {}", status, body));
        }

        resp.json()
            .await
            .map_err(|e| format!("JSON parse error: {}", e))
    }

    /// Encode an envelope as base64 with DOT prefix.
    pub fn encode_envelope(envelope_bytes: &[u8]) -> String {
        format!(
            "DOT/1/{}",
            base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(envelope_bytes)
        )
    }

    /// Decode a DOT-prefixed base64 envelope.
    pub fn decode_envelope(text: &str) -> Result<Vec<u8>, String> {
        let text = text.trim();
        let b64 = text
            .strip_prefix("DOT/1/")
            .ok_or_else(|| "Missing DOT/1/ prefix".to_string())?;
        base64::engine::general_purpose::URL_SAFE_NO_PAD
            .decode(b64)
            .map_err(|e| format!("Base64 decode error: {}", e))
    }

    /// Compute domain hash for a Discord channel ID.
    ///
    /// Hash input includes platform prefix per RFC-0850 S3.1:
    /// `BLAKE3-256("discord:{channel_id}")`
    pub fn domain_hash(channel_id: &str) -> [u8; 32] {
        let normalized = channel_id.trim().to_lowercase();
        *blake3::hash(format!("discord:{}", normalized).as_bytes()).as_bytes()
    }

    /// Platform type constant (0x0002 = Discord).
    ///
    /// Corresponds to `PlatformType::Discord` in `octo-network::dot::domain`.
    pub const PLATFORM_TYPE: u16 = 0x0002;

    /// Maximum payload bytes per message (Discord limit).
    pub fn max_payload_bytes() -> usize {
        2000
    }

    /// Rate limit: messages per second per channel.
    pub fn rate_limit_per_second() -> u32 {
        5
    }
}

// --- PlatformAdapter trait implementation ---

use async_trait::async_trait;
use octo_network::dot::adapters::{
    CapabilityReport, DeliveryReceipt, PlatformAdapter, RawPlatformMessage,
};
use octo_network::dot::domain::{BroadcastDomainId, PlatformType};
use octo_network::dot::envelope::DeterministicEnvelope;
use octo_network::dot::error::PlatformAdapterError;

fn transport_err(msg: impl Into<String>) -> PlatformAdapterError {
    PlatformAdapterError::Unreachable {
        platform: "discord".to_string(),
        reason: msg.into(),
    }
}

#[async_trait]
impl PlatformAdapter for DiscordAdapter {
    async fn send_envelope(
        &self,
        _domain: &BroadcastDomainId,
        envelope: &DeterministicEnvelope,
    ) -> Result<DeliveryReceipt, PlatformAdapterError> {
        let wire_bytes = envelope.to_wire_bytes();
        let encoded = Self::encode_envelope(&wire_bytes);

        // Retry with exponential backoff (ZeroClaw pattern)
        let retry_cfg = octo_network::dot::adapters::backoff::RetryConfig::default();
        let mut last_err = String::new();

        for attempt in 0..=retry_cfg.max_retries {
            let result = if wire_bytes.len() > Self::max_payload_bytes() {
                self.send_webhook_file("envelope.bin", &wire_bytes).await
            } else {
                self.send_webhook_message(&encoded).await
            };

            match result {
                Ok(msg) => {
                    return Ok(DeliveryReceipt {
                        platform_message_id: msg.id,
                        delivered_at: 0,
                    });
                }
                Err(e) => {
                    last_err = e.clone();
                    if (e.contains("429") || e.contains("rate limit"))
                        && retry_cfg.should_retry(attempt) {
                            let delay = retry_cfg.delay_for_attempt(attempt);
                            tokio::time::sleep(delay).await;
                            continue;
                        }
                    return Err(transport_err(e));
                }
            }
        }
        Err(transport_err(format!("Retries exhausted: {}", last_err)))
    }

    async fn receive_messages(
        &self,
        _domain: &BroadcastDomainId,
    ) -> Result<Vec<RawPlatformMessage>, PlatformAdapterError> {
        let messages = match self.config.channels.first() {
            Some(ch) => self
                .get_channel_messages(ch, 10)
                .await
                .map_err(transport_err)?,
            None => Vec::new(),
        };

        let mut result = Vec::new();
        for msg in messages {
            if let Ok(payload) = Self::decode_envelope(&msg.content) {
                let mut metadata = std::collections::BTreeMap::new();
                metadata.insert("channel_id".to_string(), msg.channel_id.clone());
                metadata.insert("message_id".to_string(), msg.id.clone());
                result.push(RawPlatformMessage {
                    platform_id: msg.id,
                    payload,
                    metadata,
                });
            }
        }
        Ok(result)
    }

    fn canonicalize(
        &self,
        raw: &RawPlatformMessage,
    ) -> Result<DeterministicEnvelope, PlatformAdapterError> {
        if raw.payload.is_empty() {
            return Err(transport_err("Empty payload"));
        }
        // payload contains full wire bytes from decode_envelope() in receive_messages()
        DeterministicEnvelope::from_wire_bytes(&raw.payload).map_err(|e| {
            PlatformAdapterError::ApiError {
                code: 400,
                message: format!("canonicalize failed: {}", e),
            }
        })
    }

    fn capabilities(&self) -> CapabilityReport {
        CapabilityReport {
            max_payload_bytes: Self::max_payload_bytes(),
            supports_fragmentation: true,
            supports_encryption: false,
            rate_limit_per_second: Self::rate_limit_per_second(),
            media_capabilities: Some(octo_network::dot::adapters::MediaCapabilities {
                max_upload_bytes: 25 * 1024 * 1024, // 25MB
                supported_mime_types: vec![
                    "image/jpeg".into(), "image/png".into(), "image/gif".into(),
                    "video/mp4".into(), "audio/mpeg".into(),
                    "application/pdf".into(), "application/octet-stream".into(),
                ],
            }),
        }
    }

    fn domain_id(&self, platform_id: &str) -> BroadcastDomainId {
        BroadcastDomainId::new(PlatformType::Discord, platform_id)
    }

    fn platform_type(&self) -> PlatformType {
        PlatformType::Discord
    }

    fn self_handle(&self) -> Option<String> {
        // Discord bot ID is extracted from the bot token (format: "BOT_TOKEN" or "Bot TOKEN")
        // For self-loop prevention, the gateway should decode the bot user ID from the token.
        // Placeholder: return None until gateway integration populates this.
        None
    }

    async fn shutdown(&self) -> Result<(), PlatformAdapterError> {
        Ok(())
    }

    async fn health_check(&self) -> Result<(), PlatformAdapterError> {
        // Verify webhook is still valid by checking Discord API
        let timeout = std::time::Duration::from_secs(5);
        match tokio::time::timeout(timeout, self.send_webhook_message("health")).await {
            Ok(Ok(_)) => Ok(()),
            Ok(Err(e)) => Err(transport_err(format!("Health check failed: {}", e))),
            Err(_) => Err(transport_err("Health check timed out after 5s")),
        }
    }
}

// --- Discord API types ---

#[derive(Debug, Clone, Deserialize)]
pub struct DiscordMessage {
    pub id: String,
    pub channel_id: String,
    pub content: String,
    pub author: Option<DiscordUser>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct DiscordUser {
    pub id: String,
    pub username: String,
    pub bot: Option<bool>,
}

// --- Plugin ABI exports (for cdylib loading) ---

#[no_mangle]
pub extern "C" fn adapter_version() -> u32 {
    1
}

#[no_mangle]
pub extern "C" fn platform_type() -> u16 {
    0x0002 // Discord
}

#[no_mangle]
/// # Safety
/// `config` must point to a valid buffer of at least `len` bytes.
pub unsafe extern "C" fn create_adapter(config: *const u8, config_len: usize) -> *mut () {
    if config.is_null() || config_len == 0 {
        return std::ptr::null_mut();
    }

    let config_bytes = std::slice::from_raw_parts(config, config_len);
    match DiscordAdapter::from_config_bytes(config_bytes) {
        Ok(adapter) => Box::into_raw(Box::new(adapter)) as *mut (),
        Err(_) => std::ptr::null_mut(),
    }
}

#[no_mangle]
/// # Safety
/// `ptr` must be a pointer previously returned by `create_adapter`.
pub unsafe extern "C" fn destroy_adapter(adapter: *mut ()) {
    if !adapter.is_null() {
        let _ = Box::from_raw(adapter as *mut DiscordAdapter);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_encode_decode_envelope() {
        let original = b"test envelope data";
        let encoded = DiscordAdapter::encode_envelope(original);
        assert!(encoded.starts_with("DOT/1/"));
        let decoded = DiscordAdapter::decode_envelope(&encoded).unwrap();
        assert_eq!(decoded, original);
    }

    #[test]
    fn test_domain_hash_deterministic() {
        let h1 = DiscordAdapter::domain_hash("987654321");
        let h2 = DiscordAdapter::domain_hash("987654321");
        assert_eq!(h1, h2);
    }

    #[test]
    fn test_domain_hash_normalized() {
        let h1 = DiscordAdapter::domain_hash("987654321");
        let h2 = DiscordAdapter::domain_hash("  987654321  ");
        assert_eq!(h1, h2);
    }

    #[test]
    fn test_platform_type() {
        assert_eq!(DiscordAdapter::PLATFORM_TYPE, 0x0002);
    }

    #[test]
    fn test_abi_exports() {
        assert_eq!(adapter_version(), 1);
        assert_eq!(platform_type(), 0x0002);
    }

    #[test]
    fn test_config_from_json() {
        let config = serde_json::json!({
            "bot_token": "Bot test",
            "webhook_url": "https://discord.com/api/webhooks/123/abc",
            "guild_id": "111",
            "channels": ["222"]
        });
        let adapter =
            DiscordAdapter::from_config_bytes(serde_json::to_vec(&config).unwrap().as_slice())
                .unwrap();
        assert_eq!(adapter.config.bot_token, "Bot test");
        assert_eq!(adapter.config.channels.len(), 1);
    }

    #[test]
    fn test_max_payload() {
        assert_eq!(DiscordAdapter::max_payload_bytes(), 2000);
        assert_eq!(DiscordAdapter::rate_limit_per_second(), 5);
    }

    #[test]
    fn test_encode_decode_roundtrip() {
        let data = vec![0u8; 256];
        let encoded = DiscordAdapter::encode_envelope(&data);
        let decoded = DiscordAdapter::decode_envelope(&encoded).unwrap();
        assert_eq!(decoded, data);
    }

    #[test]
    fn test_decode_invalid() {
        assert!(DiscordAdapter::decode_envelope("NOTDOT/1/abc").is_err());
        assert!(DiscordAdapter::decode_envelope("DOT/1/!!!invalid!!!").is_err());
    }
}
