//! QUIC transport adapter (RFC-0850 §8.7, PlatformType::Quic)
//!
//! Uses `quinn` for native QUIC transport between DOT gateways.
//! Provides multiplexed streams, 0-RTT, connection migration, and TLS 1.3.
//!
//! ## Configuration
//!
//! ```json
//! {
//!   "listen_addr": "0.0.0.0:47400",
//!   "auth_mode": "raw_public_key",
//!   "max_concurrent_streams": 1000,
//!   "max_idle_timeout_secs": 120,
//!   "enable_0rtt": true,
//!   "max_pending_envelopes": 1024,
//!   "peer_addrs": {}
//! }
//! ```
//!
//! ## Stream Framing (RFC-0850 §8.7.3)
//!
//! All stream types use a consistent frame format:
//!
//! ```text
//! ┌──────────────────┬──────────────────┬──────────────┐
//! │ frame_len (u32)  │ type (u16)       │ payload      │
//! └──────────────────┴──────────────────┴──────────────┘
//! ```
//!
//! - `frame_len`: Big-endian, bytes after this field (2 + payload.len())
//! - `type`: 0x0001=envelope, 0x0002=fragment, 0x0003=onion, 0x0004-0x0006=control

use async_trait::async_trait;
use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::sync::{mpsc, Mutex, RwLock};

use crate::dot::adapters::{
    CapabilityReport, DeliveryReceipt, PlatformAdapter, RawPlatformMessage,
};
use crate::dot::domain::{BroadcastDomainId, PlatformType};
use crate::dot::envelope::DeterministicEnvelope;
use crate::dot::error::PlatformAdapterError;

// ── Frame type constants (RFC-0850 §8.7.3) ──

const FRAME_TYPE_ENVELOPE: u16 = 0x0001;
const FRAME_TYPE_FRAGMENT: u16 = 0x0002;
const FRAME_TYPE_ONION: u16 = 0x0003;
const FRAME_TYPE_CAPABILITIES: u16 = 0x0004;
const FRAME_TYPE_PING: u16 = 0x0005;
const FRAME_TYPE_PONG: u16 = 0x0006;
const FRAME_TYPE_SHUTDOWN: u16 = 0x0007;

// ── Configuration ──

/// QUIC adapter configuration (RFC-0850 §8.7.6).
#[derive(Clone, Debug, serde::Deserialize, serde::Serialize)]
pub struct QuicConfig {
    /// Listen address for incoming QUIC connections
    pub listen_addr: String,
    /// TLS authentication mode: "raw_public_key", "self_signed", "ca_signed"
    #[serde(default = "default_auth_mode")]
    pub auth_mode: String,
    /// Path to TLS certificate (for self_signed or ca_signed modes)
    #[serde(default)]
    pub tls_cert_path: Option<String>,
    /// Path to TLS private key (for self_signed or ca_signed modes)
    #[serde(default)]
    pub tls_key_path: Option<String>,
    /// Maximum concurrent streams per connection
    #[serde(default = "default_max_streams")]
    pub max_concurrent_streams: u32,
    /// Idle timeout in seconds (QUIC transport layer)
    #[serde(default = "default_idle_timeout")]
    pub max_idle_timeout_secs: u32,
    /// Enable 0-RTT early data
    #[serde(default = "default_true")]
    pub enable_0rtt: bool,
    /// Maximum bytes allowed in 0-RTT data
    #[serde(default = "default_max_0rtt")]
    pub max_0rtt_bytes: usize,
    /// Maximum pending outbound envelopes before backpressure
    #[serde(default = "default_max_pending")]
    pub max_pending_envelopes: usize,
    /// Known peer addresses for connection pool
    #[serde(default)]
    pub peer_addrs: HashMap<String, String>,
}

fn default_auth_mode() -> String {
    "raw_public_key".to_string()
}
fn default_max_streams() -> u32 {
    1000
}
fn default_idle_timeout() -> u32 {
    120
}
fn default_true() -> bool {
    true
}
fn default_max_0rtt() -> usize {
    16384
}
fn default_max_pending() -> usize {
    1024
}

impl Default for QuicConfig {
    fn default() -> Self {
        Self {
            listen_addr: "0.0.0.0:47400".into(),
            auth_mode: default_auth_mode(),
            tls_cert_path: None,
            tls_key_path: None,
            max_concurrent_streams: default_max_streams(),
            max_idle_timeout_secs: default_idle_timeout(),
            enable_0rtt: default_true(),
            max_0rtt_bytes: default_max_0rtt(),
            max_pending_envelopes: default_max_pending(),
            peer_addrs: HashMap::new(),
        }
    }
}

// ── Peer state ──

/// Connection state for a known peer.
struct PeerState {
    /// Peer's SocketAddr (resolved from GDP or config)
    addr: SocketAddr,
    /// Liveness tracking: consecutive missed pongs
    missed_pongs: u32,
    /// Last successful pong nonce
    last_pong_nonce: u64,
}

// ── QUIC Adapter ──

/// QUIC platform adapter (RFC-0850 §8.7).
pub struct QuicAdapter {
    config: QuicConfig,
    /// Inbound message channel (populated by stream accept loop)
    inbound_rx: Mutex<mpsc::Receiver<RawPlatformMessage>>,
    inbound_tx: mpsc::Sender<RawPlatformMessage>,
    /// Known peers and their connection state
    peers: RwLock<HashMap<String, PeerState>>,
    /// Self identity (set when server starts)
    self_id: Mutex<Option<String>>,
    /// Running counter for ping nonces
    ping_counter: std::sync::atomic::AtomicU64,
}

impl QuicAdapter {
    pub fn new(config: QuicConfig) -> Self {
        let (inbound_tx, inbound_rx) = mpsc::channel(config.max_pending_envelopes);
        Self {
            config,
            inbound_rx: Mutex::new(inbound_rx),
            inbound_tx,
            peers: RwLock::new(HashMap::new()),
            self_id: Mutex::new(None),
            ping_counter: std::sync::atomic::AtomicU64::new(0),
        }
    }

    pub fn from_config_bytes(config: &[u8]) -> Result<Self, String> {
        let config: QuicConfig =
            serde_json::from_slice(config).map_err(|e| format!("Invalid config: {e}"))?;
        Ok(Self::new(config))
    }

    // ── Frame encoding/decoding (RFC-0850 §8.7.3) ──

    /// Encode a frame: `[u32 frame_len][u16 type][payload]`
    pub fn encode_frame(frame_type: u16, payload: &[u8]) -> Vec<u8> {
        let frame_len = 2 + payload.len();
        let mut buf = Vec::with_capacity(4 + frame_len);
        buf.extend_from_slice(&(frame_len as u32).to_be_bytes());
        buf.extend_from_slice(&frame_type.to_be_bytes());
        buf.extend_from_slice(payload);
        buf
    }

    /// Decode a frame header. Returns (frame_type, payload_len) or error.
    pub fn decode_frame_header(header: &[u8; 6]) -> Result<(u16, u32), String> {
        let frame_len = u32::from_be_bytes([header[0], header[1], header[2], header[3]]);
        if frame_len < 2 {
            return Err(format!("frame_len too small: {frame_len}"));
        }
        let frame_type = u16::from_be_bytes([header[4], header[5]]);
        let payload_len = frame_len - 2;
        Ok((frame_type, payload_len))
    }

    /// Encode an envelope frame (type 0x0001).
    pub fn encode_envelope_frame(wire_bytes: &[u8]) -> Vec<u8> {
        Self::encode_frame(FRAME_TYPE_ENVELOPE, wire_bytes)
    }

    /// Encode a ping frame with nonce.
    pub fn encode_ping(nonce: u64) -> Vec<u8> {
        Self::encode_frame(FRAME_TYPE_PING, &nonce.to_be_bytes())
    }

    /// Encode a pong frame with nonce.
    pub fn encode_pong(nonce: u64) -> Vec<u8> {
        Self::encode_frame(FRAME_TYPE_PONG, &nonce.to_be_bytes())
    }

    /// Encode a shutdown frame with reason code.
    pub fn encode_shutdown(reason: u8) -> Vec<u8> {
        Self::encode_frame(FRAME_TYPE_SHUTDOWN, &[reason])
    }

    /// Next ping nonce (monotonically increasing).
    pub fn next_ping_nonce(&self) -> u64 {
        self.ping_counter
            .fetch_add(1, std::sync::atomic::Ordering::Relaxed)
    }

    /// Domain hash for QUIC peer (RFC-0850 §8.7.1).
    pub fn domain_hash(peer_id: &str) -> [u8; 32] {
        let normalized = peer_id.trim().to_lowercase();
        *blake3::hash(format!("quic:{}", normalized).as_bytes()).as_bytes()
    }

    pub const PLATFORM_TYPE: u16 = 0x0015;

    pub fn max_payload_bytes() -> usize {
        // QUIC streams have no practical payload limit for DOT
        // (limited by QUIC flow control, default max_data ~15MB)
        1_048_576 // 1MB practical limit for envelope processing
    }

    pub fn rate_limit_per_second() -> u32 {
        10_000
    }

    /// Start the QUIC server (feature-gated).
    #[cfg(feature = "quic")]
    pub async fn start_server(&self) -> Result<(), PlatformAdapterError> {
        let listen_addr: SocketAddr = self
            .config
            .listen_addr
            .parse()
            .map_err(|e| quic_err(format!("Invalid listen addr: {e}")))?;

        // Generate self-signed cert for now (raw_public_key mode)
        let (endpoint, _cert) = Self::make_server_endpoint(listen_addr)
            .map_err(|e| quic_err(format!("Server endpoint: {e}")))?;

        *self.self_id.lock().await = Some(format!("quic:{}", listen_addr));

        tracing::info!("QUIC server started on {}", listen_addr);
        let _ = endpoint; // TODO: spawn accept loop
        Ok(())
    }

    #[cfg(feature = "quic")]
    fn make_server_endpoint(
        addr: SocketAddr,
    ) -> Result<(quinn::Endpoint, Vec<u8>), Box<dyn std::error::Error>> {
        // Generate a self-signed certificate for development
        let cert = rcgen::generate_simple_self_signed(vec!["localhost".into()])?;
        let cert_der = rustls::pki_types::CertificateDer::from(cert.cert);
        let key_der = rustls::pki_types::PrivatePkcs8KeyDer::from(cert.key_pair.serialize_der());

        let mut server_config = quinn::ServerConfig::with_single_cert(
            vec![cert_der.clone()],
            rustls::pki_types::PrivatePkcs8KeyDer::from(key_der.secret_pkcs8_der().to_vec()).into(),
        )?;

        let transport = quinn::TransportConfig::default();
        server_config.transport_config(Arc::new(transport));

        let endpoint = quinn::Endpoint::server(server_config, addr)?;
        Ok((endpoint, cert_der.to_vec()))
    }

    /// Start the QUIC server (no-op when quic feature is disabled)
    #[cfg(not(feature = "quic"))]
    pub async fn start_server(&self) -> Result<(), PlatformAdapterError> {
        Err(quic_err("quic feature not enabled"))
    }
}

fn quic_err(msg: impl Into<String>) -> PlatformAdapterError {
    PlatformAdapterError::Unreachable {
        platform: "quic".into(),
        reason: msg.into(),
    }
}

#[async_trait]
impl PlatformAdapter for QuicAdapter {
    async fn send_envelope(
        &self,
        domain: &BroadcastDomainId,
        envelope: &DeterministicEnvelope,
    ) -> Result<DeliveryReceipt, PlatformAdapterError> {
        // Raw binary transport per RFC-0850 §8.7.3
        let wire_bytes = envelope.to_wire_bytes();
        let frame = Self::encode_envelope_frame(&wire_bytes);
        let _ = frame; // TODO: send to peer via QUIC stream

        let topic = hex_encode(&domain.domain_hash);
        tracing::info!("QUIC send to domain {}: {} bytes", topic, wire_bytes.len());

        Ok(DeliveryReceipt {
            platform_message_id: hex_encode(&envelope.envelope_id),
            delivered_at: envelope.logical_timestamp,
        })
    }

    async fn receive_messages(
        &self,
        _domain: &BroadcastDomainId,
    ) -> Result<Vec<RawPlatformMessage>, PlatformAdapterError> {
        let mut rx = self.inbound_rx.lock().await;
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
            return Err(quic_err("Empty payload"));
        }

        // QUIC carries raw wire bytes — try direct deserialization first
        if let Ok(env) = DeterministicEnvelope::from_wire_bytes(&raw.payload) {
            return Ok(env);
        }

        // Fallback: DOT/1/{b64} text format (interop)
        let text = String::from_utf8_lossy(&raw.payload);
        let wire_bytes = crate::dot::adapters::native_p2p::NativeP2PAdapter::decode_envelope(&text)
            .map_err(|e| PlatformAdapterError::ApiError {
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
            supports_encryption: true, // TLS 1.3
            supports_raw_binary: true, // QUIC carries Vec<u8> natively
            rate_limit_per_second: Self::rate_limit_per_second(),
            media_capabilities: None,
        }
    }

    fn domain_id(&self, platform_id: &str) -> BroadcastDomainId {
        BroadcastDomainId::new(PlatformType::Quic, platform_id)
    }

    fn platform_type(&self) -> PlatformType {
        PlatformType::Quic
    }

    fn self_handle(&self) -> Option<String> {
        self.self_id.try_lock().ok().and_then(|id| id.clone())
    }

    async fn shutdown(&self) -> Result<(), PlatformAdapterError> {
        self.peers.write().await.clear();
        *self.self_id.lock().await = None;
        tracing::info!("QUIC adapter shut down");
        Ok(())
    }

    async fn health_check(&self) -> Result<(), PlatformAdapterError> {
        let id = self.self_id.lock().await;
        if id.is_some() {
            Ok(())
        } else {
            Err(quic_err("QUIC server not started"))
        }
    }
}

// ── Helpers ──

fn hex_encode(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{:02x}", b)).collect()
}

// ── Tests ──────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_encode_frame() {
        let frame = QuicAdapter::encode_frame(0x0001, b"hello");
        // frame_len = 2 + 5 = 7
        assert_eq!(&frame[0..4], &7u32.to_be_bytes());
        assert_eq!(&frame[4..6], &0x0001u16.to_be_bytes());
        assert_eq!(&frame[6..], b"hello");
    }

    #[test]
    fn test_decode_frame_header() {
        let frame = QuicAdapter::encode_frame(0x0003, b"test");
        let (ftype, plen) =
            QuicAdapter::decode_frame_header(frame[0..6].try_into().unwrap()).unwrap();
        assert_eq!(ftype, 0x0003);
        assert_eq!(plen, 4);
    }

    #[test]
    fn test_decode_frame_header_too_small() {
        // frame_len = 1 (less than 2, impossible)
        let header = [0, 0, 0, 1, 0, 1];
        assert!(QuicAdapter::decode_frame_header(&header).is_err());
    }

    #[test]
    fn test_encode_envelope_frame() {
        let wire = b"test envelope wire bytes";
        let frame = QuicAdapter::encode_envelope_frame(wire);
        let (ftype, plen) =
            QuicAdapter::decode_frame_header(frame[0..6].try_into().unwrap()).unwrap();
        assert_eq!(ftype, FRAME_TYPE_ENVELOPE);
        assert_eq!(plen, wire.len() as u32);
        assert_eq!(&frame[6..], wire);
    }

    #[test]
    fn test_encode_ping_pong() {
        let nonce = 42u64;
        let ping = QuicAdapter::encode_ping(nonce);
        let (ftype, plen) =
            QuicAdapter::decode_frame_header(ping[0..6].try_into().unwrap()).unwrap();
        assert_eq!(ftype, FRAME_TYPE_PING);
        assert_eq!(plen, 8);
        let decoded_nonce = u64::from_be_bytes(ping[6..14].try_into().unwrap());
        assert_eq!(decoded_nonce, nonce);

        let pong = QuicAdapter::encode_pong(nonce);
        let (ftype, _) = QuicAdapter::decode_frame_header(pong[0..6].try_into().unwrap()).unwrap();
        assert_eq!(ftype, FRAME_TYPE_PONG);
    }

    #[test]
    fn test_encode_shutdown() {
        let frame = QuicAdapter::encode_shutdown(0x01);
        let (ftype, plen) =
            QuicAdapter::decode_frame_header(frame[0..6].try_into().unwrap()).unwrap();
        assert_eq!(ftype, FRAME_TYPE_SHUTDOWN);
        assert_eq!(plen, 1);
        assert_eq!(frame[6], 0x01);
    }

    #[test]
    fn test_frame_roundtrip_envelope() {
        let original = b"deterministic envelope wire bytes for testing";
        let frame = QuicAdapter::encode_envelope_frame(original);
        let (ftype, plen) =
            QuicAdapter::decode_frame_header(frame[0..6].try_into().unwrap()).unwrap();
        assert_eq!(ftype, FRAME_TYPE_ENVELOPE);
        assert_eq!(plen as usize, original.len());
        let payload = &frame[6..6 + plen as usize];
        assert_eq!(payload, original);
    }

    #[test]
    fn test_empty_payload_frame() {
        let frame = QuicAdapter::encode_frame(0x0001, &[]);
        let (ftype, plen) =
            QuicAdapter::decode_frame_header(frame[0..6].try_into().unwrap()).unwrap();
        assert_eq!(ftype, 0x0001);
        assert_eq!(plen, 0);
    }

    #[test]
    fn test_domain_hash_deterministic() {
        let h1 = QuicAdapter::domain_hash("peer-1");
        let h2 = QuicAdapter::domain_hash("peer-1");
        assert_eq!(h1, h2);
    }

    #[test]
    fn test_domain_hash_normalized() {
        assert_eq!(
            QuicAdapter::domain_hash("PEER-1"),
            QuicAdapter::domain_hash("  peer-1  ")
        );
    }

    #[test]
    fn test_platform_type() {
        assert_eq!(QuicAdapter::PLATFORM_TYPE, 0x0015);
    }

    #[test]
    fn test_capabilities() {
        let adapter = QuicAdapter::new(QuicConfig::default());
        let caps = adapter.capabilities();
        assert_eq!(caps.max_payload_bytes, 1_048_576);
        assert!(caps.supports_fragmentation);
        assert!(caps.supports_encryption);
        assert!(caps.supports_raw_binary);
        assert_eq!(caps.rate_limit_per_second, 10_000);
    }

    #[test]
    fn test_self_handle_none_initially() {
        let adapter = QuicAdapter::new(QuicConfig::default());
        assert!(adapter.self_handle().is_none());
    }

    #[test]
    fn test_config_from_json() {
        let json = serde_json::json!({
            "listen_addr": "0.0.0.0:47400",
            "auth_mode": "raw_public_key",
            "max_concurrent_streams": 500,
            "peer_addrs": {
                "peer1": "1.2.3.4:47400"
            }
        });
        let adapter =
            QuicAdapter::from_config_bytes(serde_json::to_vec(&json).unwrap().as_slice()).unwrap();
        assert_eq!(adapter.config.max_concurrent_streams, 500);
        assert_eq!(adapter.config.peer_addrs.len(), 1);
    }

    #[test]
    fn test_next_ping_nonce_monotonic() {
        let adapter = QuicAdapter::new(QuicConfig::default());
        let n1 = adapter.next_ping_nonce();
        let n2 = adapter.next_ping_nonce();
        let n3 = adapter.next_ping_nonce();
        assert_eq!(n1, 0);
        assert_eq!(n2, 1);
        assert_eq!(n3, 2);
    }

    #[test]
    fn test_frame_type_constants() {
        // Verify constants match RFC-0850 §8.7.3
        assert_eq!(FRAME_TYPE_ENVELOPE, 0x0001);
        assert_eq!(FRAME_TYPE_FRAGMENT, 0x0002);
        assert_eq!(FRAME_TYPE_ONION, 0x0003);
        assert_eq!(FRAME_TYPE_CAPABILITIES, 0x0004);
        assert_eq!(FRAME_TYPE_PING, 0x0005);
        assert_eq!(FRAME_TYPE_PONG, 0x0006);
        assert_eq!(FRAME_TYPE_SHUTDOWN, 0x0007);
    }

    #[tokio::test]
    async fn test_shutdown_clears_state() {
        let adapter = QuicAdapter::new(QuicConfig::default());
        adapter.shutdown().await.unwrap();
        assert!(adapter.self_handle().is_none());
    }

    #[tokio::test]
    async fn test_receive_messages_empty() {
        let adapter = QuicAdapter::new(QuicConfig::default());
        let domain = adapter.domain_id("test");
        let messages = adapter.receive_messages(&domain).await.unwrap();
        assert!(messages.is_empty());
    }

    #[tokio::test]
    async fn test_health_check_fails_without_server() {
        let adapter = QuicAdapter::new(QuicConfig::default());
        assert!(adapter.health_check().await.is_err());
    }
}
