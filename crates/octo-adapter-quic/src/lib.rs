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

use octo_network::dot::adapters::{
    CapabilityReport, DeliveryReceipt, PlatformAdapter, RawPlatformMessage,
};
use octo_network::dot::domain::{BroadcastDomainId, PlatformType};
use octo_network::dot::envelope::DeterministicEnvelope;
use octo_network::dot::error::PlatformAdapterError;
use octo_network::dot::replay::ReplayCache;
use octo_network::gdp::discovery::{BootstrapMethod, DiscoveryState};
use octo_network::gdp::types::DiscoveryScope;

// ── Frame type constants (RFC-0850 §8.7.3) ──

const FRAME_TYPE_ENVELOPE: u16 = 0x0001;
const FRAME_TYPE_FRAGMENT: u16 = 0x0002;
#[allow(dead_code)] // Reserved for future onion-routing feature.
const FRAME_TYPE_ONION: u16 = 0x0003;
#[allow(dead_code)] // Reserved for future capability-negotiation feature.
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

/// QUIC peer registration for GDP integration (RFC-0850 §8.7.5).
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct PeerRegistration {
    /// Peer identifier (Ed25519 public key hex or "quic:addr:port")
    pub peer_id: String,
    /// Peer's QUIC address
    pub addr: SocketAddr,
    /// GDP discovery scope
    pub scope: DiscoveryScope,
    /// Bootstrap method used to discover this peer
    pub bootstrap_method: BootstrapMethod,
    /// Gateway capabilities (bitmask)
    pub capabilities: u64,
}

/// Connection migration state (RFC 9000 §9, RFC-0850 §8.7.4).
#[derive(Clone, Debug)]
pub struct MigrationState {
    /// Number of connection IDs issued for this peer
    pub cid_count: u32,
    /// Last known remote address
    pub last_remote_addr: Option<SocketAddr>,
    /// Number of successful migrations
    pub migration_count: u32,
}

/// Connection state for a known peer.
struct PeerState {
    /// Peer's SocketAddr (resolved from GDP or config)
    addr: SocketAddr,
    /// Liveness tracking: consecutive missed pongs
    #[allow(dead_code)] // Tracked for future liveness-based eviction.
    missed_pongs: u32,
    /// Last successful pong nonce.
    ///
    /// Inbound PING replay defense does NOT live here — it is per-connection
    /// state in the accept loop, which is where a PONG is actually emitted.
    /// This field tracks the *outbound* direction for future liveness-based
    /// eviction, and is still unread.
    #[allow(dead_code)] // Read when liveness-based eviction lands (CD-6 D3).
    last_pong_nonce: u64,
    /// GDP registration (if peer is registered)
    registration: Option<PeerRegistration>,
    /// Connection migration tracking
    migration: MigrationState,
    /// Peer trust level: Verified (in GDP) or Unverified (not in GDP)
    trusted: bool,
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
    /// GDP discovery state for peer management
    discovery: RwLock<DiscoveryState>,
    /// Replay cache for 0-RTT envelope deduplication (RFC-0853 §7)
    ///
    /// Shared as an `Arc` so the spawned accept loop can consult it: the loop
    /// outlives `start_server`'s borrow of `self`, and the replay check has to
    /// happen on the receive path or it is not a check at all.
    replay_cache: Arc<Mutex<ReplayCache>>,
    /// QUIC endpoint (set when server starts)
    endpoint: Mutex<Option<quinn::Endpoint>>,
    /// TLS certificate (DER bytes, set when server starts)
    cert_der: Mutex<Option<Vec<u8>>>,
}

impl QuicAdapter {
    pub fn new(config: QuicConfig) -> Self {
        let (inbound_tx, inbound_rx) = mpsc::channel(config.max_pending_envelopes);
        let replay_cache = ReplayCache::new(3600, 10_000);
        let discovery = DiscoveryState::new(BootstrapMethod::Static);
        Self {
            config,
            inbound_rx: Mutex::new(inbound_rx),
            inbound_tx,
            peers: RwLock::new(HashMap::new()),
            self_id: Mutex::new(None),
            ping_counter: std::sync::atomic::AtomicU64::new(0),
            discovery: RwLock::new(discovery),
            replay_cache: Arc::new(Mutex::new(replay_cache)),
            endpoint: Mutex::new(None),
            cert_der: Mutex::new(None),
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
        debug_assert!(
            payload.len() <= u32::MAX as usize - 2,
            "payload too large for u32 frame_len"
        );
        let frame_len = 2 + payload.len();
        let mut buf = Vec::with_capacity(4 + frame_len);
        buf.extend_from_slice(&(frame_len as u32).to_be_bytes());
        buf.extend_from_slice(&frame_type.to_be_bytes());
        buf.extend_from_slice(payload);
        buf
    }

    /// Decode a frame header. Returns (frame_type, payload_len) or error.
    ///
    /// The declared `payload_len` is bounded here, at the decode boundary, so
    /// that no caller can be tricked into allocating from an attacker-controlled
    /// length. A 6-byte header declaring `frame_len = 0xFFFFFFFF` yields
    /// `payload_len = 0xFFFFFFFD`; without this check the accept loop's
    /// `vec![0u8; plen]` attempts a ~4 GB allocation per stream (RFC-0850 §8.7.3
    /// frame limits; cap per `max_payload_bytes`).
    pub fn decode_frame_header(header: &[u8; 6]) -> Result<(u16, u32), String> {
        let frame_len = u32::from_be_bytes([header[0], header[1], header[2], header[3]]);
        if frame_len < 2 {
            return Err(format!("frame_len too small: {frame_len}"));
        }
        let frame_type = u16::from_be_bytes([header[4], header[5]]);
        let payload_len = frame_len - 2;
        if payload_len as usize > Self::max_payload_bytes() {
            return Err(format!(
                "payload too large: {payload_len} > {}",
                Self::max_payload_bytes()
            ));
        }
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
    pub async fn start_server(&self) -> Result<(), PlatformAdapterError> {
        let listen_addr: SocketAddr = self
            .config
            .listen_addr
            .parse()
            .map_err(|e| quic_err(format!("Invalid listen addr: {e}")))?;

        let (endpoint, cert) = Self::make_server_endpoint(
            listen_addr,
            self.config.tls_cert_path.as_deref(),
            self.config.tls_key_path.as_deref(),
        )
        .map_err(|e| quic_err(format!("Server endpoint: {e}")))?;

        // Store cert and endpoint
        *self.cert_der.lock().await = Some(cert);
        *self.self_id.lock().await = Some(format!("quic:{}", listen_addr));

        // Spawn accept loop
        let inbound_tx = self.inbound_tx.clone();
        let ep = endpoint.clone();
        let replay_cache = Arc::clone(&self.replay_cache);
        *self.endpoint.lock().await = Some(endpoint);

        tokio::spawn(async move {
            tracing::info!("QUIC accept loop started on {}", listen_addr);
            loop {
                match ep.accept().await {
                    Some(connecting) => {
                        let tx = inbound_tx.clone();
                        let rc = Arc::clone(&replay_cache);
                        tokio::spawn(async move {
                            // L-10: highest PING nonce reflected so far on this
                            // connection. Per-connection, so one noisy peer
                            // cannot advance another's window.
                            let mut last_ping_nonce: Option<u64> = None;
                            match connecting.await {
                                Ok(conn) => {
                                    tracing::info!(
                                        "QUIC connection from {}",
                                        conn.remote_address()
                                    );
                                    // Accept incoming bidirectional streams
                                    loop {
                                        match conn.accept_bi().await {
                                            Ok((mut send, mut recv)) => {
                                                // Read frame: [u32 frame_len][u16 type][payload]
                                                let mut header = [0u8; 6];
                                                match recv.read_exact(&mut header).await {
                                                    Ok(()) => {
                                                        if let Ok((ftype, plen)) =
                                                            QuicAdapter::decode_frame_header(
                                                                &header,
                                                            )
                                                        {
                                                            if ftype == FRAME_TYPE_ENVELOPE
                                                                || ftype == FRAME_TYPE_FRAGMENT
                                                            {
                                                                let mut payload =
                                                                    vec![0u8; plen as usize];
                                                                if recv
                                                                    .read_exact(&mut payload)
                                                                    .await
                                                                    .is_ok()
                                                                {
                                                                    // Replay check on
                                                                    // the receive path
                                                                    // (RFC-0853 §7).
                                                                    // This used to be
                                                                    // defined and never
                                                                    // called, so a
                                                                    // captured frame
                                                                    // replayed freely.
                                                                    let dedup_key =
                                                                        match DeterministicEnvelope::from_wire_bytes(
                                                                            &payload,
                                                                        ) {
                                                                            Ok(env) => env.envelope_id,
                                                                            // Not a full
                                                                            // envelope (e.g. a
                                                                            // FRAGMENT frame):
                                                                            // fall back to the
                                                                            // payload hash, which
                                                                            // is still a stable
                                                                            // replay key.
                                                                            Err(_) => {
                                                        *blake3::hash(&payload)
                                                            .as_bytes()
                                                    }
                                                                        };
                                                                    // The epoch is local
                                                                    // wall-clock, NOT the
                                                                    // envelope's
                                                                    // `logical_timestamp`:
                                                                    // the cache purges by
                                                                    // epoch, so a
                                                                    // backdated timestamp
                                                                    // from an attacker
                                                                    // would flush the
                                                                    // seen-set and unlock
                                                                    // every earlier replay.
                                                                    let now_unix =
                                                                        std::time::SystemTime::now(
                                                                        )
                                                                        .duration_since(
                                                                            std::time::UNIX_EPOCH,
                                                                        )
                                                                        .map(|d| d.as_secs())
                                                                        .unwrap_or(0);
                                                                    let is_replay = {
                                                                        let mut cache =
                                                                            rc.lock().await;
                                                                        cache
                                                                            .check_and_insert(
                                                                                dedup_key, now_unix,
                                                                            )
                                                                            .is_err()
                                                                    };
                                                                    if is_replay {
                                                                        tracing::debug!(
                                                                            "dropping replayed \
                                                                             quic frame: {}",
                                                                            blake3::hash(&payload)
                                                                        );
                                                                        continue;
                                                                    }
                                                                    let raw = RawPlatformMessage {
                                                                        platform_id: format!(
                                                                            "quic:{}",
                                                                            blake3::hash(&payload)
                                                                        ),
                                                                        payload,
                                                                        metadata: [(
                                                                            "frame_type".into(),
                                                                            format!(
                                                                                "0x{:04x}",
                                                                                ftype
                                                                            ),
                                                                        )]
                                                                        .into_iter()
                                                                        .collect(),
                                                                    };
                                                                    let _ = tx.try_send(raw);
                                                                }
                                                            } else if ftype == FRAME_TYPE_PING {
                                                                // Respond with PONG
                                                                let mut nonce_bytes = [0u8; 8];
                                                                if recv
                                                                    .read_exact(&mut nonce_bytes)
                                                                    .await
                                                                    .is_ok()
                                                                {
                                                                    let nonce = u64::from_be_bytes(
                                                                        nonce_bytes,
                                                                    );
                                                                    // L-10: the PONG used to
                                                                    // echo whatever nonce
                                                                    // arrived, with no
                                                                    // memory of what was
                                                                    // already
                                                                    // reflected — so a
                                                                    // captured ping could
                                                                    // be replayed for
                                                                    // unbounded liveness
                                                                    // responses. A
                                                                    // ping nonce is
                                                                    // monotonically
                                                                    // increasing per
                                                                    // sender, so a
                                                                    // duplicate or
                                                                    // regressed one is
                                                                    // a replay.
                                                                    if last_ping_nonce.is_some_and(
                                                                        |last| nonce <= last,
                                                                    ) {
                                                                        tracing::debug!(
                                                                            nonce,
                                                                            "dropping replayed                                                                              quic ping"
                                                                        );
                                                                    } else {
                                                                        last_ping_nonce =
                                                                            Some(nonce);
                                                                        let pong =
                                                                            QuicAdapter::encode_pong(
                                                                                nonce,
                                                                            );
                                                                        let _ = send
                                                                            .write_all(&pong)
                                                                            .await;
                                                                    }
                                                                }
                                                            }
                                                        }
                                                    }
                                                    Err(e) => {
                                                        tracing::debug!(
                                                            "QUIC stream read error: {e}"
                                                        );
                                                    }
                                                }
                                            }
                                            Err(quinn::ConnectionError::ApplicationClosed(_)) => {
                                                tracing::info!("QUIC connection closed");
                                                break;
                                            }
                                            Err(e) => {
                                                tracing::warn!("QUIC accept_bi error: {e}");
                                                break;
                                            }
                                        }
                                    }
                                }
                                Err(e) => {
                                    tracing::warn!("QUIC connection failed: {e}");
                                }
                            }
                        });
                    }
                    None => {
                        tracing::info!("QUIC endpoint closed");
                        break;
                    }
                }
            }
        });

        tracing::info!("QUIC server started on {}", listen_addr);
        Ok(())
    }

    /// Build the server endpoint from `config`.
    ///
    /// TLS material resolution (audit H-8):
    ///
    /// - **Both** `tls_cert_path` and `tls_key_path` supplied ⇒ load them.
    ///   The operator's certificate is actually used.
    /// - **Neither** supplied ⇒ mint an ephemeral self-signed certificate and
    ///   log a warning that the endpoint has no stable server identity.
    /// - **Exactly one** supplied ⇒ error. Half a TLS configuration is a
    ///   misconfiguration, and silently ignoring either half is precisely the
    ///   defect this function used to have.
    ///
    /// Previously all three cases took the self-signed path, so an operator who
    /// supplied a real CA-signed certificate got an ephemeral one instead with
    /// no warning — and `auth_mode` was never consulted at all.
    fn make_server_endpoint(
        addr: SocketAddr,
        cert_path: Option<&str>,
        key_path: Option<&str>,
    ) -> Result<(quinn::Endpoint, Vec<u8>), Box<dyn std::error::Error>> {
        let (cert_der, key_der) = match (cert_path, key_path) {
            (Some(cert_path), Some(key_path)) => {
                let cert_pem = std::fs::read(cert_path)
                    .map_err(|e| format!("read tls_cert_path {cert_path}: {e}"))?;
                let key_pem = std::fs::read(key_path)
                    .map_err(|e| format!("read tls_key_path {key_path}: {e}"))?;

                let cert_chain = parse_pem_certs(&cert_pem)
                    .map_err(|e| format!("parse tls_cert_path {cert_path}: {e}"))?;
                if cert_chain.is_empty() {
                    return Err(format!(
                        "tls_cert_path {cert_path} contained no CERTIFICATE block"
                    )
                    .into());
                }
                let key = parse_pem_key(&key_pem)
                    .map_err(|e| format!("parse tls_key_path {key_path}: {e}"))?;
                (cert_chain, key)
            }
            (None, None) => {
                // No operator TLS material. Ephemeral and unusable for anything
                // but local development — say so loudly rather than silently.
                tracing::warn!(
                    "QUIC server starting with an EPHEMERAL SELF-SIGNED certificate: \
                     clients cannot pin a stable identity and this must not be used \
                     outside local development. Set tls_cert_path + tls_key_path."
                );
                let cert = rcgen::generate_simple_self_signed(vec!["localhost".into()])?;
                let cert_der = rustls::pki_types::CertificateDer::from(cert.cert);
                let key_der = rustls::pki_types::PrivateKeyDer::Pkcs8(
                    rustls::pki_types::PrivatePkcs8KeyDer::from(cert.key_pair.serialize_der()),
                );
                (vec![cert_der], key_der)
            }
            (Some(_), None) => {
                return Err("tls_cert_path is set but tls_key_path is not: refusing to \
                            start with a self-signed certificate that silently ignores \
                            the operator's configured certificate"
                    .into())
            }
            (None, Some(_)) => {
                return Err("tls_key_path is set but tls_cert_path is not: refusing to \
                            start with a self-signed certificate that silently ignores \
                            the operator's configured key"
                    .into())
            }
        };

        let mut server_config = quinn::ServerConfig::with_single_cert(cert_der.clone(), key_der)?;

        let transport = quinn::TransportConfig::default();
        server_config.transport_config(Arc::new(transport));

        let endpoint = quinn::Endpoint::server(server_config, addr)?;
        Ok((endpoint, cert_der[0].to_vec()))
    }

    // ── GDP Integration (RFC-0850 §8.7.5, RFC-0851) ──

    /// Register a QUIC peer in GDP discovery.
    ///
    /// Called when a new QUIC connection is established and the peer's identity
    /// is validated against the GDP registry. Promotes peer from Unverified to Verified.
    pub async fn register_peer(&self, registration: PeerRegistration) {
        let peer_id = registration.peer_id.clone();
        let mut peers = self.peers.write().await;
        let mut discovery = self.discovery.write().await;

        if let Some(peer) = peers.get_mut(&peer_id) {
            peer.registration = Some(registration);
            peer.trusted = true;
            discovery.add_discovered_peers(1);
            tracing::info!("QUIC peer {} registered in GDP", peer_id);
        } else {
            // Peer not yet in connection pool — create entry
            tracing::info!(
                "QUIC peer {} registered in GDP (no active connection)",
                peer_id
            );
        }
    }

    /// Bootstrap QUIC peers from config seed list.
    ///
    /// Iterates `config.peer_addrs` and adds each as an unverified peer.
    /// Transitions GDP discovery from Bootstrap to Expansion when >= 5 peers.
    pub async fn bootstrap_from_config(&self) -> Result<u32, PlatformAdapterError> {
        let mut peers = self.peers.write().await;
        let mut count = 0u32;

        for (peer_id, addr_str) in &self.config.peer_addrs {
            let addr: SocketAddr = addr_str
                .parse()
                .map_err(|e| quic_err(format!("Invalid peer addr {}: {e}", addr_str)))?;

            peers.insert(
                peer_id.clone(),
                PeerState {
                    addr,
                    missed_pongs: 0,
                    last_pong_nonce: 0,
                    registration: None,
                    migration: MigrationState {
                        cid_count: 0,
                        last_remote_addr: None,
                        migration_count: 0,
                    },
                    trusted: false, // Unverified until GDP registration
                },
            );
            count += 1;
        }

        // Update GDP discovery state
        let mut discovery = self.discovery.write().await;
        discovery.add_discovered_peers(count);

        // Transition to Expansion if we have enough peers (RFC-0851 §8)
        if discovery.peer_count >= 5
            && discovery.phase == octo_network::gdp::types::DiscoveryLifecycle::Bootstrap
        {
            if let Err(e) = discovery.start_expansion() {
                tracing::warn!("Failed to transition to Expansion: {e}");
            } else {
                tracing::info!(
                    "GDP discovery transitioned to Expansion with {} peers",
                    discovery.peer_count
                );
            }
        }

        tracing::info!("Bootstrapped {} QUIC peers from config", count);
        Ok(count)
    }

    /// Check if a peer is verified (registered in GDP).
    pub async fn is_peer_verified(&self, peer_id: &str) -> bool {
        let peers = self.peers.read().await;
        peers.get(peer_id).map(|p| p.trusted).unwrap_or(false)
    }

    /// Get the current GDP discovery phase.
    pub async fn discovery_phase(&self) -> octo_network::gdp::types::DiscoveryLifecycle {
        let discovery = self.discovery.read().await;
        discovery.phase
    }

    /// Get the current peer count.
    pub async fn peer_count(&self) -> u32 {
        let peers = self.peers.read().await;
        peers.len() as u32
    }

    // ── 0-RTT Replay Protection (RFC-0853 §7) ──

    /// Check if an envelope ID is a replay (for 0-RTT data).
    ///
    /// Returns `Ok(())` if the envelope is fresh (NOT a replay, inserted into cache).
    /// Returns `Err` if the envelope has been seen before (IS a replay).
    ///
    /// This is called before processing any 0-RTT envelope per RFC-0850 §8.7.2.
    pub async fn check_replay(
        &self,
        envelope_id: [u8; 32],
        logical_timestamp: u64,
    ) -> Result<(), octo_network::dot::error::DotError> {
        let mut cache = self.replay_cache.lock().await;
        cache.check_and_insert(envelope_id, logical_timestamp)
    }

    /// Get the current replay cache size.
    pub async fn replay_cache_size(&self) -> usize {
        let cache = self.replay_cache.lock().await;
        cache.len()
    }

    // ── Connection Migration (RFC 9000 §9, RFC-0850 §8.7.4) ──

    /// Handle a connection migration event.
    ///
    /// Called when QUIC detects a peer's address has changed (WiFi → cellular,
    /// NAT rebinding). Updates the peer's known address and migration count.
    ///
    /// The overlay session is unaffected — connection migration is QUIC transport layer.
    pub async fn handle_migration(&self, peer_id: &str, new_remote_addr: SocketAddr) -> bool {
        let mut peers = self.peers.write().await;
        if let Some(peer) = peers.get_mut(peer_id) {
            let old_addr = peer.migration.last_remote_addr;
            peer.migration.last_remote_addr = Some(new_remote_addr);
            peer.migration.migration_count += 1;
            peer.addr = new_remote_addr;
            tracing::info!(
                "QUIC connection migration for {}: {:?} → {} (migration #{})",
                peer_id,
                old_addr,
                new_remote_addr,
                peer.migration.migration_count
            );
            true
        } else {
            tracing::warn!("Migration for unknown peer {}", peer_id);
            false
        }
    }

    /// Issue a new connection ID for a peer.
    ///
    /// Gateways MUST support at least 2 concurrent connection IDs (RFC-0850 §8.7.4).
    pub async fn issue_connection_id(&self, peer_id: &str) -> u32 {
        let mut peers = self.peers.write().await;
        if let Some(peer) = peers.get_mut(peer_id) {
            peer.migration.cid_count += 1;
            peer.migration.cid_count
        } else {
            0
        }
    }

    /// Get migration stats for a peer.
    pub async fn migration_state(&self, peer_id: &str) -> Option<MigrationState> {
        let peers = self.peers.read().await;
        peers.get(peer_id).map(|p| p.migration.clone())
    }
}

/// Parse a PEM chain into DER certificates, preserving order.
fn parse_pem_certs(
    pem: &[u8],
) -> Result<Vec<rustls::pki_types::CertificateDer<'static>>, std::io::Error> {
    let mut reader = std::io::BufReader::new(pem);
    rustls_pemfile::certs(&mut reader)
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e.to_string()))
}

/// Parse a PEM private key (PKCS#8 / PKCS#1 / SEC1).
///
/// `with_single_cert` accepts any of the three encodings, so operators are not
/// forced to reissue a certificate merely to match this crate's format.
fn parse_pem_key(pem: &[u8]) -> Result<rustls::pki_types::PrivateKeyDer<'static>, std::io::Error> {
    let mut reader = std::io::BufReader::new(pem);
    rustls_pemfile::private_key(&mut reader)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e.to_string()))?
        .ok_or_else(|| {
            std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "no PRIVATE KEY block found",
            )
        })
}

fn quic_err(msg: impl Into<String>) -> PlatformAdapterError {
    PlatformAdapterError::Unreachable {
        platform: "quic".into(),
        reason: msg.into(),
    }
}

#[async_trait]
impl PlatformAdapter for QuicAdapter {
    async fn send_message(
        &self,
        domain: &BroadcastDomainId,
        envelope: &DeterministicEnvelope,
        _payload: &[u8],
    ) -> Result<DeliveryReceipt, PlatformAdapterError> {
        // Raw binary transport per RFC-0850 §8.7.3
        let wire_bytes = envelope.to_wire_bytes();
        let domain_hex = hex_encode(&domain.domain_hash);

        // Send via QUIC endpoint if available
        {
            let frame = Self::encode_envelope_frame(&wire_bytes);
            let peers = self.peers.read().await;
            let peer = peers.values().find(|p| p.trusted);
            if let Some(peer) = peer {
                let ep = self.endpoint.lock().await;
                if let Some(ref endpoint) = *ep {
                    let addr = peer.addr;
                    let conn = endpoint
                        .connect(addr, "localhost")
                        .map_err(|e| quic_err(format!("QUIC connect: {e}")))?;
                    match conn.await {
                        Ok(connection) => {
                            let (mut send, _recv) = connection
                                .open_bi()
                                .await
                                .map_err(|e| quic_err(format!("QUIC open_bi: {e}")))?;
                            send.write_all(&frame)
                                .await
                                .map_err(|e| quic_err(format!("QUIC write: {e}")))?;
                            send.finish().ok();
                            tracing::info!("QUIC sent {} bytes to {}", frame.len(), addr);
                        }
                        Err(e) => {
                            tracing::warn!("QUIC connection failed: {e}");
                        }
                    }
                }
            } else {
                tracing::warn!("QUIC send: no trusted peer for domain {}", domain_hex);
            }
        }

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
            // Ed25519 named explicitly so this reads as an assumption rather than an
            // accident; per-peer algorithm binding is CD-6's work.
            // M-15: see the note on the fallback path below — parsing alone
            // accepts a structurally valid frame with a forged signature.
            env.verify(&octo_algo_ed25519::ed25519_algorithm_id(), &env.source_peer)
                .map_err(|e| quic_err(format!("envelope verify failed: {e}")))?;
            return Ok(env);
        }

        // Fallback: DOT/1/{b64} text format (interop)
        let text = String::from_utf8_lossy(&raw.payload);
        let wire_bytes = octo_network::dot::transport::decode_text_ref(&text).map_err(|e| {
            PlatformAdapterError::ApiError {
                code: 400,
                message: format!("canonicalize failed: {e}"),
            }
        })?;

        let envelope = DeterministicEnvelope::from_wire_bytes(&wire_bytes).map_err(|e| {
            PlatformAdapterError::ApiError {
                code: 400,
                message: format!("canonicalize failed: {e}"),
            }
        })?;
        // M-15: parsing is not verification. `from_wire_bytes` accepts a
        // structurally valid frame whose `envelope_id` does not match its
        // contents and whose signature is garbage. The key is the envelope's
        // own `source_peer`, so this proves integrity and internal consistency,
        // NOT that the claimed peer is who it claims to be — binding the key to
        // a known peer is the registry work in CD-6.
        envelope
            .verify(
                &octo_algo_ed25519::ed25519_algorithm_id(),
                &envelope.source_peer,
            )
            .map_err(|e| quic_err(format!("envelope verify failed: {e}")))?;
        Ok(envelope)
    }

    fn capabilities(&self) -> CapabilityReport {
        CapabilityReport {
            max_payload_bytes: Self::max_payload_bytes(),
            supports_fragmentation: true,
            supports_encryption: true, // TLS 1.3
            supports_raw_binary: true, // QUIC carries Vec<u8> natively
            rate_limit_per_second: Self::rate_limit_per_second(),
            media_capabilities: None,
            ..Default::default()
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
    fn test_decode_frame_header_rejects_oversized_payload() {
        // Regression (H-9): an attacker-controlled frame_len must not survive
        // decode as a ~4 GB allocation request. frame_len = 0xFFFFFFFF is the
        // worst case; anything above the 1 MB cap must be rejected here.
        let header = [0xFF, 0xFF, 0xFF, 0xFF, 0x00, 0x01];
        let err = QuicAdapter::decode_frame_header(&header).unwrap_err();
        assert!(
            err.contains("payload too large"),
            "expected a size rejection, got: {err}"
        );

        // Header layout is [u32 frame_len][u16 frame_type] — 6 bytes, so a bare
        // u32::to_be_bytes is not a header.
        let header_of = |frame_len: u32| {
            let mut h = [0u8; 6];
            h[..4].copy_from_slice(&frame_len.to_be_bytes());
            h[4..].copy_from_slice(&FRAME_TYPE_ENVELOPE.to_be_bytes());
            h
        };

        // One byte over the cap is still rejected — the boundary is the cap,
        // not some rounder number. frame_len = payload + 2, so payload = cap+1.
        let one_over = (QuicAdapter::max_payload_bytes() + 1) as u32;
        assert!(QuicAdapter::decode_frame_header(&header_of(one_over + 2)).is_err());

        // Exactly at the cap is accepted.
        let at_cap = QuicAdapter::max_payload_bytes() as u32;
        let (_, plen) = QuicAdapter::decode_frame_header(&header_of(at_cap + 2)).unwrap();
        assert_eq!(plen as usize, QuicAdapter::max_payload_bytes());
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

    // ── GDP Integration Tests ──

    #[tokio::test]
    async fn test_bootstrap_from_config() {
        let mut peer_addrs = HashMap::new();
        for i in 0..6 {
            peer_addrs.insert(format!("peer{}", i), format!("127.0.0.1:{}", 47400 + i));
        }
        let config = QuicConfig {
            peer_addrs,
            ..Default::default()
        };
        let adapter = QuicAdapter::new(config);
        let count = adapter.bootstrap_from_config().await.unwrap();
        assert_eq!(count, 6);
        assert_eq!(adapter.peer_count().await, 6);

        // Should have transitioned to Expansion (>= 5 peers)
        assert_eq!(
            adapter.discovery_phase().await,
            octo_network::gdp::types::DiscoveryLifecycle::Expansion
        );
    }

    #[tokio::test]
    async fn test_bootstrap_stays_in_bootstrap_phase() {
        let mut peer_addrs = HashMap::new();
        for i in 0..3 {
            peer_addrs.insert(format!("peer{}", i), format!("127.0.0.1:{}", 47400 + i));
        }
        let config = QuicConfig {
            peer_addrs,
            ..Default::default()
        };
        let adapter = QuicAdapter::new(config);
        adapter.bootstrap_from_config().await.unwrap();

        // Should stay in Bootstrap (< 5 peers)
        assert_eq!(
            adapter.discovery_phase().await,
            octo_network::gdp::types::DiscoveryLifecycle::Bootstrap
        );
    }

    #[tokio::test]
    async fn test_register_peer_promotes_to_verified() {
        let mut peer_addrs = HashMap::new();
        peer_addrs.insert("peer1".into(), "127.0.0.1:47401".into());
        let config = QuicConfig {
            peer_addrs,
            ..Default::default()
        };
        let adapter = QuicAdapter::new(config);
        adapter.bootstrap_from_config().await.unwrap();

        // Initially unverified
        assert!(!adapter.is_peer_verified("peer1").await);

        // Register in GDP
        adapter
            .register_peer(PeerRegistration {
                peer_id: "peer1".into(),
                addr: "127.0.0.1:47401".parse().unwrap(),
                scope: DiscoveryScope::Global,
                bootstrap_method: BootstrapMethod::Static,
                capabilities: 0x0001,
            })
            .await;

        // Now verified
        assert!(adapter.is_peer_verified("peer1").await);
    }

    #[tokio::test]
    async fn test_bootstrap_invalid_addr_fails() {
        let mut peer_addrs = HashMap::new();
        peer_addrs.insert("bad".into(), "not-an-address".into());
        let config = QuicConfig {
            peer_addrs,
            ..Default::default()
        };
        let adapter = QuicAdapter::new(config);
        assert!(adapter.bootstrap_from_config().await.is_err());
    }

    // ── 0-RTT Replay Protection Tests ──

    #[tokio::test]
    async fn test_replay_cache_fresh_envelope() {
        let adapter = QuicAdapter::new(QuicConfig::default());
        let envelope_id = [0x42u8; 32];
        assert!(adapter.check_replay(envelope_id, 1000).await.is_ok());
        assert_eq!(adapter.replay_cache_size().await, 1);
    }

    #[tokio::test]
    async fn test_replay_cache_detects_replay() {
        let adapter = QuicAdapter::new(QuicConfig::default());
        let envelope_id = [0x42u8; 32];
        adapter.check_replay(envelope_id, 1000).await.unwrap();
        // Same envelope ID → replay detected
        assert!(adapter.check_replay(envelope_id, 1001).await.is_err());
    }

    #[tokio::test]
    async fn test_replay_cache_different_envelopes() {
        let adapter = QuicAdapter::new(QuicConfig::default());
        let id1 = [0x01u8; 32];
        let id2 = [0x02u8; 32];
        assert!(adapter.check_replay(id1, 1000).await.is_ok());
        assert!(adapter.check_replay(id2, 1000).await.is_ok());
        assert_eq!(adapter.replay_cache_size().await, 2);
    }

    // ── Connection Migration Tests ──

    #[tokio::test]
    async fn test_handle_migration_updates_address() {
        let mut peer_addrs = HashMap::new();
        peer_addrs.insert("peer1".into(), "127.0.0.1:47401".into());
        let config = QuicConfig {
            peer_addrs,
            ..Default::default()
        };
        let adapter = QuicAdapter::new(config);
        adapter.bootstrap_from_config().await.unwrap();

        let new_addr: SocketAddr = "10.0.0.1:47401".parse().unwrap();
        assert!(adapter.handle_migration("peer1", new_addr).await);

        let state = adapter.migration_state("peer1").await.unwrap();
        assert_eq!(state.migration_count, 1);
        assert_eq!(state.last_remote_addr, Some(new_addr));
    }

    #[tokio::test]
    async fn test_handle_migration_unknown_peer() {
        let adapter = QuicAdapter::new(QuicConfig::default());
        let addr: SocketAddr = "10.0.0.1:47401".parse().unwrap();
        assert!(!adapter.handle_migration("unknown", addr).await);
    }

    #[tokio::test]
    async fn test_issue_connection_id() {
        let mut peer_addrs = HashMap::new();
        peer_addrs.insert("peer1".into(), "127.0.0.1:47401".into());
        let config = QuicConfig {
            peer_addrs,
            ..Default::default()
        };
        let adapter = QuicAdapter::new(config);
        adapter.bootstrap_from_config().await.unwrap();

        assert_eq!(adapter.issue_connection_id("peer1").await, 1);
        assert_eq!(adapter.issue_connection_id("peer1").await, 2);
        assert_eq!(adapter.issue_connection_id("peer1").await, 3);

        // Unknown peer returns 0
        assert_eq!(adapter.issue_connection_id("unknown").await, 0);
    }

    #[tokio::test]
    async fn test_multiple_migrations() {
        let mut peer_addrs = HashMap::new();
        peer_addrs.insert("peer1".into(), "127.0.0.1:47401".into());
        let config = QuicConfig {
            peer_addrs,
            ..Default::default()
        };
        let adapter = QuicAdapter::new(config);
        adapter.bootstrap_from_config().await.unwrap();

        adapter
            .handle_migration("peer1", "10.0.0.1:47401".parse().unwrap())
            .await;
        adapter
            .handle_migration("peer1", "10.0.0.2:47401".parse().unwrap())
            .await;
        adapter
            .handle_migration("peer1", "10.0.0.3:47401".parse().unwrap())
            .await;

        let state = adapter.migration_state("peer1").await.unwrap();
        assert_eq!(state.migration_count, 3);
        assert_eq!(
            state.last_remote_addr,
            Some("10.0.0.3:47401".parse().unwrap())
        );
    }

    // ---- H-8: operator TLS material must actually be used ----

    /// Write a self-signed cert + key to a temp dir as PEM and return the paths.
    fn operator_cert_fixture(tag: &str) -> (std::path::PathBuf, std::path::PathBuf) {
        let dir = std::env::temp_dir().join(format!("quic-h8-{tag}-{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("temp dir");
        let cert = rcgen::generate_simple_self_signed(vec!["operator.example".into()])
            .expect("rcgen cert");
        let cert_path = dir.join("cert.pem");
        let key_path = dir.join("key.pem");
        std::fs::write(&cert_path, cert.cert.pem()).expect("write cert");
        std::fs::write(&key_path, cert.key_pair.serialize_pem()).expect("write key");
        (cert_path, key_path)
    }

    /// The regression this pins: the configured certificate must be the one
    /// served. Before the fix, `make_server_endpoint` ignored both paths and
    /// minted its own, so a round-tripped DER would never match the operator's
    /// file.
    #[tokio::test]
    async fn configured_certificate_is_actually_served() {
        let (cert_path, key_path) = operator_cert_fixture("served");
        let addr: SocketAddr = "127.0.0.1:0".parse().expect("addr");

        let (_endpoint, served) = QuicAdapter::make_server_endpoint(
            addr,
            Some(cert_path.to_str().expect("utf8")),
            Some(key_path.to_str().expect("utf8")),
        )
        .expect("endpoint with operator material");
        assert!(
            !served.is_empty(),
            "served leaf certificate must be non-empty DER"
        );

        // Independently re-read the operator's file and compare, so the
        // assertion does not just echo what the function returned.
        let on_disk = parse_pem_certs(&std::fs::read(&cert_path).expect("read"))
            .expect("parse operator cert");
        assert_eq!(
            served,
            on_disk[0].to_vec(),
            "the DER served must be the operator's certificate, not a generated one"
        );
    }

    #[test]
    fn cert_without_key_is_rejected() {
        let (cert_path, _) = operator_cert_fixture("half-cert");
        let addr: SocketAddr = "127.0.0.1:0".parse().expect("addr");
        let err =
            QuicAdapter::make_server_endpoint(addr, Some(cert_path.to_str().expect("utf8")), None)
                .expect_err("half a TLS configuration must not silently fall back");
        let msg = err.to_string();
        assert!(
            msg.contains("tls_key_path") && msg.contains("refusing"),
            "error must name the missing key and the refusal, got: {msg}"
        );
    }

    #[test]
    fn key_without_cert_is_rejected() {
        let (_, key_path) = operator_cert_fixture("half-key");
        let addr: SocketAddr = "127.0.0.1:0".parse().expect("addr");
        let err =
            QuicAdapter::make_server_endpoint(addr, None, Some(key_path.to_str().expect("utf8")))
                .expect_err("half a TLS configuration must not silently fall back");
        let msg = err.to_string();
        assert!(
            msg.contains("tls_cert_path") && msg.contains("refusing"),
            "error must name the missing cert and the refusal, got: {msg}"
        );
    }

    #[tokio::test]
    async fn no_material_falls_back_to_self_signed() {
        let addr: SocketAddr = "127.0.0.1:0".parse().expect("addr");
        let (_endpoint, served) =
            QuicAdapter::make_server_endpoint(addr, None, None).expect("dev fallback still works");
        assert!(!served.is_empty(), "self-signed fallback must serve a cert");
    }

    #[test]
    fn unreadable_certificate_path_is_an_error_not_a_silent_fallback() {
        let addr: SocketAddr = "127.0.0.1:0".parse().expect("addr");
        let err = QuicAdapter::make_server_endpoint(
            addr,
            Some("/nonexistent/quic/cert.pem"),
            Some("/nonexistent/quic/key.pem"),
        )
        .expect_err("a missing cert file must not fall back to self-signed");
        assert!(
            err.to_string().contains("tls_cert_path"),
            "error must identify the unreadable path, got: {err}"
        );
    }
}
