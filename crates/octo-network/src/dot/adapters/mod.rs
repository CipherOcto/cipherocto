//! Platform adapter trait (RFC-0850 §8)

use async_trait::async_trait;

use crate::dot::domain::{BroadcastDomainId, PlatformType};
use crate::dot::envelope::DeterministicEnvelope;
use crate::dot::error::PlatformAdapterError;

pub mod abi;
pub mod backoff;
pub mod registry;
#[cfg(feature = "wasm")]
pub mod wasm_runtime;

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
    pub metadata: std::collections::BTreeMap<String, String>,
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
    /// Whether the platform carries raw binary payloads (native byte transport).
    /// Text-based platforms (chat apps) set this to `false` — they require
    /// DOT/1/{b64} or DOT/2/{msg_id} encoding. Native transports (P2P, WebRTC)
    /// set this to `true` — they send raw `to_wire_bytes()` directly.
    pub supports_raw_binary: bool,
    /// Rate limit (messages per second)
    pub rate_limit_per_second: u32,
    /// Media upload capabilities (None if not supported)
    pub media_capabilities: Option<MediaCapabilities>,
}

/// Media upload capabilities for platforms that support native file upload.
#[derive(Clone, Debug)]
pub struct MediaCapabilities {
    /// Maximum upload size in bytes
    pub max_upload_bytes: usize,
    /// Supported MIME types (empty = all)
    pub supported_mime_types: Vec<String>,
}

/// Trait for platform-specific transport adapters (RFC-0850 S8.2)
///
/// Each adapter bridges one or more broadcast domains into the DOT overlay.
///
/// **RFC method name mapping:** The implementation uses shorter names than the RFC:
/// - `capabilities()` -> RFC `validate_capabilities` (S8.2)
/// - `domain_id()` -> RFC `deterministic_domain_id` (S8.2)
/// - `platform_type()` -> RFC `replay_protection.platform_type` (S8.2)
/// - `replay_protection()` -> RFC `replay_protection.check` (S8.2, S11.2)
#[async_trait]
pub trait PlatformAdapter: Send + Sync {
    /// Send a deterministic envelope to the platform (RFC-0850 S8.2).
    async fn send_envelope(
        &self,
        domain: &BroadcastDomainId,
        envelope: &DeterministicEnvelope,
    ) -> Result<DeliveryReceipt, PlatformAdapterError>;

    /// Receive raw messages from the platform (RFC-0850 S8.2: `receive_envelope`).
    async fn receive_messages(
        &self,
        domain: &BroadcastDomainId,
    ) -> Result<Vec<RawPlatformMessage>, PlatformAdapterError>;

    /// Convert platform-specific message to canonical envelope (RFC-0850 S8.2).
    fn canonicalize(
        &self,
        raw: &RawPlatformMessage,
    ) -> Result<DeterministicEnvelope, PlatformAdapterError>;

    /// Report platform capabilities (RFC-0850 S8.2: `validate_capabilities`).
    fn capabilities(&self) -> CapabilityReport;

    /// Compute deterministic domain ID from platform-specific identifier (RFC-0850 S8.2: `deterministic_domain_id`).
    fn domain_id(&self, platform_id: &str) -> BroadcastDomainId;

    /// The platform type this adapter handles (RFC-0850 S8.2: `replay_protection.platform_type`).
    fn platform_type(&self) -> PlatformType;

    /// Check replay protection for an envelope (RFC-0850 S8.2, S11.2).
    /// Returns true if the envelope has NOT been seen before (is fresh).
    fn replay_protection(&self, _envelope_id: &[u8; 32]) -> bool {
        // Default: no replay protection at adapter level (handled by gateway)
        true
    }

    /// Health check — periodic liveness probe (RFC-0850 S8.4).
    async fn health_check(&self) -> Result<(), PlatformAdapterError> {
        Ok(())
    }

    /// Graceful shutdown — flush pending messages (RFC-0850 S8.4).
    async fn shutdown(&self) -> Result<(), PlatformAdapterError> {
        Ok(())
    }

    /// Return the bot's own handle/identity on this platform.
    ///
    /// Used by the gateway to drop self-authored messages and prevent
    /// relay loops (ZeroClaw pattern: `self_handle()` + `drop_self_messages()`).
    ///
    /// Returns `None` by default (no self-loop protection).
    /// Adapters that handle inbound traffic MUST override this.
    fn self_handle(&self) -> Option<String> {
        None
    }

    /// Upload media to the platform (RFC-0850 S8.6, dual-mode transport).
    ///
    /// Returns a platform-specific message ID that can be used to download
    /// the media later via `download_media()`. This enables the `DOT/2/{msg_id}`
    /// wire format for efficient large payload transport.
    ///
    /// Default: not supported (returns error).
    /// Platforms with `media_capabilities` in their `CapabilityReport` MUST override this.
    async fn upload_media(
        &self,
        _filename: &str,
        _data: &[u8],
        _mime_type: &str,
    ) -> Result<String, PlatformAdapterError> {
        Err(PlatformAdapterError::Unreachable {
            platform: "unknown".into(),
            reason: "media upload not supported by this adapter".into(),
        })
    }

    /// Download media from the platform using a message ID (RFC-0850 S8.6).
    ///
    /// Returns the raw bytes of the uploaded media. Used by receivers to
    /// retrieve envelopes sent via `DOT/2/{msg_id}` wire format.
    ///
    /// Default: not supported (returns error).
    /// Platforms with `media_capabilities` in their `CapabilityReport` MUST override this.
    async fn download_media(&self, _message_id: &str) -> Result<Vec<u8>, PlatformAdapterError> {
        Err(PlatformAdapterError::Unreachable {
            platform: "unknown".into(),
            reason: "media download not supported by this adapter".into(),
        })
    }
}
