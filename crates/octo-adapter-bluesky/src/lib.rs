//! Bluesky (AT Protocol) adapter for DOT (RFC-0850 S8.1, PlatformType::Bluesky)
//!
//! Bridges DOT envelopes to Bluesky via the AT Protocol (XRPC API).
//! Uses app password authentication (OAuth2-like flow with session JWT).
//!
//! ## Configuration
//!
//! ```json
//! {
//!   "handle": "alice.bsky.social",
//!   "app_password": "xxxx-xxxx-xxxx-xxxx",
//!   "pds_url": "https://bsky.social"
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
pub struct BlueskyConfig {
    /// Bluesky handle (e.g., "alice.bsky.social")
    pub handle: String,
    /// App password (NOT account password)
    pub app_password: String,
    /// PDS URL (optional, default: https://bsky.social)
    #[serde(default = "default_pds_url")]
    pub pds_url: String,
}

fn default_pds_url() -> String {
    "https://bsky.social".to_string()
}

impl std::fmt::Debug for BlueskyConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("BlueskyConfig")
            .field("handle", &self.handle)
            .field("app_password", &"***")
            .field("pds_url", &self.pds_url)
            .finish()
    }
}

// ── Session ────────────────────────────────────────────────────────

/// Cached AT Protocol session (access JWT + refresh JWT)
struct Session {
    access_jwt: String,
    refresh_jwt: String,
    did: String,
}

// ── Bluesky Adapter ─────────────────────────────────────────────────

pub struct BlueskyAdapter {
    config: BlueskyConfig,
    client: reqwest::Client,
    /// Cached session (access_jwt, refresh_jwt, did)
    session: Arc<Mutex<Option<Session>>>,
}

impl BlueskyAdapter {
    pub fn new(config: BlueskyConfig) -> Self {
        Self {
            config,
            client: reqwest::Client::new(),
            session: Arc::new(Mutex::new(None)),
        }
    }

    pub fn from_config_bytes(config: &[u8]) -> Result<Self, String> {
        let config: BlueskyConfig =
            serde_json::from_slice(config).map_err(|e| format!("Invalid config: {e}"))?;
        Ok(Self::new(config))
    }

    /// Create or refresh an AT Protocol session.
    async fn ensure_session(&self) -> Result<(), PlatformAdapterError> {
        // Check if we have a valid session
        {
            let guard = self.session.lock();
            if guard.is_some() {
                return Ok(());
            }
        }

        // Create new session
        let url = format!("{}/xrpc/com.atproto.server.createSession", self.config.pds_url);
        let body = serde_json::json!({
            "identifier": self.config.handle,
            "password": self.config.app_password,
        });

        let resp = self
            .client
            .post(&url)
            .json(&body)
            .send()
            .await
            .map_err(|e| transport_err(format!("Session creation failed: {e}")))?
            .json::<serde_json::Value>()
            .await
            .map_err(|e| transport_err(format!("Session parse failed: {e}")))?;

        let access_jwt = resp["accessJwt"]
            .as_str()
            .ok_or_else(|| transport_err("Missing accessJwt"))?
            .to_string();
        let refresh_jwt = resp["refreshJwt"]
            .as_str()
            .ok_or_else(|| transport_err("Missing refreshJwt"))?
            .to_string();
        let did = resp["did"]
            .as_str()
            .ok_or_else(|| transport_err("Missing did"))?
            .to_string();

        *self.session.lock() = Some(Session {
            access_jwt,
            refresh_jwt,
            did,
        });

        tracing::info!("Bluesky session created for {}", self.config.handle);
        Ok(())
    }

    fn api_base() -> &'static str {
        "https://bsky.social/xrpc"
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

    pub fn domain_hash(did: &str) -> [u8; 32] {
        let normalized = did.trim().to_lowercase();
        *blake3::hash(format!("bluesky:{}", normalized).as_bytes()).as_bytes()
    }

    pub const PLATFORM_TYPE: u16 = 0x000E;
    pub fn max_payload_bytes() -> usize {
        300
    }
    pub fn rate_limit_per_second() -> u32 {
        1
    }

    /// Post a text to Bluesky.
    async fn post_text(&self, text: &str) -> Result<String, PlatformAdapterError> {
        self.ensure_session().await?;

        // Clone values to avoid holding lock across await
        let (access_jwt, did) = {
            let guard = self.session.lock();
            let session = guard
                .as_ref()
                .ok_or_else(|| transport_err("No session"))?;
            (session.access_jwt.clone(), session.did.clone())
        };

        let url = format!(
            "{}/xrpc/com.atproto.repo.createRecord",
            self.config.pds_url
        );
        let body = serde_json::json!({
            "repo": did,
            "collection": "app.bsky.feed.post",
            "record": {
                "$type": "app.bsky.feed.post",
                "text": text,
                "createdAt": chrono::Utc::now().to_rfc3339(),
            }
        });

        let resp = self
            .client
            .post(&url)
            .header("Authorization", format!("Bearer {}", access_jwt))
            .json(&body)
            .send()
            .await
            .map_err(|e| transport_err(format!("Post failed: {e}")))?
            .json::<serde_json::Value>()
            .await
            .map_err(|e| transport_err(format!("Post parse failed: {e}")))?;

        let uri = resp["uri"]
            .as_str()
            .unwrap_or("unknown")
            .to_string();
        Ok(uri)
    }
}

fn transport_err(msg: impl Into<String>) -> PlatformAdapterError {
    PlatformAdapterError::Unreachable {
        platform: "bluesky".into(),
        reason: msg.into(),
    }
}

// ── PlatformAdapter ────────────────────────────────────────────────

#[async_trait]
impl PlatformAdapter for BlueskyAdapter {
    async fn send_envelope(
        &self,
        domain: &BroadcastDomainId,
        envelope: &DeterministicEnvelope,
    ) -> Result<DeliveryReceipt, PlatformAdapterError> {
        let wire_bytes = envelope.to_wire_bytes();
        let encoded = Self::encode_envelope(&wire_bytes);

        // Post to Bluesky
        let uri = self.post_text(&encoded).await?;

        Ok(DeliveryReceipt {
            platform_message_id: uri,
            delivered_at: epoch_millis(),
        })
    }

    async fn receive_messages(
        &self,
        _domain: &BroadcastDomainId,
    ) -> Result<Vec<RawPlatformMessage>, PlatformAdapterError> {
        // TODO: Implement polling of app.bsky.feed.getTimeline
        // For now, return empty (outbound-only)
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
        let wire_bytes = Self::decode_envelope(&text).map_err(|e| {
            PlatformAdapterError::ApiError {
                code: 400,
                message: format!("canonicalize failed: {e}"),
            }
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
            rate_limit_per_second: Self::rate_limit_per_second(),
            media_capabilities: Some(MediaCapabilities {
                max_upload_bytes: 976_563, // 1MB image
                supported_mime_types: vec![
                    "image/jpeg".to_string(),
                    "image/png".to_string(),
                    "image/webp".to_string(),
                ],
            }),
        }
    }

    fn domain_id(&self, platform_id: &str) -> BroadcastDomainId {
        BroadcastDomainId::new(PlatformType::Bluesky, platform_id)
    }

    fn platform_type(&self) -> PlatformType {
        PlatformType::Bluesky
    }

    fn self_handle(&self) -> Option<String> {
        let guard = self.session.lock();
        guard.as_ref().map(|s| s.did.clone())
    }

    async fn shutdown(&self) -> Result<(), PlatformAdapterError> {
        *self.session.lock() = None;
        Ok(())
    }

    async fn health_check(&self) -> Result<(), PlatformAdapterError> {
        // Try to create/refresh session
        self.ensure_session().await
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
    0x000E
}

/// Create adapter instance from JSON config bytes.
///
/// # Safety
///
/// `config` must point to a valid buffer of at least `config_len` bytes.
#[no_mangle]
pub unsafe extern "C" fn create_adapter(config: *const u8, config_len: usize) -> *mut () {
    if config.is_null() || config_len == 0 {
        return std::ptr::null_mut();
    }
    let bytes = std::slice::from_raw_parts(config, config_len);
    match BlueskyAdapter::from_config_bytes(bytes) {
        Ok(a) => Box::into_raw(Box::new(a)) as *mut (),
        Err(_) => std::ptr::null_mut(),
    }
}

/// Destroy adapter instance.
///
/// # Safety
///
/// `adapter` must be a pointer previously returned by `create_adapter`.
#[no_mangle]
pub unsafe extern "C" fn destroy_adapter(adapter: *mut ()) {
    if !adapter.is_null() {
        let _ = Box::from_raw(adapter as *mut BlueskyAdapter);
    }
}

// ── Tests ──────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_domain_hash_deterministic() {
        let h1 = BlueskyAdapter::domain_hash("did:plc:abc123");
        let h2 = BlueskyAdapter::domain_hash("did:plc:abc123");
        assert_eq!(h1, h2);
    }

    #[test]
    fn test_domain_hash_normalized() {
        assert_eq!(
            BlueskyAdapter::domain_hash("DID:PLC:ABC123"),
            BlueskyAdapter::domain_hash("  did:plc:abc123  ")
        );
    }

    #[test]
    fn test_encode_decode_envelope() {
        let original = b"test bluesky envelope";
        let encoded = BlueskyAdapter::encode_envelope(original);
        assert!(encoded.starts_with("DOT/1/"));
        let decoded = BlueskyAdapter::decode_envelope(&encoded).unwrap();
        assert_eq!(decoded, original);
    }

    #[test]
    fn test_platform_type() {
        assert_eq!(BlueskyAdapter::PLATFORM_TYPE, 0x000E);
    }

    #[test]
    fn test_abi_exports() {
        assert_eq!(adapter_version(), 1);
        assert_eq!(platform_type(), 0x000E);
    }

    #[test]
    fn test_capabilities() {
        let config = BlueskyConfig {
            handle: "test.bsky.social".into(),
            app_password: "test".into(),
            pds_url: "https://bsky.social".into(),
        };
        let adapter = BlueskyAdapter::new(config);
        let caps = adapter.capabilities();
        assert_eq!(caps.max_payload_bytes, 300);
        assert!(caps.supports_fragmentation);
        assert!(!caps.supports_encryption);
        assert_eq!(caps.rate_limit_per_second, 1);
        assert!(caps.media_capabilities.is_some());
    }

    #[test]
    fn test_decode_missing_prefix() {
        assert!(BlueskyAdapter::decode_envelope("hello").is_err());
    }

    #[test]
    fn test_decode_invalid_base64() {
        assert!(BlueskyAdapter::decode_envelope("DOT/1/!!!invalid!!!").is_err());
    }

    #[test]
    fn test_self_handle_none_initially() {
        let config = BlueskyConfig {
            handle: "test.bsky.social".into(),
            app_password: "test".into(),
            pds_url: "https://bsky.social".into(),
        };
        let adapter = BlueskyAdapter::new(config);
        assert!(adapter.self_handle().is_none());
    }

    #[test]
    fn test_config_from_json() {
        let json = serde_json::json!({
            "handle": "alice.bsky.social",
            "app_password": "xxxx-xxxx-xxxx-xxxx",
            "pds_url": "https://bsky.social"
        });
        let adapter = BlueskyAdapter::from_config_bytes(
            serde_json::to_vec(&json).unwrap().as_slice(),
        )
        .unwrap();
        assert_eq!(adapter.config.handle, "alice.bsky.social");
    }

    #[test]
    fn test_config_default_pds_url() {
        let json = serde_json::json!({
            "handle": "alice.bsky.social",
            "app_password": "xxxx"
        });
        let adapter = BlueskyAdapter::from_config_bytes(
            serde_json::to_vec(&json).unwrap().as_slice(),
        )
        .unwrap();
        assert_eq!(adapter.config.pds_url, "https://bsky.social");
    }
}
