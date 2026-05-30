//! Core GDP types — DiscoveryScope, GatewayCapability, StakeRequirement, etc.

use serde::{Deserialize, Serialize};

/// Discovery scope for gateway visibility (RFC-0851 §2)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[repr(u16)]
pub enum DiscoveryScope {
    Local = 0x0001,
    Regional = 0x0002,
    Mission = 0x0003,
    Global = 0x0004,
    Private = 0x0005,
    Consensus = 0x0006,
}

/// Gateway capability bitmask (RFC-0851 §5)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[repr(u64)]
pub enum GatewayCapability {
    // Base capabilities (RFC-0850 GatewayRoleFlags)
    Edge = 0x0001,
    Relay = 0x0002,
    Consensus = 0x0004,
    Archive = 0x0008,
    Stealth = 0x0010,
    Translation = 0x0020,
    // GDP-specific extensions
    Storage = 0x0040,
    OnionRelay = 0x0080,
    AIExecution = 0x0100,
    VectorIndex = 0x0200,
    ZkVerification = 0x0400,
    MissionCoordinator = 0x0800,
}

/// Discovery lifecycle states (RFC-0851 §8)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[repr(u16)]
pub enum DiscoveryLifecycle {
    Bootstrap = 0x0001,
    Expansion = 0x0002,
    Stabilization = 0x0003,
    Degraded = 0x0004,
    Recovering = 0x0005,
}

/// Stake requirement for a discovery scope (RFC-0851 §11.1)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StakeRequirement {
    pub scope: DiscoveryScope,
    pub min_octo_global: u64,
    pub min_octo_b_role: u64,
}

/// Advertisement expiration (RFC-0851 §4)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AdvertisementExpiration {
    pub logical_timestamp: u64,
    pub ttl_epochs: u64,
    pub scope: DiscoveryScope,
}

impl AdvertisementExpiration {
    pub fn is_expired(&self, current_epoch: u64) -> bool {
        current_epoch > self.logical_timestamp.saturating_add(self.ttl_epochs)
    }
}
