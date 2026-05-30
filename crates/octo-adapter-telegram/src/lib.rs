//! Telegram Bot API adapter for DOT (RFC-0850 S8.1, PlatformType::Telegram)
//!
//! This adapter exports C ABI functions (`adapter_version`, `platform_type`,
//! `create_adapter`, `destroy_adapter`) for dynamic loading as a cdylib plugin.
//!
//! When compiled as `rlib`, the adapter implements `PlatformAdapter` and can
//! be registered via `AdapterRegistry::register_builtin()`. When compiled as
//! `cdylib`, it exports C ABI functions for dynamic plugin loading.
//!
//! ## Configuration
//!
//! ```json
//! {
//!   "bot_token": "123456:ABC-DEF1234ghIkl-zyx57W2v1u123ew11",
//!   "groups": ["-1001234567890"],
//!   "webhook_port": null
//! }
//! ```

use base64::Engine;
use serde::Deserialize;

// NOTE: This crate is structured as both cdylib (for plugin loading) and
// rlib (for direct use in tests/integration). It does NOT depend on
// octo-network — the FFI bridge to PlatformAdapter will be in octo-network.

/// Telegram adapter configuration.
#[derive(Clone, Deserialize, serde::Serialize)]
pub struct TelegramConfig {
    /// Telegram Bot API token
    #[serde(skip_serializing)]
    pub bot_token: String,
    /// Group chat IDs to monitor
    pub groups: Vec<String>,
    /// Optional webhook port (if None, uses long-polling)
    pub webhook_port: Option<u16>,
}

impl std::fmt::Debug for TelegramConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let redacted = if self.bot_token.len() > 8 {
            format!("{}***", &self.bot_token[..8])
        } else {
            "***".to_string()
        };
        f.debug_struct("TelegramConfig")
            .field("bot_token", &redacted)
            .field("groups", &self.groups)
            .field("webhook_port", &self.webhook_port)
            .finish()
    }
}

/// Telegram Bot API client.
pub struct TelegramAdapter {
    config: TelegramConfig,
    client: reqwest::Client,
    /// Cached bot username for self-loop prevention.
    bot_username: std::sync::Mutex<Option<String>>,
    /// Next update_id to request (offset tracking for getUpdates).
    /// Prevents re-receiving already processed messages.
    update_offset: std::sync::Mutex<i64>,
}

impl TelegramAdapter {
    /// Create a new Telegram adapter from config.
    pub fn new(config: TelegramConfig) -> Self {
        Self {
            config,
            client: reqwest::Client::new(),
            bot_username: std::sync::Mutex::new(None),
            update_offset: std::sync::Mutex::new(0),
        }
    }

    /// Create from JSON config bytes (used by plugin ABI).
    pub fn from_config_bytes(config: &[u8]) -> Result<Self, String> {
        let config: TelegramConfig =
            serde_json::from_slice(config).map_err(|e| format!("Invalid config: {}", e))?;
        Ok(Self::new(config))
    }

    /// Telegram Bot API base URL.
    fn api_base(&self) -> String {
        format!("https://api.telegram.org/bot{}", self.config.bot_token)
    }

    /// Send a text message to a Telegram chat.
    pub async fn send_message(&self, chat_id: &str, text: &str) -> Result<TelegramMessage, String> {
        let url = format!("{}/sendMessage", self.api_base());
        let body = serde_json::json!({
            "chat_id": chat_id,
            "text": text
        });

        let resp = self
            .client
            .post(&url)
            .json(&body)
            .send()
            .await
            .map_err(|e| format!("HTTP error: {}", e))?;

        let data: TelegramResponse<TelegramMessage> = resp
            .json()
            .await
            .map_err(|e| format!("JSON parse error: {}", e))?;

        if !data.ok {
            return Err(format!(
                "Telegram API error: {}",
                data.description.unwrap_or_default()
            ));
        }

        data.result.ok_or_else(|| "Missing result".to_string())
    }

    /// Send a document to a Telegram chat.
    pub async fn send_document(
        &self,
        chat_id: &str,
        filename: &str,
        data: &[u8],
    ) -> Result<TelegramMessage, String> {
        let url = format!("{}/sendDocument", self.api_base());
        let file_part = reqwest::multipart::Part::bytes(data.to_vec())
            .file_name(filename.to_string())
            .mime_str("application/octet-stream")
            .map_err(|e| format!("MIME error: {}", e))?;

        let form = reqwest::multipart::Form::new()
            .text("chat_id", chat_id.to_string())
            .part("document", file_part);

        let resp = self
            .client
            .post(&url)
            .multipart(form)
            .send()
            .await
            .map_err(|e| format!("HTTP error: {}", e))?;

        let data: TelegramResponse<TelegramMessage> = resp
            .json()
            .await
            .map_err(|e| format!("JSON parse error: {}", e))?;

        if !data.ok {
            return Err(format!(
                "Telegram API error: {}",
                data.description.unwrap_or_default()
            ));
        }

        data.result.ok_or_else(|| "Missing result".to_string())
    }

    /// Get updates from the bot (long-polling).
    pub async fn get_updates(
        &self,
        offset: Option<i64>,
        timeout: u32,
    ) -> Result<Vec<TelegramUpdate>, String> {
        let url = format!("{}/getUpdates", self.api_base());
        let mut body = serde_json::json!({
            "timeout": timeout,
            "allowed_updates": ["message"]
        });
        if let Some(offset) = offset {
            body["offset"] = serde_json::json!(offset);
        }

        let resp = self
            .client
            .post(&url)
            .json(&body)
            .send()
            .await
            .map_err(|e| format!("HTTP error: {}", e))?;

        let data: TelegramResponse<Vec<TelegramUpdate>> = resp
            .json()
            .await
            .map_err(|e| format!("JSON parse error: {}", e))?;

        if !data.ok {
            return Err(format!(
                "Telegram API error: {}",
                data.description.unwrap_or_default()
            ));
        }

        Ok(data.result.unwrap_or_default())
    }

    /// Send a chat action (typing indicator) to a Telegram chat.
    pub async fn send_chat_action(&self, chat_id: &str, action: &str) -> Result<(), String> {
        let url = format!("{}/sendChatAction", self.api_base());
        let body = serde_json::json!({
            "chat_id": chat_id,
            "action": action
        });
        let resp = self.client.post(&url).json(&body).send().await
            .map_err(|e| format!("HTTP error: {}", e))?;
        let data: TelegramResponse<bool> = resp.json().await
            .map_err(|e| format!("JSON parse error: {}", e))?;
        if !data.ok {
            return Err(format!("Telegram API error: {}", data.description.unwrap_or_default()));
        }
        Ok(())
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
            .ok_or_else(|| format!("Missing DOT/1/ prefix"))?;
        base64::engine::general_purpose::URL_SAFE_NO_PAD
            .decode(b64)
            .map_err(|e| format!("Base64 decode error: {}", e))
    }

    /// Compute domain hash for a Telegram chat ID.
    ///
    /// Hash input includes platform prefix per RFC-0850 S3.1:
    /// `BLAKE3-256("telegram:{chat_id}")`
    pub fn domain_hash(chat_id: &str) -> [u8; 32] {
        let normalized = chat_id.trim().to_lowercase();
        *blake3::hash(format!("telegram:{}", normalized).as_bytes()).as_bytes()
    }

    /// Platform type constant (0x0001 = Telegram).
    ///
    /// Corresponds to `PlatformType::Telegram` in `octo-network::dot::domain`.
    pub const PLATFORM_TYPE: u16 = 0x0001;

    /// Maximum payload bytes per message (Telegram limit).
    pub fn max_payload_bytes() -> usize {
        4096
    }

    /// Rate limit: messages per second.
    pub fn rate_limit_per_second() -> u32 {
        30
    }

    /// Fetch bot username from Telegram API (cached after first call).
    pub async fn fetch_bot_username(&self) -> Result<String, String> {
        let url = format!("{}/getMe", self.api_base());
        let resp = self.client.get(&url).send().await
            .map_err(|e| format!("HTTP error: {}", e))?;
        let data: TelegramResponse<serde_json::Value> = resp.json().await
            .map_err(|e| format!("JSON parse error: {}", e))?;
        if !data.ok {
            return Err(format!("Telegram API error: {}", data.description.unwrap_or_default()));
        }
        let result = data.result.ok_or_else(|| "Missing result".to_string())?;
        let username = result["username"].as_str()
            .ok_or_else(|| "Missing username field".to_string())?
            .to_string();
        // Cache for self_handle()
        if let Ok(mut cache) = self.bot_username.lock() {
            *cache = Some(username.clone());
        }
        Ok(username)
    }

    /// Get cached bot username (fetches if not cached).
    pub fn cached_bot_username(&self) -> Option<String> {
        self.bot_username.lock().ok()?.clone()
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
        platform: "telegram".to_string(),
        reason: msg.into(),
    }
}

#[async_trait]
impl PlatformAdapter for TelegramAdapter {
    async fn send_envelope(
        &self,
        domain: &BroadcastDomainId,
        envelope: &DeterministicEnvelope,
    ) -> Result<DeliveryReceipt, PlatformAdapterError> {
        let wire_bytes = envelope.to_wire_bytes();
        let encoded = Self::encode_envelope(&wire_bytes);

        let chat_id = self
            .config
            .groups
            .iter()
            .find(|g| {
                let hash = Self::domain_hash(g);
                hash == domain.domain_hash
            })
            .ok_or_else(|| transport_err(format!("No group found for domain {:?}", domain.domain_hash)))?;

        // Retry with exponential backoff (ZeroClaw pattern)
        let retry_cfg = octo_network::dot::adapters::backoff::RetryConfig::default();
        let mut last_err = String::new();

        for attempt in 0..=retry_cfg.max_retries {
            let result = if encoded.len() > Self::max_payload_bytes() {
                self.send_document(chat_id, "envelope.bin", &wire_bytes).await
            } else {
                self.send_message(chat_id, &encoded).await
            };

            match result {
                Ok(msg) => {
                    return Ok(DeliveryReceipt {
                        platform_message_id: msg.message_id.to_string(),
                        delivered_at: 0,
                    });
                }
                Err(e) => {
                    last_err = e.clone();
                    // Check for rate limit (429) — parse Retry-After if available
                    if e.contains("429") || e.contains("Too Many Requests") {
                        if retry_cfg.should_retry(attempt) {
                            let delay = retry_cfg.delay_for_attempt(attempt);
                            tokio::time::sleep(delay).await;
                            continue;
                        }
                    }
                    // Non-retryable error
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
        // Get current offset — only receive NEW updates (ZeroClaw pattern)
        let offset = *self.update_offset.lock().unwrap();

        let updates = self
            .get_updates(Some(offset), 30)
            .await
            .map_err(|e| transport_err(e))?;

        let mut messages = Vec::new();
        let mut max_update_id = offset;

        for update in updates {
            // Track highest update_id for offset advancement
            if update.update_id >= max_update_id {
                max_update_id = update.update_id + 1;
            }

            if let Some(msg) = update.message {
                // Send typing indicator while processing (ZeroClaw pattern)
                let _ = self.send_chat_action(&msg.chat.id.to_string(), "typing").await;

                if let Some(text) = msg.text {
                    if let Ok(payload) = Self::decode_envelope(&text) {
                        let mut metadata = std::collections::BTreeMap::new();
                        metadata.insert("chat_id".to_string(), msg.chat.id.to_string());
                        metadata.insert("message_id".to_string(), msg.message_id.to_string());
                        messages.push(RawPlatformMessage {
                            platform_id: msg.message_id.to_string(),
                            payload,
                            metadata,
                        });
                    }
                }
            }
        }

        // Advance offset so next poll only returns new updates
        if max_update_id > offset {
            *self.update_offset.lock().unwrap() = max_update_id;
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
        // payload contains full wire bytes (218 signing + 64 signature = 282 bytes)
        // produced by send_envelope() → to_wire_bytes() → encode_envelope()
        // and decoded by receive_messages() → decode_envelope()
        DeterministicEnvelope::from_wire_bytes(&raw.payload)
            .map_err(|e| PlatformAdapterError::ApiError {
                code: 400,
                message: format!("canonicalize failed: {}", e),
            })
    }

    fn capabilities(&self) -> CapabilityReport {
        CapabilityReport {
            max_payload_bytes: Self::max_payload_bytes(),
            supports_fragmentation: true, // Via document attachments
            supports_encryption: false,
            rate_limit_per_second: Self::rate_limit_per_second(),
            media_capabilities: None,
        }
    }

    fn domain_id(&self, platform_id: &str) -> BroadcastDomainId {
        BroadcastDomainId::new(PlatformType::Telegram, platform_id)
    }

    fn platform_type(&self) -> PlatformType {
        PlatformType::Telegram
    }

    fn self_handle(&self) -> Option<String> {
        // Return cached bot username for self-loop prevention.
        // Gateway calls fetch_bot_username() on startup to populate cache.
        self.cached_bot_username()
    }

    async fn shutdown(&self) -> Result<(), PlatformAdapterError> {
        Ok(())
    }

    async fn health_check(&self) -> Result<(), PlatformAdapterError> {
        // Lightweight liveness probe: GET /getMe with 5s timeout (ZeroClaw pattern)
        let timeout = std::time::Duration::from_secs(5);
        let url = format!("{}/getMe", self.api_base());
        match tokio::time::timeout(timeout, self.client.get(&url).send()).await {
            Ok(Ok(resp)) => {
                if resp.status().is_success() {
                    Ok(())
                } else {
                    Err(transport_err(format!("Health check failed: HTTP {}", resp.status())))
                }
            }
            Ok(Err(e)) => Err(transport_err(format!("Health check failed: {}", e))),
            Err(_) => Err(transport_err("Health check timed out after 5s")),
        }
    }
}

// --- Telegram API types ---

#[derive(Debug, Clone, Deserialize)]
pub struct TelegramResponse<T> {
    pub ok: bool,
    pub description: Option<String>,
    pub result: Option<T>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct TelegramMessage {
    pub message_id: i64,
    pub chat: TelegramChat,
    pub text: Option<String>,
    pub document: Option<TelegramDocument>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct TelegramChat {
    pub id: i64,
    #[serde(default)]
    pub title: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct TelegramDocument {
    pub file_id: String,
    pub file_name: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct TelegramUpdate {
    pub update_id: i64,
    pub message: Option<TelegramMessage>,
}

// --- Plugin ABI exports (for cdylib loading) ---

/// ABI version exported for plugin loading.
#[no_mangle]
pub extern "C" fn adapter_version() -> u32 {
    1
}

/// Platform type exported for plugin loading.
#[no_mangle]
pub extern "C" fn platform_type() -> u16 {
    0x0001 // Telegram
}

/// Create adapter instance from JSON config. Returns opaque pointer.
///
/// # Safety
/// Caller must pass valid pointer to JSON bytes.
#[no_mangle]
pub unsafe extern "C" fn create_adapter(config: *const u8, config_len: usize) -> *mut () {
    if config.is_null() || config_len == 0 {
        return std::ptr::null_mut();
    }

    let config_bytes = std::slice::from_raw_parts(config, config_len);
    match TelegramAdapter::from_config_bytes(config_bytes) {
        Ok(adapter) => Box::into_raw(Box::new(adapter)) as *mut (),
        Err(_) => std::ptr::null_mut(),
    }
}

/// Destroy adapter instance. Takes ownership of the pointer.
///
/// # Safety
/// Caller must pass a pointer returned by `create_adapter`.
#[no_mangle]
pub unsafe extern "C" fn destroy_adapter(adapter: *mut ()) {
    if !adapter.is_null() {
        let _ = Box::from_raw(adapter as *mut TelegramAdapter);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_encode_decode_envelope() {
        let original = b"test envelope data";
        let encoded = TelegramAdapter::encode_envelope(original);
        assert!(encoded.starts_with("DOT/1/"));
        let decoded = TelegramAdapter::decode_envelope(&encoded).unwrap();
        assert_eq!(decoded, original);
    }

    #[test]
    fn test_domain_hash_deterministic() {
        let h1 = TelegramAdapter::domain_hash("-1001234567890");
        let h2 = TelegramAdapter::domain_hash("-1001234567890");
        assert_eq!(h1, h2);
    }

    #[test]
    fn test_domain_hash_normalized() {
        let h1 = TelegramAdapter::domain_hash("-1001234567890");
        let h2 = TelegramAdapter::domain_hash("  -1001234567890  ");
        assert_eq!(h1, h2);
    }

    #[test]
    fn test_platform_type() {
        assert_eq!(TelegramAdapter::PLATFORM_TYPE, 0x0001);
    }

    #[test]
    fn test_abi_exports() {
        assert_eq!(adapter_version(), 1);
        assert_eq!(platform_type(), 0x0001);
    }

    #[test]
    fn test_config_from_json() {
        let config = serde_json::json!({
            "bot_token": "test:token",
            "groups": ["-100123"],
            "webhook_port": null
        });
        let adapter =
            TelegramAdapter::from_config_bytes(serde_json::to_vec(&config).unwrap().as_slice())
                .unwrap();
        assert_eq!(adapter.config.bot_token, "test:token");
        assert_eq!(adapter.config.groups.len(), 1);
    }

    #[test]
    fn test_encode_decode_roundtrip() {
        let data = vec![0u8; 256];
        for i in 0..256 {
            let mut d = data.clone();
            d[i] = 0xFF;
            let encoded = TelegramAdapter::encode_envelope(&d);
            let decoded = TelegramAdapter::decode_envelope(&encoded).unwrap();
            assert_eq!(decoded, d);
        }
    }

    #[test]
    fn test_decode_invalid_prefix() {
        assert!(TelegramAdapter::decode_envelope("NOTDOT/1/abc").is_err());
    }

    #[test]
    fn test_decode_invalid_base64() {
        assert!(TelegramAdapter::decode_envelope("DOT/1/!!!invalid!!!").is_err());
    }
}
