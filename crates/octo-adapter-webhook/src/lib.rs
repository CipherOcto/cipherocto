//! HTTP Webhook adapter for DOT (RFC-0850 §8.1, PlatformType::Webhook)
//!
//! Enables any HTTP endpoint to participate in the DOT overlay by receiving
//! envelopes via POST/PUT and sending via configurable endpoints.
//!
//! ## Configuration
//!
//! ```json
//! {
//!   "send_url": "https://partner.example.com/dot/incoming",
//!   "listen_port": 8443,
//!   "listen_path": "/dot/v1/envelope",
//!   "send_method": "POST",
//!   "auth_header": "Bearer my-secret-token",
//!   "hmac_secret": "shared-secret-for-signature-verification"
//! }
//! ```

use async_trait::async_trait;
use axum::{extract::State, http::StatusCode, routing::post, Router};
use hmac::{Hmac, Mac};
use sha2::Sha256;
use std::sync::Arc;
use subtle::ConstantTimeEq;
use tokio::sync::{mpsc, Mutex};

use octo_network::dot::adapters::{
    backoff::RetryConfig, CapabilityReport, DeliveryReceipt, PlatformAdapter, RawPlatformMessage,
};
use octo_network::dot::domain::{BroadcastDomainId, PlatformType};
use octo_network::dot::envelope::DeterministicEnvelope;
use octo_network::dot::error::PlatformAdapterError;

// ── Configuration ──────────────────────────────────────────────────

#[derive(Clone, Debug, serde::Deserialize, serde::Serialize)]
pub struct WebhookConfig {
    /// URL to send envelopes to (outbound).
    pub send_url: Option<String>,
    /// HTTP method for sending: "POST" or "PUT".
    #[serde(default = "default_send_method")]
    pub send_method: String,
    /// Port to listen on for incoming webhooks.
    #[serde(default = "default_listen_port")]
    pub listen_port: u16,
    /// URL path to listen on.
    #[serde(default = "default_listen_path")]
    pub listen_path: String,
    /// Optional Authorization header value.
    pub auth_header: Option<String>,
    /// Optional HMAC-SHA256 shared secret for signature verification.
    pub hmac_secret: Option<String>,
}

fn default_send_method() -> String {
    "POST".into()
}
fn default_listen_port() -> u16 {
    8443
}
fn default_listen_path() -> String {
    "/dot/v1/envelope".into()
}

// ── Adapter ────────────────────────────────────────────────────────

/// Shared state between the axum HTTP server and the adapter.
struct WebhookState {
    tx: mpsc::Sender<RawPlatformMessage>,
    hmac_secret: Option<String>,
}

pub struct WebhookAdapter {
    config: WebhookConfig,
    client: reqwest::Client,
    /// Receiver for incoming webhook messages (populated by HTTP server).
    rx: Mutex<mpsc::Receiver<RawPlatformMessage>>,
    /// Sender — cloned into the HTTP server handler.
    tx: mpsc::Sender<RawPlatformMessage>,
    /// Whether the HTTP server has been started.
    server_started: Mutex<bool>,
}

impl WebhookAdapter {
    pub fn new(config: WebhookConfig) -> Self {
        let (tx, rx) = mpsc::channel(1024);
        Self {
            config,
            client: reqwest::Client::new(),
            rx: Mutex::new(rx),
            tx,
            server_started: Mutex::new(false),
        }
    }

    pub fn from_config_bytes(config: &[u8]) -> Result<Self, String> {
        let config: WebhookConfig =
            serde_json::from_slice(config).map_err(|e| format!("Invalid config: {}", e))?;
        Ok(Self::new(config))
    }

    /// Start the HTTP server for receiving webhooks (idempotent).
    async fn ensure_server_started(&self) -> Result<(), PlatformAdapterError> {
        let mut started = self.server_started.lock().await;
        if *started {
            return Ok(());
        }

        let listen_port = self.config.listen_port;
        let listen_path = self.config.listen_path.clone();

        let state = Arc::new(WebhookState {
            tx: self.tx.clone(),
            hmac_secret: self.config.hmac_secret.clone(),
        });

        let app = Router::new()
            .route(&listen_path, post(webhook_handler))
            .with_state(state);

        let addr = std::net::SocketAddr::from(([0, 0, 0, 0], listen_port));
        tokio::spawn(async move {
            if let Ok(listener) = tokio::net::TcpListener::bind(addr).await {
                let _ = axum::serve(listener, app).await;
            }
        });

        *started = true;
        Ok(())
    }

    /// Domain hash: `BLAKE3-256("webhook:{url}")`
    pub fn domain_hash(url: &str) -> [u8; 32] {
        *blake3::hash(format!("webhook:{}", url.trim().to_lowercase()).as_bytes()).as_bytes()
    }

    pub const PLATFORM_TYPE: u16 = 0x0009;
    pub fn max_payload_bytes() -> usize {
        1_048_576
    }
    pub fn rate_limit_per_second() -> u32 {
        100
    }

    /// Timing-safe HMAC-SHA256 verification.
    pub fn verify_hmac(secret: &[u8], message: &[u8], expected_hex: &str) -> bool {
        let Ok(expected_bytes) = hex_decode(expected_hex) else {
            return false;
        };
        let mut mac = Hmac::<Sha256>::new_from_slice(secret).unwrap();
        mac.update(message);
        let computed = mac.finalize().into_bytes();
        if computed.len() != expected_bytes.len() {
            return false;
        }
        bool::from(computed.as_slice().ct_eq(&expected_bytes))
    }

    /// Compute HMAC-SHA256 as hex string.
    pub fn compute_hmac(secret: &[u8], message: &[u8]) -> String {
        let mut mac = Hmac::<Sha256>::new_from_slice(secret).unwrap();
        mac.update(message);
        hex_encode(&mac.finalize().into_bytes())
    }
}

// ── Hex helpers ────────────────────────────────────────────────────

fn hex_decode(hex: &str) -> Result<Vec<u8>, ()> {
    if !hex.len().is_multiple_of(2) {
        return Err(());
    }
    (0..hex.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&hex[i..i + 2], 16).map_err(|_| ()))
        .collect()
}

fn hex_encode(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

// ── Axum handler ───────────────────────────────────────────────────

async fn webhook_handler(
    State(state): State<Arc<WebhookState>>,
    headers: axum::http::HeaderMap,
    body: axum::body::Bytes,
) -> StatusCode {
    // HMAC verification if secret is configured
    if let Some(ref secret) = state.hmac_secret {
        match headers.get("X-DOT-Signature") {
            Some(sig_header) => {
                let sig_str = sig_header.to_str().unwrap_or("");
                let hex = sig_str.strip_prefix("sha256=").unwrap_or("");
                if !WebhookAdapter::verify_hmac(secret.as_bytes(), &body, hex) {
                    return StatusCode::UNAUTHORIZED;
                }
            }
            None => return StatusCode::UNAUTHORIZED,
        }
    }

    let mut metadata = std::collections::BTreeMap::new();
    if let Some(ct) = headers.get("content-type") {
        metadata.insert("content-type".into(), ct.to_str().unwrap_or("").into());
    }

    let msg = RawPlatformMessage {
        platform_id: format!("wh-{}", epoch_millis()),
        payload: body.to_vec(),
        metadata,
    };

    match state.tx.try_send(msg) {
        Ok(()) => StatusCode::OK,
        Err(mpsc::error::TrySendError::Full(_)) => StatusCode::SERVICE_UNAVAILABLE,
        Err(mpsc::error::TrySendError::Closed(_)) => StatusCode::INTERNAL_SERVER_ERROR,
    }
}

fn epoch_millis() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

// ── PlatformAdapter ────────────────────────────────────────────────

fn transport_err(msg: impl Into<String>) -> PlatformAdapterError {
    PlatformAdapterError::Unreachable {
        platform: "webhook".into(),
        reason: msg.into(),
    }
}

#[async_trait]
impl PlatformAdapter for WebhookAdapter {
    async fn send_envelope(
        &self,
        _domain: &BroadcastDomainId,
        envelope: &DeterministicEnvelope,
    ) -> Result<DeliveryReceipt, PlatformAdapterError> {
        let send_url = self
            .config
            .send_url
            .as_ref()
            .ok_or_else(|| transport_err("No send_url configured"))?;

        let wire_bytes = envelope.to_wire_bytes();
        let retry = RetryConfig::default();
        let mut last_err = String::new();

        for attempt in 0..=retry.max_retries {
            let mut req = match self.config.send_method.as_str() {
                "PUT" => self.client.put(send_url),
                _ => self.client.post(send_url),
            };

            req = req
                .header("Content-Type", "application/octet-stream")
                .body(wire_bytes.clone());

            if let Some(ref auth) = self.config.auth_header {
                req = req.header("Authorization", auth.as_str());
            }
            if let Some(ref secret) = self.config.hmac_secret {
                let sig = Self::compute_hmac(secret.as_bytes(), &wire_bytes);
                req = req.header("X-DOT-Signature", format!("sha256={sig}"));
            }

            match req.send().await {
                Ok(resp) if resp.status().is_success() => {
                    return Ok(DeliveryReceipt {
                        platform_message_id: format!("wh-{}", epoch_millis()),
                        delivered_at: epoch_millis(),
                    });
                }
                Ok(resp) => {
                    last_err = format!("HTTP {}", resp.status());
                    if resp.status().as_u16() == 429 && retry.should_retry(attempt) {
                        tokio::time::sleep(retry.delay_for_attempt(attempt)).await;
                        continue;
                    }
                    return Err(transport_err(last_err));
                }
                Err(e) => {
                    last_err = e.to_string();
                    if retry.should_retry(attempt) {
                        tokio::time::sleep(retry.delay_for_attempt(attempt)).await;
                        continue;
                    }
                    return Err(transport_err(last_err));
                }
            }
        }
        Err(transport_err(format!("Retries exhausted: {last_err}")))
    }

    async fn receive_messages(
        &self,
        _domain: &BroadcastDomainId,
    ) -> Result<Vec<RawPlatformMessage>, PlatformAdapterError> {
        self.ensure_server_started().await?;
        let mut rx = self.rx.lock().await;
        let mut messages = Vec::new();
        while let Ok(msg) = rx.try_recv() {
            messages.push(msg);
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
            media_capabilities: None,

        ..Default::default()

        }
    }

    fn domain_id(&self, platform_id: &str) -> BroadcastDomainId {
        BroadcastDomainId::new(PlatformType::Webhook, platform_id)
    }

    fn platform_type(&self) -> PlatformType {
        PlatformType::Webhook
    }

    async fn shutdown(&self) -> Result<(), PlatformAdapterError> {
        Ok(())
    }
    async fn health_check(&self) -> Result<(), PlatformAdapterError> {
        if let Some(ref url) = self.config.send_url {
            let timeout = std::time::Duration::from_secs(5);
            match tokio::time::timeout(timeout, self.client.head(url).send()).await {
                Ok(Ok(_)) => Ok(()),
                Ok(Err(e)) => Err(transport_err(format!("Health check: {e}"))),
                Err(_) => Err(transport_err("Health check timed out")),
            }
        } else {
            Ok(()) // No send URL — nothing to check
        }
    }
}

// ── Plugin ABI ─────────────────────────────────────────────────────

#[no_mangle]
pub extern "C" fn adapter_version() -> u32 {
    1
}

#[no_mangle]
pub extern "C" fn platform_type() -> u16 {
    0x0009
}

#[no_mangle]
/// # Safety
/// `config` must point to a valid buffer of at least `len` bytes.
pub unsafe extern "C" fn create_adapter(config: *const u8, config_len: usize) -> *mut () {
    if config.is_null() || config_len == 0 {
        return std::ptr::null_mut();
    }
    let bytes = std::slice::from_raw_parts(config, config_len);
    match WebhookAdapter::from_config_bytes(bytes) {
        Ok(a) => Box::into_raw(Box::new(a)) as *mut (),
        Err(_) => std::ptr::null_mut(),
    }
}

#[no_mangle]
/// # Safety
/// `ptr` must be a pointer previously returned by `create_adapter`.
pub unsafe extern "C" fn destroy_adapter(adapter: *mut ()) {
    if !adapter.is_null() {
        let _ = Box::from_raw(adapter as *mut WebhookAdapter);
    }
}

// ── Tests ──────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_domain_hash_deterministic() {
        let h1 = WebhookAdapter::domain_hash("https://example.com/dot");
        let h2 = WebhookAdapter::domain_hash("https://example.com/dot");
        assert_eq!(h1, h2);
    }

    #[test]
    fn test_domain_hash_normalized() {
        assert_eq!(
            WebhookAdapter::domain_hash("https://Example.COM/dot"),
            WebhookAdapter::domain_hash("  https://example.com/dot  ")
        );
    }

    #[test]
    fn test_platform_type() {
        assert_eq!(WebhookAdapter::PLATFORM_TYPE, 0x0009);
    }

    #[test]
    fn test_abi_exports() {
        assert_eq!(adapter_version(), 1);
        assert_eq!(platform_type(), 0x0009);
    }

    #[test]
    fn test_config_from_json() {
        let json = serde_json::json!({
            "send_url": "https://example.com/dot",
            "listen_port": 9090,
            "send_method": "PUT",
            "hmac_secret": "test-secret"
        });
        let a = WebhookAdapter::from_config_bytes(serde_json::to_vec(&json).unwrap().as_slice())
            .unwrap();
        assert_eq!(a.config.send_url, Some("https://example.com/dot".into()));
        assert_eq!(a.config.listen_port, 9090);
        assert_eq!(a.config.send_method, "PUT");
        assert_eq!(a.config.hmac_secret, Some("test-secret".into()));
    }

    #[test]
    fn test_config_defaults() {
        let a = WebhookAdapter::from_config_bytes(b"{}").unwrap();
        assert_eq!(a.config.send_url, None);
        assert_eq!(a.config.listen_port, 8443);
        assert_eq!(a.config.send_method, "POST");
        assert_eq!(a.config.listen_path, "/dot/v1/envelope");
    }

    #[test]
    fn test_hmac_verify_correct() {
        let sig = WebhookAdapter::compute_hmac(b"secret", b"hello");
        assert!(WebhookAdapter::verify_hmac(b"secret", b"hello", &sig));
    }

    #[test]
    fn test_hmac_verify_wrong_secret() {
        let sig = WebhookAdapter::compute_hmac(b"secret", b"hello");
        assert!(!WebhookAdapter::verify_hmac(b"wrong", b"hello", &sig));
    }

    #[test]
    fn test_hmac_verify_wrong_message() {
        let sig = WebhookAdapter::compute_hmac(b"secret", b"hello");
        assert!(!WebhookAdapter::verify_hmac(b"secret", b"world", &sig));
    }

    #[test]
    fn test_hmac_verify_invalid_hex() {
        assert!(!WebhookAdapter::verify_hmac(b"secret", b"msg", "zzzz"));
    }

    #[test]
    fn test_hex_roundtrip() {
        let data = vec![0u8, 1, 127, 128, 255];
        assert_eq!(hex_decode(&hex_encode(&data)).unwrap(), data);
    }

    #[test]
    fn test_hex_decode_odd_length() {
        assert!(hex_decode("abc").is_err());
    }

    #[test]
    fn test_capabilities() {
        let a = WebhookAdapter::new(WebhookConfig {
            send_url: None,
            send_method: "POST".into(),
            listen_port: 8443,
            listen_path: "/dot/v1/envelope".into(),
            auth_header: None,
            hmac_secret: None,
        });
        let c = a.capabilities();
        assert_eq!(c.max_payload_bytes, 1_048_576);
        assert!(!c.supports_fragmentation);
        assert_eq!(c.rate_limit_per_second, 100);
    }

    #[test]
    fn test_domain_id() {
        let a = WebhookAdapter::new(WebhookConfig {
            send_url: None,
            send_method: "POST".into(),
            listen_port: 8443,
            listen_path: "/dot/v1/envelope".into(),
            auth_header: None,
            hmac_secret: None,
        });
        let d = a.domain_id("https://example.com");
        assert_eq!(d.platform_type, PlatformType::Webhook as u16);
    }
}
