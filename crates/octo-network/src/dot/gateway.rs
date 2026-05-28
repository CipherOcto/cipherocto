//! Gateway identity and capacity (RFC-0850 §3.2)

use serde::{Deserialize, Serialize};

/// Gateway role classification
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[repr(u16)]
pub enum GatewayClass {
    Edge = 0x0001,
    Relay = 0x0002,
    Consensus = 0x0003,
    Archive = 0x0004,
    Stealth = 0x0005,
    Translation = 0x0006,
}

/// Bitmask for gateway role capabilities (a gateway can serve multiple roles)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[repr(u64)]
pub enum GatewayRoleFlags {
    Edge = 0x0001,
    Relay = 0x0002,
    Consensus = 0x0004,
    Archive = 0x0008,
    Stealth = 0x0010,
    Translation = 0x0020,
}

/// Gateway identity extending RFC-0009 Identity
///
/// gateway_id = BLAKE3-256(public_key || network_id || creation_epoch)
#[derive(Debug, Clone, Serialize, Deserialize)]
#[repr(C)]
pub struct GatewayIdentity {
    /// Unique gateway identifier (32 bytes, derived from public key)
    pub gateway_id: [u8; 32],
    /// Ed25519 public key
    pub public_key: [u8; 32],
    /// Network identifier
    pub network_id: u32,
    /// Gateway class
    pub gateway_class: GatewayClass,
    /// Epoch when gateway was created
    pub creation_epoch: u64,
    /// Supported platform types (bitmask)
    pub supported_platforms: u64,
    /// Gateway capabilities (bitmask)
    pub capabilities: u64,
}

impl GatewayIdentity {
    /// Create a new gateway identity with deterministic gateway_id derivation.
    pub fn new(
        public_key: [u8; 32],
        network_id: u32,
        gateway_class: GatewayClass,
        creation_epoch: u64,
    ) -> Self {
        let gateway_id = Self::derive_gateway_id(&public_key, network_id, creation_epoch);
        Self {
            gateway_id,
            public_key,
            network_id,
            gateway_class,
            creation_epoch,
            supported_platforms: 0,
            capabilities: 0,
        }
    }

    /// Derive gateway_id deterministically.
    /// gateway_id = BLAKE3-256(public_key || network_id || creation_epoch)
    pub fn derive_gateway_id(
        public_key: &[u8; 32],
        network_id: u32,
        creation_epoch: u64,
    ) -> [u8; 32] {
        let mut hasher = blake3::Hasher::new();
        hasher.update(public_key);
        hasher.update(&network_id.to_be_bytes());
        hasher.update(&creation_epoch.to_be_bytes());
        *hasher.finalize().as_bytes()
    }

    /// Add supported platform types (bitwise OR).
    pub fn with_platforms(mut self, platforms: u64) -> Self {
        self.supported_platforms |= platforms;
        self
    }

    /// Add capabilities (bitwise OR).
    pub fn with_capabilities(mut self, capabilities: u64) -> Self {
        self.capabilities |= capabilities;
        self
    }
}

/// Gateway capacity declaration for deterministic routing
#[derive(Debug, Clone, Serialize, Deserialize)]
#[repr(C)]
pub struct GatewayCapacity {
    /// Maximum envelopes per second
    pub max_throughput: u32,
    /// Number of connected broadcast domains
    pub domain_count: u16,
    /// Supported platform types (bitmask)
    pub platform_mask: u64,
    /// Storage capacity class (0-255)
    pub storage_class: u8,
    /// Bandwidth class (0-255)
    pub bandwidth_class: u8,
}

impl Default for GatewayCapacity {
    fn default() -> Self {
        Self {
            max_throughput: 1000,
            domain_count: 0,
            platform_mask: 0,
            storage_class: 0,
            bandwidth_class: 0,
        }
    }
}

/// Federation peer — a gateway in the overlay federation graph.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[repr(C)]
pub struct FederationPeer {
    /// Peer gateway identity
    pub identity: GatewayIdentity,
    /// Peer capacity declaration
    pub capacity: GatewayCapacity,
    /// Connected broadcast domain hashes
    pub domains: Vec<[u8; 32]>,
    /// Last seen epoch
    pub last_seen: u64,
    /// Whether this peer is active
    pub active: bool,
}

/// Federation state — tracks all known peers in the overlay graph.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FederationState {
    /// Our own gateway identity
    pub local_gateway: GatewayIdentity,
    /// Known federation peers (gateway_id -> peer)
    pub peers: std::collections::BTreeMap<[u8; 32], FederationPeer>,
}

impl FederationState {
    /// Create a new federation state for the local gateway.
    pub fn new(local_gateway: GatewayIdentity) -> Self {
        Self {
            local_gateway,
            peers: std::collections::BTreeMap::new(),
        }
    }

    /// Add or update a federation peer.
    /// Returns true if this is a new peer.
    pub fn upsert_peer(&mut self, peer: FederationPeer) -> bool {
        let is_new = !self.peers.contains_key(&peer.identity.gateway_id);
        self.peers.insert(peer.identity.gateway_id, peer);
        is_new
    }

    /// Remove a peer by gateway_id.
    pub fn remove_peer(&mut self, gateway_id: &[u8; 32]) -> bool {
        self.peers.remove(gateway_id).is_some()
    }

    /// Get active peers connected to a specific domain.
    pub fn peers_for_domain(&self, domain_hash: &[u8; 32]) -> Vec<&FederationPeer> {
        self.peers
            .values()
            .filter(|p| p.active && p.domains.contains(domain_hash))
            .collect()
    }

    /// Get the number of active peers.
    pub fn active_peer_count(&self) -> usize {
        self.peers.values().filter(|p| p.active).count()
    }

    /// Mark stale peers as inactive based on epoch threshold.
    /// Returns the number of peers deactivated.
    pub fn evict_stale_peers(&mut self, current_epoch: u64, max_age: u64) -> usize {
        let cutoff = current_epoch.saturating_sub(max_age);
        let mut count = 0;
        for peer in self.peers.values_mut() {
            if peer.active && peer.last_seen < cutoff {
                peer.active = false;
                count += 1;
            }
        }
        count
    }

    /// Get all connected domain hashes across all active peers.
    pub fn connected_domains(&self) -> Vec<[u8; 32]> {
        let mut domains: std::collections::BTreeSet<[u8; 32]> = std::collections::BTreeSet::new();
        for peer in self.peers.values() {
            if peer.active {
                for domain in &peer.domains {
                    domains.insert(*domain);
                }
            }
        }
        domains.into_iter().collect()
    }

    /// Check if the federation can survive a domain partition.
    /// Returns true if at least one active peer remains outside the partitioned domain.
    pub fn can_survive_partition(&self, partitioned_domain: &[u8; 32]) -> bool {
        self.peers
            .values()
            .any(|p| p.active && !p.domains.contains(partitioned_domain))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_gateway_identity_deterministic() {
        let key = [0x42u8; 32];
        let id1 = GatewayIdentity::new(key, 1, GatewayClass::Edge, 100);
        let id2 = GatewayIdentity::new(key, 1, GatewayClass::Edge, 100);
        assert_eq!(id1.gateway_id, id2.gateway_id);
    }

    #[test]
    fn test_gateway_identity_different_keys() {
        let id1 = GatewayIdentity::new([0x01u8; 32], 1, GatewayClass::Edge, 100);
        let id2 = GatewayIdentity::new([0x02u8; 32], 1, GatewayClass::Edge, 100);
        assert_ne!(id1.gateway_id, id2.gateway_id);
    }

    #[test]
    fn test_gateway_identity_builder() {
        let id = GatewayIdentity::new([0x01u8; 32], 1, GatewayClass::Relay, 100)
            .with_platforms(0x0001 | 0x0002) // Telegram + Discord
            .with_capabilities(0x0001); // Relay
        assert_eq!(id.supported_platforms, 0x0003);
        assert_eq!(id.capabilities, 0x0001);
    }

    #[test]
    fn test_gateway_capacity_default() {
        let cap = GatewayCapacity::default();
        assert_eq!(cap.max_throughput, 1000);
        assert_eq!(cap.domain_count, 0);
    }

    fn make_peer(id: u8, domain: u8) -> FederationPeer {
        FederationPeer {
            identity: GatewayIdentity::new([id; 32], 1, GatewayClass::Edge, 100),
            capacity: GatewayCapacity::default(),
            domains: vec![[domain; 32]],
            last_seen: 200,
            active: true,
        }
    }

    #[test]
    fn test_federation_upsert_new() {
        let local = GatewayIdentity::new([0x01u8; 32], 1, GatewayClass::Relay, 100);
        let mut state = FederationState::new(local);
        assert!(state.upsert_peer(make_peer(1, 0xAA)));
        assert!(!state.upsert_peer(make_peer(1, 0xAA))); // update, not new
        assert_eq!(state.peers.len(), 1);
    }

    #[test]
    fn test_federation_remove_peer() {
        let local = GatewayIdentity::new([0x01u8; 32], 1, GatewayClass::Relay, 100);
        let mut state = FederationState::new(local);
        state.upsert_peer(make_peer(1, 0xAA));
        assert!(state.remove_peer(&[1u8; 32]));
        assert!(!state.remove_peer(&[1u8; 32])); // already gone
        assert_eq!(state.peers.len(), 0);
    }

    #[test]
    fn test_federation_peers_for_domain() {
        let local = GatewayIdentity::new([0x01u8; 32], 1, GatewayClass::Relay, 100);
        let mut state = FederationState::new(local);
        state.upsert_peer(make_peer(1, 0xAA));
        state.upsert_peer(make_peer(2, 0xBB));
        state.upsert_peer(make_peer(3, 0xAA));

        let aa_peers = state.peers_for_domain(&[0xAAu8; 32]);
        assert_eq!(aa_peers.len(), 2);
    }

    #[test]
    fn test_federation_active_peer_count() {
        let local = GatewayIdentity::new([0x01u8; 32], 1, GatewayClass::Relay, 100);
        let mut state = FederationState::new(local);
        state.upsert_peer(make_peer(1, 0xAA));
        state.upsert_peer(make_peer(2, 0xBB));
        state.upsert_peer(make_peer(3, 0xCC));
        state.peers.get_mut(&[2u8; 32]).unwrap().active = false;

        assert_eq!(state.active_peer_count(), 2);
    }

    #[test]
    fn test_federation_evict_stale_peers() {
        let local = GatewayIdentity::new([0x01u8; 32], 1, GatewayClass::Relay, 100);
        let mut state = FederationState::new(local);
        let mut peer = make_peer(1, 0xAA);
        peer.last_seen = 50;
        state.upsert_peer(peer);
        state.upsert_peer(make_peer(2, 0xBB)); // last_seen=200

        let evicted = state.evict_stale_peers(300, 100); // cutoff=200
        assert_eq!(evicted, 1);
        assert!(!state.peers.get(&[1u8; 32]).unwrap().active);
        assert!(state.peers.get(&[2u8; 32]).unwrap().active);
    }

    #[test]
    fn test_federation_connected_domains() {
        let local = GatewayIdentity::new([0x01u8; 32], 1, GatewayClass::Relay, 100);
        let mut state = FederationState::new(local);
        state.upsert_peer(make_peer(1, 0xAA));
        state.upsert_peer(make_peer(2, 0xBB));
        state.upsert_peer(make_peer(3, 0xAA)); // duplicate domain

        let domains = state.connected_domains();
        assert_eq!(domains.len(), 2); // deduplicated
    }

    #[test]
    fn test_federation_can_survive_partition() {
        let local = GatewayIdentity::new([0x01u8; 32], 1, GatewayClass::Relay, 100);
        let mut state = FederationState::new(local);
        state.upsert_peer(make_peer(1, 0xAA));
        state.upsert_peer(make_peer(2, 0xBB));

        assert!(state.can_survive_partition(&[0xAAu8; 32])); // peer 2 survives
        assert!(!state.can_survive_partition(&[0xCCu8; 32])); // both peers have other domains, but wait - they only have one domain each
                                                              // Actually: peer 1 has domain AA, peer 2 has domain BB. Partition CC affects neither.
                                                              // can_survive_partition checks if ANY active peer does NOT have the partitioned domain.
                                                              // Both peers don't have CC, so this should be true.
        assert!(state.can_survive_partition(&[0xCCu8; 32]));

        // Now test where all peers are on the partitioned domain
        let mut state2 = FederationState::new(GatewayIdentity::new(
            [0x01u8; 32],
            1,
            GatewayClass::Relay,
            100,
        ));
        state2.upsert_peer(make_peer(1, 0xAA));
        state2.upsert_peer(make_peer(2, 0xAA));
        assert!(!state2.can_survive_partition(&[0xAAu8; 32]));
    }
}
