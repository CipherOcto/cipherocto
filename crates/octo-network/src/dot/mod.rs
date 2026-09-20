//! Deterministic Overlay Transport (DOT) — RFC-0850
//!
//! Transforms existing communication platforms into deterministic transport
//! substrates for decentralized consensus.

pub mod adapters;
pub mod audit_store;
pub mod binding;
pub mod config;
pub mod dc;
pub mod dc_envelopes;
pub mod decommission;
pub mod domain;
pub mod envelope;
pub mod error;
pub mod fragment;
pub mod gateway;
pub mod group_registry;
pub mod handover;
pub mod kick_envelopes;
pub mod pce;
pub mod replay;
pub mod route;
pub mod sequence;
pub mod slash;
pub mod sub_group;
pub mod subgroup_delegation;
pub mod subgroup_routing;
pub mod subgroup_state;
pub mod subgroup_teardown;
pub mod transport;
pub mod witness;

pub use adapters::{
    coordinator_admin::{
        dispatch_coordinator_admin_action, AdminCapabilityReport, CoordinatorAdmin,
        CoordinatorAdminAction, CoordinatorAdminActionError, GroupHandle, GroupId, GroupMemberSpec,
        GroupMetadata, GroupModeFlags, InviteRef, PeerId,
    },
    CapabilityReport, DeliveryReceipt, PlatformAdapter, RawPlatformMessage,
};
pub use binding::{
    BindAck, BindEnvelope, BindingError, GroupBinding, GroupState, GroupVisibility,
    PlatformLossEnvelope, RebindEnvelope, UnbindAuthority, UnbindEnvelope, WitnessAssertion,
    ENVELOPE_TYPE as BINDING_ENVELOPE_TYPE, ENVELOPE_VERSION as BINDING_ENVELOPE_VERSION,
};
pub use config::DotConfig;
pub use dc::{DcConfig, DcOrchestrator, KickDecision, RaceOutcome};
pub use dc_envelopes::{
    CreateGroupAckEnvelope, CreateGroupDoneEnvelope, CreateGroupEnvelope, CreateGroupFailEnvelope,
    InviteEnvelope, UnbindAllAckEnvelope, UnbindAllEnvelope, UnbindReason,
};
pub use decommission::{AuditEntry, AuditLog, UnbindAllAuditEnvelope, UnbindAllDoneEnvelope};
pub use domain::{BroadcastDomainId, PlatformType};
pub use envelope::{DeterministicEnvelope, MessageType};
pub use error::{DotError, PlatformAdapterError};
pub use gateway::{GatewayCapacity, GatewayClass, GatewayIdentity, GatewayRoleFlags};
pub use group_registry::{
    GroupRegistry, UnboundQuarantineEntry, UnboundQuarantineKey, DEFAULT_MAX_REJOIN_ATTEMPTS,
    REJOIN_GRANT_TIMEOUT,
};
pub use handover::{
    CoordinatorRole, HandoverAckEnvelope, HandoverDoneEnvelope, HandoverError, HandoverReason,
    HandoverRequestEnvelope, SlashEvent, SlashTally,
};
pub use kick_envelopes::{
    KickDetectedEnvelope, MemberRemovedEnvelope, PlatformKickEvent, RejoinGrantEnvelope,
    RejoinRequestEnvelope, SelfKickedEnvelope,
};
pub use replay::ReplayCache;
pub use sequence::OverlaySequence;
pub use slash::{cross_platform_code, is_cross_platform, reserved as slash_reserved, SlashCode};
pub use sub_group::{
    CreateSubGroupEnvelope as LegacyCreateSubGroupEnvelope,
    MAX_SUB_LABEL_LEN as LEGACY_MAX_SUB_LABEL_LEN, SUBGROUP_TAG,
};
pub use subgroup_delegation::{
    check_delegation_policy, CoordinatorTermId, DelegationCheckInputs, DelegationPolicyError,
    Ed25519Signature, RevocationReasonCode, RevocationReasonError, SdcdReplayKey, SdrtReplayKey,
    SdrvReplayKey, SubDCDelegationEnvelope, SubDCRevocationEnvelope, SubDCRotationEnvelope,
    MAX_DELEGATION_CHAIN_PER_TERM, MAX_ROOT_DELEGATION, REVOCATION_REASON_COORDINATOR_ROTATION,
    REVOCATION_REASON_GROUP_DECOMMISSION, REVOCATION_REASON_SUBDC_KEY_COMPROMISE,
    REVOCATION_REASON_SUBDC_MISCONDUCT, REVOCATION_REASON_SUBDC_VOLUNTARY_RESIGNATION,
    REVOCATION_REASON_TERM_EXPIRED, SDCD_CONTEXT, SDRT_CONTEXT, SDRV_CONTEXT, SUBDC_DELEGATION_TAG,
    SUBDC_REVOCATION_TAG, SUBDC_ROTATION_TAG,
};
pub use subgroup_state::{
    derive_sub_domain_id, reconcile_pending_bind, transition, ActionEncodeError,
    CreateSubGroupEnvelope, CreateSubGroupError, DelegationId, GroupBindingState,
    ParentBindingInvariant, SubDCDelegationProof, SubDomainDerivationInvariant, SubGroupAction,
    SubGroupAuthorityCheck, SubGroupExtension, SubGroupHeaderError, SubGroupLabel,
    SubGroupLabelError, SubGroupQuery, SubGroupRecord, SubGroupResponse, SubGroupState,
    TeardownProof, TransitionError, TransitionResult, TransitionTrigger, CREATE_SUBGROUP,
    MAX_BIND_AWAIT_EPOCHS, MAX_BIND_RETRY_COUNT, MAX_FSKEW_EPOCHS, MAX_ROOT_DEPTH,
    MAX_SUBGROUP_DEPTH, MAX_SUB_LABEL_BYTES, RACE_EPOCHS, SUBGROUP_ACTION_AGGREGATE,
    SUBGROUP_ACTION_INVITE, SUBGROUP_ACTION_PAYLOAD_MAX, SUBGROUP_ACTION_ROUTE,
    SUBGROUP_DOMAIN_CONTEXT, SUBGROUP_RESPONSE_BOUND, SUBGROUP_RESPONSE_DISSOLVED,
    SUBGROUP_RESPONSE_DISSOLVING, SUBGROUP_RESPONSE_NOT_FOUND, SUBGROUP_RESPONSE_PENDING_BIND,
};
pub use witness::{BINDHook, NonceReplayTable, ValidationOutcome, WitnessContext};

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

        // 0b. Validate flags — reserved bits must be zero
        // Note: DOT flags are separate from DGP flags; both must be validated at their layer.
        // The envelope flags field uses the same convention: bits 0-15 defined, 16-63 reserved.
        const DOT_VALID_FLAGS_MASK: u64 = 0xFFFF;
        if (envelope.flags & !DOT_VALID_FLAGS_MASK) != 0 {
            return Err(DotError::Serialization(format!(
                "Invalid envelope flags: reserved bits set (flags=0x{:016x})",
                envelope.flags
            )));
        }

        // 1. Verify envelope_id derivation (Class A)
        envelope.verify(source_peer_key)?;

        // 2. TTL check — reject expired envelopes before forwarding
        if envelope.ttl_hops == 0 {
            return Err(DotError::TtlExpired { ttl: 0, hops: 0 });
        }

        // 3. Check replay cache (Class A)
        let mut cache = self.replay_cache.write().await;
        cache.check_and_insert(envelope.envelope_id, current_epoch)?;

        // 4. Forward to all adapters (Class C — transport-dependent)
        //
        // Limitation: This iterates ALL adapters regardless of domain match.
        // Each adapter receives the envelope with a domain derived from the
        // adapter's own platform type. In production, a domain registry would
        // route envelopes only to adapters that handle the matching domain.
        //
        // Current behavior: try every adapter, log failures, continue.
        // This is safe (fail-open with logging) but inefficient. A future
        // domain-registry layer (RFC-0863 Phase 2) will fix the routing.
        for adapter in &self.adapters {
            let domain = BroadcastDomainId::new(
                adapter.platform_type(),
                &format!("{:02x?}", &envelope.source_peer[..8]),
            );
            match adapter.send_message(&domain, envelope, b"test").await {
                Ok(_receipt) => {}
                Err(_e) => {
                    // Adapter failed — continue to next adapter.
                }
            }
        }

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
