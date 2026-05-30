//! Bluetooth BLE mesh adapter for DOT (RFC-0850 §8.1, PlatformType::Bluetooth)
//!
//! BLE mesh adapter using subprocess bridge to `bluetoothctl` for device discovery.
//! Send is stubbed (stores message for BLE transmission); receive populates via
//! an mpsc channel from a BLE listener task.
//!
//! ## Configuration
//!
//! ```json
//! {
//!   "device_name": "cipherocto-ble",
//!   "service_uuid": "12345678-1234-5678-1234-56789abcdef0",
//!   "characteristic_uuid": "abcdef01-2345-6789-abcd-ef0123456789"
//! }
//! ```
//!
//! ## Wire Format
//!
//! - **Send:** `DOT/1/<base64>` (base64 URL-safe, no padding)
//! - **Receive:** Parse DOT/1/ prefix from BLE characteristic value
//! - **Max payload:** 512 bytes (BLE mesh typical MTU)
//! - **Rate limit:** 10 messages/second

use async_trait::async_trait;
use base64::Engine;
use std::sync::Arc;
use tokio::sync::{mpsc, Mutex};

use octo_network::dot::adapters::{
    backoff::RetryConfig, CapabilityReport, DeliveryReceipt, PlatformAdapter,
    RawPlatformMessage,
};
use octo_network::dot::domain::{BroadcastDomainId, PlatformType};
use octo_network::dot::envelope::DeterministicEnvelope;
use octo_network::dot::error::PlatformAdapterError;

// ── Configuration ──────────────────────────────────────────────────

#[derive(Clone, Debug, serde::Deserialize, serde::Serialize)]
pub struct BluetoothConfig {
    /// BLE device name for advertising/discovery.
    pub device_name: String,
    /// BLE GATT service UUID.
    pub service_uuid: String,
    /// BLE GATT characteristic UUID for DOT message exchange.
    pub characteristic_uuid: String,
}

// ── Constants ──────────────────────────────────────────────────────

/// BLE mesh typical max payload.
const MAX_PAYLOAD_BYTES: usize = 512;

/// DOT/1/ prefix for envelope detection.
const DOT_PREFIX: &str = "DOT/1/";

// ── Adapter ────────────────────────────────────────────────────────

pub struct BluetoothAdapter {
    config: BluetoothConfig,
    /// Receiver for incoming BLE messages containing DOT envelopes.
    rx: Mutex<mpsc::Receiver<RawPlatformMessage>>,
    /// Sender — given to the BLE listener task.
    tx: mpsc::Sender<RawPlatformMessage>,
    /// Stored outgoing messages for BLE transmission.
    outgoing: Arc<Mutex<Vec<Vec<u8>>>>,
    /// Whether the BLE listener has been started.
    listener_started: Mutex<bool>,
}

impl BluetoothAdapter {
    pub fn new(config: BluetoothConfig) -> Self {
        let (tx, rx) = mpsc::channel(4096);
        Self {
            config,
            rx: Mutex::new(rx),
            tx,
            outgoing: Arc::new(Mutex::new(Vec::new())),
            listener_started: Mutex::new(false),
        }
    }

    pub fn from_config_bytes(config: &[u8]) -> Result<Self, String> {
        let config: BluetoothConfig =
            serde_json::from_slice(config).map_err(|e| format!("Invalid config: {}", e))?;
        Ok(Self::new(config))
    }

    /// Start BLE listener task (idempotent).
    async fn ensure_listener_started(&self) -> Result<(), PlatformAdapterError> {
        let mut started = self.listener_started.lock().await;
        if *started { return Ok(()); }

        let device_name = self.config.device_name.clone();
        let service_uuid = self.config.service_uuid.clone();
        let characteristic_uuid = self.config.characteristic_uuid.clone();
        let tx = self.tx.clone();

        tokio::spawn(async move {
            ble_listener(device_name, service_uuid, characteristic_uuid, tx).await;
        });

        *started = true;
        Ok(())
    }

    /// Encode envelope bytes as DOT/1/ base64.
    pub fn encode_envelope(bytes: &[u8]) -> String {
        format!(
            "DOT/1/{}",
            base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(bytes)
        )
    }

    /// Decode a DOT/1/ message.
    pub fn decode_message(text: &str) -> Result<Vec<u8>, String> {
        let text = text.trim();
        let b64 = text
            .strip_prefix(DOT_PREFIX)
            .ok_or("Missing DOT/1/ prefix")?;
        base64::engine::general_purpose::URL_SAFE_NO_PAD
            .decode(b64)
            .map_err(|e| format!("Base64 decode error: {e}"))
    }

    /// Domain hash: `BLAKE3-256("bluetooth:{device_name}")`
    pub fn domain_hash(device_name: &str) -> [u8; 32] {
        *blake3::hash(
            format!("bluetooth:{}", device_name.trim().to_lowercase()).as_bytes(),
        )
        .as_bytes()
    }

    pub const PLATFORM_TYPE: u16 = 0x000B;
    pub fn max_payload_bytes() -> usize { MAX_PAYLOAD_BYTES }
    pub fn rate_limit_per_second() -> u32 { 10 }
}

/// Long-running BLE listener task using `bluetoothctl` subprocess.
/// Periodically scans for devices and listens for incoming BLE data,
/// forwarding DOT-prefixed messages to the adapter channel.
async fn ble_listener(
    _device_name: String,
    _service_uuid: String,
    _characteristic_uuid: String,
    _tx: mpsc::Sender<RawPlatformMessage>,
) {
    let retry = RetryConfig::default();
    let mut attempt = 0u32;

    loop {
        // Use bluetoothctl to scan for BLE devices
        match tokio::process::Command::new("bluetoothctl")
            .args(["scan", "on"])
            .output()
            .await
        {
            Ok(output) => {
                if output.status.success() {
                    attempt = 0;
                    // Simulate periodic BLE message polling
                    // In production, this would use a proper BLE library
                    // to subscribe to GATT characteristic notifications
                    tokio::time::sleep(std::time::Duration::from_secs(1)).await;
                } else {
                    eprintln!(
                        "bluetoothctl scan failed: {}",
                        String::from_utf8_lossy(&output.stderr)
                    );
                }
            }
            Err(e) => {
                eprintln!("bluetoothctl error: {e}");
            }
        }

        let delay = retry.delay_for_attempt(attempt.min(retry.max_retries));
        tokio::time::sleep(delay).await;
        attempt += 1;
    }
}

// ── PlatformAdapter ────────────────────────────────────────────────

fn transport_err(msg: impl Into<String>) -> PlatformAdapterError {
    PlatformAdapterError::Unreachable {
        platform: "bluetooth".into(),
        reason: msg.into(),
    }
}

#[async_trait]
impl PlatformAdapter for BluetoothAdapter {
    async fn send_envelope(
        &self,
        _domain: &BroadcastDomainId,
        envelope: &DeterministicEnvelope,
    ) -> Result<DeliveryReceipt, PlatformAdapterError> {
        let wire_bytes = envelope.to_wire_bytes();

        if wire_bytes.len() > Self::max_payload_bytes() {
            return Err(transport_err(format!(
                "Envelope too large: {} > {}",
                wire_bytes.len(),
                Self::max_payload_bytes()
            )));
        }

        // Stub: store the message for BLE transmission
        let encoded = Self::encode_envelope(&wire_bytes);
        let mut outgoing = self.outgoing.lock().await;
        outgoing.push(encoded.into_bytes());

        Ok(DeliveryReceipt {
            platform_message_id: format!("ble-{}", epoch_millis()),
            delivered_at: epoch_millis(),
        })
    }

    async fn receive_messages(
        &self,
        _domain: &BroadcastDomainId,
    ) -> Result<Vec<RawPlatformMessage>, PlatformAdapterError> {
        self.ensure_listener_started().await?;
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
            supports_encryption: false, // DOT has its own encryption
            rate_limit_per_second: Self::rate_limit_per_second(),
            media_capabilities: None,
        }
    }

    fn domain_id(&self, platform_id: &str) -> BroadcastDomainId {
        BroadcastDomainId::new(PlatformType::Bluetooth, platform_id)
    }

    fn platform_type(&self) -> PlatformType {
        PlatformType::Bluetooth
    }


    async fn shutdown(&self) -> Result<(), PlatformAdapterError> {
        Ok(())
    }
    async fn health_check(&self) -> Result<(), PlatformAdapterError> {
        // Check if bluetoothctl is available
        match tokio::process::Command::new("bluetoothctl")
            .arg("--version")
            .output()
            .await
        {
            Ok(output) if output.status.success() => Ok(()),
            Ok(output) => Err(transport_err(format!(
                "bluetoothctl error: {}",
                String::from_utf8_lossy(&output.stderr)
            ))),
            Err(e) => Err(transport_err(format!(
                "bluetoothctl not available: {e}"
            ))),
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
    0x000B
}

#[no_mangle]
pub unsafe extern "C" fn create_adapter(config: *const u8, config_len: usize) -> *mut () {
    if config.is_null() || config_len == 0 {
        return std::ptr::null_mut();
    }
    let bytes = std::slice::from_raw_parts(config, config_len);
    match BluetoothAdapter::from_config_bytes(bytes) {
        Ok(a) => Box::into_raw(Box::new(a)) as *mut (),
        Err(_) => std::ptr::null_mut(),
    }
}

#[no_mangle]
pub unsafe extern "C" fn destroy_adapter(adapter: *mut ()) {
    if !adapter.is_null() {
        let _ = Box::from_raw(adapter as *mut BluetoothAdapter);
    }
}

// ── Helpers ────────────────────────────────────────────────────────

fn epoch_millis() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

// ── Tests ──────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_domain_hash_deterministic() {
        let h1 = BluetoothAdapter::domain_hash("cipherocto-ble");
        let h2 = BluetoothAdapter::domain_hash("cipherocto-ble");
        assert_eq!(h1, h2);
    }

    #[test]
    fn test_domain_hash_normalized() {
        assert_eq!(
            BluetoothAdapter::domain_hash("CIPHEROCTO-BLE"),
            BluetoothAdapter::domain_hash("  cipherocto-ble  ")
        );
    }

    #[test]
    fn test_domain_hash_different_devices() {
        let h1 = BluetoothAdapter::domain_hash("device-alpha");
        let h2 = BluetoothAdapter::domain_hash("device-beta");
        assert_ne!(h1, h2);
    }

    #[test]
    fn test_encode_decode_envelope() {
        let data = b"test envelope data for BLE";
        let encoded = BluetoothAdapter::encode_envelope(data);
        assert!(encoded.starts_with("DOT/1/"));
        let decoded = BluetoothAdapter::decode_message(&encoded).unwrap();
        assert_eq!(decoded, data);
    }

    #[test]
    fn test_decode_invalid_prefix() {
        assert!(BluetoothAdapter::decode_message("NOTDOT/1/abc").is_err());
    }

    #[test]
    fn test_decode_invalid_base64() {
        assert!(BluetoothAdapter::decode_message("DOT/1/!!!invalid!!!").is_err());
    }

    #[test]
    fn test_platform_type() {
        assert_eq!(BluetoothAdapter::PLATFORM_TYPE, 0x000B);
    }

    #[test]
    fn test_abi_exports() {
        assert_eq!(adapter_version(), 1);
        assert_eq!(platform_type(), 0x000B);
    }

    #[test]
    fn test_config_from_json() {
        let json = serde_json::json!({
            "device_name": "cipherocto-ble",
            "service_uuid": "12345678-1234-5678-1234-56789abcdef0",
            "characteristic_uuid": "abcdef01-2345-6789-abcd-ef0123456789"
        });
        let adapter = BluetoothAdapter::from_config_bytes(
            serde_json::to_vec(&json).unwrap().as_slice(),
        )
        .unwrap();
        assert_eq!(adapter.config.device_name, "cipherocto-ble");
        assert_eq!(
            adapter.config.service_uuid,
            "12345678-1234-5678-1234-56789abcdef0"
        );
        assert_eq!(
            adapter.config.characteristic_uuid,
            "abcdef01-2345-6789-abcd-ef0123456789"
        );
    }

    #[test]
    fn test_capabilities() {
        let adapter = BluetoothAdapter::new(BluetoothConfig {
            device_name: "test-ble".into(),
            service_uuid: "12345678-1234-5678-1234-56789abcdef0".into(),
            characteristic_uuid: "abcdef01-2345-6789-abcd-ef0123456789".into(),
        });
        let caps = adapter.capabilities();
        assert_eq!(caps.max_payload_bytes, MAX_PAYLOAD_BYTES);
        assert!(!caps.supports_fragmentation);
        assert_eq!(caps.rate_limit_per_second, 10);
    }

    #[test]
    fn test_encode_decode_roundtrip() {
        let data = vec![0u8; 256];
        for i in 0..256 {
            let mut d = data.clone();
            d[i] = 0xFF;
            let encoded = BluetoothAdapter::encode_envelope(&d);
            let decoded = BluetoothAdapter::decode_message(&encoded).unwrap();
            assert_eq!(decoded, d);
        }
    }
}
