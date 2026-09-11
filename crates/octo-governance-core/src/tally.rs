//! Pure tally helpers per RFC-0013 §Cross-Replica Tally Equivalence.
//!
//! These helpers are **pure functions** — no IO, no clock, no randomness.
//! Given identical inputs, all replicas MUST produce identical outputs
//! (this is what makes the cross-replica tally invariant hold).

use std::collections::BTreeMap;

/// Compute the voting weight assigned to a single voter given its
/// stake weight and any pre-applied multipliers. Returns the
/// post-multiplier weight in basis points (0..=10000).
///
/// # Determinism
///
/// Pure function — `BTreeMap`-ordered iteration ensures all replicas
/// produce identical sums given identical inputs.
pub fn voting_weight(stake_bps: u32, multiplier_bps: u32) -> u32 {
    let product = (stake_bps as u64) * (multiplier_bps as u64);
    // Cap at 10000 bps (100%) — a single voter cannot exceed 100%.
    (product / 10_000).min(10_000) as u32
}

/// Tally all votes and return `(approval_bps, rejection_bps)` totals.
/// `votes` maps voter DID → `(weight_bps, approve_bool)`.
///
/// # Determinism
///
/// `BTreeMap` over voter DIDs ensures ordered iteration — all replicas
/// produce identical totals byte-for-byte given identical inputs.
///
/// # Errors
///
/// Returns `GovernanceError::InvalidWeight` if any individual voter
/// weight exceeds 10_000 bps.
pub fn tally_quorum(
    votes: &BTreeMap<String, (u32, bool)>,
) -> Result<(u32, u32), crate::error::GovernanceError> {
    let mut approval = 0u32;
    let mut rejection = 0u32;
    for (did, (weight, approve)) in votes {
        if *weight > 10_000 {
            return Err(crate::error::GovernanceError::InvalidWeight {
                voter: did.clone(),
                weight: *weight,
            });
        }
        if *approve {
            approval = approval.saturating_add(*weight);
        } else {
            rejection = rejection.saturating_add(*weight);
        }
        // Total votes cannot exceed 100% (caller should clamp before
        // passing; we just guard against absurd inputs here).
        if approval.saturating_add(rejection) > 100_000 {
            return Err(crate::error::GovernanceError::InvalidWeight {
                voter: did.clone(),
                weight: approval.saturating_add(rejection),
            });
        }
    }
    Ok((approval, rejection))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn voting_weight_identity() {
        assert_eq!(voting_weight(5000, 10_000), 5000);
    }

    #[test]
    fn voting_weight_double() {
        assert_eq!(voting_weight(5000, 20_000), 10_000);
    }

    #[test]
    fn voting_weight_capped() {
        // 5000 bps * 50_000 multiplier = 25M bps = 2500% — capped to 100%.
        assert_eq!(voting_weight(5000, 50_000), 10_000);
    }

    #[test]
    fn tally_empty() {
        let votes = BTreeMap::new();
        assert_eq!(tally_quorum(&votes).unwrap(), (0, 0));
    }

    #[test]
    fn tally_majority_approval() {
        let mut votes = BTreeMap::new();
        votes.insert("did:oct:a".to_owned(), (6000, true));
        votes.insert("did:oct:b".to_owned(), (4000, false));
        assert_eq!(tally_quorum(&votes).unwrap(), (6000, 4000));
    }

    #[test]
    fn tally_rejection() {
        let mut votes = BTreeMap::new();
        votes.insert("did:oct:a".to_owned(), (4000, false));
        votes.insert("did:oct:b".to_owned(), (3000, false));
        votes.insert("did:oct:c".to_owned(), (3000, true));
        assert_eq!(tally_quorum(&votes).unwrap(), (3000, 7000));
    }

    #[test]
    fn tally_invalid_weight_rejects() {
        let mut votes = BTreeMap::new();
        votes.insert("did:oct:bad".to_owned(), (15_000, true));
        assert!(matches!(
            tally_quorum(&votes),
            Err(crate::error::GovernanceError::InvalidWeight { .. })
        ));
    }

    #[test]
    fn tally_deterministic_btreemap_order() {
        // BTreeMap iteration is sorted by key — gives byte-identical
        // results across replicas.
        let mut votes_a = BTreeMap::new();
        votes_a.insert("did:oct:b".to_owned(), (5000, true));
        votes_a.insert("did:oct:a".to_owned(), (5000, false));
        let mut votes_b = BTreeMap::new();
        votes_b.insert("did:oct:a".to_owned(), (5000, false));
        votes_b.insert("did:oct:b".to_owned(), (5000, true));
        assert_eq!(
            tally_quorum(&votes_a).unwrap(),
            tally_quorum(&votes_b).unwrap()
        );
    }
}
