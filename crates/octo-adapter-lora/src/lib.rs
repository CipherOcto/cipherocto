//! LoRa long-range radio adapter for DOT (RFC-0850 §8.1, PlatformType::LoRa)
//!
//! LoRa radio adapter using serial port bridge. Envelopes are base64-encoded
//! and written to the serial port; incoming data is read and decoded.
//!
//! ## Configuration
//!
//! ```json
//! {
//!   "serial_port": "/dev/ttyUSB0",
//!   "baud_rate": 9600,
//!   "device_id": "lora-node-001"
//! }
//! ```
//!
//! ## Wire Format
//!
//! - **Send:** `DOT/1/<base64>\n` (base64 URL-safe, no padding, newline-delimited)
//! - **Receive:** Parse DOT/1/ prefix from serial line
//! - **Max payload:** 256 bytes (LoRa typical max)
//! - **Rate limit:** 1 message/second (LoRa duty cycle)

use async_trait::async_trait;
use base64::Engine;
use std::collections::BTreeMap;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::sync::{mpsc, Mutex};

use octo_network::dot::adapters::{
    backoff::RetryConfig, CapabilityReport, DeliveryReceipt, PlatformAdapter, RawPlatformMessage,
};
use octo_network::dot::domain::{BroadcastDomainId, PlatformType};
use octo_network::dot::envelope::DeterministicEnvelope;
use octo_network::dot::error::PlatformAdapterError;

// ── Configuration ──────────────────────────────────────────────────

#[derive(Clone, Debug, serde::Deserialize, serde::Serialize)]
pub struct LoraConfig {
    /// Serial port path (e.g., "/dev/ttyUSB0").
    pub serial_port: String,
    /// Baud rate for serial communication. Default: 9600.
    #[serde(default = "default_baud_rate")]
    pub baud_rate: u32,
    /// Device identifier for domain hashing.
    pub device_id: String,
}

fn default_baud_rate() -> u32 {
    9600
}

// ── Constants ──────────────────────────────────────────────────────

/// LoRa typical max payload.
const MAX_PAYLOAD_BYTES: usize = 256;

/// DOT/1/ prefix for envelope detection.
const DOT_PREFIX: &str = "DOT/1/";

// ── Adapter ────────────────────────────────────────────────────────

pub struct LoraAdapter {
    config: LoraConfig,
    /// Receiver for incoming serial messages containing DOT envelopes.
    rx: Mutex<mpsc::Receiver<RawPlatformMessage>>,
    /// Sender — given to the serial listener task.
    tx: mpsc::Sender<RawPlatformMessage>,
    /// Whether the serial listener has been started.
    listener_started: Mutex<bool>,
}

impl LoraAdapter {
    pub fn new(config: LoraConfig) -> Self {
        let (tx, rx) = mpsc::channel(4096);
        Self {
            config,
            rx: Mutex::new(rx),
            tx,
            listener_started: Mutex::new(false),
        }
    }

    pub fn from_config_bytes(config: &[u8]) -> Result<Self, String> {
        let config: LoraConfig =
            serde_json::from_slice(config).map_err(|e| format!("Invalid config: {}", e))?;
        Ok(Self::new(config))
    }

    /// Start serial listener task (idempotent).
    async fn ensure_listener_started(&self) -> Result<(), PlatformAdapterError> {
        let mut started = self.listener_started.lock().await;
        if *started {
            return Ok(());
        }

        let serial_port = self.config.serial_port.clone();
        let baud_rate = self.config.baud_rate;
        let tx = self.tx.clone();

        tokio::spawn(async move {
            serial_listener(serial_port, baud_rate, tx).await;
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

    /// Domain hash: `BLAKE3-256("lora:{device_id}")`
    pub fn domain_hash(device_id: &str) -> [u8; 32] {
        *blake3::hash(format!("lora:{}", device_id.trim().to_lowercase()).as_bytes()).as_bytes()
    }

    /// Split a payload into LoRa-safe chunks that fit within the max payload size.
    /// Each chunk is a valid byte slice suitable for individual LoRa transmissions.
    pub fn split_for_lora(data: &[u8], max_bytes: usize) -> Vec<Vec<u8>> {
        if max_bytes == 0 {
            return vec![data.to_vec()];
        }
        data.chunks(max_bytes).map(|c| c.to_vec()).collect()
    }

    pub const PLATFORM_TYPE: u16 = 0x000C;
    pub fn max_payload_bytes() -> usize {
        MAX_PAYLOAD_BYTES
    }
    pub fn rate_limit_per_second() -> u32 {
        1
    }
}

// ── Serial Listener ────────────────────────────────────────────────

/// Long-running serial listener task.
/// Reads from the serial port line-by-line and forwards DOT-prefixed
/// messages to the adapter channel.
async fn serial_listener(
    serial_port: String,
    _baud_rate: u32,
    tx: mpsc::Sender<RawPlatformMessage>,
) {
    let retry = RetryConfig::default();
    let mut attempt = 0u32;

    loop {
        match tokio::fs::File::open(&serial_port).await {
            Ok(mut file) => {
                attempt = 0;
                let mut buf = vec![0u8; 4096];
                let mut line_buf = Vec::new();

                loop {
                    match file.read(&mut buf).await {
                        Ok(0) => {
                            // EOF — wait and retry
                            tokio::time::sleep(std::time::Duration::from_millis(100)).await;
                        }
                        Ok(n) => {
                            line_buf.extend_from_slice(&buf[..n]);

                            // Process complete lines
                            while let Some(pos) = line_buf.iter().position(|&b| b == b'\n') {
                                let line_bytes = line_buf[..pos].to_vec();
                                line_buf.drain(..=pos);

                                if let Ok(line) = std::str::from_utf8(&line_bytes) {
                                    let trimmed = line.trim();
                                    if trimmed.starts_with(DOT_PREFIX) {
                                        if let Ok(payload) = LoraAdapter::decode_message(trimmed) {
                                            let mut metadata = BTreeMap::new();
                                            metadata
                                                .insert("serial_port".into(), serial_port.clone());
                                            let _ = tx.try_send(RawPlatformMessage {
                                                platform_id: format!("lora-{}", epoch_millis()),
                                                payload,
                                                metadata,
                                            });
                                        }
                                    }
                                }
                            }
                        }
                        Err(e) => {
                            eprintln!("Serial read error: {e}");
                            break;
                        }
                    }
                }
            }
            Err(e) => {
                eprintln!("Serial port open error ({serial_port}): {e}");
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
        platform: "lora".into(),
        reason: msg.into(),
    }
}

#[async_trait]
impl PlatformAdapter for LoraAdapter {
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

        // Encode as DOT/1/ base64 and write to serial port
        let encoded = Self::encode_envelope(&wire_bytes);
        let line = format!("{encoded}\n");

        match tokio::fs::OpenOptions::new()
            .write(true)
            .open(&self.config.serial_port)
            .await
        {
            Ok(mut file) => {
                file.write_all(line.as_bytes())
                    .await
                    .map_err(|e| transport_err(format!("Serial write error: {e}")))?;
                file.flush()
                    .await
                    .map_err(|e| transport_err(format!("Serial flush error: {e}")))?;
            }
            Err(e) => {
                return Err(transport_err(format!(
                    "Cannot open serial port {}: {e}",
                    self.config.serial_port
                )));
            }
        }

        Ok(DeliveryReceipt {
            platform_message_id: format!("lora-{}", epoch_millis()),
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
            supports_raw_binary: false,
            rate_limit_per_second: Self::rate_limit_per_second(),
            media_capabilities: None,
        }
    }

    fn domain_id(&self, platform_id: &str) -> BroadcastDomainId {
        BroadcastDomainId::new(PlatformType::LoRa, platform_id)
    }

    fn platform_type(&self) -> PlatformType {
        PlatformType::LoRa
    }

    async fn shutdown(&self) -> Result<(), PlatformAdapterError> {
        Ok(())
    }
    async fn health_check(&self) -> Result<(), PlatformAdapterError> {
        // Check if serial port exists
        match tokio::fs::metadata(&self.config.serial_port).await {
            Ok(_) => Ok(()),
            Err(e) => Err(transport_err(format!(
                "Serial port {} not available: {e}",
                self.config.serial_port
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
    0x000C
}

#[no_mangle]
/// # Safety
/// `config` must point to a valid buffer of at least `len` bytes.
pub unsafe extern "C" fn create_adapter(config: *const u8, config_len: usize) -> *mut () {
    if config.is_null() || config_len == 0 {
        return std::ptr::null_mut();
    }
    let bytes = std::slice::from_raw_parts(config, config_len);
    match LoraAdapter::from_config_bytes(bytes) {
        Ok(a) => Box::into_raw(Box::new(a)) as *mut (),
        Err(_) => std::ptr::null_mut(),
    }
}

#[no_mangle]
/// # Safety
/// `ptr` must be a pointer previously returned by `create_adapter`.
pub unsafe extern "C" fn destroy_adapter(adapter: *mut ()) {
    if !adapter.is_null() {
        let _ = Box::from_raw(adapter as *mut LoraAdapter);
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
        let h1 = LoraAdapter::domain_hash("lora-node-001");
        let h2 = LoraAdapter::domain_hash("lora-node-001");
        assert_eq!(h1, h2);
    }

    #[test]
    fn test_domain_hash_normalized() {
        assert_eq!(
            LoraAdapter::domain_hash("LORA-NODE-001"),
            LoraAdapter::domain_hash("  lora-node-001  ")
        );
    }

    #[test]
    fn test_domain_hash_different_devices() {
        let h1 = LoraAdapter::domain_hash("node-alpha");
        let h2 = LoraAdapter::domain_hash("node-beta");
        assert_ne!(h1, h2);
    }

    #[test]
    fn test_encode_decode_envelope() {
        let data = b"test envelope data for LoRa";
        let encoded = LoraAdapter::encode_envelope(data);
        assert!(encoded.starts_with("DOT/1/"));
        let decoded = LoraAdapter::decode_message(&encoded).unwrap();
        assert_eq!(decoded, data);
    }

    #[test]
    fn test_decode_invalid_prefix() {
        assert!(LoraAdapter::decode_message("NOTDOT/1/abc").is_err());
    }

    #[test]
    fn test_decode_invalid_base64() {
        assert!(LoraAdapter::decode_message("DOT/1/!!!invalid!!!").is_err());
    }

    #[test]
    fn test_platform_type() {
        assert_eq!(LoraAdapter::PLATFORM_TYPE, 0x000C);
    }

    #[test]
    fn test_abi_exports() {
        assert_eq!(adapter_version(), 1);
        assert_eq!(platform_type(), 0x000C);
    }

    #[test]
    fn test_config_from_json() {
        let json = serde_json::json!({
            "serial_port": "/dev/ttyUSB0",
            "baud_rate": 115200,
            "device_id": "lora-node-001"
        });
        let adapter =
            LoraAdapter::from_config_bytes(serde_json::to_vec(&json).unwrap().as_slice()).unwrap();
        assert_eq!(adapter.config.serial_port, "/dev/ttyUSB0");
        assert_eq!(adapter.config.baud_rate, 115200);
        assert_eq!(adapter.config.device_id, "lora-node-001");
    }

    #[test]
    fn test_config_default_baud_rate() {
        let json = serde_json::json!({
            "serial_port": "/dev/ttyUSB0",
            "device_id": "lora-node-001"
        });
        let adapter =
            LoraAdapter::from_config_bytes(serde_json::to_vec(&json).unwrap().as_slice()).unwrap();
        assert_eq!(adapter.config.baud_rate, 9600);
    }

    #[test]
    fn test_capabilities() {
        let adapter = LoraAdapter::new(LoraConfig {
            serial_port: "/dev/ttyUSB0".into(),
            baud_rate: 9600,
            device_id: "test-lora".into(),
        });
        let caps = adapter.capabilities();
        assert_eq!(caps.max_payload_bytes, MAX_PAYLOAD_BYTES);
        assert!(!caps.supports_fragmentation);
        assert_eq!(caps.rate_limit_per_second, 1);
    }

    #[test]
    fn test_split_for_lora_small() {
        let data = b"hello";
        let chunks = LoraAdapter::split_for_lora(data, 256);
        assert_eq!(chunks.len(), 1);
        assert_eq!(chunks[0], data);
    }

    #[test]
    fn test_split_for_lora_large() {
        let data = vec![0xABu8; 700];
        let chunks = LoraAdapter::split_for_lora(&data, 256);
        assert_eq!(chunks.len(), 3);
        assert_eq!(chunks[0].len(), 256);
        assert_eq!(chunks[1].len(), 256);
        assert_eq!(chunks[2].len(), 188);
        // Reassembled matches original
        let reassembled: Vec<u8> = chunks.into_iter().flatten().collect();
        assert_eq!(reassembled, data);
    }

    #[test]
    fn test_split_for_lora_empty() {
        let data = b"";
        let chunks = LoraAdapter::split_for_lora(data, 256);
        assert_eq!(chunks.len(), 0);
    }

    #[test]
    fn test_split_for_lora_exact_boundary() {
        let data = vec![0x42u8; 256];
        let chunks = LoraAdapter::split_for_lora(&data, 256);
        assert_eq!(chunks.len(), 1);
        assert_eq!(chunks[0], data);
    }

    #[test]
    fn test_encode_decode_roundtrip() {
        let data = vec![0u8; 200];
        for i in 0..200 {
            let mut d = data.clone();
            d[i] = 0xFF;
            let encoded = LoraAdapter::encode_envelope(&d);
            let decoded = LoraAdapter::decode_message(&encoded).unwrap();
            assert_eq!(decoded, d);
        }
    }
}
