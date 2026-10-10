//! Mission Coordinator slashing substrate (Layer B per CLAUDE.md §Architectural
//! Principles).
//!
//! Canonical home per RFC-0855p-b §Implementation Phase 5 (L834-841) +
//! §Data Structures SlashProof (L218-230) + §Appendix B Slash Offense Codes
//! (L945+) for slash verification + Active → Demoting + Demoting →
//! Inactive transitions + cool-down tracking.
//!
//! ## Substrate surface
//!
//! - [`verify_slash_proof`] — full verification (offense ∈ canonical set,
//!   `coordinator_term_id` match, `penalty ≤ octo_o_stake_locked`,
//!   adjudicator signature against governance multi-sig).
//! - [`apply_slash`] — emits [`crate::SlashTallyUpdate`] + transitions
//!   `Active → Demoting` (then Demoting → Inactive after penalty applied).
//! - [`CoolDownTracker`] — coordinator → `cool_down_end_epoch` for
//!   re-election gate after `Resigned → Inactive`.
//!
//! Composition:
//! - [`crate::SlashReasonCode`] — canonical-set enum (RFC-0855p-b §Appendix B)
//!   + `try_from_reason_id` for reserved-range rejection.
//! - [`crate::SlashTallyUpdate::new`] — Layer B event carrier (added L56 of
//!   `crate::lib`).
//! - [`crate::state::SlashProof`] + [`crate::state::validate_transition`] —
//!   Phase 1 substrate.

use borsh::{BorshDeserialize, BorshSerialize};
use ed25519_dalek::VerifyingKey;
use octo_algo_ed25519::ed25519_algorithm_id;
use octo_ident::AlgorithmId;

use crate::state::{
    validate_transition, CoordinatorError, CoordinatorLifecycle, CoordinatorRecord, SlashProof,
};
use crate::{SlashReasonCode, SlashTallyUpdate};

// -----------------------------------------------------------------------------
// CoolDownTracker (per-coordinator cool-down gating re-election)
// -----------------------------------------------------------------------------

/// Cool-down tracker keyed by `CoordinatorId`. After `Resigned → Inactive`
/// (or post-slash `Demoting → Inactive`), a coordinator MUST NOT re-enter
/// the eligibility filter until `cool_down_end_epoch`.
///
/// Per RFC-0855p-b §Data Structures `slash_count` field + §Appendix A
/// mermaid `Resigned → Inactive` + `slash_count >= MAX_SLASHES_BEFORE_BAN`
/// ban gate.
#[derive(
    Clone,
    Debug,
    Default,
    PartialEq,
    Eq,
    BorshSerialize,
    BorshDeserialize,
    serde::Serialize,
    serde::Deserialize,
)]
pub struct CoolDownTracker {
    /// Per-coordinator cool-down end epochs.
    pub cool_downs: Vec<([u8; 32], u64)>,
}

impl CoolDownTracker {
    /// Empty tracker.
    pub fn new() -> Self {
        Self::default()
    }

    /// Register a cool-down end epoch for `coordinator`.
    pub fn register(&mut self, coordinator: [u8; 32], cool_down_end_epoch: u64) {
        self.cool_downs.push((coordinator, cool_down_end_epoch));
    }

    /// Returns `true` iff the coordinator's cool-down has expired by
    /// `current_epoch` (or no cool-down is registered).
    pub fn is_eligible(&self, coordinator: &[u8; 32], current_epoch: u64) -> bool {
        self.cool_downs
            .iter()
            .find(|(c, _)| c == coordinator)
            .is_none_or(|(_, end)| current_epoch >= *end)
    }

    /// Remove a cool-down entry (e.g., on governance override).
    pub fn clear(&mut self, coordinator: &[u8; 32]) {
        self.cool_downs.retain(|(c, _)| c != coordinator);
    }
}

// -----------------------------------------------------------------------------
// Slash proof payload (canonical bytes for signature verification)
// -----------------------------------------------------------------------------

/// Canonical payload bytes for `SlashProof` (excluding
/// `adjudicator_signature`). Used by both signing sites (sign these bytes)
/// and verifying sites (derive the same bytes from the wire-format
/// `SlashProof`).
///
/// Written by hand (no Borsh derive) because the payload struct contains
/// `&[u8]` (Borsh does not implement `BorshSerialize`/`BorshDeserialize`
/// for `&[u8]` / `&[u8; N]` references). Wire format is identical to
/// the Borsh encoding of an owned-mirror of the same fields.
pub fn slash_proof_payload(proof: &SlashProof) -> Vec<u8> {
    let mut out = Vec::with_capacity(32 + 32 + 32 + 2 + 4 + 8 + 32 + proof.evidence.len());
    out.extend_from_slice(&proof.slash_id);
    out.extend_from_slice(&proof.coordinator);
    out.extend_from_slice(&proof.coordinator_term_id);
    out.extend_from_slice(&proof.offense.to_le_bytes());
    out.extend_from_slice(&(proof.evidence.len() as u32).to_le_bytes());
    out.extend_from_slice(&proof.evidence);
    out.extend_from_slice(&proof.penalty.to_le_bytes());
    out.extend_from_slice(&proof.adjudicator);
    out
}

// -----------------------------------------------------------------------------
// verify_slash_proof (full verification)
// -----------------------------------------------------------------------------

/// Verifying-keys map for governance multi-sig:
/// `(adjudicator_id, algorithm_id, verifying_key)`. Slash proof verifies
/// iff at least one entry matches `proof.adjudicator` AND the signature
/// verifies under that entry's own algorithm.
///
/// The algorithm is part of the entry rather than assumed of it
/// (RFC-0009 AC#11). Use [`ed25519_governance_keys`] to build the
/// Ed25519-only form for callers that genuinely have nothing else.
pub type GovernanceVerifyingKeys<'a> = &'a [([u8; 32], AlgorithmId, VerifyingKey)];

/// Build the Ed25519-only governance key set from a plain `(id, key)` list.
///
/// Convenience, **not a fallback**: an entry naming an unregistered
/// algorithm is refused at verification time rather than quietly verified
/// under whatever this helper defaults to.
pub fn ed25519_governance_keys(
    keys: &[([u8; 32], VerifyingKey)],
) -> Vec<([u8; 32], AlgorithmId, VerifyingKey)> {
    keys.iter()
        .map(|(id, vk)| (*id, ed25519_algorithm_id(), *vk))
        .collect()
}

/// Verify a slash proof against the §Phase 5 substrate.
///
/// Returns `Ok(())` on valid slash; one of the following errors on failure:
/// - [`CoordinatorError::UnknownOffenseCode`] — `offense` not in RFC
///   §Appendix B canonical set (incl. reserved `0x000C..=0x000D` rejection).
/// - [`CoordinatorError::TermIdMismatch`] — `coordinator_term_id` mismatch.
/// - [`CoordinatorError::StakeUnderflow`] — `penalty > octo_o_stake_locked`.
/// - [`CoordinatorError::InvalidAdjudicatorSignature`] — adjudication
///   signature did not verify against any governance multi-sig key.
///
/// Successful verification does NOT trigger the state transition —
/// call [`apply_slash`] for that.
pub fn verify_slash_proof(
    proof: &SlashProof,
    record: &CoordinatorRecord,
    governance_set: GovernanceVerifyingKeys<'_>,
) -> Result<(), CoordinatorError> {
    // (a) Offense code canonical check.
    let reason = SlashReasonCode::try_from_reason_id(proof.offense)
        .map_err(|_| CoordinatorError::UnknownOffenseCode { got: proof.offense })?;
    let _ = reason; // Used implicitly via canonical-set rejection.

    // (b) Term ID match.
    if proof.coordinator_term_id != record.coordinator_term_id {
        return Err(CoordinatorError::TermIdMismatch {
            proof: proof.coordinator_term_id,
            record: record.coordinator_term_id,
        });
    }

    // (c) Penalty ≤ locked stake.
    if proof.penalty > record.octo_o_stake_locked {
        return Err(CoordinatorError::StakeUnderflow {
            available: record.octo_o_stake_locked,
            requested: proof.penalty,
        });
    }

    // (d) Adjudicator signature verification, dispatched on the algorithm the
    // matching governance key was registered under (RFC-0009 AC#11).
    //
    // Raw bytes, not a dalek `Signature`: building one first would assert
    // Ed25519 before the registry had a say, and an unregistered algorithm
    // would fail as a malformed signature rather than as an unknown one.
    let payload = slash_proof_payload(proof);
    let verified = governance_set.iter().any(|(id, algorithm, vk)| {
        *id == proof.adjudicator
            && octo_algo_ed25519::verify_dispatch(
                algorithm,
                vk.as_bytes(),
                &payload,
                &proof.adjudicator_signature,
            )
            .is_ok()
    });
    if !verified {
        return Err(CoordinatorError::InvalidAdjudicatorSignature);
    }

    Ok(())
}

// -----------------------------------------------------------------------------
// apply_slash (state transition + SlashTallyUpdate emission)
// -----------------------------------------------------------------------------

/// Apply a verified slash to a [`CoordinatorRecord`]:
/// - `Active → Demoting` transition (via Phase 1 `validate_transition`).
/// - After penalty applied: `Demoting → Inactive`.
/// - Decrements `octo_o_stake_locked` by `proof.penalty`.
/// - Increments `slash_count`.
/// - Registers a cool-down via [`CoolDownTracker::register`] using the
///   exponential-backoff formula `cool_down_end = current_epoch +
///   2^slash_count` per RFC-0855p-b L401 + L608 ("2^slash_count epochs
///   before eligible for re-election").
/// - Emits [`SlashTallyUpdate`] via the existing Layer B constructor.
///
/// Caller MUST have already called [`verify_slash_proof`] and gotten `Ok`.
pub fn apply_slash(
    record: &mut CoordinatorRecord,
    proof: &SlashProof,
    cool_down_tracker: &mut CoolDownTracker,
    current_epoch: u64,
) -> Result<SlashTallyUpdate, CoordinatorError> {
    // Active → Demoting transition.
    if record.state == CoordinatorLifecycle::Active {
        validate_transition(record, CoordinatorLifecycle::Demoting)?;
        record.state = CoordinatorLifecycle::Demoting;
    } else {
        return Err(CoordinatorError::InvalidTransition {
            from: record.state,
            to: CoordinatorLifecycle::Demoting,
        });
    }

    // Apply penalty.
    record.octo_o_stake_locked = record
        .octo_o_stake_locked
        .checked_sub(proof.penalty)
        .ok_or(CoordinatorError::StakeUnderflow {
            available: record.octo_o_stake_locked,
            requested: proof.penalty,
        })?;

    // Increment slash_count.
    record.slash_count =
        record
            .slash_count
            .checked_add(1)
            .ok_or(CoordinatorError::SlashCountOverflow {
                current: record.slash_count,
            })?;

    // Demoting → Inactive. Register exponential-backoff cool-down
    // (RFC-0855p-b L401 + L608): `2^slash_count` epochs from `current_epoch`.
    validate_transition(record, CoordinatorLifecycle::Inactive)?;
    let cool_down_shift =
        1u64.checked_shl(record.slash_count)
            .ok_or(CoordinatorError::SlashCountOverflow {
                current: record.slash_count,
            })?;
    let cool_down_end =
        current_epoch
            .checked_add(cool_down_shift)
            .ok_or(CoordinatorError::SlashCountOverflow {
                current: record.slash_count,
            })?;
    cool_down_tracker.register(record.coordinator_peer_id, cool_down_end);
    record.state = CoordinatorLifecycle::Inactive;

    // Emit SlashTallyUpdate.
    let reason = SlashReasonCode::try_from_reason_id(proof.offense)
        .unwrap_or(SlashReasonCode::Extension(proof.offense));
    let witness_count_u16 = u16::try_from(record.slash_count)
        .expect("slash_count fits u16 in substrate invariant (u32::MAX >> u16::MAX)");
    Ok(SlashTallyUpdate::new(
        reason.reason_id(),
        proof.coordinator,
        witness_count_u16,
        record.term_end_epoch,
    ))
}

// -----------------------------------------------------------------------------
// Inline unit tests
// -----------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::{CoordinatorSource, SlashProof};
    use ed25519_dalek::SigningKey;

    fn keypair_from_seed(seed: [u8; 32]) -> SigningKey {
        SigningKey::from_bytes(&seed)
    }

    fn sample_record() -> CoordinatorRecord {
        CoordinatorRecord {
            coordinator_peer_id: [0xAAu8; 32],
            state: CoordinatorLifecycle::Active,
            term_start_epoch: 100,
            term_end_epoch: 200,
            source: CoordinatorSource::Election,
            coordinator_term_id: [0x33u8; 32],
            slash_count: 0,
            octo_o_stake_locked: 10_000,
            last_heartbeat_epoch: 100,
            heartbeat_interval: 50,
        }
    }

    fn sample_proof_with_signature(offense: u16, penalty: u64) -> SlashProof {
        let mut proof = SlashProof {
            slash_id: [0x99u8; 32],
            coordinator: [0xAAu8; 32],
            coordinator_term_id: [0x33u8; 32],
            offense,
            evidence: vec![0x01, 0x02, 0x03],
            penalty,
            adjudicator: [0x77u8; 32],
            adjudicator_signature: [0u8; 64],
        };
        let payload = slash_proof_payload(&proof);
        let sk = keypair_from_seed([0x77u8; 32]);
        use ed25519_dalek::Signer;
        let sig = sk.sign(&payload);
        proof.adjudicator_signature = sig.to_bytes();
        proof
    }

    // ── AC#11: dispatch on the adjudicator key's declared algorithm ─────────
    use ed25519_dalek::Signer;

    /// The happy path with the algorithm stated explicitly on the entry.
    ///
    /// **Control:** this alone cannot distinguish dispatch from a hardcoded
    /// Ed25519 — a verifier that ignored the recorded algorithm entirely
    /// would also pass it. The unregistered-algorithm vector below is what
    /// makes the pair meaningful.
    #[test]
    fn tv_sl_ac11_dispatch_on_the_declared_algorithm_verifies() {
        let proof = sample_proof_with_signature(0x0001, 1_000);
        let sk = keypair_from_seed([0x77u8; 32]);
        let governance_set = vec![([0x77u8; 32], ed25519_algorithm_id(), sk.verifying_key())];
        let record = sample_record();
        assert!(verify_slash_proof(&proof, &record, &governance_set).is_ok());
    }

    /// The fail-closed half, and the control that proves the dispatch is real.
    ///
    /// A genuine Ed25519 signature, a matching adjudicator id, a valid key —
    /// the only thing wrong is that the entry names an algorithm this
    /// deployment has not registered. It must be refused, not verified
    /// anyway. Before the change this was inexpressible: the key set was a
    /// list of `(id, VerifyingKey)`, so the algorithm was whatever the type
    /// said and no entry could disagree with it.
    #[test]
    fn tv_sl_ac11_refuses_an_unregistered_algorithm() {
        let proof = sample_proof_with_signature(0x0001, 1_000);
        let sk = keypair_from_seed([0x77u8; 32]);
        let bogus = AlgorithmId::derive("not-a-registered-slashing-algorithm");
        assert_ne!(bogus, ed25519_algorithm_id());
        let governance_set = vec![([0x77u8; 32], bogus, sk.verifying_key())];
        let record = sample_record();
        let err = verify_slash_proof(&proof, &record, &governance_set).unwrap_err();
        assert!(
            matches!(err, CoordinatorError::InvalidAdjudicatorSignature),
            "an unregistered algorithm must fail closed, got {err:?}"
        );
    }

    /// Dispatch must not degrade the check: a corrupted signature under the
    /// *correct* algorithm is still refused.
    #[test]
    fn tv_sl_ac11_still_refuses_a_bad_signature() {
        let mut proof = sample_proof_with_signature(0x0001, 1_000);
        let sk = keypair_from_seed([0x77u8; 32]);
        // Sign the same payload with a different key.
        let other = keypair_from_seed([0x11u8; 32]);
        proof.adjudicator_signature = other
            .sign(&crate::slashing::slash_proof_payload(&proof))
            .to_bytes();
        let governance_set = ed25519_governance_keys(&[([0x77u8; 32], sk.verifying_key())]);
        let record = sample_record();
        assert!(verify_slash_proof(&proof, &record, &governance_set).is_err());
    }

    // TV-SL-1: valid slash → Active → Demoting → Inactive.
    #[test]
    fn tv_sl_1_valid_slash_progression() {
        let proof = sample_proof_with_signature(0x0001, 1_000); // DoubleSign
        let sk = keypair_from_seed([0x77u8; 32]);
        let vk = sk.verifying_key();
        let plain_set = vec![([0x77u8; 32], vk)];
        let governance_set = ed25519_governance_keys(&plain_set);
        let mut record = sample_record();
        let mut cool_down = CoolDownTracker::new();

        verify_slash_proof(&proof, &record, &governance_set).unwrap();
        let update = apply_slash(&mut record, &proof, &mut cool_down, 100).unwrap();
        assert_eq!(record.state, CoordinatorLifecycle::Inactive);
        assert_eq!(record.octo_o_stake_locked, 9_000);
        assert_eq!(record.slash_count, 1);
        assert_eq!(update.slash_reason_code, 0x0001);
        assert_eq!(update.slashed_peer_id, [0xAAu8; 32]);
        assert_eq!(update.witness_count, 1);
    }

    // TV-SL-2: Demoting → Inactive already covered by TV-SL-1; this is the
    // CoolDownTracker companion check.
    #[test]
    fn tv_sl_2_cool_down_tracker_register_and_check() {
        let mut tracker = CoolDownTracker::new();
        tracker.register([0xAAu8; 32], 200);
        assert!(!tracker.is_eligible(&[0xAAu8; 32], 100));
        assert!(!tracker.is_eligible(&[0xAAu8; 32], 199));
        assert!(tracker.is_eligible(&[0xAAu8; 32], 200));
        assert!(tracker.is_eligible(&[0xAAu8; 32], 250));
        tracker.clear(&[0xAAu8; 32]);
        assert!(tracker.is_eligible(&[0xAAu8; 32], 0));
    }

    // TV-SL-3: invalid signature → InvalidAdjudicatorSignature.
    #[test]
    fn tv_sl_3_invalid_adjudicator_signature_rejected() {
        let mut proof = sample_proof_with_signature(0x0001, 1_000);
        // Tamper signature.
        proof.adjudicator_signature[0] = 0xFF;
        let sk = keypair_from_seed([0x77u8; 32]);
        let vk = sk.verifying_key();
        let plain_set = vec![([0x77u8; 32], vk)];
        let governance_set = ed25519_governance_keys(&plain_set);
        let record = sample_record();
        let err = verify_slash_proof(&proof, &record, &governance_set).unwrap_err();
        assert_eq!(err, CoordinatorError::InvalidAdjudicatorSignature);
    }

    // TV-SL-4: unknown offense code → UnknownOffenseCode.
    #[test]
    fn tv_sl_4_unknown_offense_code_rejected() {
        // 0x000C is RESERVED per RFC-0855p-d; try_from_reason_id rejects.
        let proof = sample_proof_with_signature(0x000C, 100);
        let record = sample_record();
        let sk = keypair_from_seed([0x77u8; 32]);
        let plain_set = vec![([0x77u8; 32], sk.verifying_key())];
        let governance_set = ed25519_governance_keys(&plain_set);
        let err = verify_slash_proof(&proof, &record, &governance_set).unwrap_err();
        assert_eq!(err, CoordinatorError::UnknownOffenseCode { got: 0x000C });
    }

    // Supplementary: penalty exceeds locked stake → StakeUnderflow.
    #[test]
    fn tv_sl_supplementary_penalty_underflow() {
        let proof = sample_proof_with_signature(0x0001, 20_000);
        let record = sample_record();
        let sk = keypair_from_seed([0x77u8; 32]);
        let plain_set = vec![([0x77u8; 32], sk.verifying_key())];
        let governance_set = ed25519_governance_keys(&plain_set);
        let err = verify_slash_proof(&proof, &record, &governance_set).unwrap_err();
        match err {
            CoordinatorError::StakeUnderflow {
                available,
                requested,
            } => {
                assert_eq!(available, 10_000);
                assert_eq!(requested, 20_000);
            }
            _ => panic!("expected StakeUnderflow"),
        }
    }

    // Supplementary: term id mismatch → TermIdMismatch.
    #[test]
    fn tv_sl_supplementary_term_id_mismatch() {
        let mut proof = sample_proof_with_signature(0x0001, 100);
        proof.coordinator_term_id = [0xFFu8; 32];
        let record = sample_record();
        let sk = keypair_from_seed([0x77u8; 32]);
        let plain_set = vec![([0x77u8; 32], sk.verifying_key())];
        let governance_set = ed25519_governance_keys(&plain_set);
        let err = verify_slash_proof(&proof, &record, &governance_set).unwrap_err();
        match err {
            CoordinatorError::TermIdMismatch {
                proof: p,
                record: r,
            } => {
                assert_eq!(p, [0xFFu8; 32]);
                assert_eq!(r, [0x33u8; 32]);
            }
            _ => panic!("expected TermIdMismatch"),
        }
    }

    // unused-must-use helper to prevent dead-code warnings.
    fn _unused_helper<'a>(
        proof: &'a SlashProof,
        governance_set: &'a [([u8; 32], AlgorithmId, VerifyingKey)],
        record: &CoordinatorRecord,
    ) -> Result<SlashProof, CoordinatorError> {
        verify_slash_proof(proof, record, governance_set)?;
        Ok(proof.clone())
    }
}
