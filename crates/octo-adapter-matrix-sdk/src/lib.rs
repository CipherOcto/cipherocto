//! Matrix adapter for DOT using matrix-rust-sdk (RFC-0850 S8.1, PlatformType::Matrix)
//!
//! This adapter replaces the in-house reqwest-based Matrix adapter with the
//! official matrix-rust-sdk for better federation support, media handling,
//! and long-term maintenance.
//!
//! Exports C ABI functions (`adapter_version`, `platform_type`, `create_adapter`,
//! `destroy_adapter`) for dynamic loading as a cdylib plugin.
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

use base64::Engine;
use matrix_sdk::Client;
use serde::Deserialize;
use std::sync::Arc;
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

/// Matrix adapter using matrix-rust-sdk.
pub struct MatrixAdapter {
    config: MatrixConfig,
    client: Client,
    runtime: tokio::runtime::Runtime,
    /// Sync token for incremental /sync (ZeroClaw pattern).
    next_batch: std::sync::Mutex<Option<String>>,
    /// Cached bot user ID (e.g., @bot:server)
    user_id: std::sync::Mutex<Option<String>>,
}

impl MatrixAdapter {
    /// Create a new Matrix adapter from config.
    ///
    /// Builds a tokio runtime and initializes the matrix-sdk Client.
    pub fn new(config: MatrixConfig) -> Result<Self, String> {
        let runtime = tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .build()
            .map_err(|e| format!("Failed to create tokio runtime: {}", e))?;

        let client = runtime.block_on(async {
            Client::builder()
                .homeserver_url(&config.homeserver_url)
                .build()
                .await
                .map_err(|e| format!("Failed to build Matrix client: {}", e))
        })?;

        Ok(Self {
            config,
            client,
            runtime,
            next_batch: std::sync::Mutex::new(None),
            user_id: std::sync::Mutex::new(None),
        })
    }

    /// Create from JSON config bytes (used by plugin ABI).
    pub fn from_config_bytes(config: &[u8]) -> Result<Self, String> {
        let config: MatrixConfig =
            serde_json::from_slice(config).map_err(|e| format!("Invalid config: {}", e))?;
        Self::new(config)
    }

    /// Run an async operation on the embedded tokio runtime.
    fn block_on<F: std::future::Future>(&self, future: F) -> F::Output {
        self.runtime.block_on(future)
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
use matrix_sdk::ruma::{events::room::message::RoomMessageEventContent, RoomId};
use matrix_sdk::media::MediaFormat;
use octo_network::dot::adapters::{
    backoff::RetryConfig, CapabilityReport, DeliveryReceipt, MediaCapabilities,
    PlatformAdapter, RawPlatformMessage,
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
    async fn send_envelope(
        &self,
        domain: &BroadcastDomainId,
        envelope: &DeterministicEnvelope,
    ) -> Result<DeliveryReceipt, PlatformAdapterError> {
        let wire_bytes = envelope.to_wire_bytes();
        let encoded = Self::encode_envelope(&wire_bytes);

        // Find matching room by domain hash
        let room_id_str = self
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

        let room_id = RoomId::parse(room_id_str).map_err(|e| {
            transport_err(format!("Invalid room ID '{}': {}", room_id_str, e))
        })?;

        let room = self.client.get_room(&room_id).ok_or_else(|| {
            transport_err(format!("Room {} not found in joined rooms", room_id_str))
        })?;

        // Retry with exponential backoff
        let retry_cfg = RetryConfig::default();
        let mut last_err = String::new();

        for attempt in 0..=retry_cfg.max_retries {
            let result = if wire_bytes.len() > Self::max_payload_bytes() {
                // Upload as media file for large envelopes
                match self
                    .client
                    .media()
                    .upload(
                        &matrix_sdk::mime::APPLICATION_OCTET_STREAM,
                        wire_bytes.clone(),
                        None,
                    )
                    .await
                {
                    Ok(response) => {
                        let content = RoomMessageEventContent::text_plain(format!(
                            "DOT/1/{}",
                            response.content_uri.as_str()
                        ));
                        room.send(content).await.map(|_| String::new())
                    }
                    Err(e) => Err(e),
                }
            } else {
                let content = RoomMessageEventContent::text_plain(&encoded);
                room.send(content).await.map(|_| String::new())
            };

            match result {
                Ok(_) => {
                    // SDK doesn't return event_id directly from send(),
                    // use a transaction ID as receipt
                    let txn_id = Uuid::new_v4().to_string();
                    return Ok(DeliveryReceipt {
                        platform_message_id: txn_id,
                        delivered_at: 0,
                    });
                }
                Err(e) => {
                    let err_str = format!("{}", e);
                    last_err = err_str.clone();
                    if (err_str.contains("429")
                        || err_str.contains("rate limit")
                        || err_str.contains("M_LIMIT_EXCEEDED"))
                        && retry_cfg.should_retry(attempt)
                    {
                        let delay = retry_cfg.delay_for_attempt(attempt);
                        tokio::time::sleep(delay).await;
                        continue;
                    }
                    return Err(transport_err(err_str));
                }
            }
        }
        Err(transport_err(format!("Retries exhausted: {}", last_err)))
    }

    async fn receive_messages(
        &self,
        _domain: &BroadcastDomainId,
    ) -> Result<Vec<RawPlatformMessage>, PlatformAdapterError> {
        use matrix_sdk::sync::SyncSettings;
        use std::time::Duration;

        let since = self.next_batch.lock().unwrap().clone();

        let mut sync_settings = SyncSettings::default().timeout(Duration::from_secs(5));
        if let Some(ref token) = since {
            sync_settings = sync_settings.token(token);
        }

        let response = self
            .client
            .sync_once(sync_settings)
            .await
            .map_err(|e| transport_err(format!("Sync failed: {}", e)))?;

        // Persist next_batch for next call
        if !response.next_batch.is_empty() {
            *self.next_batch.lock().unwrap() = Some(response.next_batch.clone());
        }

        let mut result = Vec::new();
        for (room_id, joined) in &response.rooms.join {
            for event in &joined.timeline.events {
                // Try to extract message body from the event
                if let Ok(deserialized) = event.event.deserialize() {
                    if let Some(content) = deserialized.as_original().map(|o| &o.content) {
                        // Check if this is a room message with text body
                        if let Ok(msg_content) =
                            content.get_field::<serde_json::Value>("body")
                        {
                            if let Some(body) = msg_content.and_then(|v| v.as_str()) {
                                if let Ok(payload) = Self::decode_envelope(body) {
                                    let mut metadata = std::collections::BTreeMap::new();
                                    let event_id =
                                        deserialized.event_id().to_string();
                                    let sender = deserialized.sender().to_string();
                                    metadata
                                        .insert("sender".to_string(), sender);
                                    metadata
                                        .insert("event_id".to_string(), event_id.clone());
                                    metadata
                                        .insert("room_id".to_string(), room_id.to_string());
                                    result.push(RawPlatformMessage {
                                        platform_id: event_id,
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
            media_capabilities: Some(MediaCapabilities {
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
        }
    }

    fn domain_id(&self, platform_id: &str) -> BroadcastDomainId {
        BroadcastDomainId::new(PlatformType::Matrix, platform_id)
    }

    fn platform_type(&self) -> PlatformType {
        PlatformType::Matrix
    }

    fn self_handle(&self) -> Option<String> {
        self.user_id.lock().ok().and_then(|guard| guard.clone())
    }

    async fn shutdown(&self) -> Result<(), PlatformAdapterError> {
        *self.user_id.lock().unwrap() = None;
        *self.next_batch.lock().unwrap() = None;
        Ok(())
    }

    async fn health_check(&self) -> Result<(), PlatformAdapterError> {
        use std::time::Duration;

        // Lightweight liveness probe: sync_once with zero timeout
        let sync_settings =
            matrix_sdk::sync::SyncSettings::default().timeout(Duration::from_millis(1));

        match tokio::time::timeout(
            Duration::from_secs(5),
            self.client.sync_once(sync_settings),
        )
        .await
        {
            Ok(Ok(_)) => {
                // Resolve and cache user_id
                match self.client.whoami().await {
                    Ok(resp) => {
                        let mut guard = self.user_id.lock().unwrap();
                        *guard = Some(resp.user_id.to_string());
                        Ok(())
                    }
                    Err(e) => Err(transport_err(format!("whoami failed: {}", e))),
                }
            }
            Ok(Err(e)) => Err(transport_err(format!("Health check failed: {}", e))),
            Err(_) => Err(transport_err("Health check timed out after 5s")),
        }
    }

    async fn upload_media(
        &self,
        _filename: &str,
        data: &[u8],
        mime_type: &str,
    ) -> Result<String, PlatformAdapterError> {
        let mime: matrix_sdk::mime::Mime = mime_type.parse().map_err(|e| {
            transport_err(format!("Invalid MIME type '{}': {}", mime_type, e))
        })?;

        let response = self
            .client
            .media()
            .upload(&mime, data.to_vec(), None)
            .await
            .map_err(|e| transport_err(format!("Upload failed: {}", e)))?;

        Ok(response.content_uri.as_str().to_string())
    }

    async fn download_media(&self, media_id: &str) -> Result<Vec<u8>, PlatformAdapterError> {
        use matrix_sdk::ruma::MxcUri;

        let mxc_uri = MxcUri::parse(media_id).map_err(|e| {
            transport_err(format!("Invalid MXC URI '{}': {}", media_id, e))
        })?;

        let request = matrix_sdk::media::MediaRequestParameters {
            source: matrix_sdk::media::MediaSource::Plain(mxc_uri.to_owned()),
            format: MediaFormat::File,
        };

        let bytes = self
            .client
            .media()
            .get_media_content(&request, false)
            .await
            .map_err(|e| transport_err(format!("Download failed: {}", e)))?;

        Ok(bytes.to_vec())
    }
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
        let data = vec![0xABu8; 60000];
        let encoded = MatrixAdapter::encode_envelope(&data);
        let decoded = MatrixAdapter::decode_envelope(&encoded).unwrap();
        assert_eq!(decoded, data);
    }

    #[test]
    fn test_adapter_creation() {
        let config = serde_json::json!({
            "homeserver_url": "https://matrix.example.com",
            "access_token": "syt_test_token_12345",
            "rooms": ["!abc:example.com"]
        });
        let adapter =
            MatrixAdapter::from_config_bytes(serde_json::to_vec(&config).unwrap().as_slice());
        assert!(adapter.is_ok(), "Adapter creation should succeed: {:?}", adapter.err());
    }

    #[test]
    fn test_self_handle_initially_none() {
        let config = serde_json::json!({
            "homeserver_url": "https://matrix.example.com",
            "access_token": "syt_test",
            "rooms": ["!abc:example.com"]
        });
        let adapter =
            MatrixAdapter::from_config_bytes(serde_json::to_vec(&config).unwrap().as_slice())
                .unwrap();
        assert!(adapter.self_handle().is_none());
    }
}
