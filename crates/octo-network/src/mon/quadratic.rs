//! Stake-weighted quadratic-cost voting (mission 0855p-b-stake-weighted-quadratic).
//!
//! `voting_weight = sqrt(stake) * cosigners`
//!
//! - `stake`: the candidate's OCTO-O stake in the mission
//!   (per RFC-0855 §13 "Token Economics Integration")
//! - `cosigners`: the count of distinct `CosignEnvelope` signatures
//!   on the candidate's `CoordinatorRecord` (excluding the
//!   candidate's own self-attestation)
//!
//! The square root dampens the influence of large stakeholders:
//! a candidate with 4× the stake has only 2× the voting weight.
//! The `cosigners` multiplier rewards social trust.
//!
//! ## Per-model application
//!
//! - **Centralized**: voting weight is irrelevant (the designator
//!   picks). This module's `voting_weight` returns None for
//!   Centralized.
//! - **DAO / Federated**: use the formula.
//! - **AiAssisted / Autonomous**: depends on deployment; default
//!   to using the formula.

use serde::{Deserialize, Serialize};

use super::governance::GovernanceModel;

/// A candidate for coordinator election.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CoordinatorCandidate {
    /// The candidate's pubkey.
    pub pubkey: String,
    /// The candidate's stake (OCTO-O tokens).
    pub stake: u64,
    /// The number of distinct cosigners on the candidate's
    /// `CoordinatorRecord` (excluding self).
    pub cosigners: u32,
}

impl CoordinatorCandidate {
    /// Compute the raw voting weight: `sqrt(stake) * cosigners`.
    /// We use integer arithmetic to avoid floating point: the
    /// square root is computed using a Babylon-style integer
    /// approximation (Newton's method on u128).
    pub fn raw_voting_weight(&self) -> u64 {
        // Use u128 throughout to avoid overflow on large stakes.
        let sqrt_stake = isqrt(self.stake as u128) as u64;
        // Multiply carefully.
        (sqrt_stake as u128 * self.cosigners as u128).min(u64::MAX as u128) as u64
    }
}

/// Election result (one candidate wins).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ElectionResult {
    pub winner: CoordinatorCandidate,
    pub voting_weight: u64,
    pub model: GovernanceModel,
}

/// Compute the voting weight for a candidate under a given
/// governance model.
///
/// - `Centralized`: returns `None` (the designator picks; weight
///   is irrelevant).
/// - `Dao` / `Federated` / `AiAssisted` / `Autonomous`: returns
///   the raw `sqrt(stake) * cosigners` value.
pub fn voting_weight(candidate: &CoordinatorCandidate, model: GovernanceModel) -> Option<u64> {
    match model {
        GovernanceModel::Centralized => None,
        _ => Some(candidate.raw_voting_weight()),
    }
}

/// Hold an election: select the candidate with the highest
/// voting weight.
///
/// Returns `None` if `candidates` is empty.
///
/// For `Centralized` governance, the result is the first
/// candidate (the designator's choice). Per mission text, the
/// designator "picks"; the election module respects that.
pub fn elect(
    candidates: &[CoordinatorCandidate],
    model: GovernanceModel,
) -> Option<ElectionResult> {
    if candidates.is_empty() {
        return None;
    }
    match model {
        GovernanceModel::Centralized => {
            // Centralized: the first candidate is the designator's
            // pick. (The actual choice is external; this module
            // just respects the contract.)
            let winner = candidates[0].clone();
            Some(ElectionResult {
                winner,
                voting_weight: 0,
                model,
            })
        }
        _ => {
            let mut best: Option<(&CoordinatorCandidate, u64)> = None;
            for c in candidates {
                let w = c.raw_voting_weight();
                best = match best {
                    None => Some((c, w)),
                    Some((_, bw)) if w > bw => Some((c, w)),
                    // Tie-break: lower pubkey wins.
                    Some((bc, bw)) if w == bw => {
                        if c.pubkey < bc.pubkey {
                            Some((c, w))
                        } else {
                            Some((bc, bw))
                        }
                    }
                    Some(other) => Some(other),
                };
            }
            let (winner, voting_weight) = best.unwrap();
            Some(ElectionResult {
                winner: winner.clone(),
                voting_weight,
                model,
            })
        }
    }
}

/// Integer square root (floor). Uses Newton's method on u128.
/// Result fits in u64 for inputs up to 2^126.
///
/// Note: callers should only pass values that fit in u64 (e.g.,
/// a `u64` stake cast to `u128`). The function panics on
/// `n + 1` overflow when `n == u128::MAX`; this is acceptable
/// since the public API takes a u64 stake and the maximum
/// input is therefore `u64::MAX as u128`.
fn isqrt(n: u128) -> u128 {
    if n == 0 {
        return 0;
    }
    let mut x = n;
    let mut y = (x + 1) / 2;
    while y < x {
        x = y;
        y = (x + n / x) / 2;
    }
    x
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cand(pubkey: &str, stake: u64, cosigners: u32) -> CoordinatorCandidate {
        CoordinatorCandidate {
            pubkey: pubkey.into(),
            stake,
            cosigners,
        }
    }

    #[test]
    fn raw_voting_weight_basic() {
        // sqrt(100) = 10; * 1 = 10
        let c = cand("a", 100, 1);
        assert_eq!(c.raw_voting_weight(), 10);
    }

    #[test]
    fn raw_voting_weight_quadratic_dampening() {
        // 4x stake gives 2x weight, not 4x.
        let c1 = cand("a", 100, 1); // sqrt(100) = 10
        let c2 = cand("b", 400, 1); // sqrt(400) = 20
        assert_eq!(c1.raw_voting_weight(), 10);
        assert_eq!(c2.raw_voting_weight(), 20);
    }

    #[test]
    fn raw_voting_weight_cosigners_multiplier() {
        // Same stake, 10x cosigners → 10x weight.
        let c1 = cand("a", 100, 1);
        let c2 = cand("b", 100, 10);
        assert_eq!(c1.raw_voting_weight(), 10);
        assert_eq!(c2.raw_voting_weight(), 100);
    }

    #[test]
    fn voting_weight_dao_uses_formula() {
        let c = cand("a", 100, 2);
        assert_eq!(voting_weight(&c, GovernanceModel::Dao), Some(20));
    }

    #[test]
    fn voting_weight_centralized_returns_none() {
        let c = cand("a", 100, 2);
        assert_eq!(voting_weight(&c, GovernanceModel::Centralized), None);
    }

    #[test]
    fn voting_weight_federated_uses_formula() {
        let c = cand("a", 100, 2);
        assert_eq!(voting_weight(&c, GovernanceModel::Federated), Some(20));
    }

    #[test]
    fn elect_picks_highest_weight() {
        let cands = vec![cand("a", 100, 1), cand("b", 100, 5), cand("c", 400, 1)];
        let r = elect(&cands, GovernanceModel::Dao).unwrap();
        // b: sqrt(100)*5 = 50, c: sqrt(400)*1 = 20, a: 10
        assert_eq!(r.winner.pubkey, "b");
        assert_eq!(r.voting_weight, 50);
    }

    #[test]
    fn elect_tie_break_lower_pubkey() {
        let cands = vec![cand("z", 100, 1), cand("a", 100, 1)];
        let r = elect(&cands, GovernanceModel::Dao).unwrap();
        assert_eq!(r.winner.pubkey, "a");
    }

    #[test]
    fn elect_centralized_picks_first() {
        let cands = vec![cand("a", 1, 100), cand("b", 1000000, 100)];
        let r = elect(&cands, GovernanceModel::Centralized).unwrap();
        // Centralized: first wins.
        assert_eq!(r.winner.pubkey, "a");
    }

    #[test]
    fn elect_empty_returns_none() {
        assert!(elect(&[], GovernanceModel::Dao).is_none());
    }

    #[test]
    fn isqrt_correctness() {
        assert_eq!(isqrt(0), 0);
        assert_eq!(isqrt(1), 1);
        assert_eq!(isqrt(4), 2);
        assert_eq!(isqrt(9), 3);
        assert_eq!(isqrt(16), 4);
        assert_eq!(isqrt(100), 10);
        assert_eq!(isqrt(99), 9);
        assert_eq!(isqrt(1000000), 1000);
    }

    #[test]
    fn isqrt_u64_max_no_overflow() {
        // isqrt(u64::MAX) must not panic. Property: r*r <= n < (r+1)^2.
        let n = u64::MAX as u128;
        let r = isqrt(n);
        assert!(r * r <= n);
        // The exact value: sqrt(2^64 - 1) ≈ 2^32.
        assert_eq!(r, (1u128 << 32) - 1);
    }

    #[test]
    fn isqrt_property_square_and_plus_one() {
        // For any k, isqrt(k*k) = k and isqrt(k*k+1) = k
        // (or = k+1 if k*k+1 is itself a perfect square,
        // which only happens for k=0).
        for k in [0u128, 1, 2, 3, 7, 10, 100, 1000, 1 << 32, 1 << 60] {
            let sq = k * k;
            assert_eq!(isqrt(sq), k, "isqrt({sq}) should be {k}");
            // k*k + 1: not a perfect square for k > 0.
            if k > 0 {
                assert_eq!(isqrt(sq + 1), k, "isqrt({}) should be {k}", sq + 1);
            }
        }
    }
}
