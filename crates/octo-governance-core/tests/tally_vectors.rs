//! Canonical tally test vectors (RFC-0013 §Cross-Replica Tally
//! Equivalence + §Test Vectors).
//!
//! 10 vectors exercising the substrate `voting_weight` + `tally_quorum`
//! pure-function suite. All vectors assert byte-identical cross-replica
//! outputs given identical inputs (BTreeMap-ordered iteration).
//!
//! Run with:
//!   cargo test -p octo-governance-core --test tally_vectors

use std::collections::BTreeMap;

use octo_governance_core::error::GovernanceError;
use octo_governance_core::tally::{tally_quorum, voting_weight};

#[test]
fn vector_01_voting_weight_identity() {
    // 5000 bps stake, 10000 bps multiplier (1x) → 5000 bps.
    assert_eq!(voting_weight(5000, 10_000), 5000);
}

#[test]
fn vector_02_voting_weight_double() {
    // 5000 bps stake, 20000 bps multiplier (2x) → 10000 bps (capped).
    assert_eq!(voting_weight(5000, 20_000), 10_000);
}

#[test]
fn vector_03_voting_weight_capped_above_100pct() {
    // 5000 bps stake * 50_000 bps multiplier (5x) = 25M bps → capped 10_000.
    assert_eq!(voting_weight(5000, 50_000), 10_000);
}

#[test]
fn vector_04_voting_weight_zero_stake() {
    // 0 bps stake, any multiplier → 0 bps.
    assert_eq!(voting_weight(0, 10_000), 0);
    assert_eq!(voting_weight(0, 50_000), 0);
}

#[test]
fn vector_05_tally_empty_map_zero_zero() {
    // No votes cast → (0, 0).
    let votes = BTreeMap::new();
    assert_eq!(tally_quorum(&votes).unwrap(), (0, 0));
}

#[test]
fn vector_06_tally_majority_approval() {
    // Two voters: 6000 for, 4000 against → (6000, 4000).
    let mut votes = BTreeMap::new();
    votes.insert("did:oct:v06-a".to_owned(), (6000, true));
    votes.insert("did:oct:v06-b".to_owned(), (4000, false));
    assert_eq!(tally_quorum(&votes).unwrap(), (6000, 4000));
}

#[test]
fn vector_07_tally_unanimous_rejection() {
    // Three voters all against → (0, 10_000).
    let mut votes = BTreeMap::new();
    votes.insert("did:oct:v07-a".to_owned(), (4000, false));
    votes.insert("did:oct:v07-b".to_owned(), (3000, false));
    votes.insert("did:oct:v07-c".to_owned(), (3000, false));
    assert_eq!(tally_quorum(&votes).unwrap(), (0, 10_000));
}

#[test]
fn vector_08_tally_saturating_sum() {
    // Approval = 9_000 + rejection = 9_000 → (9_000, 9_000). No overflow.
    let mut votes = BTreeMap::new();
    votes.insert("did:oct:v08-a".to_owned(), (9000, true));
    votes.insert("did:oct:v08-b".to_owned(), (9000, false));
    assert_eq!(tally_quorum(&votes).unwrap(), (9000, 9000));
}

#[test]
fn vector_09_tally_invalid_weight_rejects() {
    // 15_000 bps weight per voter → exceeds 100% cap → InvalidWeight.
    let mut votes = BTreeMap::new();
    votes.insert("did:oct:v09-bad".to_owned(), (15_000, true));
    assert!(matches!(
        tally_quorum(&votes),
        Err(GovernanceError::InvalidWeight { .. })
    ));
}

#[test]
fn vector_10_tally_btreemap_order_deterministic() {
    // Two BTreeMaps constructed in different insertion orders yield
    // identical tally totals — proves the BTreeMap iteration
    // determinism invariant required for cross-replica consensus.
    let mut votes_a = BTreeMap::new();
    votes_a.insert("did:oct:v10-b".to_owned(), (5000, true));
    votes_a.insert("did:oct:v10-a".to_owned(), (5000, false));
    votes_a.insert("did:oct:v10-c".to_owned(), (5000, true));
    let mut votes_b = BTreeMap::new();
    votes_b.insert("did:oct:v10-c".to_owned(), (5000, true));
    votes_b.insert("did:oct:v10-a".to_owned(), (5000, false));
    votes_b.insert("did:oct:v10-b".to_owned(), (5000, true));
    assert_eq!(
        tally_quorum(&votes_a).unwrap(),
        tally_quorum(&votes_b).unwrap()
    );
    // Both produce (10000, 5000).
    assert_eq!(tally_quorum(&votes_a).unwrap(), (10_000, 5000));
}
