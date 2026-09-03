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
use ed25519_dalek::{Verifier, VerifyingKey};

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

/// Verifying-keys map for governance multi-sig: `(adjudicator_id,
/// verifying_key)`. Slash proof verifies iff at least one `(id, vk)`
/// pair matches `proof.adjudicator` AND `vk.verify(payload, signature)`
/// succeeds.
pub type GovernanceVerifyingKeys<'a> = &'a [([u8; 32], VerifyingKey)];

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

    // (d) Adjudicator signature verification.
    let payload = slash_proof_payload(proof);
    let signature = ed25519_dalek::Signature::from_bytes(&proof.adjudicator_signature);
    let verified = governance_set.iter().find_map(|(id, vk)| {
        if *id == proof.adjudicator {
            vk.verify(&payload, &signature).ok()
        } else {
            None
        }
    });
    if verified.is_none() {
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

    // TV-SL-1: valid slash → Active → Demoting → Inactive.
    #[test]
    fn tv_sl_1_valid_slash_progression() {
        let proof = sample_proof_with_signature(0x0001, 1_000); // DoubleSign
        let sk = keypair_from_seed([0x77u8; 32]);
        let vk = sk.verifying_key();
        let governance_set = vec![([0x77u8; 32], vk)];
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
        let governance_set = vec![([0x77u8; 32], vk)];
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
        let governance_set = vec![([0x77u8; 32], sk.verifying_key())];
        let err = verify_slash_proof(&proof, &record, &governance_set).unwrap_err();
        assert_eq!(err, CoordinatorError::UnknownOffenseCode { got: 0x000C });
    }

    // Supplementary: penalty exceeds locked stake → StakeUnderflow.
    #[test]
    fn tv_sl_supplementary_penalty_underflow() {
        let proof = sample_proof_with_signature(0x0001, 20_000);
        let record = sample_record();
        let sk = keypair_from_seed([0x77u8; 32]);
        let governance_set = vec![([0x77u8; 32], sk.verifying_key())];
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
        let governance_set = vec![([0x77u8; 32], sk.verifying_key())];
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
        governance_set: &'a [([u8; 32], VerifyingKey)],
        record: &CoordinatorRecord,
    ) -> Result<SlashProof, CoordinatorError> {
        verify_slash_proof(proof, record, governance_set)?;
        Ok(proof.clone())
    }
}
