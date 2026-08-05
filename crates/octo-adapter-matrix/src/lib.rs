//! Matrix Client-Server API adapter for DOT (RFC-0850 S8.1, PlatformType::Matrix)
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
//! Matrix is the most aligned platform for CipherOcto — it is itself federated
//! and decentralized, matching CipherOcto's architecture.
//!
//! ## Configuration
//!
//! ```json
//! {
//!   "homeserver_url": "https://matrix.example.com",
//!   "access_token": "syt_xxx",
//!   "rooms": ["!abcdef:example.com"]
//! }
//! ```

// Clippy `[disallowed-methods]` allowlist: this is a messaging-platform
// adapter (NOT an LLM model provider). The cipherocto capability
// token boundary lives at `quota-router-core/src/egress/`; this
// adapter talks to its own platform API using platform-issued
// credentials.
#![allow(clippy::disallowed_methods)]

use base64::Engine;
use serde::Deserialize;
use uuid::Uuid;

/// Matrix adapter configuration.
#[derive(Clone, Deserialize, serde::Serialize)]
pub struct MatrixConfig {
    /// Matrix homeserver URL
    pub homeserver_url: String,
    /// Access token for authentication
    #[serde(skip_serializing)]
    pub access_token: String,
    /// Room IDs to monitor
    pub rooms: Vec<String>,
}

impl std::fmt::Debug for MatrixConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let redacted = if self.access_token.len() > 8 {
            format!("{}***", &self.access_token[..8])
        } else {
            "***".to_string()
        };
        f.debug_struct("MatrixConfig")
            .field("homeserver_url", &self.homeserver_url)
            .field("access_token", &redacted)
            .field("rooms", &self.rooms)
            .finish()
    }
}

/// Matrix adapter client.
pub struct MatrixAdapter {
    config: MatrixConfig,
    client: reqwest::Client,
    /// Sync token for incremental /sync (ZeroClaw pattern).
    next_batch: std::sync::Mutex<Option<String>>,
    /// Cached bot user ID (e.g., @bot:server)
    user_id: tokio::sync::Mutex<Option<String>>,
}

impl MatrixAdapter {
    /// Create a new Matrix adapter from config.
    pub fn new(config: MatrixConfig) -> Self {
        Self {
            config,
            client: reqwest::Client::new(),
            next_batch: std::sync::Mutex::new(None),
            user_id: tokio::sync::Mutex::new(None),
        }
    }

    /// Create from JSON config bytes (used by plugin ABI).
    pub fn from_config_bytes(config: &[u8]) -> Result<Self, String> {
        let config: MatrixConfig =
            serde_json::from_slice(config).map_err(|e| format!("Invalid config: {}", e))?;
        Ok(Self::new(config))
    }

    /// Send a message event to a Matrix room.
    pub async fn send_message(
        &self,
        room_id: &str,
        content: serde_json::Value,
    ) -> Result<MatrixSendResponse, String> {
        let txn_id = Uuid::new_v4().to_string();
        let url = format!(
            "{}/_matrix/client/v3/rooms/{}/send/m.room.message/{}",
            self.config.homeserver_url, room_id, txn_id
        );

        let resp = self
            .client
            .put(&url)
            .header(
                "Authorization",
                format!("Bearer {}", self.config.access_token),
            )
            .json(&content)
            .send()
            .await
            .map_err(|e| format!("HTTP error: {}", e))?;

        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            return Err(format!("Matrix API error {}: {}", status, body));
        }

        resp.json()
            .await
            .map_err(|e| format!("JSON parse error: {}", e))
    }

    /// Send a text message to a Matrix room.
    pub async fn send_text(&self, room_id: &str, text: &str) -> Result<MatrixSendResponse, String> {
        let content = serde_json::json!({
            "msgtype": "m.text",
            "body": text
        });
        self.send_message(room_id, content).await
    }

    /// Upload media to the Matrix homeserver.
    pub async fn upload_media(
        &self,
        filename: &str,
        data: &[u8],
        content_type: &str,
    ) -> Result<MatrixMediaUploadResponse, String> {
        let url = format!(
            "{}/_matrix/media/v3/upload/{}",
            self.config.homeserver_url, filename
        );

        let resp = self
            .client
            .post(&url)
            .header(
                "Authorization",
                format!("Bearer {}", self.config.access_token),
            )
            .header("Content-Type", content_type)
            .body(data.to_vec())
            .send()
            .await
            .map_err(|e| format!("HTTP error: {}", e))?;

        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            return Err(format!("Matrix API error {}: {}", status, body));
        }

        resp.json()
            .await
            .map_err(|e| format!("JSON parse error: {}", e))
    }

    /// Sync messages from rooms (long-polling).
    pub async fn sync(
        &self,
        since: Option<&str>,
        timeout_ms: u32,
    ) -> Result<MatrixSyncResponse, String> {
        let mut url = format!(
            "{}/_matrix/client/v3/sync?timeout={}",
            self.config.homeserver_url, timeout_ms
        );
        if let Some(since) = since {
            url.push_str(&format!("&since={}", since));
        }

        let resp = self
            .client
            .get(&url)
            .header(
                "Authorization",
                format!("Bearer {}", self.config.access_token),
            )
            .send()
            .await
            .map_err(|e| format!("HTTP error: {}", e))?;

        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            return Err(format!("Matrix API error {}: {}", status, body));
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

    /// Compute domain hash for a Matrix room ID.
    ///
    /// Hash input includes platform prefix per RFC-0850 S3.1:
    /// `BLAKE3-256("matrix:{room_id}")`
    pub fn domain_hash(room_id: &str) -> [u8; 32] {
        let normalized = room_id.trim().to_lowercase();
        *blake3::hash(format!("matrix:{}", normalized).as_bytes()).as_bytes()
    }

    /// Platform type constant (0x0003 = Matrix).
    ///
    /// Corresponds to `PlatformType::Matrix` in `octo-network::dot::domain`.
    pub const PLATFORM_TYPE: u16 = 0x0003;

    /// Maximum payload bytes per event (Matrix limit).
    pub fn max_payload_bytes() -> usize {
        65536
    }

    /// Rate limit: requests per second.
    pub fn rate_limit_per_second() -> u32 {
        100
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
        platform: "matrix".to_string(),
        reason: msg.into(),
    }
}

#[async_trait]
impl PlatformAdapter for MatrixAdapter {
    async fn send_message(
        &self,
        domain: &BroadcastDomainId,
        envelope: &DeterministicEnvelope,
        _payload: &[u8],
    ) -> Result<DeliveryReceipt, PlatformAdapterError> {
        let wire_bytes = envelope.to_wire_bytes();
        let encoded = Self::encode_envelope(&wire_bytes);

        let room_id = self
            .config
            .rooms
            .iter()
            .find(|r| {
                let hash = Self::domain_hash(r);
                hash == domain.domain_hash
            })
            .ok_or_else(|| {
                transport_err(format!("No room found for domain {:?}", domain.domain_hash))
            })?;

        // Retry with exponential backoff
        let retry_cfg = octo_network::dot::adapters::backoff::RetryConfig::default();
        let mut last_err = String::new();

        for attempt in 0..=retry_cfg.max_retries {
            let result = if wire_bytes.len() > Self::max_payload_bytes() {
                // Upload as media file for large envelopes
                match self
                    .upload_media("envelope.bin", &wire_bytes, "application/octet-stream")
                    .await
                {
                    Ok(upload) => {
                        let content = serde_json::json!({
                            "msgtype": "m.file",
                            "body": "envelope.bin",
                            "url": upload.content_uri
                        });
                        self.send_message(room_id, content)
                            .await
                            .map(|r| r.event_id)
                    }
                    Err(e) => Err(e),
                }
            } else {
                self.send_text(room_id, &encoded).await.map(|r| r.event_id)
            };

            match result {
                Ok(event_id) => {
                    return Ok(DeliveryReceipt {
                        platform_message_id: event_id,
                        delivered_at: 0,
                    });
                }
                Err(e) => {
                    last_err = e.clone();
                    if (e.contains("429")
                        || e.contains("rate limit")
                        || e.contains("M_LIMIT_EXCEEDED"))
                        & retry_cfg.should_retry(attempt)
                    {
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
        // Use incremental sync with next_batch token (ZeroClaw pattern)
        let since = self.next_batch.lock().unwrap().clone();
        let sync = self
            .sync(since.as_deref(), 5000)
            .await
            .map_err(transport_err)?;

        // Persist next_batch for next call
        if !sync.next_batch.is_empty() {
            *self.next_batch.lock().unwrap() = Some(sync.next_batch.clone());
        }

        let mut result = Vec::new();
        if let Some(rooms) = sync.rooms {
            for (_room_id, room) in rooms.join {
                if let Some(timeline) = room.timeline {
                    for event in timeline.events {
                        if let Some(content) = event.content {
                            if let Some(body) = content.get("body").and_then(|b| b.as_str()) {
                                if let Ok(payload) = Self::decode_envelope(body) {
                                    let mut metadata = std::collections::BTreeMap::new();
                                    if let Some(sender) = &event.sender {
                                        metadata.insert("sender".to_string(), sender.clone());
                                    }
                                    if let Some(event_id) = &event.event_id {
                                        metadata.insert("event_id".to_string(), event_id.clone());
                                    }
                                    result.push(RawPlatformMessage {
                                        platform_id: event.event_id.unwrap_or_default(),
                                        payload,
                                        metadata,
                                    });
                                }
                            }
                        }
                    }
                }
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
            supports_fragmentation: true, // Via media upload
            supports_encryption: false,
            supports_raw_binary: false,
            rate_limit_per_second: Self::rate_limit_per_second(),
            media_capabilities: Some(octo_network::dot::adapters::MediaCapabilities {
                max_upload_bytes: 50 * 1024 * 1024, // 50MB
                supported_mime_types: vec![
                    "image/jpeg".into(),
                    "image/png".into(),
                    "image/gif".into(),
                    "video/mp4".into(),
                    "audio/ogg".into(),
                    "application/pdf".into(),
                    "application/octet-stream".into(),
                ],
            }),

            ..Default::default()
        }
    }

    fn domain_id(&self, platform_id: &str) -> BroadcastDomainId {
        BroadcastDomainId::new(PlatformType::Matrix, platform_id)
    }

    fn platform_type(&self) -> PlatformType {
        PlatformType::Matrix
    }

    fn self_handle(&self) -> Option<String> {
        // Return cached value if available (sync access)
        self.user_id.try_lock().ok().and_then(|guard| guard.clone())
    }

    async fn shutdown(&self) -> Result<(), PlatformAdapterError> {
        *self.user_id.lock().await = None;
        *self.next_batch.lock().unwrap() = None;
        Ok(())
    }

    async fn health_check(&self) -> Result<(), PlatformAdapterError> {
        // Lightweight liveness probe: GET /versions with 5s timeout
        let timeout = std::time::Duration::from_secs(5);
        let url = format!("{}/_matrix/client/versions", self.config.homeserver_url);
        match tokio::time::timeout(timeout, self.client.get(&url).send()).await {
            Ok(Ok(resp)) => {
                if resp.status().is_success() {
                    // Also resolve and cache user_id via whoami
                    let whoami_url = format!(
                        "{}/_matrix/client/v3/account/whoami",
                        self.config.homeserver_url
                    );
                    if let Ok(whoami_resp) = self
                        .client
                        .get(&whoami_url)
                        .header(
                            "Authorization",
                            format!("Bearer {}", self.config.access_token),
                        )
                        .send()
                        .await
                    {
                        if let Ok(data) = whoami_resp.json::<serde_json::Value>().await {
                            if let Some(uid) = data["user_id"].as_str() {
                                let mut guard = self.user_id.lock().await;
                                *guard = Some(uid.to_string());
                            }
                        }
                    }
                    Ok(())
                } else {
                    Err(transport_err(format!(
                        "Health check failed: HTTP {}",
                        resp.status()
                    )))
                }
            }
            Ok(Err(e)) => Err(transport_err(format!("Health check failed: {}", e))),
            Err(_) => Err(transport_err("Health check timed out after 5s")),
        }
    }

    async fn upload_media(
        &self,
        filename: &str,
        data: &[u8],
        mime_type: &str,
    ) -> Result<String, PlatformAdapterError> {
        // Use the existing upload_media method
        self.upload_media(filename, data, mime_type)
            .await
            .map(|resp| resp.content_uri)
            .map_err(|e| transport_err(format!("Upload failed: {e}")))
    }

    async fn download_media(&self, media_id: &str) -> Result<Vec<u8>, PlatformAdapterError> {
        // Download via Matrix media API
        let url = format!(
            "{}/_matrix/media/v3/download/{}",
            self.config.homeserver_url, media_id
        );
        let resp = self
            .client
            .get(&url)
            .header(
                "Authorization",
                format!("Bearer {}", self.config.access_token),
            )
            .send()
            .await
            .map_err(|e| transport_err(format!("Download failed: {e}")))?;

        let bytes = resp
            .bytes()
            .await
            .map_err(|e| transport_err(format!("Download read: {e}")))?;
        Ok(bytes.to_vec())
    }
}

// --- Matrix API types ---

#[derive(Debug, Clone, Deserialize)]
pub struct MatrixSendResponse {
    pub event_id: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct MatrixMediaUploadResponse {
    pub content_uri: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct MatrixSyncResponse {
    pub next_batch: String,
    #[serde(default)]
    pub rooms: Option<MatrixSyncRooms>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct MatrixSyncRooms {
    #[serde(default)]
    pub join: std::collections::BTreeMap<String, MatrixJoinedRoom>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct MatrixJoinedRoom {
    #[serde(default)]
    pub timeline: Option<MatrixTimeline>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct MatrixTimeline {
    #[serde(default)]
    pub events: Vec<MatrixEvent>,
    #[serde(default)]
    pub limited: bool,
    #[serde(default)]
    pub prev_batch: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct MatrixEvent {
    pub event_id: Option<String>,
    pub sender: Option<String>,
    #[serde(rename = "type")]
    pub event_type: Option<String>,
    pub content: Option<serde_json::Value>,
    pub origin_server_ts: Option<i64>,
}

// --- Plugin ABI exports (for cdylib loading) ---

#[no_mangle]
pub extern "C" fn adapter_version() -> u32 {
    1
}

#[no_mangle]
pub extern "C" fn platform_type() -> u16 {
    0x0003 // Matrix
}

#[no_mangle]
/// # Safety
/// `config` must point to a valid buffer of at least `len` bytes.
pub unsafe extern "C" fn create_adapter(config: *const u8, config_len: usize) -> *mut () {
    if config.is_null() || config_len == 0 {
        return std::ptr::null_mut();
    }

    let config_bytes = std::slice::from_raw_parts(config, config_len);
    match MatrixAdapter::from_config_bytes(config_bytes) {
        Ok(adapter) => Box::into_raw(Box::new(adapter)) as *mut (),
        Err(_) => std::ptr::null_mut(),
    }
}

#[no_mangle]
/// # Safety
/// `ptr` must be a pointer previously returned by `create_adapter`.
pub unsafe extern "C" fn destroy_adapter(adapter: *mut ()) {
    if !adapter.is_null() {
        let _ = Box::from_raw(adapter as *mut MatrixAdapter);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;

    #[test]
    fn test_encode_decode_envelope() {
        let original = b"test envelope data for matrix";
        let encoded = MatrixAdapter::encode_envelope(original);
        assert!(encoded.starts_with("DOT/1/"));
        let decoded = MatrixAdapter::decode_envelope(&encoded).unwrap();
        assert_eq!(decoded, original);
    }

    #[test]
    fn test_domain_hash_deterministic() {
        let h1 = MatrixAdapter::domain_hash("!abcdef:example.com");
        let h2 = MatrixAdapter::domain_hash("!abcdef:example.com");
        assert_eq!(h1, h2);
    }

    #[test]
    fn test_domain_hash_normalized() {
        let h1 = MatrixAdapter::domain_hash("!abcdef:example.com");
        let h2 = MatrixAdapter::domain_hash("  !abcdef:example.com  ");
        assert_eq!(h1, h2);
    }

    #[test]
    fn test_domain_hash_case_insensitive() {
        let h1 = MatrixAdapter::domain_hash("!ABCDEF:Example.COM");
        let h2 = MatrixAdapter::domain_hash("!abcdef:example.com");
        assert_eq!(h1, h2);
    }

    #[test]
    fn test_platform_type() {
        assert_eq!(MatrixAdapter::PLATFORM_TYPE, 0x0003);
    }

    #[test]
    fn test_abi_exports() {
        assert_eq!(adapter_version(), 1);
        assert_eq!(platform_type(), 0x0003);
    }

    #[test]
    fn test_config_from_json() {
        let config = serde_json::json!({
            "homeserver_url": "https://matrix.example.com",
            "access_token": "syt_test",
            "rooms": ["!abc:example.com"]
        });
        let adapter =
            MatrixAdapter::from_config_bytes(serde_json::to_vec(&config).unwrap().as_slice())
                .unwrap();
        assert_eq!(adapter.config.homeserver_url, "https://matrix.example.com");
        assert_eq!(adapter.config.rooms.len(), 1);
    }

    #[test]
    fn test_capabilities() {
        assert_eq!(MatrixAdapter::max_payload_bytes(), 65536);
        assert_eq!(MatrixAdapter::rate_limit_per_second(), 100);
    }

    #[test]
    fn test_encode_decode_roundtrip() {
        let data = vec![0u8; 1024];
        let encoded = MatrixAdapter::encode_envelope(&data);
        let decoded = MatrixAdapter::decode_envelope(&encoded).unwrap();
        assert_eq!(decoded, data);
    }

    #[test]
    fn test_decode_invalid() {
        assert!(MatrixAdapter::decode_envelope("NOTDOT/1/abc").is_err());
        assert!(MatrixAdapter::decode_envelope("DOT/1/!!!invalid!!!").is_err());
    }

    #[test]
    fn test_largest_payload() {
        // Matrix supports 65KB — verify we can encode/decode near that limit
        let data = vec![0xABu8; 60000];
        let encoded = MatrixAdapter::encode_envelope(&data);
        let decoded = MatrixAdapter::decode_envelope(&encoded).unwrap();
        assert_eq!(decoded, data);
    }

    #[test]
    fn test_trait_capabilities() {
        let adapter = make_test_adapter();
        let caps = PlatformAdapter::capabilities(&adapter);
        assert_eq!(caps.max_payload_bytes, 65536);
        assert!(caps.supports_fragmentation);
        assert!(!caps.supports_raw_binary);
        assert!(caps.media_capabilities.is_some());
        let media = caps.media_capabilities.unwrap();
        assert_eq!(media.max_upload_bytes, 50 * 1024 * 1024);
        assert!(media
            .supported_mime_types
            .contains(&"image/jpeg".to_string()));
    }

    #[test]
    fn test_trait_platform_type() {
        let adapter = make_test_adapter();
        assert_eq!(
            PlatformAdapter::platform_type(&adapter),
            PlatformType::Matrix
        );
    }

    #[test]
    fn test_trait_domain_id() {
        let adapter = make_test_adapter();
        let domain = PlatformAdapter::domain_id(&adapter, "!abc:example.com");
        assert_eq!(domain.platform_type, PlatformType::Matrix as u16);
    }

    #[test]
    fn test_trait_self_handle_none_initially() {
        let adapter = make_test_adapter();
        assert!(adapter.self_handle().is_none());
    }

    #[test]
    fn test_canonicalize_empty_payload() {
        let adapter = make_test_adapter();
        let raw = RawPlatformMessage {
            platform_id: "test".into(),
            payload: vec![],
            metadata: BTreeMap::new(),
        };
        assert!(adapter.canonicalize(&raw).is_err());
    }

    #[test]
    fn test_canonicalize_valid_envelope() {
        let adapter = make_test_adapter();
        let envelope = DeterministicEnvelope::default();
        let wire = envelope.to_wire_bytes();
        let raw = RawPlatformMessage {
            platform_id: "test".into(),
            payload: wire,
            metadata: BTreeMap::new(),
        };
        let parsed = adapter.canonicalize(&raw).unwrap();
        assert_eq!(parsed.envelope_id, envelope.envelope_id);
    }

    #[test]
    fn test_config_debug_redacts_token() {
        let config = MatrixConfig {
            homeserver_url: "https://matrix.example.com".into(),
            access_token: "syt_verysecrettoken123".into(),
            rooms: vec!["!abc:example.com".into()],
        };
        let debug = format!("{:?}", config);
        // Token should be redacted (not fully visible)
        assert!(!debug.contains("verysecrettoken123"));
        assert!(debug.contains("***"));
    }

    #[test]
    fn test_config_invalid_json() {
        let result = MatrixAdapter::from_config_bytes(b"not json");
        assert!(result.is_err());
    }

    #[test]
    fn test_config_missing_field() {
        let json = r#"{"homeserver_url": "https://example.com"}"#;
        let result = MatrixAdapter::from_config_bytes(json.as_bytes());
        assert!(result.is_err());
    }

    fn make_test_adapter() -> MatrixAdapter {
        let config = MatrixConfig {
            homeserver_url: "https://matrix.example.com".into(),
            access_token: "syt_test".into(),
            rooms: vec!["!abc:example.com".into()],
        };
        MatrixAdapter::new(config)
    }
}
