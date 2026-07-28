//! Presentation-layer derivation of the canonical `0-100` Reputation Score.
//!
//! Per RFC-0968-A1 §22 (amendment 30) + mission 0968-b Phase C acceptance.
//! The presentation value is a *read-time* derivation from `score_ewma`
//! and NEVER feeds protocol calculations (routing priority, election
//! deprioritization, severity suspension, election_priority adapter).
//!
//! ## Formula
//!
//! `u8 = round(((score_ewma + 1.0) * 50.0).clamp(0.0, 100.0))`
//!
//! A non-finite `score_ewma` (NaN, +Inf, -Inf) is rejected with
//! `ReputationError::ScoreEncodingInvalid` BEFORE any arithmetic.
//!
//! ## Arithmetic discipline
//!
//! The upstream `octo_determin::Dfp` type does not expose `Add`/`Sub`/`Mul`/
//! `Div` trait impls as of `determin` v1.0 (only `from_f64` / `to_f64`).
//! All existing reputation computation paths (EWMA update in `store/memory`
//! and `store/stoolap`) likewise perform arithmetic in `f64` then
//! convert back via `Dfp::from_f64`. The presentation function follows
//! the same discipline: the non-finite rejection happens on the `Dfp`
//! class field BEFORE the cast; the multiplication + clamp + round
//! happen in `f64` against the canonical `to_f64` representation. The
//! resulting `u8` is therefore byte-deterministic across replicas for
//! the same `Dfp` input, satisfying the RFC-0104 bit-determinism
//! contract for protocol operations.
//!
//! ## Why a separate `Result` signature
//!
//! Earlier versions returned `u8` directly and propagated 0 / 100 silently
//! on corrupted inputs. The post-Round 7 / persistence-5 signature changes
//! to `Result<u8, ReputationError>` so callers (CLI, marketplace listing
//! display) can surface the error rather than display a misleading score.

use octo_determin::{Dfp, DfpClass};

use crate::error::ReputationError;

/// Derive the presentation-layer `0-100` Reputation Score.
///
/// # Errors
///
/// - `ReputationError::ScoreEncodingInvalid` when `score_ewma` is NaN,
///   +Inf, or -Inf (RFC-0968-A1 §22 requires non-finite rejection BEFORE
///   arithmetic).
///
/// # Returns
///
/// `Ok(u8)` in `0..=100` for any finite `score_ewma` (clamped + rounded).
pub fn reputation_score_0_100(score_ewma: Dfp) -> Result<u8, ReputationError> {
    // Reject non-finite inputs BEFORE any arithmetic so a corrupted
    // score does not silently propagate as 0 or 100.
    if matches!(score_ewma.class, DfpClass::NaN | DfpClass::Infinity) {
        return Err(ReputationError::ScoreEncodingInvalid);
    }

    // Finite input (Zero or Normal): cast to f64, apply the formula,
    // clamp + round. to_f64() is bit-deterministic per RFC-0104, so two
    // replicas with the same Dfp input produce identical u8 output.
    let raw = (score_ewma.to_f64() + 1.0) * 50.0;
    let clamped = raw.clamp(0.0, 100.0);
    // round() is half-away-from-zero per Rust std.
    Ok(clamped.round() as u8)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn finite_neg() -> Dfp {
        Dfp::from_f64(-1.0)
    }
    fn finite_neg_small() -> Dfp {
        Dfp::from_f64(-0.001)
    }
    fn finite_zero() -> Dfp {
        Dfp::from_f64(0.0)
    }
    fn finite_one() -> Dfp {
        Dfp::from_f64(1.0)
    }

    // ----- Per-mission AC boundary tests -----

    #[test]
    fn nan_returns_err() {
        assert_eq!(
            reputation_score_0_100(Dfp::nan()),
            Err(ReputationError::ScoreEncodingInvalid)
        );
    }

    #[test]
    fn pos_inf_returns_err() {
        assert_eq!(
            reputation_score_0_100(Dfp::infinity()),
            Err(ReputationError::ScoreEncodingInvalid)
        );
    }

    #[test]
    fn neg_inf_returns_err() {
        assert_eq!(
            reputation_score_0_100(Dfp::neg_infinity()),
            Err(ReputationError::ScoreEncodingInvalid)
        );
    }

    #[test]
    fn neg_001_maps_to_50() {
        // (-0.001 + 1.0) * 50.0 = 49.95 → round to 50
        assert_eq!(reputation_score_0_100(finite_neg_small()), Ok(50));
    }

    #[test]
    fn zero_maps_to_50() {
        assert_eq!(reputation_score_0_100(finite_zero()), Ok(50));
    }

    #[test]
    fn one_maps_to_100() {
        assert_eq!(reputation_score_0_100(finite_one()), Ok(100));
    }

    #[test]
    fn clamp_lower_bound() {
        // score_ewma = -2.0 → (-1.0) * 50.0 = -50 → clamped to 0
        assert_eq!(reputation_score_0_100(Dfp::from_f64(-2.0)), Ok(0));
    }

    #[test]
    fn clamp_upper_bound() {
        // score_ewma = 2.0 → (3.0) * 50.0 = 150 → clamped to 100
        assert_eq!(reputation_score_0_100(Dfp::from_f64(2.0)), Ok(100));
    }

    #[test]
    fn neg_one_maps_to_zero() {
        // (-1.0 + 1.0) * 50.0 = 0 → round to 0
        assert_eq!(reputation_score_0_100(finite_neg()), Ok(0));
    }

    // ----- RFC-0968 §22 101-unique-values property -----

    #[test]
    fn reputation_score_0_100_unique_finite_values() {
        // Spaced evenly over [-1.0, 1.0]: i / 100 for i ∈ 0..=100, then
        // 2.0 * x - 1.0 to map [0,1] → [-1,1]. All 101 inputs are finite
        // so each call returns Ok(u8); the 101 results should be exactly
        // the set {0, 1, ..., 100}.
        let mut values: Vec<u8> = Vec::with_capacity(101);
        for i in 0..=100u32 {
            let x = (i as f64) / 100.0;
            let score = Dfp::from_f64(2.0 * x - 1.0);
            let r = reputation_score_0_100(score);
            values.push(r.expect("all 101 inputs are finite, must return Ok"));
        }
        // The presentation function is monotonically non-decreasing over
        // [-1, 1] (linear + clamp + round); collect the unique values and
        // assert the set is exactly {0, ..., 100}.
        let unique: std::collections::BTreeSet<u8> = values.iter().copied().collect();
        let expected: std::collections::BTreeSet<u8> = (0u8..=100).collect();
        assert_eq!(
            unique, expected,
            "presentation layer must cover the full 0..=100 range for finite inputs"
        );
        assert_eq!(values.len(), 101);
    }

    #[test]
    fn non_finite_returns_err_collection() {
        // Explicit collection of the 3 boundary cases the AC names.
        let cases = [Dfp::nan(), Dfp::infinity(), Dfp::neg_infinity()];
        for c in cases {
            assert_eq!(
                reputation_score_0_100(c),
                Err(ReputationError::ScoreEncodingInvalid),
                "non-finite Dfp must return ScoreEncodingInvalid"
            );
        }
    }

    #[test]
    fn byte_deterministic_across_replays() {
        // RFC-0104 determinism: two calls with the same Dfp must agree.
        let d = Dfp::from_f64(0.371);
        let a = reputation_score_0_100(d).unwrap();
        let b = reputation_score_0_100(d).unwrap();
        assert_eq!(a, b);
    }
}
