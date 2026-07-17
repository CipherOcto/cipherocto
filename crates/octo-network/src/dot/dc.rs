//! Domain Coordinator (DC) Orchestrator — RFC-0850p-d Phase 3,
//! RFC-0850p-e Phase 6, RFC-0855p-b §5
//!
//! `DcOrchestrator` is the high-level API used by the
//! DomainCoordinator node to coordinate group creation, invitation,
//! third-party BIND, and decommission flows.
//!
//! The orchestrator does NOT make platform-side API calls directly;
//! it emits envelopes and delegates the actual platform work to the
//! per-adapter implementations (e.g., `octo-adapter-whatsapp`). The
//! orchestrator is responsible for:
//!
//! - building and signing envelopes
//! - enforcing the founder race tiebreak (lexicographic `dc_id`)
//! - the kick decision tree (per RFC-0850p-e §Algorithm C, R16 R1-C3
//!   fix — the decision tree is in 0850p-e, NOT 0850p-d §C "Atomic
//!   Migration via CREATE+REBIND")
//! - coordinating with the `GroupRegistry` for state transitions
//!
//! See missions:
//! - `missions/claimed/0850p-d-dc-initiated-group-creation.md` (Phase 3)
//! - `missions/claimed/0850p-e-kick-detection.md` (Phase 6)

use ed25519_dalek::SigningKey;

use super::binding::{
    BindEnvelope, BindingError, GroupVisibility, ThirdPartyBindResult, UnbindAuthority,
    UnbindEnvelope, WitnessAssertion,
};

#[cfg(test)]
use super::binding::{GroupBinding, GroupState};
use super::dc_envelopes::{
    CreateGroupDoneEnvelope, CreateGroupEnvelope, CreateGroupFailEnvelope, InviteEnvelope,
    UnbindAllAckEnvelope, UnbindAllEnvelope, UnbindReason,
};
use super::error::DotError;
use super::group_registry::GroupRegistry;

// rand is part of the workspace dependency tree (transitively via blake3
// tests and ed25519-dalek); if not directly available, fall back to a
// simple deterministic LCG for nonce generation in tests.
//
// The orchestrator uses a `RngCore` trait object so callers can inject
// a deterministic RNG for testing.

/// Outcome of a founder race (two DCs attempt to create the same group).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RaceOutcome {
    /// Local CGROUP wins the race; cancel the remote CGROUP.
    LocalWins,
    /// Remote CGROUP wins the race; cancel the local CGROUP.
    RemoteWins,
}

/// DC kick decision outcome.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum KickDecision {
    /// The DC was kicked; transition to `CoordinatorLifecycle::Handover`
    /// (per RFC-0855p-b).
    DcKicked,
    /// A witness was kicked; emit `KICK_DETECTED`; check if quorum is
    /// still met.
    WitnessKicked {
        /// The witness that was kicked.
        witness_id: [u8; 32],
    },
    /// A regular member was kicked; emit informational `MEMBER_REMOVED`;
    /// do NOT trigger REBIND.
    MemberRemoved {
        /// The member that was kicked.
        member_id: [u8; 32],
    },
    /// Multiple nodes were kicked within the grace period; emit
    /// `UNBIND_ALL` and quarantine the group.
    MassKick {
        /// Number of nodes kicked within the grace period.
        count: u32,
    },
}

/// Configuration for the DC orchestrator.
#[derive(Debug, Clone)]
pub struct DcConfig {
    /// Maximum number of invites a single CGROUP can issue.
    pub max_invites_per_cgroup: u32,
    /// CGROUP timeout in epochs (per RFC-0850p-d §A.6).
    pub cgroup_timeout_epochs: u64,
    /// Default invite expiry in epochs.
    pub invite_expiry_epochs: u64,
    /// Default recovery window for quarantine.
    pub recovery_window_epochs: u64,
    /// Default cap on rejoin attempts.
    pub max_rejoin_attempts: u16,
}

impl Default for DcConfig {
    fn default() -> Self {
        Self {
            max_invites_per_cgroup: 100,
            cgroup_timeout_epochs: 50, // per mission 0850p-d Phase 2
            invite_expiry_epochs: 100,
            recovery_window_epochs: 50, // = REJOIN_GRANT_TIMEOUT
            max_rejoin_attempts: 3,
        }
    }
}

/// DC orchestrator.
///
/// The orchestrator is generic over the nonce generator. The default
/// constructor accepts a `Box<dyn FnMut() -> [u8; 32] + Send + Sync>`,
/// which can wrap any RNG (including `OsRng`, `ThreadRng`,
/// `StdRng::from_entropy()`, or a deterministic test RNG).
pub struct DcOrchestrator {
    /// The DC's signing key.
    dc_key: SigningKey,
    /// Configuration.
    config: DcConfig,
    /// Nonce generator closure.
    nonce_fn: Box<dyn FnMut() -> [u8; 32] + Send + Sync>,
}

impl DcOrchestrator {
    /// Create a new orchestrator with a custom nonce generator.
    pub fn new<F>(dc_key: SigningKey, config: DcConfig, nonce_fn: F) -> Self
    where
        F: FnMut() -> [u8; 32] + Send + Sync + 'static,
    {
        Self {
            dc_key,
            config,
            nonce_fn: Box::new(nonce_fn),
        }
    }

    /// Returns the DC's public key.
    pub fn dc_pubkey(&self) -> [u8; 32] {
        self.dc_key.verifying_key().to_bytes()
    }

    /// Returns the orchestrator's configuration.
    pub fn config(&self) -> &DcConfig {
        &self.config
    }

    /// Generate a fresh 32-byte nonce via the configured generator.
    fn fresh_nonce(&mut self) -> [u8; 32] {
        (self.nonce_fn)()
    }

    // -------------------------------------------------------------------------
    // create_group (Phase 3 of 0850p-d)
    // -------------------------------------------------------------------------

    /// Build a `CreateGroupEnvelope` for a new group on the given
    /// platform. Does NOT create the group on the platform; the local
    /// adapter is expected to handle the platform-side call and emit
    /// either a `CreateGroupDoneEnvelope` (with the `group_jid`) or
    /// `CreateGroupFailEnvelope` (with the reason).
    #[allow(clippy::too_many_arguments)] // Arguments map 1:1 to envelope fields.
    pub fn build_create_group(
        &mut self,
        domain_id: [u8; 32],
        mission_id: [u8; 32],
        platform: &str,
        proposed_metadata: Vec<u8>,
        visibility: GroupVisibility,
        current_epoch: u64,
        coordinator_term_id: u64,
        initial_invite_count: u32,
    ) -> Result<CreateGroupEnvelope, DotError> {
        if initial_invite_count > self.config.max_invites_per_cgroup {
            return Err(DotError::Serialization(format!(
                "initial_invite_count {} exceeds max {}",
                initial_invite_count, self.config.max_invites_per_cgroup
            )));
        }
        let mut env = CreateGroupEnvelope {
            domain_id,
            mission_id,
            platform: platform.into(),
            proposed_group_metadata: proposed_metadata,
            initial_invite_count,
            dc_id: self.dc_pubkey(),
            nonce: self.fresh_nonce(),
            current_epoch,
            coordinator_term_id,
            group_visibility: visibility,
            cgroup_hash: [0u8; 32],
            signature: [0u8; 64],
        };
        env.sign(&self.dc_key);
        Ok(env)
    }

    /// Build a `CreateGroupDoneEnvelope` after the local adapter has
    /// successfully created the group.
    pub fn build_create_group_done(
        &mut self,
        cgroup_hash: [u8; 32],
        nonce: [u8; 32],
        domain_id: [u8; 32],
        group_jid: String,
        platform: String,
    ) -> CreateGroupDoneEnvelope {
        let mut env = CreateGroupDoneEnvelope {
            domain_id,
            group_jid,
            platform,
            cgroup_hash,
            nonce,
            done_hash: [0u8; 32],
            signature: [0u8; 64],
        };
        env.sign(&self.dc_key);
        env
    }

    /// Build a `CreateGroupFailEnvelope` after the local adapter has
    /// failed to create the group.
    pub fn build_create_group_fail(
        &mut self,
        cgroup_hash: [u8; 32],
        domain_id: [u8; 32],
        platform: String,
        reason_code: u16,
        platform_error: String,
    ) -> CreateGroupFailEnvelope {
        let mut env = CreateGroupFailEnvelope {
            domain_id,
            platform,
            cgroup_hash,
            reason_code,
            platform_error,
            fail_hash: [0u8; 32],
            signature: [0u8; 64],
        };
        env.sign(&self.dc_key);
        env
    }

    // -------------------------------------------------------------------------
    // invite_member
    // -------------------------------------------------------------------------

    /// Build an `InviteEnvelope` inviting a node to join the group.
    pub fn build_invite(
        &mut self,
        domain_id: [u8; 32],
        mission_id: [u8; 32],
        group_jid: String,
        platform: String,
        invitee_pubkey: [u8; 32],
        current_epoch: u64,
    ) -> InviteEnvelope {
        let mut inv = InviteEnvelope {
            domain_id,
            group_jid,
            platform,
            invitee_pubkey,
            nonce: self.fresh_nonce(),
            invite_token: [0u8; 32],
            mission_id,
            current_epoch,
            expires_at_epoch: current_epoch + self.config.invite_expiry_epochs,
            invite_hash: [0u8; 32],
            signature: [0u8; 64],
        };
        inv.sign(&self.dc_key);
        inv
    }

    // -------------------------------------------------------------------------
    // unbind_all
    // -------------------------------------------------------------------------

    /// Build an `UnbindAllEnvelope` requesting all members to leave the
    /// group.
    #[allow(clippy::too_many_arguments)] // Arguments map 1:1 to envelope fields.
    pub fn build_unbind_all(
        &mut self,
        domain_id: [u8; 32],
        group_jid: String,
        platform: String,
        binding_hash: [u8; 32],
        reason: UnbindReason,
        current_epoch: u64,
        coordinator_term_id: u64,
    ) -> UnbindAllEnvelope {
        let mut env = UnbindAllEnvelope {
            domain_id,
            group_jid,
            platform,
            reason,
            binding_hash,
            nonce: self.fresh_nonce(),
            current_epoch,
            coordinator_term_id,
            unbind_hash: [0u8; 32],
            signature: [0u8; 64],
        };
        env.sign(&self.dc_key);
        env
    }

    // -------------------------------------------------------------------------
    // bind_third_party_group (RFC-0850p-d §B)
    // -------------------------------------------------------------------------

    /// Build a `BindEnvelope` for a third-party group (a group that was
    /// NOT created by the DC but is being bound to a `domain_id`).
    ///
    /// The caller MUST have a valid `WitnessAssertion` from at least one
    /// witness that has confirmed the group's existence. The assertion
    /// is cryptographically verified inside this function — a forged or
    /// stale assertion is rejected with `BindingError::InvalidAssertion`.
    ///
    /// R17 R1-HIGH-5 fix: the assertion is no longer a parameter that
    /// gets silently discarded with `let _ = ...`. It is verified, and
    /// a `witness_seal` is returned in `ThirdPartyBindResult` so the
    /// bind is cryptographically tied to the specific assertion.
    pub fn build_third_party_bind(
        &mut self,
        group_jid: String,
        platform: String,
        mission_id: [u8; 32],
        domain_id: [u8; 32],
        witness_assertion: &WitnessAssertion,
        current_epoch: u64,
    ) -> Result<ThirdPartyBindResult, BindingError> {
        // Verify the witness assertion signature. We need the witness's
        // public key; derive it from `witness_id` if it's stored as the
        // raw 32-byte form, otherwise reject.
        let witness_pubkey = ed25519_dalek::VerifyingKey::from_bytes(&witness_assertion.witness_id)
            .map_err(|e| BindingError::InvalidAssertion {
                reason: format!("witness_id is not a valid ed25519 public key: {e}"),
            })?;

        // The assertion must be fresh: the witness_epoch should be
        // within `witness_assertion_max_age_epochs` of `current_epoch`.
        const WITNESS_ASSERTION_MAX_AGE_EPOCHS: u64 = 50;
        if witness_assertion.witness_epoch + WITNESS_ASSERTION_MAX_AGE_EPOCHS < current_epoch
            || witness_assertion.witness_epoch > current_epoch
        {
            return Err(BindingError::InvalidAssertion {
                reason: format!(
                    "witness_epoch {} is outside the freshness window around current_epoch {}",
                    witness_assertion.witness_epoch, current_epoch
                ),
            });
        }

        witness_assertion
            .verify(&witness_pubkey)
            .map_err(|e| BindingError::InvalidAssertion {
                reason: format!("assertion signature did not verify: {e}"),
            })?;

        let mut env = BindEnvelope {
            group_jid,
            platform,
            mission_id,
            domain_id,
            domain_coordinator_id: self.dc_pubkey(),
            founder_peer_id: self.dc_pubkey(),
            nonce: self.fresh_nonce(),
            current_epoch,
            is_reconnect: false, // third-party BIND is not a reconnect
            bind_hash: [0u8; 32],
            signature: [0u8; 64],
        };
        env.sign(&self.dc_key);

        // witness_seal = BLAKE3-256(bind_hash || assertion.assertion_hash)
        let mut seal_buf = Vec::with_capacity(64);
        seal_buf.extend_from_slice(&env.bind_hash);
        seal_buf.extend_from_slice(&witness_assertion.assertion_hash);
        let witness_seal = *blake3::hash(&seal_buf).as_bytes();

        Ok(ThirdPartyBindResult {
            envelope: env,
            witness_seal,
            assertion: witness_assertion.clone(),
        })
    }

    // -------------------------------------------------------------------------
    // handle_founder_race (RFC-0850p-d §A.4)
    // -------------------------------------------------------------------------

    /// Resolve a founder race: when two DCs simultaneously emit
    /// `CreateGroupEnvelope` for the same `domain_id`, the one with the
    /// lexicographically smaller `dc_id` wins.
    ///
    /// Returns `LocalWins` if the local DC's id is smaller; `RemoteWins`
    /// otherwise.
    pub fn handle_founder_race(
        &self,
        local_dc_id: &[u8; 32],
        remote_dc_id: &[u8; 32],
    ) -> RaceOutcome {
        if local_dc_id < remote_dc_id {
            RaceOutcome::LocalWins
        } else {
            RaceOutcome::RemoteWins
        }
    }

    // -------------------------------------------------------------------------
    // handle_kick (RFC-0850p-e §Algorithm C, R16 R1-C3 fix)
    // -------------------------------------------------------------------------

    /// Apply the DC kick decision tree to a kick event.
    ///
    /// Per RFC-0850p-e §Algorithm C:
    /// - if the kicked node is the DC itself, transition to handover
    /// - if the kicked node is a witness, emit KICK_DETECTED and check
    ///   quorum
    /// - if the kicked node is a regular member, emit MEMBER_REMOVED
    ///   (informational; no REBIND)
    /// - if ≥ 2 nodes are kicked within the grace period, emit
    ///   UNBIND_ALL
    ///
    /// `is_dc` is `true` if the kicked node is the DC.
    /// `is_witness` is `true` if the kicked node is a witness.
    /// `recent_kick_count` is the number of kicks in the recent grace
    /// period (>= 2 triggers mass-kick).
    pub fn handle_kick(
        &self,
        kicked_node: [u8; 32],
        is_dc: bool,
        is_witness: bool,
        recent_kick_count: u32,
    ) -> KickDecision {
        if is_dc {
            KickDecision::DcKicked
        } else if recent_kick_count >= 2 {
            KickDecision::MassKick {
                count: recent_kick_count,
            }
        } else if is_witness {
            KickDecision::WitnessKicked {
                witness_id: kicked_node,
            }
        } else {
            KickDecision::MemberRemoved {
                member_id: kicked_node,
            }
        }
    }

    // -------------------------------------------------------------------------
    // ack handling (Phase 3 of 0850p-d)
    // -------------------------------------------------------------------------

    /// Build a `UnbindAllAckEnvelope` acknowledging a DC's UNBIND_ALL.
    ///
    /// R17 R1-HIGH-4 fix: previously the nonce was hardcoded to `[0u8; 32]`
    /// (trivially replayable). The method now takes `&mut self` so the
    /// orchestrator's nonce generator can produce a fresh nonce.
    pub fn build_unbind_all_ack(
        &mut self,
        witness_key: &SigningKey,
        unbind_hash: [u8; 32],
        domain_id: [u8; 32],
        witness_epoch: u64,
    ) -> UnbindAllAckEnvelope {
        let mut ack = UnbindAllAckEnvelope {
            domain_id,
            unbind_hash,
            witness_id: witness_key.verifying_key().to_bytes(),
            witness_epoch,
            ack_hash: [0u8; 32],
            nonce: self.fresh_nonce(),
            signature: [0u8; 64],
        };
        ack.sign(witness_key);
        ack
    }

    // -------------------------------------------------------------------------
    // Registry helpers
    // -------------------------------------------------------------------------

    /// Apply the `Creating → Bound` transition after a successful
    /// `CreateGroupDoneEnvelope` and ≥ 1 witness BIND ACK.
    pub fn complete_cgroup(
        &self,
        registry: &mut GroupRegistry,
        platform: &str,
        group_jid: &str,
        renewed_at_epoch: u64,
        binding_hash: [u8; 32],
    ) -> Result<(), BindingError> {
        registry.transition_to_bound(platform, group_jid, renewed_at_epoch, binding_hash)
    }

    /// Apply the `Creating → Unbound` transition after a CGROUP_FAIL
    /// or CGROUP timeout. The binding is removed from the registry.
    ///
    /// R17 R1-LOW-6 fix: previously this method discarded the
    /// synthetic `UnbindEnvelope` returned by
    /// `GroupRegistry::transition_to_unbound` (via `let _ = ...`).
    /// The envelope is now returned to the caller so they can sign
    /// and broadcast it. Without this, a `CGROUP_FAIL` leaves no
    /// wire-level record of the failure — the registry silently
    /// deletes the binding and the group keeps existing on the
    /// platform side.
    pub fn fail_cgroup(
        &self,
        registry: &mut GroupRegistry,
        platform: &str,
        group_jid: &str,
    ) -> Result<UnbindEnvelope, BindingError> {
        registry.transition_to_unbound(platform, group_jid)
    }

    /// Apply the `Creating → UnboundQuarantined` transition on
    /// `SELF_KICKED` or `KICK_DETECTED` mid-create. The DC emits slash
    /// 0x000E (`CreateGroupFailed`).
    pub fn quarantine_cgroup(
        &self,
        registry: &mut GroupRegistry,
        platform: &str,
        group_jid: &str,
        current_epoch: u64,
    ) -> Result<(), BindingError> {
        registry.transition_to_unbound_quarantined(
            platform,
            group_jid,
            current_epoch,
            UnbindAuthority::SlashVote,
            self.config.recovery_window_epochs,
        )
    }
}

// -----------------------------------------------------------------------------
// UnbindAuthority re-export for the convenience of the dc.rs callers
// (the original definition lives in super::binding to keep the
// module dependency graph acyclic).
// -----------------------------------------------------------------------------
pub use super::binding::UnbindAuthority as _UnbindAuthority;

#[cfg(test)]
mod tests {
    use super::*;

    /// Deterministic test nonce generator (returns a counter-based
    /// value, not cryptographically random; for tests only).
    fn test_nonce_fn() -> impl FnMut() -> [u8; 32] + Send + Sync + 'static {
        let mut counter: u64 = 0;
        move || {
            counter += 1;
            let mut n = [0u8; 32];
            n[0..8].copy_from_slice(&counter.to_be_bytes());
            n
        }
    }

    fn test_orchestrator() -> DcOrchestrator {
        let key = SigningKey::from_bytes(&[1u8; 32]);
        DcOrchestrator::new(key, DcConfig::default(), test_nonce_fn())
    }

    #[test]
    fn dc_pubkey_matches_key() {
        let orch = test_orchestrator();
        let expected = SigningKey::from_bytes(&[1u8; 32])
            .verifying_key()
            .to_bytes();
        assert_eq!(orch.dc_pubkey(), expected);
    }

    #[test]
    fn create_group_envelope_is_valid() {
        let mut orch = test_orchestrator();
        let env = orch
            .build_create_group(
                [1u8; 32],
                [2u8; 32],
                "whatsapp",
                b"{}".to_vec(),
                GroupVisibility::Private,
                100,
                1,
                5,
            )
            .unwrap();
        assert!(env.verify(&orch.dc_key.verifying_key()).is_ok());
    }

    #[test]
    fn create_group_rejects_too_many_invites() {
        let mut orch = test_orchestrator();
        let res = orch.build_create_group(
            [1u8; 32],
            [2u8; 32],
            "whatsapp",
            b"{}".to_vec(),
            GroupVisibility::Private,
            100,
            1,
            10_000,
        );
        assert!(res.is_err());
    }

    #[test]
    fn founder_race_tiebreak() {
        let orch = test_orchestrator();
        // Local < remote: LocalWins
        assert_eq!(
            orch.handle_founder_race(&[0u8; 32], &[1u8; 32]),
            RaceOutcome::LocalWins
        );
        // Remote < local: RemoteWins
        assert_eq!(
            orch.handle_founder_race(&[1u8; 32], &[0u8; 32]),
            RaceOutcome::RemoteWins
        );
        // Equal: RemoteWins (deterministic; the second is "remote")
        assert_eq!(
            orch.handle_founder_race(&[0u8; 32], &[0u8; 32]),
            RaceOutcome::RemoteWins
        );
    }

    #[test]
    fn kick_dc_triggers_handover() {
        let orch = test_orchestrator();
        let d = orch.handle_kick([1u8; 32], true, false, 1);
        assert_eq!(d, KickDecision::DcKicked);
    }

    #[test]
    fn kick_witness_triggers_kick_detected() {
        let orch = test_orchestrator();
        let d = orch.handle_kick([1u8; 32], false, true, 1);
        assert!(matches!(d, KickDecision::WitnessKicked { .. }));
    }

    #[test]
    fn kick_member_triggers_member_removed() {
        let orch = test_orchestrator();
        let d = orch.handle_kick([1u8; 32], false, false, 1);
        assert!(matches!(d, KickDecision::MemberRemoved { .. }));
    }

    #[test]
    fn mass_kick_triggers_unbind_all() {
        let orch = test_orchestrator();
        let d = orch.handle_kick([1u8; 32], false, false, 3);
        assert!(matches!(d, KickDecision::MassKick { count: 3 }));
    }

    #[test]
    fn invite_envelope_is_valid() {
        let mut orch = test_orchestrator();
        let inv = orch.build_invite(
            [1u8; 32],
            [2u8; 32],
            "g1@g.us".into(),
            "whatsapp".into(),
            [3u8; 32],
            100,
        );
        assert!(inv.verify(&orch.dc_key.verifying_key()).is_ok());
    }

    #[test]
    fn unbind_all_envelope_is_valid() {
        let mut orch = test_orchestrator();
        let env = orch.build_unbind_all(
            [1u8; 32],
            "g1@g.us".into(),
            "whatsapp".into(),
            [4u8; 32],
            UnbindReason::Scheduled,
            100,
            1,
        );
        assert!(env.verify(&orch.dc_key.verifying_key()).is_ok());
    }

    #[test]
    fn unbind_all_ack_envelope_is_valid() {
        let mut orch = test_orchestrator();
        let wkey = SigningKey::from_bytes(&[2u8; 32]);
        let ack = orch.build_unbind_all_ack(&wkey, [5u8; 32], [1u8; 32], 101);
        assert!(ack.verify(&wkey.verifying_key()).is_ok());
    }

    #[test]
    fn unbind_all_ack_nonce_varies_per_call() {
        // R17 R1-HIGH-4 regression: ack must carry a fresh nonce each call,
        // not the same `[0u8; 32]` every time (which would be trivially
        // replayable).
        let mut orch = test_orchestrator();
        let wkey = SigningKey::from_bytes(&[2u8; 32]);
        let a1 = orch.build_unbind_all_ack(&wkey, [5u8; 32], [1u8; 32], 101);
        let a2 = orch.build_unbind_all_ack(&wkey, [5u8; 32], [1u8; 32], 101);
        assert_ne!(a1.nonce, a2.nonce);
    }

    #[test]
    fn third_party_bind_uses_witness_assertion() {
        // R17 R1-HIGH-5 regression: build_third_party_bind must actually
        // use the witness_assertion parameter (verify it, then tie the
        // bind to it via witness_seal). Previously the parameter was
        // silently discarded with `let _ = witness_assertion`.
        let mut orch = test_orchestrator();

        // Build a valid witness assertion.
        let wkey = SigningKey::from_bytes(&[7u8; 32]);
        let mut assertion = WitnessAssertion {
            subject_hash: [0xABu8; 32],
            witness_id: wkey.verifying_key().to_bytes(),
            witness_epoch: 50,
            assertion_hash: [0u8; 32],
            signature: [0u8; 64],
        };
        assertion.sign(&wkey);

        let result = orch
            .build_third_party_bind(
                "legacy@g.us".into(),
                "whatsapp".into(),
                [1u8; 32],
                [2u8; 32],
                &assertion,
                60, // current_epoch (assertion is fresh)
            )
            .expect("third-party bind should succeed");
        // Envelope signed by DC.
        assert!(result.envelope.verify(&orch.dc_key.verifying_key()).is_ok());
        // witness_seal ties the bind to the assertion.
        let mut seal_buf = Vec::with_capacity(64);
        seal_buf.extend_from_slice(&result.envelope.bind_hash);
        seal_buf.extend_from_slice(&assertion.assertion_hash);
        let expected_seal = *blake3::hash(&seal_buf).as_bytes();
        assert_eq!(result.witness_seal, expected_seal);
    }

    #[test]
    fn third_party_bind_rejects_forged_assertion() {
        // R17 R1-HIGH-5 regression: a forged assertion (signed by a
        // different key than the one claimed in `witness_id`) must be
        // rejected.
        let mut orch = test_orchestrator();
        let wkey = SigningKey::from_bytes(&[7u8; 32]);
        let mut assertion = WitnessAssertion {
            subject_hash: [0xABu8; 32],
            witness_id: wkey.verifying_key().to_bytes(),
            witness_epoch: 50,
            assertion_hash: [0u8; 32],
            signature: [0u8; 64],
        };
        // Sign with a DIFFERENT key — signature will not verify.
        let other_key = SigningKey::from_bytes(&[8u8; 32]);
        assertion.sign(&other_key);

        let result = orch.build_third_party_bind(
            "legacy@g.us".into(),
            "whatsapp".into(),
            [1u8; 32],
            [2u8; 32],
            &assertion,
            60,
        );
        assert!(matches!(
            result,
            Err(BindingError::InvalidAssertion { reason: _ })
        ));
    }

    #[test]
    fn third_party_bind_rejects_stale_assertion() {
        // R17 R1-HIGH-5 regression: an assertion whose witness_epoch is
        // too far in the past must be rejected.
        let mut orch = test_orchestrator();
        let wkey = SigningKey::from_bytes(&[7u8; 32]);
        let mut assertion = WitnessAssertion {
            subject_hash: [0xABu8; 32],
            witness_id: wkey.verifying_key().to_bytes(),
            witness_epoch: 1, // very old
            assertion_hash: [0u8; 32],
            signature: [0u8; 64],
        };
        assertion.sign(&wkey);
        let result = orch.build_third_party_bind(
            "legacy@g.us".into(),
            "whatsapp".into(),
            [1u8; 32],
            [2u8; 32],
            &assertion,
            1000, // current_epoch far ahead
        );
        assert!(matches!(
            result,
            Err(BindingError::InvalidAssertion { reason: _ })
        ));
    }

    #[test]
    fn complete_cgroup_transitions_to_bound() {
        let orch = test_orchestrator();
        let mut reg = GroupRegistry::new();
        let b = GroupBinding {
            group_jid: "g1@g.us".into(),
            platform: "whatsapp".into(),
            mission_id: [1u8; 32],
            domain_id: [2u8; 32],
            domain_coordinator_id: orch.dc_pubkey(),
            bound_at_epoch: 100,
            renewed_at_epoch: 100,
            state: GroupState::Unbound,
            binding_hash: [0u8; 32],
        };
        reg.register_binding(b).unwrap();
        reg.transition_to_creating("whatsapp", "g1@g.us").unwrap();
        orch.complete_cgroup(&mut reg, "whatsapp", "g1@g.us", 200, [42u8; 32])
            .unwrap();
        let b = reg.lookup_by_group("whatsapp", "g1@g.us").unwrap();
        assert_eq!(b.state, GroupState::Bound);
        assert_eq!(b.renewed_at_epoch, 200);
    }

    #[test]
    fn quarantine_cgroup_transitions_to_unbound_quarantined() {
        let orch = test_orchestrator();
        let mut reg = GroupRegistry::new();
        let b = GroupBinding {
            group_jid: "g1@g.us".into(),
            platform: "whatsapp".into(),
            mission_id: [1u8; 32],
            domain_id: [2u8; 32],
            domain_coordinator_id: orch.dc_pubkey(),
            bound_at_epoch: 100,
            renewed_at_epoch: 100,
            state: GroupState::Unbound,
            binding_hash: [0u8; 32],
        };
        reg.register_binding(b).unwrap();
        reg.transition_to_creating("whatsapp", "g1@g.us").unwrap();
        orch.quarantine_cgroup(&mut reg, "whatsapp", "g1@g.us", 150)
            .unwrap();
        assert_eq!(reg.quarantine_len(), 1);
        assert!(reg.lookup_by_group("whatsapp", "g1@g.us").is_none());
    }

    #[test]
    fn fail_cgroup_returns_unbind_envelope() {
        // R17 R1-LOW-6 regression: fail_cgroup used to discard the
        // synthetic UnbindEnvelope. It must return it so the caller
        // can sign and broadcast it on CGROUP_FAIL.
        let orch = test_orchestrator();
        let mut reg = GroupRegistry::new();
        let b = GroupBinding {
            group_jid: "g1@g.us".into(),
            platform: "whatsapp".into(),
            mission_id: [1u8; 32],
            domain_id: [2u8; 32],
            domain_coordinator_id: orch.dc_pubkey(),
            bound_at_epoch: 100,
            renewed_at_epoch: 100,
            state: GroupState::Unbound,
            binding_hash: [0u8; 32],
        };
        reg.register_binding(b).unwrap();
        reg.transition_to_creating("whatsapp", "g1@g.us").unwrap();
        let env = orch
            .fail_cgroup(&mut reg, "whatsapp", "g1@g.us")
            .expect("fail_cgroup should return the synthetic UnbindEnvelope");
        // The envelope references the now-deleted binding.
        assert_eq!(env.group_jid, "g1@g.us");
        assert_eq!(env.platform, "whatsapp");
        // Registry no longer has the binding.
        assert!(reg.lookup_by_group("whatsapp", "g1@g.us").is_none());
    }
}
