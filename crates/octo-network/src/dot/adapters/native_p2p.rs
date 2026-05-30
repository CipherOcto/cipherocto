//! Native P2P adapter (RFC-0850 §3.1, PlatformType::NativeP2P)

use async_trait::async_trait;

use crate::dot::adapters::{
    CapabilityReport, DeliveryReceipt, PlatformAdapter, RawPlatformMessage,
};
use crate::dot::domain::{BroadcastDomainId, PlatformType};
use crate::dot::envelope::DeterministicEnvelope;
use crate::dot::error::PlatformAdapterError;

/// Native P2P adapter using libp2p gossipsub.
///
/// This is the preferred DOT transport — lowest latency, highest reliability,
/// no platform API limits.
pub struct NativeP2PAdapter {
    listen_addr: String,
}

impl NativeP2PAdapter {
    /// Create a new NativeP2P adapter.
    pub fn new(listen_addr: String) -> Self {
        Self { listen_addr }
    }

    /// Get the configured listen address.
    pub fn listen_addr(&self) -> &str {
        &self.listen_addr
    }
}

#[async_trait]
impl PlatformAdapter for NativeP2PAdapter {
    async fn send_envelope(
        &self,
        _domain: &BroadcastDomainId,
        _envelope: &DeterministicEnvelope,
    ) -> Result<DeliveryReceipt, PlatformAdapterError> {
        // TODO: Implement libp2p gossipsub publish
        Err(PlatformAdapterError::ApiError {
            code: 501,
            message: "NativeP2P send_envelope not yet implemented".to_string(),
        })
    }

    async fn receive_messages(
        &self,
        _domain: &BroadcastDomainId,
    ) -> Result<Vec<RawPlatformMessage>, PlatformAdapterError> {
        // TODO: Implement libp2p gossipsub subscribe
        Ok(vec![])
    }

    fn canonicalize(
        &self,
        _raw: &RawPlatformMessage,
    ) -> Result<DeterministicEnvelope, PlatformAdapterError> {
        Err(PlatformAdapterError::ApiError {
            code: 501,
            message: "NativeP2P canonicalize not yet implemented".to_string(),
        })
    }

    fn capabilities(&self) -> CapabilityReport {
        CapabilityReport {
            max_payload_bytes: 65536,
            supports_fragmentation: true,
            supports_encryption: true,
            rate_limit_per_second: 10000,
            media_capabilities: None,
        }
    }

    fn domain_id(&self, platform_id: &str) -> BroadcastDomainId {
        BroadcastDomainId::new(PlatformType::NativeP2P, platform_id)
    }

    fn platform_type(&self) -> PlatformType {
        PlatformType::NativeP2P
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_native_p2p_capabilities() {
        let adapter = NativeP2PAdapter::new("/ip4/0.0.0.0/tcp/4001".to_string());
        let caps = adapter.capabilities();
        assert_eq!(caps.max_payload_bytes, 65536);
        assert!(caps.supports_fragmentation);
        assert!(caps.supports_encryption);
    }

    #[test]
    fn test_native_p2p_domain_id() {
        let adapter = NativeP2PAdapter::new("/ip4/0.0.0.0/tcp/4001".to_string());
        let domain = adapter.domain_id("test-topic");
        assert_eq!(domain.platform_type, PlatformType::NativeP2P as u16);
    }

    #[test]
    fn test_native_p2p_listen_addr() {
        let adapter = NativeP2PAdapter::new("/ip4/0.0.0.0/tcp/4001".to_string());
        assert_eq!(adapter.listen_addr(), "/ip4/0.0.0.0/tcp/4001");
    }
}
