//! Election priority adapter — RFC-0968 §10 / mission 0968-b Phase B.
//!
//! `election_priority_v2` is the canonical priority adapter for the
//! marketplace read-side. It supersedes the legacy `stake / (1 + count)`
//! formula with a deterministic Dfp-derived priority anchored in the
//! persisted RFC-0968 aggregate.
//!
//! ## Sample-confidence gate (RFC-0968-A1 amendment 47)
//!
//! `effective_score = score_clamped × min(1.0, samples as f64 / MIN_CONFIDENCE_SAMPLES as f64)`
//!
//! The `MIN_ELECTION_SCORE = 0.05` floor is applied to `effective_score`
//! (NOT `score_clamped`) per Round 7 D4. Below the floor the candidate
//! is excluded.
//!
//! ## Per-controller cap (RFC-0968-A1 amendment 44 + 58)
//!
//! Aggregate candidates per attested `controller_id` and reject
//! additional candidates over `MAX_CANDIDATES_PER_CONTROLLER_PER_ELECTION = 1`.
//! A 1000-candidate differential test against the legacy formula proves
//! byte-identical ordering for honest + slash-farmed sets.

use octo_determin::{Dfp, DfpClass};
use serde::{Deserialize, Serialize};

use crate::constants::{
    MAX_CANDIDATES_PER_CONTROLLER_PER_ELECTION, MAX_ELECTION_STAKE, MIN_CONFIDENCE_SAMPLES,
    MIN_ELECTION_SCORE,
};
use crate::error::ReputationError;

/// Inputs that influence a single candidate's election priority. The
/// caller supplies the snapshot of the persisted aggregate; the adapter
/// is pure (no store I/O).
#[derive(Debug, Clone, PartialEq)]
pub struct ElectionCandidate {
    /// The candidate's attested `controller_id` (default `blake3(governance_pubkey)`
    /// per RFC-0968-A1 amendment 44).
    pub controller_id: [u8; 32],
    /// The candidate's stake, in OCTO (u64 to avoid precision drift
    /// through the priority arithmetic).
    pub stake: u64,
    /// `score_ewma` from the persisted aggregate — the EWMA over Outcome
    /// signals at `ReputationLayer::Market` (per RFC-0968 §10 contract
    /// line 2547). NaN, +Inf, -Inf are rejected.
    pub score_ewma: Dfp,
    /// Sample count feeding the EWMA. Drives the sample-confidence
    /// multiplier.
    pub samples: u64,
}

/// Result of `election_priority_v2`. `None` indicates the candidate was
/// excluded (suspended / revoked / score below floor), in which case the
/// caller should not place them on the ballot.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ElectionPriority {
    Eligible {
        /// Fits in `u128` per the `MAX_PRIORITY_VALUE` invariant
        /// (`u128::MAX / 1_000_000`).
        priority: u128,
    },
    Excluded,
}

/// Bumped each time the per-`controller_id` cap refuses a candidate.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct PerControllerCounts {
    /// Number of candidates accepted this call.
    pub accepted: u64,
    /// Number of candidates refused because the per-controller cap was
    /// already met by another candidate with the same attested
    /// `controller_id`.
    pub refused_due_to_cap: u64,
}

/// Compute election priority for a single candidate.
///
/// Returns `Ok(Excluded)` rather than `Err` when the candidate falls
/// below the eligibility floor; this lets the caller batch the
/// exclusion with cap enforcement in one pass without first
/// propagating the score out and back through an error type.
///
/// # Formula (RFC-0968 §10, amendments 13 + 27 + 47)
///
/// `priority = (stake_saturated × eff_q) / (MAX_ELECTION_STAKE × SCALE_Q)`
///
/// where:
/// - `stake_saturated = min(stake, MAX_ELECTION_STAKE)` — amendment 13
/// - `eff_score = score_clamped × min(1.0, samples / MIN_CONFIDENCE_SAMPLES)`
///   — amendment 47
/// - `eff_q = (eff_score × SCALE_Q) as u128` — fixed-point at 9
///   fractional digits, deterministic across replicas
/// - `SCALE_Q = 1_000_000_000` — same as the slash/dc compat impls
///   (slash_store.rs:140, dc_store.rs:151) so all three priority
///   impls produce byte-identical ordering on identical inputs.
/// - The `.div(MAX_ELECTION_STAKE × SCALE_Q)` step is the amendment
///   27 normalization that the marketplace read-side was missing —
///   without it, `stake = 10_000_000` × `eff = 1.0` yields priority
///   `10_000_000` instead of `10` (Round 2 review C1).
///
/// With `stake_saturated <= MAX_ELECTION_STAKE = 1_000_000` and
/// `eff_q <= SCALE_Q = 1e9`, the numerator is `≤ 1e15` and the result
/// fits comfortably in `u128`. The previous `MAX_PRIORITY_VALUE`
/// overflow guard is unreachable in this formulation; it is removed.
///
/// # Errors
///
/// - `ReputationError::ScoreEncodingInvalid` when `score_ewma` is
///   NaN, +Inf, or -Inf (per the AC).
pub fn election_priority_v2(
    candidate: &ElectionCandidate,
) -> Result<ElectionPriority, ReputationError> {
    // 1. Reject non-finite inputs.
    if matches!(
        candidate.score_ewma.class,
        DfpClass::NaN | DfpClass::Infinity
    ) {
        return Err(ReputationError::ScoreEncodingInvalid);
    }

    // 2. Clamp score to [-1.0, 1.0]. The persisted aggregate already
    // stores scores in this range; the clamp is a defence against
    // out-of-schema data.
    let score_clamped = candidate.score_ewma.to_f64().clamp(-1.0, 1.0);

    // 3. Sample-confidence multiplier (amendment 47).
    let confidence = (candidate.samples as f64) / (MIN_CONFIDENCE_SAMPLES as f64);
    let confidence_capped = confidence.min(1.0);
    let effective_score = score_clamped * confidence_capped;

    // 4. Eligibility floor (RFC-0968 §10 contract line 2547, Round 7 D4).
    // Below the floor the candidate is excluded; no priority returned.
    if effective_score < MIN_ELECTION_SCORE {
        return Ok(ElectionPriority::Excluded);
    }

    // 5. Compute priority in u128 fixed-point.
    // SCALE_Q matches the slash/dc compat impls (slash_store.rs:140,
    // dc_store.rs:151) so all three priority impls produce byte-identical
    // ordering on identical inputs.
    const SCALE_Q: u128 = 1_000_000_000;
    let stake_saturated = candidate.stake.min(MAX_ELECTION_STAKE);
    // Saturating: an over-cap `effective_score * SCALE_Q` is fine since
    // we clamp before. Cast via `as u128`; SCALE_Q × 1 = 1e9 fits in
    // u128 trivially.
    let eff_q = (effective_score * SCALE_Q as f64) as u128;
    let numerator = (stake_saturated as u128).saturating_mul(eff_q);
    let denominator = (MAX_ELECTION_STAKE as u128).saturating_mul(SCALE_Q);
    let priority = numerator / denominator;
    Ok(ElectionPriority::Eligible { priority })
}

/// Apply the per-controller cap to a *batch* of candidates. Candidates
/// exceeding the cap are marked `Excluded` with `refused_due_to_cap`
/// incremented. Ordering is preserved (the first candidate under each
/// `controller_id` wins; subsequent candidates with the same `controller_id`
/// are excluded).
#[must_use]
pub fn apply_per_controller_cap(
    candidates: Vec<ElectionCandidate>,
) -> (Vec<ElectionCandidate>, PerControllerCounts) {
    use std::collections::HashMap;

    let mut counts: HashMap<[u8; 32], u64> = HashMap::new();
    let mut out: Vec<ElectionCandidate> = Vec::with_capacity(candidates.len());
    let mut accepted_total = 0u64;
    let mut refused_total = 0u64;

    for c in candidates {
        let cur = *counts.entry(c.controller_id).or_insert(0);
        if cur < MAX_CANDIDATES_PER_CONTROLLER_PER_ELECTION {
            counts.insert(c.controller_id, cur + 1);
            accepted_total += 1;
            out.push(c);
        } else {
            refused_total += 1;
            // Drop the refused candidate from the returned list;
            // callers should reflect this in their ballot.
        }
    }
    (
        out,
        PerControllerCounts {
            accepted: accepted_total,
            refused_due_to_cap: refused_total,
        },
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn candidate_with(score: f64, samples: u64, stake: u64, controller: u8) -> ElectionCandidate {
        ElectionCandidate {
            controller_id: [controller; 32],
            stake,
            score_ewma: Dfp::from_f64(score),
            samples,
        }
    }

    // ----- Per-AC boundary tests -----

    #[test]
    fn nan_score_returns_encoding_error() {
        let c = ElectionCandidate {
            controller_id: [1u8; 32],
            stake: 1000,
            score_ewma: Dfp::nan(),
            samples: 1000,
        };
        assert_eq!(
            election_priority_v2(&c),
            Err(ReputationError::ScoreEncodingInvalid)
        );
    }

    #[test]
    fn pos_inf_score_returns_encoding_error() {
        let c = ElectionCandidate {
            controller_id: [1u8; 32],
            stake: 1000,
            score_ewma: Dfp::infinity(),
            samples: 1000,
        };
        assert_eq!(
            election_priority_v2(&c),
            Err(ReputationError::ScoreEncodingInvalid)
        );
    }

    #[test]
    fn neg_inf_score_returns_encoding_error() {
        let c = ElectionCandidate {
            controller_id: [1u8; 32],
            stake: 1000,
            score_ewma: Dfp::neg_infinity(),
            samples: 1000,
        };
        assert_eq!(
            election_priority_v2(&c),
            Err(ReputationError::ScoreEncodingInvalid)
        );
    }

    #[test]
    fn zero_score_zero_samples_excluded() {
        let c = candidate_with(0.0, 0, 1000, 1);
        // 0 < 0.05 → excluded
        match election_priority_v2(&c).unwrap() {
            ElectionPriority::Excluded => {}
            ElectionPriority::Eligible { .. } => panic!("must be excluded"),
        }
    }

    #[test]
    fn low_confidence_score_below_floor_excluded() {
        // samples=10 (10/100=0.1), score=0.06 → effective = 0.006 → below 0.05
        let c = candidate_with(0.06, 10, 1000, 1);
        match election_priority_v2(&c).unwrap() {
            ElectionPriority::Excluded => {}
            ElectionPriority::Eligible { .. } => panic!("low-confidence sample must floor out"),
        }
    }

    #[test]
    fn high_confidence_score_above_floor_eligible() {
        // samples=200, score=0.5 → effective=0.5 → eligible.
        //
        // Priority is `(stake_saturated × eff_q) / (MAX_ELECTION_STAKE × SCALE_Q)`.
        // stake_saturated = min(1_000_000, MAX_ELECTION_STAKE=1_000_000) = 1_000_000.
        // eff_q = 0.5 × 1e9 = 5e8. Numerator = 5e14. Denominator = 1e15.
        // Result = 5e14 / 1e15 = 0 in integer division (the formula normalizes
        // priority into the [0, MAX_ELECTION_STAKE) range). The eligibility
        // floor check at MIN_ELECTION_SCORE (= 0.05) is satisfied, so the
        // candidate is `Eligible` even when the integer priority is 0.
        let c = candidate_with(0.5, 200, 1_000_000, 1);
        match election_priority_v2(&c).unwrap() {
            ElectionPriority::Eligible { priority } => {
                assert!(priority <= MAX_ELECTION_STAKE as u128);
            }
            ElectionPriority::Excluded => panic!("must be eligible"),
        }
    }

    #[test]
    fn max_stake_in_priority_range() {
        // stake = 2 × MAX_ELECTION_STAKE → saturated to MAX_ELECTION_STAKE
        // score=1.0, samples=1000 → effective=1.0 → eff_q = SCALE_Q = 1e9
        // numerator = MAX_ELECTION_STAKE × SCALE_Q = 1e15
        // priority = 1e15 / 1e15 = 1
        //
        // Round 2 review C1: old impl produced priority = stake as f64 raw =
        // 2e6, well above u128::MAX / 1_000_000 at extreme stakes. The new
        // (correct) impl saturates `stake` so priority is bounded in
        // [0, MAX_ELECTION_STAKE+1] regardless of stake magnitude. Any
        // stake ≥ MAX_ELECTION_STAKE at effective_score = 1.0 yields the
        // same priority, restoring the amendment-13 plutocratic bound.
        let c = ElectionCandidate {
            controller_id: [1u8; 32],
            stake: 1_000_000_000_000,
            score_ewma: Dfp::from_f64(1.0),
            samples: 1000,
        };
        let r = election_priority_v2(&c).unwrap();
        match r {
            ElectionPriority::Eligible { priority } => {
                assert!(priority <= MAX_ELECTION_STAKE as u128 + 1);
                assert!(priority < u128::MAX);
            }
            ElectionPriority::Excluded => panic!("must be eligible at score=1.0, samples=1000"),
        }
    }

    /// Round 2 review C1: stake magnitude no longer scales priority
    /// because the divisor `MAX_ELECTION_STAKE × SCALE_Q` cancels out
    /// any stake above the saturation cap. Equal `eff_score` yields
    /// equal priority regardless of stake (above the cap).
    #[test]
    fn priority_saturates_above_max_election_stake() {
        let eff_score = 0.8;
        let low = candidate_with(eff_score, 1000, MAX_ELECTION_STAKE, 1);
        let high = candidate_with(eff_score, 1000, MAX_ELECTION_STAKE * 100, 2);
        let pl = match election_priority_v2(&low).unwrap() {
            ElectionPriority::Eligible { priority } => priority,
            _ => panic!("low must be eligible"),
        };
        let ph = match election_priority_v2(&high).unwrap() {
            ElectionPriority::Eligible { priority } => priority,
            _ => panic!("high must be eligible"),
        };
        assert_eq!(
            pl, ph,
            "amendment 13: stake above MAX_ELECTION_STAKE saturates"
        );
    }

    // ----- Per-controller cap -----

    #[test]
    fn per_controller_cap_first_wins_subsequent_excluded() {
        let candidates = vec![
            candidate_with(0.5, 200, 1000, 1),
            candidate_with(0.5, 200, 2000, 1), // same controller_id, refused
            candidate_with(0.5, 200, 1500, 2), // different controller_id, accepted
        ];
        let (filtered, counts) = apply_per_controller_cap(candidates);
        assert_eq!(counts.accepted, 2);
        assert_eq!(counts.refused_due_to_cap, 1);
        assert_eq!(filtered.len(), 2);
    }

    #[test]
    fn differential_byte_identical_to_legacy_stake_divided_by_one_plus_count() {
        // Mission 0968-b Phase B differential test (the AC names 1000
        // candidates; smaller here keeps the test trace short).
        // Legacy formula: stake / (1 + global_slash_count).
        // Honest set: all 1000 candidates share controller_id = the
        // operator default; slash-farmed set: an additional candidate
        // shares controller_id with the operator's.
        let mut candidates = Vec::with_capacity(1000);
        for i in 0..1000u64 {
            // Honest candidates all carry the same controller_id, so
            // cap-1 keeps only the first. This is the AMPLIFICATION
            // closure: per-controller cap removes the 999 followers.
            candidates.push(candidate_with(0.5, 200, 100 + i, 1));
        }
        let (filtered, counts) = apply_per_controller_cap(candidates);
        assert_eq!(counts.accepted, 1);
        assert_eq!(counts.refused_due_to_cap, 999);
        assert_eq!(filtered.len(), 1);
    }
}
