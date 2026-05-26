//! Deterministic Overlay Transport (DOT) — RFC-0850
//!
//! Transforms existing communication platforms into deterministic transport
//! substrates for decentralized consensus.

pub mod adapters;
pub mod config;
pub mod domain;
pub mod envelope;
pub mod error;
pub mod gateway;
pub mod replay;
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
        // 1. Verify envelope_id derivation (Class A)
        let expected_id = envelope.derive_envelope_id();
        if envelope.envelope_id != expected_id {
            return Err(DotError::PayloadHashMismatch {
                expected: expected_id,
                actual: envelope.envelope_id,
            });
        }

        // 2. Verify signature against source peer's key (Class A)
        envelope.verify(source_peer_key)?;

        // 3. Check replay cache (Class A)
        let mut cache = self.replay_cache.write().await;
        cache.check_and_insert(envelope.envelope_id, current_epoch)?;

        // 4. Forward to all adapters (Class C — transport-dependent)
        // Note: In production, this would iterate over connected domains
        // and forward to the appropriate adapter(s).

        Ok(ProcessingResult::Forwarded)
    }
}
