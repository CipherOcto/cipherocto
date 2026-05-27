//! Platform adapter trait (RFC-0850 §8)

use async_trait::async_trait;

use crate::dot::domain::{BroadcastDomainId, PlatformType};
use crate::dot::envelope::DeterministicEnvelope;
use crate::dot::error::PlatformAdapterError;

pub mod native_p2p;

/// Result of delivering an envelope to a platform
#[derive(Clone, Debug)]
pub struct DeliveryReceipt {
    /// Platform-specific message identifier
    pub platform_message_id: String,
    /// Epoch when delivery was confirmed
    pub delivered_at: u64,
}

/// Raw message received from a platform
#[derive(Clone, Debug)]
pub struct RawPlatformMessage {
    /// Platform-specific message identifier
    pub platform_id: String,
    /// Raw payload bytes
    pub payload: Vec<u8>,
    /// Platform-specific metadata (opaque to DOT)
    pub metadata: std::collections::HashMap<String, String>,
}

/// Platform capabilities report
#[derive(Clone, Debug)]
pub struct CapabilityReport {
    /// Maximum payload bytes for this platform
    pub max_payload_bytes: usize,
    /// Whether the platform supports message fragmentation
    pub supports_fragmentation: bool,
    /// Whether the platform supports encryption
    pub supports_encryption: bool,
    /// Rate limit (messages per second)
    pub rate_limit_per_second: u32,
}

/// Trait for platform-specific transport adapters
///
/// Each adapter bridges one or more broadcast domains into the DOT overlay.
#[async_trait]
pub trait PlatformAdapter: Send + Sync {
    /// Send a deterministic envelope to the platform.
    async fn send_envelope(
        &self,
        domain: &BroadcastDomainId,
        envelope: &DeterministicEnvelope,
    ) -> Result<DeliveryReceipt, PlatformAdapterError>;

    /// Receive raw messages from the platform.
    async fn receive_messages(
        &self,
        domain: &BroadcastDomainId,
    ) -> Result<Vec<RawPlatformMessage>, PlatformAdapterError>;

    /// Convert platform-specific message to canonical envelope.
    fn canonicalize(
        &self,
        raw: &RawPlatformMessage,
    ) -> Result<DeterministicEnvelope, PlatformAdapterError>;

    /// Report platform capabilities.
    fn capabilities(&self) -> CapabilityReport;

    /// Compute deterministic domain ID from platform-specific identifier.
    fn domain_id(&self, platform_id: &str) -> BroadcastDomainId;

    /// The platform type this adapter handles.
    fn platform_type(&self) -> PlatformType;
}
