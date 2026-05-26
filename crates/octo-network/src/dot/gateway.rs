//! Gateway identity and capacity (RFC-0850 §3.2)

/// Gateway role classification
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
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
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
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
#[derive(Clone, Debug)]
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
#[derive(Clone, Debug)]
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
}
