//! Signal CLI bridge adapter for DOT (RFC-0850 §8.1, PlatformType::Signal)
//!
//! Bridges DOT envelopes to Signal groups via the `signal-cli` daemon.
//! Uses `tokio::process::Command` to invoke `signal-cli` for send/receive.
//!
//! ## Configuration
//!
//! ```json
//! {
//!   "signal_cli_path": "signal-cli",
//!   "phone_number": "+15551234567",
//!   "groups": ["group.abc123..."]
//! }
//! ```

use async_trait::async_trait;
use base64::Engine;
use serde::Deserialize;
use tokio::sync::Mutex;

use octo_network::dot::adapters::{
    backoff::RetryConfig, CapabilityReport, DeliveryReceipt, PlatformAdapter, RawPlatformMessage,
};
use octo_network::dot::domain::{BroadcastDomainId, PlatformType};
use octo_network::dot::envelope::DeterministicEnvelope;
use octo_network::dot::error::PlatformAdapterError;

// ── Configuration ──────────────────────────────────────────────────

#[derive(Clone, Deserialize, serde::Serialize)]
pub struct SignalConfig {
    /// Path to signal-cli binary
    #[serde(default = "default_signal_cli_path")]
    pub signal_cli_path: String,
    /// Phone number registered with Signal
    pub phone_number: String,
    /// Signal group IDs to monitor
    pub groups: Vec<String>,
}

fn default_signal_cli_path() -> String {
    "signal-cli".into()
}

impl std::fmt::Debug for SignalConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SignalConfig")
            .field("signal_cli_path", &self.signal_cli_path)
            .field("phone_number", &self.phone_number)
            .field("groups", &self.groups)
            .finish()
    }
}

// ── Adapter ────────────────────────────────────────────────────────

pub struct SignalAdapter {
    config: SignalConfig,
    /// Last processed message offset for dedup (used by gateway)
    #[allow(dead_code)]
    last_offset: Mutex<u64>,
}

impl SignalAdapter {
    pub fn new(config: SignalConfig) -> Self {
        Self {
            config,
            last_offset: Mutex::new(0),
        }
    }

    pub fn from_config_bytes(config: &[u8]) -> Result<Self, String> {
        let config: SignalConfig =
            serde_json::from_slice(config).map_err(|e| format!("Invalid config: {}", e))?;
        Ok(Self::new(config))
    }

    /// Domain hash: `BLAKE3-256("signal:{group_id}")`
    pub fn domain_hash(group_id: &str) -> [u8; 32] {
        let normalized = group_id.trim().to_lowercase();
        *blake3::hash(format!("signal:{}", normalized).as_bytes()).as_bytes()
    }

    pub const PLATFORM_TYPE: u16 = 0x0005;
    pub fn max_payload_bytes() -> usize {
        65_536
    }
    pub fn rate_limit_per_second() -> u32 {
        5
    }

    /// Encode an envelope as base64 with DOT/1/ prefix.
    pub fn encode_envelope(envelope_bytes: &[u8]) -> String {
        format!(
            "DOT/1/{}",
            base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(envelope_bytes)
        )
    }

    /// Decode a DOT/1/-prefixed base64 envelope.
    pub fn decode_envelope(text: &str) -> Result<Vec<u8>, String> {
        let text = text.trim();
        let b64 = text
            .strip_prefix("DOT/1/")
            .ok_or_else(|| "Missing DOT/1/ prefix".to_string())?;
        base64::engine::general_purpose::URL_SAFE_NO_PAD
            .decode(b64)
            .map_err(|e| format!("Base64 decode error: {}", e))
    }
}

// ── PlatformAdapter ────────────────────────────────────────────────

fn transport_err(msg: impl Into<String>) -> PlatformAdapterError {
    PlatformAdapterError::Unreachable {
        platform: "signal".into(),
        reason: msg.into(),
    }
}

#[async_trait]
impl PlatformAdapter for SignalAdapter {
    async fn send_message(
        &self,
        domain: &BroadcastDomainId,
        envelope: &DeterministicEnvelope,
        _payload: &[u8],
    ) -> Result<DeliveryReceipt, PlatformAdapterError> {
        let wire_bytes = envelope.to_wire_bytes();
        let encoded = Self::encode_envelope(&wire_bytes);

        let group_id = self
            .config
            .groups
            .iter()
            .find(|g| {
                let hash = Self::domain_hash(g);
                hash == domain.domain_hash
            })
            .ok_or_else(|| {
                transport_err(format!(
                    "No group found for domain {:?}",
                    domain.domain_hash
                ))
            })?;

        let retry = RetryConfig::default();
        let mut last_err = String::new();

        for attempt in 0..=retry.max_retries {
            let output = tokio::process::Command::new(&self.config.signal_cli_path)
                .args([
                    "-u",
                    &self.config.phone_number,
                    "send",
                    "-m",
                    &encoded,
                    group_id,
                ])
                .output()
                .await;

            match output {
                Ok(out) if out.status.success() => {
                    return Ok(DeliveryReceipt {
                        platform_message_id: format!("sig-{}", epoch_millis()),
                        delivered_at: epoch_millis(),
                    });
                }
                Ok(out) => {
                    last_err = String::from_utf8_lossy(&out.stderr).to_string();
                    if retry.should_retry(attempt) {
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
        let output = tokio::process::Command::new(&self.config.signal_cli_path)
            .args(["-u", &self.config.phone_number, "receive", "--json"])
            .output()
            .await
            .map_err(|e| transport_err(e.to_string()))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(transport_err(stderr.to_string()));
        }

        let stdout = String::from_utf8_lossy(&output.stdout);
        let mut messages = Vec::new();

        for line in stdout.lines() {
            let line = line.trim();
            if line.is_empty() {
                continue;
            }
            // Each line is a JSON envelope object
            if let Ok(val) = serde_json::from_str::<serde_json::Value>(line) {
                // Extract message text from Signal JSON format
                if let Some(text) = val
                    .get("envelope")
                    .and_then(|e| e.get("dataMessage"))
                    .and_then(|d| d.get("message"))
                    .and_then(|m| m.as_str())
                {
                    if let Ok(payload) = Self::decode_envelope(text) {
                        let group_id = val
                            .get("envelope")
                            .and_then(|e| e.get("dataMessage"))
                            .and_then(|d| d.get("groupInfo"))
                            .and_then(|g| g.get("groupId"))
                            .and_then(|g| g.as_str())
                            .unwrap_or("unknown");

                        let mut metadata = std::collections::BTreeMap::new();
                        metadata.insert("group_id".into(), group_id.to_string());

                        let msg_id = val
                            .get("envelope")
                            .and_then(|e| e.get("timestamp"))
                            .and_then(|t| t.as_u64())
                            .unwrap_or(0);

                        messages.push(RawPlatformMessage {
                            platform_id: msg_id.to_string(),
                            payload,
                            metadata,
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
        BroadcastDomainId::new(PlatformType::Signal, platform_id)
    }

    fn platform_type(&self) -> PlatformType {
        PlatformType::Signal
    }

    fn self_handle(&self) -> Option<String> {
        Some(self.config.phone_number.clone())
    }

    async fn shutdown(&self) -> Result<(), PlatformAdapterError> {
        Ok(())
    }

    async fn health_check(&self) -> Result<(), PlatformAdapterError> {
        let timeout = std::time::Duration::from_secs(5);
        match tokio::time::timeout(
            timeout,
            tokio::process::Command::new(&self.config.signal_cli_path)
                .args(["-u", &self.config.phone_number, "listGroups"])
                .output(),
        )
        .await
        {
            Ok(Ok(out)) if out.status.success() => Ok(()),
            Ok(Ok(out)) => {
                let stderr = String::from_utf8_lossy(&out.stderr);
                Err(transport_err(format!("Health check failed: {stderr}")))
            }
            Ok(Err(e)) => Err(transport_err(format!("Health check failed: {e}"))),
            Err(_) => Err(transport_err("Health check timed out")),
        }
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
    0x0005
}

#[no_mangle]
/// # Safety
/// `config` must point to a valid buffer of at least `len` bytes.
pub unsafe extern "C" fn create_adapter(config: *const u8, config_len: usize) -> *mut () {
    if config.is_null() || config_len == 0 {
        return std::ptr::null_mut();
    }
    let bytes = std::slice::from_raw_parts(config, config_len);
    match SignalAdapter::from_config_bytes(bytes) {
        Ok(a) => Box::into_raw(Box::new(a)) as *mut (),
        Err(_) => std::ptr::null_mut(),
    }
}

#[no_mangle]
/// # Safety
/// `ptr` must be a pointer previously returned by `create_adapter`.
pub unsafe extern "C" fn destroy_adapter(adapter: *mut ()) {
    if !adapter.is_null() {
        let _ = Box::from_raw(adapter as *mut SignalAdapter);
    }
}

// ── Tests ──────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_domain_hash_deterministic() {
        let h1 = SignalAdapter::domain_hash("group.abc123");
        let h2 = SignalAdapter::domain_hash("group.abc123");
        assert_eq!(h1, h2);
    }

    #[test]
    fn test_domain_hash_normalized() {
        assert_eq!(
            SignalAdapter::domain_hash("GROUP.ABC123"),
            SignalAdapter::domain_hash("  group.abc123  ")
        );
    }

    #[test]
    fn test_encode_decode_envelope() {
        let original = b"test signal envelope";
        let encoded = SignalAdapter::encode_envelope(original);
        assert!(encoded.starts_with("DOT/1/"));
        let decoded = SignalAdapter::decode_envelope(&encoded).unwrap();
        assert_eq!(decoded, original);
    }

    #[test]
    fn test_platform_type() {
        assert_eq!(SignalAdapter::PLATFORM_TYPE, 0x0005);
    }

    #[test]
    fn test_abi_exports() {
        assert_eq!(adapter_version(), 1);
        assert_eq!(platform_type(), 0x0005);
    }

    #[test]
    fn test_config_from_json() {
        let json = serde_json::json!({
            "signal_cli_path": "/usr/local/bin/signal-cli",
            "phone_number": "+15551234567",
            "groups": ["group.abc123"]
        });
        let a = SignalAdapter::from_config_bytes(serde_json::to_vec(&json).unwrap().as_slice())
            .unwrap();
        assert_eq!(a.config.signal_cli_path, "/usr/local/bin/signal-cli");
        assert_eq!(a.config.phone_number, "+15551234567");
        assert_eq!(a.config.groups.len(), 1);
    }

    #[test]
    fn test_config_defaults() {
        let json = serde_json::json!({
            "phone_number": "+15551234567",
            "groups": []
        });
        let a = SignalAdapter::from_config_bytes(serde_json::to_vec(&json).unwrap().as_slice())
            .unwrap();
        assert_eq!(a.config.signal_cli_path, "signal-cli");
    }

    #[test]
    fn test_capabilities() {
        let a = SignalAdapter::new(SignalConfig {
            signal_cli_path: "signal-cli".into(),
            phone_number: "+15551234567".into(),
            groups: vec![],
        });
        let c = a.capabilities();
        assert_eq!(c.max_payload_bytes, 65_536);
        assert!(!c.supports_fragmentation);
        assert_eq!(c.rate_limit_per_second, 5);
    }
}
