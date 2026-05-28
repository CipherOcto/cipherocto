//! GDP Gateway Identity (RFC-0851 §1)
//!
//! Extends DOT's GatewayIdentity with GDP-specific fields.

use crate::dot::gateway::{GatewayClass, GatewayIdentity};
use serde::{Deserialize, Serialize};

/// GDP-specific gateway identity wrapping DOT's GatewayIdentity.
///
/// gateway_id = BLAKE3-256(public_key || network_id || creation_epoch)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GdpGatewayIdentity {
    /// Base DOT gateway identity
    pub base: GatewayIdentity,
    /// Supported platform types (bitmask)
    pub supported_platforms: u64,
    /// Gateway capabilities (bitmask)
    pub capabilities: u64,
}

impl GdpGatewayIdentity {
    pub fn new(base: GatewayIdentity) -> Self {
        Self {
            base,
            supported_platforms: 0,
            capabilities: 0,
        }
    }

    pub fn with_platforms(mut self, platforms: u64) -> Self {
        self.supported_platforms |= platforms;
        self
    }

    pub fn with_capabilities(mut self, capabilities: u64) -> Self {
        self.capabilities |= capabilities;
        self
    }

    pub fn gateway_id(&self) -> [u8; 32] {
        self.base.gateway_id
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_base_identity() -> GatewayIdentity {
        GatewayIdentity::new([0x42u8; 32], 1, GatewayClass::Edge, 100)
    }

    #[test]
    fn test_gdp_identity_wraps_dot() {
        let base = test_base_identity();
        let gdp = GdpGatewayIdentity::new(base.clone());
        assert_eq!(gdp.gateway_id(), base.gateway_id);
    }

    #[test]
    fn test_gdp_identity_builder() {
        let base = test_base_identity();
        let gdp = GdpGatewayIdentity::new(base)
            .with_platforms(0x0001 | 0x0002)
            .with_capabilities(0x0001 | 0x0004);
        assert_eq!(gdp.supported_platforms, 0x0003);
        assert_eq!(gdp.capabilities, 0x0005);
    }
}
