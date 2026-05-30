//! GDP Gateway Identity (RFC-0851 §1)
//!
//! Extends DOT's GatewayIdentity with GDP-specific fields.

use crate::dot::gateway::GatewayIdentity;
use serde::{Deserialize, Serialize};

/// GDP-specific gateway identity wrapping DOT's GatewayIdentity.
///
/// Delegates `supported_platforms` and `capabilities` to the base
/// GatewayIdentity to avoid field duplication.
///
/// gateway_id = BLAKE3-256(public_key || network_id || creation_epoch)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GdpGatewayIdentity {
    /// Base DOT gateway identity (contains supported_platforms and capabilities)
    pub base: GatewayIdentity,
}

impl GdpGatewayIdentity {
    pub fn new(base: GatewayIdentity) -> Self {
        Self { base }
    }

    pub fn with_platforms(mut self, platforms: u64) -> Self {
        self.base.supported_platforms |= platforms;
        self
    }

    pub fn with_capabilities(mut self, capabilities: u64) -> Self {
        self.base.capabilities |= capabilities;
        self
    }

    pub fn gateway_id(&self) -> [u8; 32] {
        self.base.gateway_id
    }

    pub fn supported_platforms(&self) -> u64 {
        self.base.supported_platforms
    }

    pub fn capabilities(&self) -> u64 {
        self.base.capabilities
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dot::gateway::GatewayClass;

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
        assert_eq!(gdp.supported_platforms(), 0x0003);
        assert_eq!(gdp.capabilities(), 0x0005);
    }
}
