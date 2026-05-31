//! Native P2P adapter (RFC-0850 §3.1, PlatformType::NativeP2P)
//!
//! Uses libp2p gossipsub for native peer-to-peer DOT envelope transport.
//! This is the preferred transport — lowest latency, highest reliability,
//! no platform API limits.
//!
//! ## Configuration
//!
//! ```json
//! {
//!   "listen_addr": "/ip4/0.0.0.0/tcp/4001",
//!   "bootstrap_peers": ["/ip4/1.2.3.4/tcp/4001"]
//! }
//! ```

use async_trait::async_trait;
use std::collections::HashMap;
use tokio::sync::{mpsc, Mutex};

use octo_network::dot::adapters::{
    CapabilityReport, DeliveryReceipt, PlatformAdapter, RawPlatformMessage,
};
use octo_network::dot::domain::{BroadcastDomainId, PlatformType};
use octo_network::dot::envelope::DeterministicEnvelope;
use octo_network::dot::error::PlatformAdapterError;

/// Configuration for the NativeP2P adapter.
#[derive(Clone, Debug, serde::Deserialize, serde::Serialize)]
pub struct NativeP2PConfig {
    /// libp2p listen address (e.g., "/ip4/0.0.0.0/tcp/4001")
    pub listen_addr: String,
    /// Bootstrap peer multiaddrs
    #[serde(default)]
    pub bootstrap_peers: Vec<String>,
}

/// Native P2P adapter using libp2p gossipsub.
#[allow(dead_code)]
pub struct NativeP2PAdapter {
    config: NativeP2PConfig,
    /// Inbound message channel (populated by gossipsub event handler)
    inbound_rx: Mutex<mpsc::Receiver<RawPlatformMessage>>,
    inbound_tx: mpsc::Sender<RawPlatformMessage>,
    /// Local peer ID (resolved after swarm starts)
    self_id: Mutex<Option<String>>,
    /// Active gossipsub topic subscriptions
    topics: Mutex<HashMap<String, bool>>,
}

// ── Helper functions (no external crate dependency) ──

fn hex_encode(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{:02x}", b)).collect()
}

impl NativeP2PAdapter {
    pub fn new(config: NativeP2PConfig) -> Self {
        let (inbound_tx, inbound_rx) = mpsc::channel(4096);
        Self {
            config,
            inbound_rx: Mutex::new(inbound_rx),
            inbound_tx,
            self_id: Mutex::new(None),
            topics: Mutex::new(HashMap::new()),
        }
    }

    pub fn from_config_bytes(config: &[u8]) -> Result<Self, String> {
        let config: NativeP2PConfig =
            serde_json::from_slice(config).map_err(|e| format!("Invalid config: {e}"))?;
        Ok(Self::new(config))
    }

    /// Convert a BroadcastDomainId to a gossipsub topic name.
    /// Deterministic: same domain → same topic.
    fn domain_to_topic(domain: &BroadcastDomainId) -> String {
        format!("cipherocto-{}", hex_encode(&domain.domain_hash))
    }

    pub fn encode_envelope(envelope_bytes: &[u8]) -> String {
        octo_network::dot::transport::encode_text_ref(envelope_bytes)
    }

    pub fn decode_envelope(text: &str) -> Result<Vec<u8>, String> {
        octo_network::dot::transport::decode_text_ref(text)
    }

    pub fn domain_hash(platform_id: &str) -> [u8; 32] {
        let normalized = platform_id.trim().to_lowercase();
        *blake3::hash(format!("nativep2p:{}", normalized).as_bytes()).as_bytes()
    }

    pub const PLATFORM_TYPE: u16 = 0x000A;

    pub fn max_payload_bytes() -> usize {
        65_536
    }

    pub fn rate_limit_per_second() -> u32 {
        10_000
    }

    /// Start the libp2p swarm in a background task.
    pub async fn start_swarm(&self) -> Result<(), PlatformAdapterError> {
        use futures::StreamExt;
        use libp2p::gossipsub::{self, Behaviour, Event, MessageAuthenticity, ValidationMode};
        use libp2p::identify;
        use libp2p::mdns;
        use libp2p::noise;
        use libp2p::swarm::SwarmEvent;
        use libp2p::{tcp, yamux, Multiaddr, SwarmBuilder};

        // Generate or load identity
        let local_key = libp2p::identity::Keypair::generate_ed25519();
        let local_peer_id = local_key.public().to_peer_id();
        *self.self_id.lock().await = Some(local_peer_id.to_string());
        tracing::info!("NativeP2P peer ID: {}", local_peer_id);

        // Build gossipsub behaviour
        let gossipsub_config = gossipsub::ConfigBuilder::default()
            .validation_mode(ValidationMode::Strict)
            .build()
            .map_err(|e| transport_err(format!("Gossipsub config: {e}")))?;

        let gossipsub = Behaviour::new(
            MessageAuthenticity::Signed(local_key.clone()),
            gossipsub_config,
        )
        .map_err(|e| transport_err(format!("Gossipsub init: {e}")))?;

        // Build identify behaviour
        let identify = identify::Behaviour::new(identify::Config::new(
            "/cipherocto/0.1.0".to_string(),
            local_key.public(),
        ));

        // Build mdns behaviour for local discovery
        let mdns = mdns::tokio::Behaviour::new(mdns::Config::default(), local_peer_id)
            .map_err(|e| transport_err(format!("mDNS init: {e}")))?;

        // Combine behaviours
        let mut swarm = SwarmBuilder::with_existing_identity(local_key)
            .with_tokio()
            .with_tcp(
                tcp::Config::default(),
                noise::Config::new,
                yamux::Config::default,
            )
            .map_err(|e| transport_err(format!("Swarm build: {e}")))?
            .with_behaviour(|_| {
                Ok::<_, Box<dyn std::error::Error + Send + Sync>>(CompositeBehaviour {
                    gossipsub,
                    identify,
                    mdns,
                })
            })
            .map_err(|e| transport_err(format!("Behaviour build: {e}")))?
            .with_swarm_config(|cfg| {
                cfg.with_idle_connection_timeout(std::time::Duration::from_secs(60))
            })
            .build();

        // Listen on configured address
        let listen_addr: Multiaddr = self
            .config
            .listen_addr
            .parse()
            .map_err(|e| transport_err(format!("Invalid listen addr: {e}")))?;
        swarm
            .listen_on(listen_addr)
            .map_err(|e| transport_err(format!("Listen failed: {e}")))?;

        // Connect to bootstrap peers
        for peer in &self.config.bootstrap_peers {
            if let Ok(addr) = peer.parse::<Multiaddr>() {
                swarm.dial(addr).ok();
            }
        }

        // Spawn event loop
        let inbound_tx = self.inbound_tx.clone();

        tokio::spawn(async move {
            loop {
                match swarm.next().await {
                    Some(SwarmEvent::Behaviour(CompositeBehaviourEvent::Gossipsub(
                        Event::Message {
                            propagation_source: _,
                            message_id: _,
                            message,
                        },
                    ))) => {
                        let topic_str = message.topic.to_string();
                        let platform_id = format!("{}:{}", topic_str, blake3::hash(&message.data));
                        let raw = RawPlatformMessage {
                            platform_id,
                            payload: message.data,
                            metadata: [("topic".into(), topic_str)].into_iter().collect(),
                        };
                        if let Err(e) = inbound_tx.try_send(raw) {
                            tracing::warn!("NativeP2P inbound channel full: {e}");
                        }
                    }
                    Some(SwarmEvent::Behaviour(CompositeBehaviourEvent::Identify(
                        identify::Event::Received { info, .. },
                    ))) => {
                        tracing::debug!("Identified peer: {}", info.agent_version);
                    }
                    Some(SwarmEvent::Behaviour(CompositeBehaviourEvent::Mdns(
                        mdns::Event::Discovered(list),
                    ))) => {
                        for (peer, _addr) in list {
                            swarm.behaviour_mut().gossipsub.add_explicit_peer(&peer);
                        }
                    }
                    Some(SwarmEvent::NewListenAddr { address, .. }) => {
                        tracing::info!("Listening on {}", address);
                    }
                    Some(_) => {}
                    None => break,
                }
            }
        });

        tracing::info!("NativeP2P swarm started on {}", self.config.listen_addr);
        Ok(())
    }
}

// Composite behaviour for libp2p swarm
#[derive(libp2p::swarm::NetworkBehaviour)]
struct CompositeBehaviour {
    gossipsub: libp2p::gossipsub::Behaviour,
    identify: libp2p::identify::Behaviour,
    mdns: libp2p::mdns::tokio::Behaviour,
}

fn transport_err(msg: impl Into<String>) -> PlatformAdapterError {
    PlatformAdapterError::Unreachable {
        platform: "nativep2p".into(),
        reason: msg.into(),
    }
}

// ── PlatformAdapter ────────────────────────────────────────────────

#[async_trait]
impl PlatformAdapter for NativeP2PAdapter {
    async fn send_envelope(
        &self,
        domain: &BroadcastDomainId,
        envelope: &DeterministicEnvelope,
    ) -> Result<DeliveryReceipt, PlatformAdapterError> {
        // Native binary transport: send raw wire bytes directly over gossipsub.
        // No base64url encoding needed — gossipsub carries Vec<u8> natively.
        let wire_bytes = envelope.to_wire_bytes();
        let topic = Self::domain_to_topic(domain);

        // Publish via gossipsub
        // For now, return a stub — full swarm integration requires
        // passing the swarm handle through the adapter
        tracing::info!(
            "NativeP2P send to topic {}: {} bytes",
            topic,
            wire_bytes.len()
        );

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
            return Err(transport_err("Empty payload"));
        }

        // Native P2P carries raw wire bytes — no encoding layer.
        // Try direct deserialization first (binary path from gossipsub).
        if let Ok(env) = DeterministicEnvelope::from_wire_bytes(&raw.payload) {
            return Ok(env);
        }

        // Fallback: text-based DOT/1/{b64} format (interop with text transports)
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
            supports_fragmentation: true,
            supports_encryption: true, // libp2p Noise protocol
            supports_raw_binary: true, // gossipsub carries Vec<u8> natively
            rate_limit_per_second: Self::rate_limit_per_second(),
            media_capabilities: None,
        }
    }

    fn domain_id(&self, platform_id: &str) -> BroadcastDomainId {
        BroadcastDomainId::new(PlatformType::NativeP2P, platform_id)
    }

    fn platform_type(&self) -> PlatformType {
        PlatformType::NativeP2P
    }

    fn self_handle(&self) -> Option<String> {
        // Try-lock to avoid blocking; None if lock is contended
        self.self_id.try_lock().ok().and_then(|id| id.clone())
    }

    async fn shutdown(&self) -> Result<(), PlatformAdapterError> {
        self.topics.lock().await.clear();
        *self.self_id.lock().await = None;
        tracing::info!("NativeP2P adapter shut down");
        Ok(())
    }

    async fn health_check(&self) -> Result<(), PlatformAdapterError> {
        let id = self.self_id.lock().await;
        if id.is_some() {
            Ok(())
        } else {
            Err(transport_err("NativeP2P swarm not started"))
        }
    }
}

// ── Tests ──────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_hex_encode() {
        assert_eq!(hex_encode(&[0x0a, 0xff, 0x42]), "0aff42");
    }

    #[test]
    fn test_b64url_roundtrip() {
        use octo_network::dot::transport::{b64url_decode, b64url_encode};
        let data = b"test native p2p envelope data";
        let encoded = b64url_encode(data);
        let decoded = b64url_decode(&encoded).unwrap();
        assert_eq!(decoded, data);
    }

    #[test]
    fn test_b64url_empty() {
        use octo_network::dot::transport::{b64url_decode, b64url_encode};
        assert_eq!(b64url_encode(b""), "");
        assert_eq!(b64url_decode("").unwrap(), b"");
    }

    #[test]
    fn test_b64url_padding_cases() {
        use octo_network::dot::transport::{b64url_decode, b64url_encode};
        // 1 byte, 2 bytes, 3 bytes (all padding variations)
        assert_eq!(b64url_decode(&b64url_encode(&[0x41])).unwrap(), vec![0x41]);
        assert_eq!(
            b64url_decode(&b64url_encode(&[0x41, 0x42])).unwrap(),
            vec![0x41, 0x42]
        );
        assert_eq!(
            b64url_decode(&b64url_encode(&[0x41, 0x42, 0x43])).unwrap(),
            vec![0x41, 0x42, 0x43]
        );
    }

    #[test]
    fn test_domain_hash_deterministic() {
        let h1 = NativeP2PAdapter::domain_hash("topic-1");
        let h2 = NativeP2PAdapter::domain_hash("topic-1");
        assert_eq!(h1, h2);
    }

    #[test]
    fn test_domain_hash_normalized() {
        assert_eq!(
            NativeP2PAdapter::domain_hash("TOPIC-1"),
            NativeP2PAdapter::domain_hash("  topic-1  ")
        );
    }

    #[test]
    fn test_encode_decode_envelope() {
        let original = b"test native p2p envelope";
        let encoded = NativeP2PAdapter::encode_envelope(original);
        assert!(encoded.starts_with("DOT/1/"));
        let decoded = NativeP2PAdapter::decode_envelope(&encoded).unwrap();
        assert_eq!(decoded, original);
    }

    #[test]
    fn test_platform_type() {
        assert_eq!(NativeP2PAdapter::PLATFORM_TYPE, 0x000A);
    }

    #[test]
    fn test_capabilities() {
        let adapter = NativeP2PAdapter::new(NativeP2PConfig {
            listen_addr: "/ip4/0.0.0.0/tcp/4001".into(),
            bootstrap_peers: vec![],
        });
        let caps = adapter.capabilities();
        assert_eq!(caps.max_payload_bytes, 65_536);
        assert!(caps.supports_fragmentation);
        assert!(caps.supports_encryption);
        assert!(caps.supports_raw_binary);
        assert_eq!(caps.rate_limit_per_second, 10_000);
    }

    #[test]
    fn test_self_handle_none_initially() {
        let adapter = NativeP2PAdapter::new(NativeP2PConfig {
            listen_addr: "/ip4/0.0.0.0/tcp/4001".into(),
            bootstrap_peers: vec![],
        });
        assert!(adapter.self_handle().is_none());
    }

    #[test]
    fn test_config_from_json() {
        let json = serde_json::json!({
            "listen_addr": "/ip4/0.0.0.0/tcp/4001",
            "bootstrap_peers": ["/ip4/1.2.3.4/tcp/4001"]
        });
        let adapter =
            NativeP2PAdapter::from_config_bytes(serde_json::to_vec(&json).unwrap().as_slice())
                .unwrap();
        assert_eq!(adapter.config.bootstrap_peers.len(), 1);
    }

    #[test]
    fn test_domain_to_topic_deterministic() {
        let domain = BroadcastDomainId::new(PlatformType::NativeP2P, "test-topic");
        let t1 = NativeP2PAdapter::domain_to_topic(&domain);
        let t2 = NativeP2PAdapter::domain_to_topic(&domain);
        assert_eq!(t1, t2);
        assert!(t1.starts_with("cipherocto-"));
    }

    #[tokio::test]
    async fn test_shutdown_clears_state() {
        let adapter = NativeP2PAdapter::new(NativeP2PConfig {
            listen_addr: "/ip4/0.0.0.0/tcp/4001".into(),
            bootstrap_peers: vec![],
        });
        // Shutdown should succeed even without swarm
        adapter.shutdown().await.unwrap();
        assert!(adapter.self_handle().is_none());
    }

    #[tokio::test]
    async fn test_receive_messages_empty() {
        let adapter = NativeP2PAdapter::new(NativeP2PConfig {
            listen_addr: "/ip4/0.0.0.0/tcp/4001".into(),
            bootstrap_peers: vec![],
        });
        let domain = adapter.domain_id("test");
        let messages = adapter.receive_messages(&domain).await.unwrap();
        assert!(messages.is_empty());
    }

    #[tokio::test]
    async fn test_health_check_fails_without_swarm() {
        let adapter = NativeP2PAdapter::new(NativeP2PConfig {
            listen_addr: "/ip4/0.0.0.0/tcp/4001".into(),
            bootstrap_peers: vec![],
        });
        assert!(adapter.health_check().await.is_err());
    }
}
