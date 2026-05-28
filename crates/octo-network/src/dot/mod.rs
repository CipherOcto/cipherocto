//! Deterministic Overlay Transport (DOT) — RFC-0850
//!
//! Transforms existing communication platforms into deterministic transport
//! substrates for decentralized consensus.

pub mod adapters;
pub mod config;
pub mod domain;
pub mod envelope;
pub mod error;
pub mod fragment;
pub mod gateway;
pub mod pce;
pub mod replay;
pub mod route;
pub mod sequence;

pub use adapters::native_p2p::NativeP2PAdapter;
pub use adapters::{CapabilityReport, DeliveryReceipt, PlatformAdapter, RawPlatformMessage};
pub use config::DotConfig;
pub use domain::{BroadcastDomainId, PlatformType};
pub use envelope::{DeterministicEnvelope, MessageType};
pub use error::{DotError, PlatformAdapterError};
pub use gateway::{GatewayCapacity, GatewayClass, GatewayIdentity, GatewayRoleFlags};
pub use replay::ReplayCache;
pub use sequence::OverlaySequence;

use tokio::sync::RwLock;

const DOT_PROTOCOL_VERSION: u16 = 1;

/// DOT Gateway — extends Network with overlay transport capabilities
///
/// Wraps the existing Network with DOT-specific functionality:
/// - Envelope verification (signature + id)
/// - Replay cache
/// - Multi-platform forwarding
pub struct DotGateway {
    identity: GatewayIdentity,
    adapters: Vec<Box<dyn PlatformAdapter>>,
    replay_cache: RwLock<ReplayCache>,
    config: DotConfig,
}

/// Result of processing an envelope
#[derive(Debug)]
pub enum ProcessingResult {
    /// Envelope was forwarded to all adapters
    Forwarded,
    /// Envelope was dropped (replay, expired, etc.)
    Dropped(String),
}

impl DotGateway {
    /// Create a new DotGateway with the given identity and config.
    pub fn new(identity: GatewayIdentity, config: DotConfig) -> Self {
        let replay_cache = ReplayCache::new(
            config.replay_cache.window_duration_secs,
            config.replay_cache.max_entries,
        );
        Self {
            identity,
            adapters: Vec::new(),
            replay_cache: RwLock::new(replay_cache),
            config,
        }
    }

    /// Register a platform adapter.
    pub fn add_adapter(&mut self, adapter: Box<dyn PlatformAdapter>) {
        self.adapters.push(adapter);
    }

    /// Get the gateway identity.
    pub fn identity(&self) -> &GatewayIdentity {
        &self.identity
    }

    /// Get the gateway configuration.
    pub fn config(&self) -> &DotConfig {
        &self.config
    }

    /// Process an incoming envelope through the deterministic pipeline:
    ///
    /// 0. Validate protocol version (fail-fast)
    /// 1. Verify envelope_id derivation (Class A)
    /// 2. Verify signature against source peer's public key (Class A)
    /// 3. Check replay cache (Class A)
    /// 4. Forward to all adapters
    ///
    /// The `source_peer_key` is the public key of the envelope's source peer,
    /// resolved from the identity registry (RFC-0009). The gateway verifies
    /// the envelope was signed by its actual source, NOT by itself.
    pub async fn process_envelope(
        &self,
        envelope: &DeterministicEnvelope,
        source_peer_key: &[u8; 32],
        current_epoch: u64,
    ) -> Result<ProcessingResult, DotError> {
        // 0. Version validation — RFC MUST: reject unsupported versions
        if envelope.version != DOT_PROTOCOL_VERSION {
            return Err(DotError::UnsupportedVersion {
                version: envelope.version,
            });
        }

        // 1. Verify envelope_id derivation (Class A)
        envelope.verify(source_peer_key)?;

        // 2. Check replay cache (Class A)
        let mut cache = self.replay_cache.write().await;
        cache.check_and_insert(envelope.envelope_id, current_epoch)?;

        // 3. Forward to all adapters (Class C — transport-dependent)
        // Note: In production, this would iterate over connected domains
        // and forward to the appropriate adapter(s).

        Ok(ProcessingResult::Forwarded)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ed25519_dalek::{Signer, SigningKey};

    fn test_gateway() -> (DotGateway, SigningKey) {
        let signing_key = SigningKey::from_bytes(&[42u8; 32]);
        let identity = GatewayIdentity::new(
            signing_key.verifying_key().to_bytes(),
            1,
            GatewayClass::Edge,
            0,
        );
        let config = DotConfig::default();
        let gateway = DotGateway::new(identity, config);
        (gateway, signing_key)
    }

    fn sign_envelope(envelope: &mut DeterministicEnvelope, signing_key: &SigningKey) {
        envelope.envelope_id = envelope.derive_envelope_id();
        let signing_bytes = envelope.to_signing_bytes();
        envelope.signature = signing_key.sign(&signing_bytes).to_bytes();
    }

    #[tokio::test]
    async fn test_process_envelope_valid() {
        let (gateway, signing_key) = test_gateway();
        let mut envelope = DeterministicEnvelope {
            version: 1,
            network_id: 1,
            message_type: MessageType::Message as u16,
            envelope_id: [0u8; 32],
            mission_id: [0u8; 32],
            source_peer: [1u8; 32],
            origin_gateway: [2u8; 32],
            logical_timestamp: 1000,
            ttl_hops: 10,
            payload_hash: *blake3::hash(b"test").as_bytes(),
            route_trace_root: [0u8; 32],
            flags: 0,
            signature: [0u8; 64],
        };
        sign_envelope(&mut envelope, &signing_key);

        let result = gateway
            .process_envelope(&envelope, &signing_key.verifying_key().to_bytes(), 100)
            .await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_process_envelope_rejects_unsupported_version() {
        let (gateway, signing_key) = test_gateway();
        let mut envelope = DeterministicEnvelope {
            version: 99, // unsupported
            network_id: 1,
            message_type: MessageType::Message as u16,
            envelope_id: [0u8; 32],
            mission_id: [0u8; 32],
            source_peer: [1u8; 32],
            origin_gateway: [2u8; 32],
            logical_timestamp: 1000,
            ttl_hops: 10,
            payload_hash: *blake3::hash(b"test").as_bytes(),
            route_trace_root: [0u8; 32],
            flags: 0,
            signature: [0u8; 64],
        };
        sign_envelope(&mut envelope, &signing_key);

        let result = gateway
            .process_envelope(&envelope, &signing_key.verifying_key().to_bytes(), 100)
            .await;
        assert!(matches!(
            result,
            Err(DotError::UnsupportedVersion { version: 99 })
        ));
    }

    #[tokio::test]
    async fn test_process_envelope_rejects_bad_signature() {
        let (gateway, signing_key) = test_gateway();
        let mut envelope = DeterministicEnvelope {
            version: 1,
            network_id: 1,
            message_type: MessageType::Message as u16,
            envelope_id: [0u8; 32],
            mission_id: [0u8; 32],
            source_peer: [1u8; 32],
            origin_gateway: [2u8; 32],
            logical_timestamp: 1000,
            ttl_hops: 10,
            payload_hash: *blake3::hash(b"test").as_bytes(),
            route_trace_root: [0u8; 32],
            flags: 0,
            signature: [0u8; 64],
        };
        sign_envelope(&mut envelope, &signing_key);

        // Verify with a different valid key
        let other_key = SigningKey::from_bytes(&[99u8; 32]);
        let wrong_pubkey = other_key.verifying_key().to_bytes();
        let result = gateway
            .process_envelope(&envelope, &wrong_pubkey, 100)
            .await;
        assert!(matches!(result, Err(DotError::InvalidSignature { .. })));
    }

    #[tokio::test]
    async fn test_process_envelope_replay_detection() {
        let (gateway, signing_key) = test_gateway();
        let mut envelope = DeterministicEnvelope {
            version: 1,
            network_id: 1,
            message_type: MessageType::Message as u16,
            envelope_id: [0u8; 32],
            mission_id: [0u8; 32],
            source_peer: [1u8; 32],
            origin_gateway: [2u8; 32],
            logical_timestamp: 1000,
            ttl_hops: 10,
            payload_hash: *blake3::hash(b"test").as_bytes(),
            route_trace_root: [0u8; 32],
            flags: 0,
            signature: [0u8; 64],
        };
        let peer_key = signing_key.verifying_key().to_bytes();
        sign_envelope(&mut envelope, &signing_key);

        // First insert succeeds
        assert!(gateway
            .process_envelope(&envelope, &peer_key, 100)
            .await
            .is_ok());
        // Second insert is a replay
        assert!(matches!(
            gateway.process_envelope(&envelope, &peer_key, 101).await,
            Err(DotError::ReplayDetected { .. })
        ));
    }
}
