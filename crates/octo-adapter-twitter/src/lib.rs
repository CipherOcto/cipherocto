//! Twitter/X adapter for DOT (RFC-0850 S8.1, PlatformType::Twitter)
//!
//! Bridges DOT envelopes to Twitter via the Twitter API v2.
//! Uses Bearer token authentication (OAuth2).
//!
//! ## Configuration
//!
//! ```json
//! {
//!   "bearer_token": "AAAAAAAAAAAAAAAAAAAAA...",
//!   "account_id": "1234567890"
//! }
//! ```

use async_trait::async_trait;
use base64::Engine;
use parking_lot::Mutex;
use std::sync::Arc;

use octo_network::dot::adapters::{
    CapabilityReport, DeliveryReceipt, MediaCapabilities, PlatformAdapter, RawPlatformMessage,
};
use octo_network::dot::domain::{BroadcastDomainId, PlatformType};
use octo_network::dot::envelope::DeterministicEnvelope;
use octo_network::dot::error::PlatformAdapterError;

// ── Configuration ──────────────────────────────────────────────────

#[derive(Clone, serde::Deserialize, serde::Serialize)]
pub struct TwitterConfig {
    /// Bearer token for Twitter API v2
    pub bearer_token: String,
    /// Account ID (optional, for polling mentions)
    pub account_id: Option<String>,
}

impl std::fmt::Debug for TwitterConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TwitterConfig")
            .field("bearer_token", &"***")
            .field("account_id", &self.account_id)
            .finish()
    }
}

// ── Twitter Adapter ────────────────────────────────────────────────

pub struct TwitterAdapter {
    config: TwitterConfig,
    client: reqwest::Client,
    /// Cached self user ID
    self_id: Arc<Mutex<Option<String>>>,
}

impl TwitterAdapter {
    pub fn new(config: TwitterConfig) -> Self {
        Self {
            config,
            client: reqwest::Client::new(),
            self_id: Arc::new(Mutex::new(None)),
        }
    }

    pub fn from_config_bytes(config: &[u8]) -> Result<Self, String> {
        let config: TwitterConfig =
            serde_json::from_slice(config).map_err(|e| format!("Invalid config: {e}"))?;
        Ok(Self::new(config))
    }

    fn api_base() -> &'static str {
        "https://api.x.com/2"
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

    pub fn domain_hash(user_id: &str) -> [u8; 32] {
        let normalized = user_id.trim().to_lowercase();
        *blake3::hash(format!("twitter:{}", normalized).as_bytes()).as_bytes()
    }

    pub const PLATFORM_TYPE: u16 = 0x000F;
    pub fn max_payload_bytes() -> usize {
        280
    }
    pub fn rate_limit_per_second() -> u32 {
        1
    }

    /// Post a tweet.
    async fn post_tweet(&self, text: &str) -> Result<String, PlatformAdapterError> {
        let url = format!("{}/tweets", Self::api_base());
        let body = serde_json::json!({ "text": text });

        let resp = self
            .client
            .post(&url)
            .header(
                "Authorization",
                format!("Bearer {}", self.config.bearer_token),
            )
            .json(&body)
            .send()
            .await
            .map_err(|e| transport_err(format!("Post failed: {e}")))?
            .json::<serde_json::Value>()
            .await
            .map_err(|e| transport_err(format!("Post parse failed: {e}")))?;

        let tweet_id = resp["data"]["id"].as_str().unwrap_or("unknown").to_string();
        Ok(tweet_id)
    }

    /// Resolve self user ID via /users/me.
    async fn resolve_self_id(&self) -> Result<(), PlatformAdapterError> {
        {
            let guard = self.self_id.lock();
            if guard.is_some() {
                return Ok(());
            }
        }

        let url = format!("{}/users/me", Self::api_base());
        let resp = self
            .client
            .get(&url)
            .header(
                "Authorization",
                format!("Bearer {}", self.config.bearer_token),
            )
            .send()
            .await
            .map_err(|e| transport_err(format!("Self ID resolve failed: {e}")))?
            .json::<serde_json::Value>()
            .await
            .map_err(|e| transport_err(format!("Self ID parse failed: {e}")))?;

        if let Some(id) = resp["data"]["id"].as_str() {
            *self.self_id.lock() = Some(id.to_string());
            tracing::info!("Twitter self ID resolved: {}", id);
        }

        Ok(())
    }
}

fn transport_err(msg: impl Into<String>) -> PlatformAdapterError {
    PlatformAdapterError::Unreachable {
        platform: "twitter".into(),
        reason: msg.into(),
    }
}

// ── PlatformAdapter ────────────────────────────────────────────────

#[async_trait]
impl PlatformAdapter for TwitterAdapter {
    async fn send_message(
        &self,
        _domain: &BroadcastDomainId,
        envelope: &DeterministicEnvelope,
        _payload: &[u8],
    ) -> Result<DeliveryReceipt, PlatformAdapterError> {
        let wire_bytes = envelope.to_wire_bytes();
        let encoded = Self::encode_envelope(&wire_bytes);

        let tweet_id = self.post_tweet(&encoded).await?;

        Ok(DeliveryReceipt {
            platform_message_id: tweet_id,
            delivered_at: epoch_millis(),
        })
    }

    async fn receive_messages(
        &self,
        _domain: &BroadcastDomainId,
    ) -> Result<Vec<RawPlatformMessage>, PlatformAdapterError> {
        // TODO: Implement polling of GET /2/users/:id/mentions
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
            supports_fragmentation: true,
            supports_encryption: false,
            supports_raw_binary: false,
            rate_limit_per_second: Self::rate_limit_per_second(),
            media_capabilities: Some(MediaCapabilities {
                max_upload_bytes: 5_242_880, // 5MB
                supported_mime_types: vec![
                    "image/jpeg".to_string(),
                    "image/png".to_string(),
                    "image/gif".to_string(),
                    "image/webp".to_string(),
                ],
            }),

            ..Default::default()
        }
    }

    fn domain_id(&self, platform_id: &str) -> BroadcastDomainId {
        BroadcastDomainId::new(PlatformType::Twitter, platform_id)
    }

    fn platform_type(&self) -> PlatformType {
        PlatformType::Twitter
    }

    fn self_handle(&self) -> Option<String> {
        self.self_id.lock().clone()
    }

    async fn shutdown(&self) -> Result<(), PlatformAdapterError> {
        *self.self_id.lock() = None;
        Ok(())
    }

    async fn health_check(&self) -> Result<(), PlatformAdapterError> {
        self.resolve_self_id().await
    }

    async fn upload_media(
        &self,
        _filename: &str,
        data: &[u8],
        _mime_type: &str,
    ) -> Result<String, PlatformAdapterError> {
        let url = "https://upload.twitter.com/1.1/media/upload.json";
        let form = reqwest::multipart::Form::new().part(
            "media",
            reqwest::multipart::Part::bytes(data.to_vec())
                .file_name("upload.bin")
                .mime_str("application/octet-stream")
                .map_err(|e| transport_err(format!("MIME: {e}")))?,
        );
        let resp = self
            .client
            .post(url)
            .header(
                "Authorization",
                format!("Bearer {}", self.config.bearer_token),
            )
            .multipart(form)
            .send()
            .await
            .map_err(|e| transport_err(format!("Upload failed: {e}")))?
            .json::<serde_json::Value>()
            .await
            .map_err(|e| transport_err(format!("Parse: {e}")))?;
        let media_id = resp["media_id_string"]
            .as_str()
            .unwrap_or("unknown")
            .to_string();
        Ok(media_id)
    }
    async fn download_media(&self, media_id: &str) -> Result<Vec<u8>, PlatformAdapterError> {
        // media_id can be a direct URL or a media ID
        let url = if media_id.starts_with("https://") {
            media_id.to_string()
        } else {
            format!("https://pbs.twimg.com/media/{}", media_id)
        };
        let bytes = self
            .client
            .get(&url)
            .send()
            .await
            .map_err(|e| transport_err(format!("Download failed: {e}")))?
            .bytes()
            .await
            .map_err(|e| transport_err(format!("Download read: {e}")))?;
        Ok(bytes.to_vec())
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
    0x000F
}

/// # Safety
/// `config` must point to a valid buffer of at least `config_len` bytes.
#[no_mangle]
pub unsafe extern "C" fn create_adapter(config: *const u8, config_len: usize) -> *mut () {
    if config.is_null() || config_len == 0 {
        return std::ptr::null_mut();
    }
    let bytes = std::slice::from_raw_parts(config, config_len);
    match TwitterAdapter::from_config_bytes(bytes) {
        Ok(a) => Box::into_raw(Box::new(a)) as *mut (),
        Err(_) => std::ptr::null_mut(),
    }
}

/// # Safety
/// `adapter` must be a pointer previously returned by `create_adapter`.
#[no_mangle]
pub unsafe extern "C" fn destroy_adapter(adapter: *mut ()) {
    if !adapter.is_null() {
        let _ = Box::from_raw(adapter as *mut TwitterAdapter);
    }
}

// ── Tests ──────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_domain_hash_deterministic() {
        let h1 = TwitterAdapter::domain_hash("1234567890");
        let h2 = TwitterAdapter::domain_hash("1234567890");
        assert_eq!(h1, h2);
    }

    #[test]
    fn test_domain_hash_normalized() {
        assert_eq!(
            TwitterAdapter::domain_hash("1234567890"),
            TwitterAdapter::domain_hash("  1234567890  ")
        );
    }

    #[test]
    fn test_encode_decode_envelope() {
        let original = b"test twitter envelope";
        let encoded = TwitterAdapter::encode_envelope(original);
        assert!(encoded.starts_with("DOT/1/"));
        let decoded = TwitterAdapter::decode_envelope(&encoded).unwrap();
        assert_eq!(decoded, original);
    }

    #[test]
    fn test_platform_type() {
        assert_eq!(TwitterAdapter::PLATFORM_TYPE, 0x000F);
    }

    #[test]
    fn test_abi_exports() {
        assert_eq!(adapter_version(), 1);
        assert_eq!(platform_type(), 0x000F);
    }

    #[test]
    fn test_capabilities() {
        let config = TwitterConfig {
            bearer_token: "test".into(),
            account_id: None,
        };
        let adapter = TwitterAdapter::new(config);
        let caps = adapter.capabilities();
        assert_eq!(caps.max_payload_bytes, 280);
        assert!(caps.supports_fragmentation);
        assert!(!caps.supports_encryption);
        assert!(caps.media_capabilities.is_some());
    }

    #[test]
    fn test_decode_missing_prefix() {
        assert!(TwitterAdapter::decode_envelope("hello").is_err());
    }

    #[test]
    fn test_decode_invalid_base64() {
        assert!(TwitterAdapter::decode_envelope("DOT/1/!!!invalid!!!").is_err());
    }

    #[test]
    fn test_self_handle_none_initially() {
        let config = TwitterConfig {
            bearer_token: "test".into(),
            account_id: None,
        };
        let adapter = TwitterAdapter::new(config);
        assert!(adapter.self_handle().is_none());
    }

    #[test]
    fn test_config_from_json() {
        let json = serde_json::json!({
            "bearer_token": "AAAAAAAAAAAAAAAAAAAAA",
            "account_id": "1234567890"
        });
        let adapter =
            TwitterAdapter::from_config_bytes(serde_json::to_vec(&json).unwrap().as_slice())
                .unwrap();
        assert_eq!(adapter.config.account_id, Some("1234567890".to_string()));
    }
}
