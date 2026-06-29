//! Reddit adapter for DOT (RFC-0850 S8.1, PlatformType::Reddit)
//!
//! Bridges DOT envelopes to Reddit via the Reddit API.
//! Uses OAuth2 with client credentials + refresh token.
//!
//! ## Configuration
//!
//! ```json
//! {
//!   "client_id": "your_client_id",
//!   "client_secret": "your_client_secret",
//!   "refresh_token": "your_refresh_token",
//!   "subreddits": ["cipherocto", "dotnetwork"]
//! }
//! ```

use async_trait::async_trait;
use base64::Engine;
use parking_lot::Mutex;
use std::sync::Arc;

use octo_network::dot::adapters::{
    CapabilityReport, DeliveryReceipt, PlatformAdapter, RawPlatformMessage,
};
use octo_network::dot::domain::{BroadcastDomainId, PlatformType};
use octo_network::dot::envelope::DeterministicEnvelope;
use octo_network::dot::error::PlatformAdapterError;

// ── Configuration ──────────────────────────────────────────────────

#[derive(Clone, serde::Deserialize, serde::Serialize)]
pub struct RedditConfig {
    /// Reddit app client ID
    pub client_id: String,
    /// Reddit app client secret
    pub client_secret: String,
    /// OAuth2 refresh token
    pub refresh_token: String,
    /// Subreddits to post to
    pub subreddits: Vec<String>,
}

impl std::fmt::Debug for RedditConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RedditConfig")
            .field("client_id", &self.client_id)
            .field("client_secret", &"***")
            .field("refresh_token", &"***")
            .field("subreddits", &self.subreddits)
            .finish()
    }
}

// ── Reddit Adapter ─────────────────────────────────────────────────

pub struct RedditAdapter {
    config: RedditConfig,
    client: reqwest::Client,
    /// Cached access token + expiry
    token_cache: Arc<Mutex<Option<(String, u64)>>>,
}

impl RedditAdapter {
    pub fn new(config: RedditConfig) -> Self {
        Self {
            config,
            client: reqwest::Client::new(),
            token_cache: Arc::new(Mutex::new(None)),
        }
    }

    pub fn from_config_bytes(config: &[u8]) -> Result<Self, String> {
        let config: RedditConfig =
            serde_json::from_slice(config).map_err(|e| format!("Invalid config: {e}"))?;
        Ok(Self::new(config))
    }

    fn api_base() -> &'static str {
        "https://oauth.reddit.com"
    }

    fn token_url() -> &'static str {
        "https://www.reddit.com/api/v1/access_token"
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

    pub fn domain_hash(subreddit: &str) -> [u8; 32] {
        let normalized = subreddit.trim().to_lowercase();
        *blake3::hash(format!("reddit:{}", normalized).as_bytes()).as_bytes()
    }

    pub const PLATFORM_TYPE: u16 = 0x0010;
    pub fn max_payload_bytes() -> usize {
        10_000
    }
    pub fn rate_limit_per_second() -> u32 {
        1
    }

    /// Get a valid access token, refreshing if needed.
    async fn get_access_token(&self) -> Result<String, PlatformAdapterError> {
        // Check cache
        {
            let guard = self.token_cache.lock();
            if let Some((token, expires_at)) = guard.as_ref() {
                if epoch_millis() < *expires_at {
                    return Ok(token.clone());
                }
            }
        }

        // Refresh token
        let params = serde_json::json!({
            "grant_type": "refresh_token",
            "refresh_token": self.config.refresh_token,
        });

        let resp = self
            .client
            .post(Self::token_url())
            .basic_auth(&self.config.client_id, Some(&self.config.client_secret))
            .form(&params)
            .send()
            .await
            .map_err(|e| transport_err(format!("Token refresh failed: {e}")))?
            .json::<serde_json::Value>()
            .await
            .map_err(|e| transport_err(format!("Token parse failed: {e}")))?;

        let access_token = resp["access_token"]
            .as_str()
            .ok_or_else(|| transport_err("Missing access_token"))?
            .to_string();
        let expires_in = resp["expires_in"].as_u64().unwrap_or(3600);

        *self.token_cache.lock() = Some((access_token.clone(), epoch_millis() + expires_in * 1000));

        tracing::info!("Reddit access token refreshed");
        Ok(access_token)
    }

    /// Post a text submission to a subreddit.
    async fn post_submission(
        &self,
        subreddit: &str,
        title: &str,
        text: &str,
    ) -> Result<String, PlatformAdapterError> {
        let token = self.get_access_token().await?;
        let url = format!("{}/api/submit", Self::api_base());
        let body = serde_json::json!({
            "sr": subreddit,
            "kind": "self",
            "title": title,
            "text": text,
        });

        let resp = self
            .client
            .post(&url)
            .header("Authorization", format!("Bearer {}", token))
            .json(&body)
            .send()
            .await
            .map_err(|e| transport_err(format!("Post failed: {e}")))?
            .json::<serde_json::Value>()
            .await
            .map_err(|e| transport_err(format!("Post parse failed: {e}")))?;

        let post_id = resp["json"]["data"]["name"]
            .as_str()
            .unwrap_or("unknown")
            .to_string();
        Ok(post_id)
    }
}

fn transport_err(msg: impl Into<String>) -> PlatformAdapterError {
    PlatformAdapterError::Unreachable {
        platform: "reddit".into(),
        reason: msg.into(),
    }
}

fn epoch_millis() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

// ── PlatformAdapter ────────────────────────────────────────────────

#[async_trait]
impl PlatformAdapter for RedditAdapter {
    async fn send_message(
        &self,
        domain: &BroadcastDomainId,
        envelope: &DeterministicEnvelope,
        _payload: &[u8],
    ) -> Result<DeliveryReceipt, PlatformAdapterError> {
        let wire_bytes = envelope.to_wire_bytes();
        let encoded = Self::encode_envelope(&wire_bytes);

        // Find subreddit for this domain
        let subreddit = self
            .config
            .subreddits
            .iter()
            .find(|s| Self::domain_hash(s) == domain.domain_hash)
            .ok_or_else(|| {
                transport_err(format!("No subreddit for domain {:?}", domain.domain_hash))
            })?;

        let title = format!(
            "DOT Envelope {:02x}{:02x}{:02x}{:02x}",
            envelope.envelope_id[0],
            envelope.envelope_id[1],
            envelope.envelope_id[2],
            envelope.envelope_id[3]
        );
        let post_id = self.post_submission(subreddit, &title, &encoded).await?;

        Ok(DeliveryReceipt {
            platform_message_id: post_id,
            delivered_at: epoch_millis(),
        })
    }

    async fn receive_messages(
        &self,
        _domain: &BroadcastDomainId,
    ) -> Result<Vec<RawPlatformMessage>, PlatformAdapterError> {
        // TODO: Implement polling of GET /r/{subreddit}/new
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
            media_capabilities: Some(octo_network::dot::adapters::MediaCapabilities {
                max_upload_bytes: 20_971_520, // 20MB
                supported_mime_types: vec![
                    "image/jpeg".to_string(),
                    "image/png".to_string(),
                    "image/gif".to_string(),
                ],
            }),

            ..Default::default()
        }
    }

    fn domain_id(&self, platform_id: &str) -> BroadcastDomainId {
        BroadcastDomainId::new(PlatformType::Reddit, platform_id)
    }

    fn platform_type(&self) -> PlatformType {
        PlatformType::Reddit
    }

    fn self_handle(&self) -> Option<String> {
        None // Reddit doesn't expose self ID easily
    }

    async fn shutdown(&self) -> Result<(), PlatformAdapterError> {
        *self.token_cache.lock() = None;
        Ok(())
    }

    async fn health_check(&self) -> Result<(), PlatformAdapterError> {
        // Try to get access token
        self.get_access_token().await.map(|_| ())
    }

    async fn upload_media(
        &self,
        filename: &str,
        data: &[u8],
        mime_type: &str,
    ) -> Result<String, PlatformAdapterError> {
        // Reddit supports image uploads via the media upload endpoint
        let token = self.get_access_token().await?;
        let subreddit = self
            .config
            .subreddits
            .first()
            .ok_or_else(|| transport_err("No subreddits configured"))?;
        let url = format!("{}/api/media/asset", Self::api_base());
        let file_part = reqwest::multipart::Part::bytes(data.to_vec())
            .file_name(filename.to_string())
            .mime_str(mime_type)
            .map_err(|e| transport_err(format!("MIME: {e}")))?;
        let form = reqwest::multipart::Form::new()
            .part("file", file_part)
            .text("sr", subreddit.clone());
        let resp = self
            .client
            .post(&url)
            .header("Authorization", format!("Bearer {}", token))
            .multipart(form)
            .send()
            .await
            .map_err(|e| transport_err(format!("Upload failed: {e}")))?
            .json::<serde_json::Value>()
            .await
            .map_err(|e| transport_err(format!("Parse: {e}")))?;
        let asset_id = resp["asset_id"].as_str().unwrap_or("unknown").to_string();
        Ok(asset_id)
    }
    async fn download_media(&self, media_id: &str) -> Result<Vec<u8>, PlatformAdapterError> {
        // Reddit media can be downloaded via direct URL
        let url = if media_id.starts_with("https://") {
            media_id.to_string()
        } else {
            format!("https://i.redd.it/{}", media_id)
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

// ── Plugin ABI ─────────────────────────────────────────────────────

#[no_mangle]
pub extern "C" fn adapter_version() -> u32 {
    1
}

#[no_mangle]
pub extern "C" fn platform_type() -> u16 {
    0x0010
}

/// # Safety
/// `config` must point to a valid buffer of at least `config_len` bytes.
#[no_mangle]
pub unsafe extern "C" fn create_adapter(config: *const u8, config_len: usize) -> *mut () {
    if config.is_null() || config_len == 0 {
        return std::ptr::null_mut();
    }
    let bytes = std::slice::from_raw_parts(config, config_len);
    match RedditAdapter::from_config_bytes(bytes) {
        Ok(a) => Box::into_raw(Box::new(a)) as *mut (),
        Err(_) => std::ptr::null_mut(),
    }
}

/// # Safety
/// `adapter` must be a pointer previously returned by `create_adapter`.
#[no_mangle]
pub unsafe extern "C" fn destroy_adapter(adapter: *mut ()) {
    if !adapter.is_null() {
        let _ = Box::from_raw(adapter as *mut RedditAdapter);
    }
}

// ── Tests ──────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_domain_hash_deterministic() {
        let h1 = RedditAdapter::domain_hash("cipherocto");
        let h2 = RedditAdapter::domain_hash("cipherocto");
        assert_eq!(h1, h2);
    }

    #[test]
    fn test_domain_hash_normalized() {
        assert_eq!(
            RedditAdapter::domain_hash("CipherOcto"),
            RedditAdapter::domain_hash("  cipherocto  ")
        );
    }

    #[test]
    fn test_encode_decode_envelope() {
        let original = b"test reddit envelope";
        let encoded = RedditAdapter::encode_envelope(original);
        assert!(encoded.starts_with("DOT/1/"));
        let decoded = RedditAdapter::decode_envelope(&encoded).unwrap();
        assert_eq!(decoded, original);
    }

    #[test]
    fn test_platform_type() {
        assert_eq!(RedditAdapter::PLATFORM_TYPE, 0x0010);
    }

    #[test]
    fn test_abi_exports() {
        assert_eq!(adapter_version(), 1);
        assert_eq!(platform_type(), 0x0010);
    }

    #[test]
    fn test_capabilities() {
        let config = RedditConfig {
            client_id: "test".into(),
            client_secret: "test".into(),
            refresh_token: "test".into(),
            subreddits: vec!["test".into()],
        };
        let adapter = RedditAdapter::new(config);
        let caps = adapter.capabilities();
        assert_eq!(caps.max_payload_bytes, 10_000);
        assert!(!caps.supports_fragmentation);
        assert!(!caps.supports_encryption);
        assert!(caps.media_capabilities.is_some());
    }

    #[test]
    fn test_decode_missing_prefix() {
        assert!(RedditAdapter::decode_envelope("hello").is_err());
    }

    #[test]
    fn test_decode_invalid_base64() {
        assert!(RedditAdapter::decode_envelope("DOT/1/!!!invalid!!!").is_err());
    }

    #[test]
    fn test_self_handle_none() {
        let config = RedditConfig {
            client_id: "test".into(),
            client_secret: "test".into(),
            refresh_token: "test".into(),
            subreddits: vec![],
        };
        let adapter = RedditAdapter::new(config);
        assert!(adapter.self_handle().is_none());
    }

    #[test]
    fn test_config_from_json() {
        let json = serde_json::json!({
            "client_id": "id123",
            "client_secret": "secret456",
            "refresh_token": "token789",
            "subreddits": ["cipherocto", "dotnetwork"]
        });
        let adapter =
            RedditAdapter::from_config_bytes(serde_json::to_vec(&json).unwrap().as_slice())
                .unwrap();
        assert_eq!(adapter.config.subreddits.len(), 2);
    }
}
