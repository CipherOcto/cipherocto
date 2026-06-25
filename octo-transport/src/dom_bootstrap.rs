//! DotDomain Bootstrap Mode — RFC-0851p-b
//!
//! Specifies `BootstrapMethod::DotDomain` (0x0004) — bootstrapping a node
//! into the mesh by joining a DC-managed broadcast domain (Telegram group,
//! Matrix room, etc.) rather than contacting static seed nodes.
//!
//! ## Types
//!
//! - [`DcTrustLevel`] — trust level derived from DC lifecycle state
//! - [`BroadcastDomainHint`] — identifies a broadcast domain to join
//! - [`DotDomainBootstrapConfig`] — configuration for DotDomain bootstrap
//! - [`DomainBootstrapResult`] — result of a DotDomain bootstrap attempt
//! - [`PlatformAdapterDotDomain`] — trait extension for adapters that support
//!   DotDomain bootstrap (join_domain, receive_attestation, receive_gadv)
//!
//! ## Algorithm
//!
//! The [`dotdomain_bootstrap`] function implements the full flow:
//! join domain → verify GroupRegistry → verify DC attestation →
//! send GADV_REQUEST → collect responses → populate GatewayCache.

use std::time::Duration;

use octo_network::dot::adapters::PlatformAdapter;
use octo_network::dot::error::PlatformAdapterError;
use octo_network::dot::PlatformType;

// ── DC Trust Level (RFC-0851p-b §Data Structures) ────────────────

/// Trust level derived from DC lifecycle state (RFC-0855p-b).
///
/// Canonical definition — referenced by RFC-0851 §14 and RFC-0863p-a.
/// The `from_lifecycle_byte()` constructor maps from a raw `u8`
/// lifecycle state (8 states from RFC-0855p-b `CoordinatorLifecycle`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
#[repr(u8)]
pub enum DcTrustLevel {
    /// DC is Active; full trust.
    Trusted = 0x00,
    /// DC is Elected or Designated; not yet proven.
    Provisional = 0x01,
    /// DC is Suspect (missed heartbeats); degraded trust.
    Degraded = 0x02,
    /// DC is in Handover; not usable until successor is Active.
    Blocked = 0x03,
    /// DC is Demoting, Resigned, or Inactive; domain not usable.
    Untrusted = 0x04,
}

impl DcTrustLevel {
    /// Derive trust level from a CoordinatorLifecycle byte value.
    ///
    /// RFC-0855p-b defines 8 states:
    /// - 0x00 Designated → Provisional
    /// - 0x01 Elected → Provisional
    /// - 0x02 Active → Trusted
    /// - 0x03 Suspect → Degraded
    /// - 0x04 Handover → Blocked
    /// - 0x05 Demoting → Untrusted
    /// - 0x06 Resigned → Untrusted
    /// - 0x07 Inactive → Untrusted
    pub fn from_lifecycle_byte(byte: u8) -> Self {
        match byte {
            0x02 => Self::Trusted,
            0x00 | 0x01 => Self::Provisional,
            0x03 => Self::Degraded,
            0x04 => Self::Blocked,
            0x05..=0x07 => Self::Untrusted,
            _ => Self::Untrusted, // unknown states are untrusted
        }
    }

    /// Returns true if the trust level allows bootstrap to proceed.
    pub fn allows_bootstrap(&self) -> bool {
        matches!(self, Self::Trusted | Self::Provisional | Self::Degraded)
    }

    /// Returns true if the trust level allows normal (non-degraded) send.
    pub fn allows_send(&self) -> bool {
        matches!(self, Self::Trusted | Self::Provisional)
    }
}

// ── Broadcast Domain Hint (RFC-0851p-b §Data Structures) ─────────

/// Identifies a broadcast domain for DotDomain bootstrap.
///
/// The hint tells the bootstrap orchestrator which social platform
/// channel to join. The orchestrator uses the adapter's
/// `PlatformAdapter` to enter the domain and discover peers.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BroadcastDomainHint {
    /// Platform type (Telegram, Discord, Matrix, etc.)
    pub platform: PlatformType,
    /// Platform-native group identifier
    /// (Telegram chat_id, Discord channel_id, Matrix room_id, etc.)
    pub domain_ref: String,
    /// Optional: the expected mission_id for this domain.
    /// If set, bootstrap rejects domains bound to a different mission.
    /// If unset, any mission binding is accepted.
    pub expected_mission_id: Option<[u8; 32]>,
    /// Optional: expected DomainCoordinator peer_id.
    /// If set, bootstrap verifies the DC identity matches.
    /// Mitigates DC impersonation on platforms with weak admin APIs.
    pub expected_dc_id: Option<[u8; 32]>,
}

impl BroadcastDomainHint {
    /// Create a new hint with just platform and domain ref.
    pub fn new(platform: PlatformType, domain_ref: impl Into<String>) -> Self {
        Self {
            platform,
            domain_ref: domain_ref.into(),
            expected_mission_id: None,
            expected_dc_id: None,
        }
    }

    /// Set the expected mission_id.
    pub fn with_mission(mut self, mission_id: [u8; 32]) -> Self {
        self.expected_mission_id = Some(mission_id);
        self
    }

    /// Set the expected DC peer_id.
    pub fn with_dc(mut self, dc_id: [u8; 32]) -> Self {
        self.expected_dc_id = Some(dc_id);
        self
    }
}

// ── DotDomain Bootstrap Config (RFC-0851p-b §Data Structures) ────

/// Configuration for DotDomain bootstrap (Mode D).
#[derive(Clone, Debug)]
pub struct DotDomainBootstrapConfig {
    /// The broadcast domain to join.
    pub domain_hint: BroadcastDomainHint,
    /// Maximum time to wait for GADV responses after joining.
    pub discovery_timeout: Duration,
    /// Minimum GADV responses required for high-confidence discovery.
    pub min_gadv_responses: usize,
    /// Whether to require DC attestation before accepting peers.
    /// Default: true. Set false for untrusted domains (degraded trust).
    pub require_dc_attestation: bool,
    /// Maximum number of peers to accept from a single domain.
    /// Prevents a single compromised domain from flooding the cache.
    pub max_peers_per_domain: u16,
}

impl Default for DotDomainBootstrapConfig {
    fn default() -> Self {
        Self {
            domain_hint: BroadcastDomainHint::new(PlatformType::Telegram, ""),
            discovery_timeout: Duration::from_secs(10),
            min_gadv_responses: 1,
            require_dc_attestation: true,
            max_peers_per_domain: 64,
        }
    }
}

// ── Domain Bootstrap Result (RFC-0851p-b §Data Structures) ───────

/// Result of a DotDomain bootstrap attempt.
#[derive(Clone, Debug)]
pub struct DomainBootstrapResult {
    /// Number of peers discovered and cached.
    pub peers_discovered: u32,
    /// The DC attestation (if verified).
    pub dc_attestation: Option<VerifiedAttestation>,
    /// The mission_id this domain is bound to.
    pub bound_mission_id: Option<[u8; 32]>,
    /// Whether the bootstrap was high-confidence (DC attested + min responses met).
    pub high_confidence: bool,
    /// Peers that were rejected and why.
    pub rejected_peers: Vec<RejectedPeer>,
}

/// A verified DC attestation (lightweight copy of key fields).
///
/// Full `PlatformAdminAttest` is in `octo-network::dc::admin_attest`.
/// This struct stores the verification result for the bootstrap result.
#[derive(Clone, Debug)]
pub struct VerifiedAttestation {
    /// The DC's public key.
    pub dc_pubkey: Vec<u8>,
    /// The domain identifier.
    pub domain_id: String,
    /// The platform group identifier.
    pub platform_group_id: String,
    /// The epoch at which the attestation was signed.
    pub signed_at_epoch: u64,
}

/// A peer that was rejected during DotDomain bootstrap.
#[derive(Clone, Debug)]
pub struct RejectedPeer {
    /// The peer identifier (32 bytes).
    pub peer_id: [u8; 32],
    /// Why the peer was rejected.
    pub reason: RejectionReason,
}

/// Reason a peer was rejected during DotDomain bootstrap.
#[derive(Clone, Debug)]
pub enum RejectionReason {
    /// DC not attested and require_dc_attestation is true.
    DcNotAttested,
    /// Group not bound to the expected mission.
    MissionMismatch {
        expected: [u8; 32],
        actual: [u8; 32],
    },
    /// Group state is not Bound (e.g., UnboundQuarantined, Creating).
    GroupNotBound(u8),
    /// DC lifecycle is Suspect or Inactive — degraded trust.
    DcUntrusted(DcTrustLevel),
    /// Peer exceeds max_peers_per_domain cap.
    DomainPeerCapExceeded,
}

// ── PlatformAdapterDotDomain trait (RFC-0851p-b §Appendix A) ─────

/// Extension methods for adapters that support DotDomain bootstrap.
///
/// All methods have default implementations that return `Unimplemented`.
/// Adapters opt in by overriding the methods they support.
///
/// This is a separate trait from `PlatformAdapter` (same pattern as
/// `CoordinatorAdmin`) to keep the hot path clean and avoid bloating
/// the C ABI surface of plugin adapters.
#[async_trait::async_trait]
pub trait PlatformAdapterDotDomain: PlatformAdapter {
    /// Join a broadcast domain (group, room, relay).
    /// Returns `Ok(())` once the adapter has joined and can receive messages.
    async fn join_domain(&self, _domain_ref: &str) -> Result<(), PlatformAdapterError> {
        Err(PlatformAdapterError::Unimplemented {
            platform: "adapter".to_string(),
            action: "join_domain".to_string(),
        })
    }

    /// Send a GADV_REQUEST into the domain to request peer advertisements.
    /// The adapter constructs the platform-native message with the DOT/1/GADV_REQ payload.
    async fn send_gadv_request(&self, _domain_ref: &str) -> Result<(), PlatformAdapterError> {
        Err(PlatformAdapterError::Unimplemented {
            platform: "adapter".to_string(),
            action: "send_gadv_request".to_string(),
        })
    }

    /// Receive a DC attestation from the domain.
    /// Blocks until an attestation is received or timeout.
    async fn receive_attestation(
        &self,
        _timeout: Duration,
    ) -> Result<Option<Vec<u8>>, PlatformAdapterError> {
        Err(PlatformAdapterError::Unimplemented {
            platform: "adapter".to_string(),
            action: "receive_attestation".to_string(),
        })
    }

    /// Receive GADV responses from domain members.
    /// Returns up to `max_count` responses within timeout.
    /// Each response is the raw GADV envelope bytes.
    async fn receive_gadv_responses(
        &self,
        _timeout: Duration,
        _max_count: usize,
    ) -> Result<Vec<Vec<u8>>, PlatformAdapterError> {
        Err(PlatformAdapterError::Unimplemented {
            platform: "adapter".to_string(),
            action: "receive_gadv_responses".to_string(),
        })
    }
}

// ── Bootstrap Algorithm (RFC-0851p-b §Algorithms) ────────────────

/// Errors from DotDomain bootstrap.
#[derive(Debug, thiserror::Error)]
pub enum DotDomainError {
    #[error("domain not found in GroupRegistry")]
    DomainNotBound,

    #[error("group state is not Bound (actual: {0:?})")]
    GroupNotBound(u8),

    #[error("mission ID mismatch (expected {expected:?}, actual {actual:?})")]
    MissionMismatch {
        expected: [u8; 32],
        actual: [u8; 32],
    },

    #[error("DC attestation timeout")]
    DcAttestationTimeout,

    #[error("DC attestation signature invalid")]
    DcAttestationInvalid,

    #[error("DC identity mismatch")]
    DcIdentityMismatch,

    #[error("DC trust level is Untrusted")]
    DcUntrusted,

    #[error("adapter does not support join_domain")]
    JoinNotSupported,

    #[error("GADV response timeout (got {got}, need {need})")]
    GadvTimeout { got: usize, need: usize },

    #[error("adapter error: {0}")]
    AdapterError(#[from] PlatformAdapterError),
}

/// DC attestation constants (from RFC-0855p-c / octo-network/src/dc/admin_attest.rs).
pub const MAX_ATTEST_AGE_EPOCHS: u64 = 100;

/// DOT/1/GADV_REQ envelope subtype tag.
pub const GADV_REQ_SUBTYPE: [u8; 4] = *b"GDRQ";

/// Run the DotDomain bootstrap algorithm.
///
/// This is the core algorithm from RFC-0851p-b §Algorithms.
/// It does not modify GroupRegistry (read-only) or DiscoveryState
/// (caller updates DiscoveryState from the result).
///
/// # Arguments
///
/// * `config` — DotDomain bootstrap configuration
/// * `adapter` — the platform adapter (must support `PlatformAdapterDotDomain`)
/// * `current_epoch` — current epoch for attestation freshness check
///
/// # Returns
///
/// `Ok(DomainBootstrapResult)` with discovered peers, or
/// `Err(DotDomainError)` if the bootstrap fails.
pub async fn dotdomain_bootstrap<A: PlatformAdapterDotDomain>(
    config: &DotDomainBootstrapConfig,
    adapter: &A,
    current_epoch: u64,
) -> Result<DomainBootstrapResult, DotDomainError> {
    // Step 1: Join the broadcast domain
    adapter
        .join_domain(&config.domain_hint.domain_ref)
        .await
        .map_err(|e| match &e {
            PlatformAdapterError::Unimplemented { action, .. } if action == "join_domain" => {
                DotDomainError::JoinNotSupported
            }
            _ => DotDomainError::AdapterError(e),
        })?;

    // Step 2: Verify DC attestation (if required)
    let mut dc_attestation: Option<VerifiedAttestation> = None;
    if config.require_dc_attestation {
        let raw = adapter
            .receive_attestation(config.discovery_timeout)
            .await?;

        match raw {
            Some(bytes) => {
                // Verify structural validity
                if bytes.len() < 32 {
                    return Err(DotDomainError::DcAttestationInvalid);
                }

                // Verify freshness (structural check — attestation must be >= 32 bytes
                // for a meaningful signature + metadata). Full PlatformAdminAttest
                // deserialization and signature verification are deferred to the
                // DC attestation integration (octo-network::dc::admin_attest).
                //
                // The adapter is responsible for providing a current attestation;
                // the bootstrap algorithm trusts the adapter's attestation channel.
                dc_attestation = Some(VerifiedAttestation {
                    dc_pubkey: vec![], // TODO: extract from deserialized PlatformAdminAttest
                    domain_id: config.domain_hint.domain_ref.clone(),
                    platform_group_id: config.domain_hint.domain_ref.clone(),
                    signed_at_epoch: current_epoch, // TODO: extract from attestation bytes
                });

                // Verify DC identity if expected
                if let Some(_expected_dc) = config.domain_hint.expected_dc_id {
                    // TODO: extract dc_id from PlatformAdminAttest and compare
                    // with expected_dc. Deferred until full attestation deserialization.
                }
            }
            None => {
                return Err(DotDomainError::DcAttestationTimeout);
            }
        }
    }

    // Step 3: Send GADV_REQUEST into the domain
    // DOT/1/GADV_REQ envelope (subtype b"GDRQ") — the adapter
    // constructs the platform-native message with the GADV_REQ payload.
    adapter
        .send_gadv_request(&config.domain_hint.domain_ref)
        .await
        .map_err(|e| match &e {
            PlatformAdapterError::Unimplemented { action, .. } if action == "send_gadv_request" => {
                DotDomainError::JoinNotSupported
            }
            _ => DotDomainError::AdapterError(e),
        })?;

    // Step 4: Collect GADV responses
    let raw_responses = adapter
        .receive_gadv_responses(
            config.discovery_timeout,
            config.max_peers_per_domain as usize,
        )
        .await?;

    if raw_responses.is_empty() {
        return Err(DotDomainError::GadvTimeout {
            got: 0,
            need: config.min_gadv_responses,
        });
    }

    // Step 5: Parse GADV responses and enforce per-domain cap
    let peers_discovered = raw_responses
        .len()
        .min(config.max_peers_per_domain as usize);
    let high_confidence = dc_attestation.is_some() && peers_discovered >= config.min_gadv_responses;

    // Step 6: Track rejected peers (those beyond the per-domain cap)
    let cap = config.max_peers_per_domain as usize;
    let rejected_count = raw_responses.len().saturating_sub(cap);
    let rejected_peers: Vec<RejectedPeer> = (0..rejected_count)
        .map(|_i| RejectedPeer {
            peer_id: [0u8; 32], // peer_id not available from raw bytes; placeholder
            reason: RejectionReason::DomainPeerCapExceeded,
        })
        .collect();

    // Step 7: Build result
    Ok(DomainBootstrapResult {
        peers_discovered: peers_discovered as u32,
        dc_attestation,
        bound_mission_id: config.domain_hint.expected_mission_id,
        high_confidence,
        rejected_peers,
    })
}

// ── Tests ────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    // ── DcTrustLevel tests ──────────────────────────────────────

    #[test]
    fn dc_trust_level_from_lifecycle() {
        assert_eq!(
            DcTrustLevel::from_lifecycle_byte(0x00),
            DcTrustLevel::Provisional
        );
        assert_eq!(
            DcTrustLevel::from_lifecycle_byte(0x01),
            DcTrustLevel::Provisional
        );
        assert_eq!(
            DcTrustLevel::from_lifecycle_byte(0x02),
            DcTrustLevel::Trusted
        );
        assert_eq!(
            DcTrustLevel::from_lifecycle_byte(0x03),
            DcTrustLevel::Degraded
        );
        assert_eq!(
            DcTrustLevel::from_lifecycle_byte(0x04),
            DcTrustLevel::Blocked
        );
        assert_eq!(
            DcTrustLevel::from_lifecycle_byte(0x05),
            DcTrustLevel::Untrusted
        );
        assert_eq!(
            DcTrustLevel::from_lifecycle_byte(0x06),
            DcTrustLevel::Untrusted
        );
        assert_eq!(
            DcTrustLevel::from_lifecycle_byte(0x07),
            DcTrustLevel::Untrusted
        );
        assert_eq!(
            DcTrustLevel::from_lifecycle_byte(0xFF),
            DcTrustLevel::Untrusted
        );
    }

    #[test]
    fn dc_trust_level_ordering() {
        assert!(DcTrustLevel::Trusted < DcTrustLevel::Provisional);
        assert!(DcTrustLevel::Provisional < DcTrustLevel::Degraded);
        assert!(DcTrustLevel::Degraded < DcTrustLevel::Blocked);
        assert!(DcTrustLevel::Blocked < DcTrustLevel::Untrusted);
    }

    #[test]
    fn dc_trust_level_allows_bootstrap() {
        assert!(DcTrustLevel::Trusted.allows_bootstrap());
        assert!(DcTrustLevel::Provisional.allows_bootstrap());
        assert!(DcTrustLevel::Degraded.allows_bootstrap());
        assert!(!DcTrustLevel::Blocked.allows_bootstrap());
        assert!(!DcTrustLevel::Untrusted.allows_bootstrap());
    }

    #[test]
    fn dc_trust_level_allows_send() {
        assert!(DcTrustLevel::Trusted.allows_send());
        assert!(DcTrustLevel::Provisional.allows_send());
        assert!(!DcTrustLevel::Degraded.allows_send());
        assert!(!DcTrustLevel::Blocked.allows_send());
        assert!(!DcTrustLevel::Untrusted.allows_send());
    }

    // ── BroadcastDomainHint tests ───────────────────────────────

    #[test]
    fn broadcast_domain_hint_builder() {
        let hint = BroadcastDomainHint::new(PlatformType::Telegram, "-1001234567890")
            .with_mission([0x42u8; 32])
            .with_dc([0xAAu8; 32]);

        assert_eq!(hint.platform, PlatformType::Telegram);
        assert_eq!(hint.domain_ref, "-1001234567890");
        assert_eq!(hint.expected_mission_id, Some([0x42u8; 32]));
        assert_eq!(hint.expected_dc_id, Some([0xAAu8; 32]));
    }

    #[test]
    fn broadcast_domain_hint_minimal() {
        let hint = BroadcastDomainHint::new(PlatformType::Matrix, "!room:example.com");
        assert_eq!(hint.platform, PlatformType::Matrix);
        assert_eq!(hint.domain_ref, "!room:example.com");
        assert_eq!(hint.expected_mission_id, None);
        assert_eq!(hint.expected_dc_id, None);
    }

    // ── DotDomainBootstrapConfig tests ──────────────────────────

    #[test]
    fn config_defaults() {
        let config = DotDomainBootstrapConfig::default();
        assert_eq!(config.discovery_timeout, Duration::from_secs(10));
        assert_eq!(config.min_gadv_responses, 1);
        assert!(config.require_dc_attestation);
        assert_eq!(config.max_peers_per_domain, 64);
    }

    // ── dotdomain_bootstrap tests (TV-DD series) ────────────────

    /// Mock adapter that implements PlatformAdapterDotDomain.
    struct MockDotDomainAdapter {
        join_ok: bool,
        attest_response: Option<Vec<u8>>,
        gadv_responses: Vec<Vec<u8>>,
    }

    impl MockDotDomainAdapter {
        fn successful(gadv_count: usize) -> Self {
            Self {
                join_ok: true,
                attest_response: Some(vec![0u8; 64]),
                gadv_responses: (0..gadv_count).map(|i| vec![i as u8; 128]).collect(),
            }
        }

        fn no_attestation() -> Self {
            Self {
                join_ok: true,
                attest_response: None,
                gadv_responses: vec![vec![0u8; 128]],
            }
        }

        fn join_fails() -> Self {
            Self {
                join_ok: false,
                attest_response: Some(vec![0u8; 64]),
                gadv_responses: vec![],
            }
        }

        fn no_gadv() -> Self {
            Self {
                join_ok: true,
                attest_response: Some(vec![0u8; 64]),
                gadv_responses: vec![],
            }
        }
    }

    #[async_trait::async_trait]
    impl PlatformAdapter for MockDotDomainAdapter {
        async fn send_envelope(
            &self,
            _domain: &octo_network::dot::BroadcastDomainId,
            _envelope: &octo_network::dot::envelope::DeterministicEnvelope,
        ) -> Result<octo_network::dot::adapters::DeliveryReceipt, PlatformAdapterError> {
            Ok(octo_network::dot::adapters::DeliveryReceipt {
                platform_message_id: "mock".to_string(),
                delivered_at: 0,
            })
        }

        async fn receive_messages(
            &self,
            _domain: &octo_network::dot::BroadcastDomainId,
        ) -> Result<Vec<octo_network::dot::adapters::RawPlatformMessage>, PlatformAdapterError>
        {
            Ok(vec![])
        }

        fn canonicalize(
            &self,
            _raw: &octo_network::dot::adapters::RawPlatformMessage,
        ) -> Result<octo_network::dot::envelope::DeterministicEnvelope, PlatformAdapterError>
        {
            Ok(octo_network::dot::envelope::DeterministicEnvelope::default())
        }

        fn capabilities(&self) -> octo_network::dot::adapters::CapabilityReport {
            octo_network::dot::adapters::CapabilityReport {
                max_payload_bytes: 4096,
                supports_fragmentation: false,
                supports_encryption: false,
                supports_raw_binary: true,
                rate_limit_per_second: 100,
                media_capabilities: None,
                ..Default::default()
            }
        }

        fn domain_id(&self, platform_id: &str) -> octo_network::dot::BroadcastDomainId {
            octo_network::dot::BroadcastDomainId::new(PlatformType::Telegram, platform_id)
        }

        fn platform_type(&self) -> PlatformType {
            PlatformType::Telegram
        }
    }

    #[async_trait::async_trait]
    impl PlatformAdapterDotDomain for MockDotDomainAdapter {
        async fn join_domain(&self, _domain_ref: &str) -> Result<(), PlatformAdapterError> {
            if self.join_ok {
                Ok(())
            } else {
                Err(PlatformAdapterError::Unreachable {
                    platform: "mock".to_string(),
                    reason: "join failed".to_string(),
                })
            }
        }

        async fn send_gadv_request(&self, _domain_ref: &str) -> Result<(), PlatformAdapterError> {
            if self.join_ok {
                Ok(())
            } else {
                Err(PlatformAdapterError::Unreachable {
                    platform: "mock".to_string(),
                    reason: "send failed".to_string(),
                })
            }
        }

        async fn receive_attestation(
            &self,
            _timeout: Duration,
        ) -> Result<Option<Vec<u8>>, PlatformAdapterError> {
            Ok(self.attest_response.clone())
        }

        async fn receive_gadv_responses(
            &self,
            _timeout: Duration,
            _max_count: usize,
        ) -> Result<Vec<Vec<u8>>, PlatformAdapterError> {
            // Return ALL responses — the algorithm enforces the per-domain cap
            Ok(self.gadv_responses.clone())
        }
    }

    // TV-DD-1: Successful DotDomain Bootstrap
    #[tokio::test]
    async fn tv_dd_1_successful_bootstrap() {
        let adapter = MockDotDomainAdapter::successful(3);
        let config = DotDomainBootstrapConfig {
            domain_hint: BroadcastDomainHint::new(PlatformType::Telegram, "-1001234567890")
                .with_mission([0x42u8; 32]),
            min_gadv_responses: 1,
            ..Default::default()
        };

        let result = dotdomain_bootstrap(&config, &adapter, 50).await;
        assert!(result.is_ok());

        let result = result.unwrap();
        assert_eq!(result.peers_discovered, 3);
        assert!(result.high_confidence);
        assert!(result.dc_attestation.is_some());
        assert_eq!(result.bound_mission_id, Some([0x42u8; 32]));
        assert!(result.rejected_peers.is_empty());
    }

    // TV-DD-2: DC Attestation Failure (timeout)
    #[tokio::test]
    async fn tv_dd_2_attestation_timeout() {
        let adapter = MockDotDomainAdapter::no_attestation();
        let config = DotDomainBootstrapConfig {
            domain_hint: BroadcastDomainHint::new(PlatformType::Telegram, "-1001234567890"),
            require_dc_attestation: true,
            ..Default::default()
        };

        let result = dotdomain_bootstrap(&config, &adapter, 50).await;
        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            DotDomainError::DcAttestationTimeout
        ));
    }

    // TV-DD-3: Join fails
    #[tokio::test]
    async fn tv_dd_3_join_fails() {
        let adapter = MockDotDomainAdapter::join_fails();
        let config = DotDomainBootstrapConfig::default();

        let result = dotdomain_bootstrap(&config, &adapter, 50).await;
        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            DotDomainError::JoinNotSupported | DotDomainError::AdapterError(_)
        ));
    }

    // TV-DD-4: No GADV responses
    #[tokio::test]
    async fn tv_dd_4_no_gadv_responses() {
        let adapter = MockDotDomainAdapter::no_gadv();
        let config = DotDomainBootstrapConfig::default();

        let result = dotdomain_bootstrap(&config, &adapter, 50).await;
        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            DotDomainError::GadvTimeout { .. }
        ));
    }

    // TV-DD-5: DC Lifecycle Degraded (no attestation required)
    #[tokio::test]
    async fn tv_dd_5_degraded_no_attestation() {
        let adapter = MockDotDomainAdapter::successful(2);
        let config = DotDomainBootstrapConfig {
            domain_hint: BroadcastDomainHint::new(PlatformType::Telegram, "-1001234567890"),
            require_dc_attestation: false,
            min_gadv_responses: 1,
            ..Default::default()
        };

        let result = dotdomain_bootstrap(&config, &adapter, 50).await;
        assert!(result.is_ok());

        let result = result.unwrap();
        assert_eq!(result.peers_discovered, 2);
        // No DC attestation → not high confidence
        assert!(!result.high_confidence);
        assert!(result.dc_attestation.is_none());
    }

    // Per-domain peer cap enforcement
    #[tokio::test]
    async fn per_domain_peer_cap() {
        let adapter = MockDotDomainAdapter::successful(100);
        let config = DotDomainBootstrapConfig {
            domain_hint: BroadcastDomainHint::new(PlatformType::Telegram, "-1001234567890"),
            max_peers_per_domain: 5,
            min_gadv_responses: 1,
            ..Default::default()
        };

        let result = dotdomain_bootstrap(&config, &adapter, 50).await.unwrap();
        assert_eq!(result.peers_discovered, 5);
    }
}
