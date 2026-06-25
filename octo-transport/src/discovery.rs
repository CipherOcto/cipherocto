//! Transport Discovery — bridges GDP gateway discovery with the transport stack.
//!
//! Connects `NodeTransport` capabilities to GDP's `GatewayAdvertisement` and
//! `GatewayCache` so nodes can discover each other's transport capabilities.
//!
//! # Usage
//!
//! ```text
//! let discovery = TransportDiscovery::new(identity, mission_id);
//!
//! // Advertise local node's transport capabilities
//! let adv = discovery.build_advertisement(&node_transport);
//!
//! // Register a peer's advertisement
//! discovery.register_peer(adv);
//!
//! // Query: what transports does peer X support?
//! if let Some(entry) = discovery.peer_capabilities(&peer_id) {
//!     for ep in &entry.endpoints {
//!         println!("transport_type={}", ep.transport_type);
//!     }
//! }
//! ```

use std::sync::Mutex;

use octo_network::dot::gateway::{GatewayClass, GatewayIdentity};
use octo_network::dot::PlatformType;
use octo_network::gdp::advertisement::GatewayAdvertisement;
use octo_network::gdp::cache::{GatewayCache, GatewayCacheEntry};
use octo_network::gdp::identity::GdpGatewayIdentity;
use octo_network::gdp::overlay_endpoint::OverlayEndpoint;
use octo_network::gdp::types::GatewayCapability;

use crate::node_transport::NodeTransport;

/// Bridges the transport stack with GDP gateway discovery.
///
/// Provides:
/// - Build a `GatewayAdvertisement` from local `NodeTransport` capabilities
/// - Register peer advertisements into a `GatewayCache`
/// - Query peer transport capabilities for routing decisions
pub struct TransportDiscovery {
    identity: GdpGatewayIdentity,
    cache: Mutex<GatewayCache>,
    sequence: Mutex<u64>,
}

impl TransportDiscovery {
    /// Create a new `TransportDiscovery` instance.
    pub fn new(identity: GdpGatewayIdentity, mission_id: [u8; 32], cache_size: u32) -> Self {
        let _ = mission_id; // reserved for future GDP scope filtering
        Self {
            identity,
            cache: Mutex::new(GatewayCache::new(cache_size)),
            sequence: Mutex::new(0),
        }
    }

    /// Build a `GatewayAdvertisement` from a `NodeTransport`'s adapter capabilities.
    ///
    /// Each adapter becomes an `OverlayEndpoint` in the advertisement.
    /// Capabilities are derived from the adapter's `CapabilityReport`.
    pub fn build_advertisement(
        &self,
        transport: &NodeTransport,
        network_id: u32,
        current_epoch: u64,
    ) -> GatewayAdvertisement {
        let seq = {
            let mut s = self.sequence.lock().unwrap();
            *s += 1;
            *s
        };

        // Build overlay endpoints from healthy transports
        let mut endpoints: Vec<OverlayEndpoint> = Vec::new();
        let mut caps: Vec<GatewayCapability> = Vec::new();

        // We need adapter capabilities — build endpoints from transport names
        // Since NodeTransport only exposes names and health, we derive
        // transport_type from the platform name hash
        for name in transport.healthy_transports() {
            let transport_type = name_to_transport_type(&name);
            let endpoint_hash = blake3::hash(name.as_bytes()).into();
            endpoints.push(OverlayEndpoint {
                transport_type,
                endpoint_hash,
                priority: 100,
                bandwidth_class: 0,
                flags: 0,
            });
        }

        // Derive capabilities from transport count
        if transport.transport_count() > 0 {
            caps.push(GatewayCapability::Relay);
        }
        if transport.healthy_transports().len() >= 3 {
            caps.push(GatewayCapability::Storage);
        }

        // Compute Merkle roots
        let transport_items: Vec<[u8; 32]> = endpoints
            .iter()
            .map(|ep| {
                let mut h = blake3::Hasher::new();
                h.update(&ep.transport_type.to_be_bytes());
                h.update(&ep.endpoint_hash);
                *h.finalize().as_bytes()
            })
            .collect();
        let transport_root =
            GatewayAdvertisement::compute_merkle_root(&transport_items);

        let cap_items: Vec<[u8; 32]> = caps
            .iter()
            .map(|c| {
                let mut h = blake3::Hasher::new();
                h.update(&(*c as u64).to_be_bytes());
                *h.finalize().as_bytes()
            })
            .collect();
        let capabilities_root =
            GatewayAdvertisement::compute_merkle_root(&cap_items);

        GatewayAdvertisement {
            version: 1,
            gateway_id: self.identity.gateway_id(),
            network_id,
            sequence: seq,
            logical_timestamp: current_epoch,
            gateway_class: GatewayClass::Edge as u16,
            capabilities_root,
            transport_root,
            route_root: [0u8; 32],
            trust_root: [0u8; 32],
            overlay_endpoints: endpoints,
            signature: [0u8; 64],
        }
    }

    /// Register a peer's advertisement in the local cache.
    pub fn register_peer(&self, adv: &GatewayAdvertisement, current_epoch: u64) {
        let caps = adv
            .overlay_endpoints
            .iter()
            .map(|_ep| GatewayCapability::Relay)
            .collect();
        let entry = GatewayCacheEntry {
            advertisement_hash: blake3::hash(&adv.to_signing_bytes()).into(),
            first_seen: current_epoch,
            last_seen: current_epoch,
            trust_score: 500,
            identity: GatewayIdentity {
                gateway_id: adv.gateway_id,
                public_key: adv.gateway_id,
                network_id: adv.network_id,
                gateway_class: GatewayClass::Edge,
                creation_epoch: current_epoch,
                supported_platforms: 0,
                capabilities: 0,
            },
            capabilities: caps,
            endpoints: adv.overlay_endpoints.clone(),
        };
        self.cache.lock().unwrap().insert(entry, current_epoch);
    }

    /// Query: what endpoints does a peer support?
    pub fn peer_endpoints(&self, peer_id: &[u8; 32]) -> Vec<OverlayEndpoint> {
        self.cache
            .lock()
            .unwrap()
            .get(peer_id)
            .map(|e| e.endpoints.clone())
            .unwrap_or_default()
    }

    /// Check if a peer supports a specific transport type.
    pub fn peer_supports_transport(&self, peer_id: &[u8; 32], transport_type: u16) -> bool {
        self.cache
            .lock()
            .unwrap()
            .get(peer_id)
            .map(|e| e.endpoints.iter().any(|ep| ep.transport_type == transport_type))
            .unwrap_or(false)
    }

    /// Find all peers that support a given transport type.
    pub fn peers_with_transport(&self, transport_type: u16) -> Vec<[u8; 32]> {
        let cache = self.cache.lock().unwrap();
        cache
            .iter()
            .filter(|(_, e)| e.endpoints.iter().any(|ep| ep.transport_type == transport_type))
            .map(|(id, _)| *id)
            .collect()
    }

    /// Return the number of discovered peers.
    pub fn peer_count(&self) -> usize {
        self.cache.lock().unwrap().len()
    }

    /// Get the local identity.
    pub fn identity(&self) -> &GdpGatewayIdentity {
        &self.identity
    }

    /// Build a minimal advertisement from identity alone (no transport info).
    ///
    /// Used for TCP handshake exchange when no NodeTransport is available.
    /// Returns a GatewayAdvertisement with the node's identity and empty endpoints.
    pub fn build_advertisement_from_identity(
        &self,
        current_epoch: u64,
    ) -> GatewayAdvertisement {
        let seq = {
            let mut s = self.sequence.lock().unwrap();
            *s += 1;
            *s
        };
        GatewayAdvertisement {
            version: 1,
            gateway_id: self.identity.gateway_id(),
            network_id: 1,
            sequence: seq,
            logical_timestamp: current_epoch,
            gateway_class: GatewayClass::Edge as u16,
            capabilities_root: [0u8; 32],
            transport_root: [0u8; 32],
            route_root: [0u8; 32],
            trust_root: [0u8; 32],
            overlay_endpoints: Vec::new(),
            signature: [0u8; 64],
        }
    }

    /// Return a snapshot of all cached peer entries.
    ///
    /// Returns a Vec of (gateway_id, GatewayCacheEntry) pairs.
    pub fn cache_entries(&self) -> Vec<([u8; 32], GatewayCacheEntry)> {
        self.cache
            .lock()
            .unwrap()
            .iter()
            .map(|(id, entry)| (*id, entry.clone()))
            .collect()
    }

    /// Insert a pre-built cache entry directly.
    ///
    /// Used when receiving peer advertisements via TCP handshake exchange.
    pub fn cache_insert(&self, entry: GatewayCacheEntry, current_epoch: u64) {
        self.cache.lock().unwrap().insert(entry, current_epoch);
    }
}

/// Map a transport name to a GDP transport type identifier.
fn name_to_transport_type(name: &str) -> u16 {
    // Use PlatformType discriminant values as transport type identifiers
    match name {
        "telegram" => PlatformType::Telegram as u16,
        "discord" => PlatformType::Discord as u16,
        "matrix" => PlatformType::Matrix as u16,
        "nostr" => PlatformType::Nostr as u16,
        "signal" => PlatformType::Signal as u16,
        "irc" => PlatformType::IRC as u16,
        "slack" => PlatformType::Slack as u16,
        "whatsapp" => PlatformType::WhatsApp as u16,
        "webhook" => PlatformType::Webhook as u16,
        "native-p2p" => PlatformType::NativeP2P as u16,
        "bluetooth" => PlatformType::Bluetooth as u16,
        "lora" => PlatformType::LoRa as u16,
        "webrtc" => PlatformType::WebRTC as u16,
        "bluesky" => PlatformType::Bluesky as u16,
        "twitter" => PlatformType::Twitter as u16,
        "reddit" => PlatformType::Reddit as u16,
        "wechat" => PlatformType::WeChat as u16,
        "dingtalk" => PlatformType::DingTalk as u16,
        "lark" => PlatformType::Lark as u16,
        "qq" => PlatformType::QQ as u16,
        "quic" => PlatformType::Quic as u16,
        _ => 0xFFFF, // unknown
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use crate::sender::{NetworkSender, SendContext, TransportError};
    use async_trait::async_trait;

    struct MockSender {
        name: String,
        healthy: bool,
    }

    #[async_trait]
    impl NetworkSender for MockSender {
        async fn send(&self, _p: &[u8], _c: &SendContext) -> Result<(), TransportError> {
            Ok(())
        }
        fn name(&self) -> &str {
            &self.name
        }
        fn is_healthy(&self) -> bool {
            self.healthy
        }
    }

    fn make_identity() -> GdpGatewayIdentity {
        GdpGatewayIdentity::new(GatewayIdentity::new(
            [0x42u8; 32],
            1,
            GatewayClass::Edge,
            100,
        ))
    }

    #[test]
    fn build_advertisement_from_transport() {
        let discovery = TransportDiscovery::new(make_identity(), [0xABu8; 32], 100);
        let transport = NodeTransport::new(vec![
            Arc::new(MockSender {
                name: "webhook".into(),
                healthy: true,
            }) as Arc<dyn NetworkSender>,
            Arc::new(MockSender {
                name: "quic".into(),
                healthy: true,
            }) as Arc<dyn NetworkSender>,
            Arc::new(MockSender {
                name: "native-p2p".into(),
                healthy: true,
            }) as Arc<dyn NetworkSender>,
        ]);

        let adv = discovery.build_advertisement(&transport, 1, 1000);
        assert_eq!(adv.version, 1);
        assert_eq!(adv.gateway_id, discovery.identity().gateway_id());
        assert_eq!(adv.network_id, 1);
        assert_eq!(adv.sequence, 1);
        assert_eq!(adv.overlay_endpoints.len(), 3);
    }

    #[test]
    fn build_advertisement_skips_unhealthy() {
        let discovery = TransportDiscovery::new(make_identity(), [0xABu8; 32], 100);
        let transport = NodeTransport::new(vec![
            Arc::new(MockSender {
                name: "webhook".into(),
                healthy: true,
            }) as Arc<dyn NetworkSender>,
            Arc::new(MockSender {
                name: "quic".into(),
                healthy: false,
            }) as Arc<dyn NetworkSender>,
        ]);

        let adv = discovery.build_advertisement(&transport, 1, 1000);
        assert_eq!(adv.overlay_endpoints.len(), 1);
        assert_eq!(adv.overlay_endpoints[0].transport_type, PlatformType::Webhook as u16);
    }

    #[test]
    fn register_and_query_peer() {
        let discovery = TransportDiscovery::new(make_identity(), [0xABu8; 32], 100);
        let transport = NodeTransport::new(vec![
            Arc::new(MockSender {
                name: "webhook".into(),
                healthy: true,
            }) as Arc<dyn NetworkSender>,
        ]);

        let adv = discovery.build_advertisement(&transport, 1, 1000);
        discovery.register_peer(&adv, 1000);

        assert_eq!(discovery.peer_count(), 1);
        assert!(discovery.peer_supports_transport(&adv.gateway_id, PlatformType::Webhook as u16));
        assert!(!discovery.peer_supports_transport(&adv.gateway_id, PlatformType::Quic as u16));
    }

    #[test]
    fn peers_with_transport() {
        let discovery = TransportDiscovery::new(make_identity(), [0xABu8; 32], 100);

        let identity2 = GdpGatewayIdentity::new(GatewayIdentity::new(
            [0x99u8; 32],
            1,
            GatewayClass::Edge,
            200,
        ));
        let discovery2 = TransportDiscovery::new(identity2, [0xCDu8; 32], 100);

        let t1 = NodeTransport::new(vec![
            Arc::new(MockSender {
                name: "webhook".into(),
                healthy: true,
            }) as Arc<dyn NetworkSender>,
        ]);
        let t2 = NodeTransport::new(vec![
            Arc::new(MockSender {
                name: "quic".into(),
                healthy: true,
            }) as Arc<dyn NetworkSender>,
        ]);
        let adv1 = discovery.build_advertisement(&t1, 1, 1000);
        let adv2 = discovery2.build_advertisement(&t2, 1, 1001);
        discovery.register_peer(&adv1, 1000);
        discovery.register_peer(&adv2, 1001);

        let webhook_peers = discovery.peers_with_transport(PlatformType::Webhook as u16);
        assert_eq!(webhook_peers.len(), 1);
        assert_eq!(webhook_peers[0], adv1.gateway_id);

        let quic_peers = discovery.peers_with_transport(PlatformType::Quic as u16);
        assert_eq!(quic_peers.len(), 1);
        assert_eq!(quic_peers[0], adv2.gateway_id);
    }

    #[test]
    fn name_to_transport_type_mapping() {
        assert_eq!(
            name_to_transport_type("webhook"),
            PlatformType::Webhook as u16
        );
        assert_eq!(
            name_to_transport_type("quic"),
            PlatformType::Quic as u16
        );
        assert_eq!(
            name_to_transport_type("native-p2p"),
            PlatformType::NativeP2P as u16
        );
        assert_eq!(name_to_transport_type("unknown"), 0xFFFF);
    }

    #[test]
    fn sequence_increments() {
        let discovery = TransportDiscovery::new(make_identity(), [0xABu8; 32], 100);
        let transport = NodeTransport::new(vec![
            Arc::new(MockSender {
                name: "webhook".into(),
                healthy: true,
            }) as Arc<dyn NetworkSender>,
        ]);

        let adv1 = discovery.build_advertisement(&transport, 1, 1000);
        let adv2 = discovery.build_advertisement(&transport, 1, 1001);
        assert_eq!(adv1.sequence, 1);
        assert_eq!(adv2.sequence, 2);
    }
}
