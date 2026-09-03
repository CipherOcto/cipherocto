//! Canonical test vectors for §Phase 2 election algorithm
//! (RFC-0855p-b §Election Algorithm L260-275).

use octo_coordinator_types::election::{
    elect_coordinator, tie_break, GovernanceModel, StakeEntry, VoterEligibility,
};
use octo_coordinator_types::state::{CoordinatorError, ElectionBallot};

fn ballot(voter: [u8; 32], candidate: [u8; 32], epoch: u64) -> ElectionBallot {
    ElectionBallot {
        voter_peer_id: voter,
        candidate_peer_id: candidate,
        ballot_epoch: epoch,
        signature: [0u8; 64],
    }
}

fn voter(v: [u8; 32]) -> VoterEligibility {
    VoterEligibility {
        voter: v,
        trust_score: 600,
        slash_count: 0,
        is_mission_participant: true,
    }
}

// TV-EL-1: DAO top-stake wins if no candidate >50% (TV-2 verbatim RFC L696-718).
#[test]
fn tv_el_1_dao_top_stake_if_no_majority() {
    let c1 = [0x01u8; 32];
    let c2 = [0x02u8; 32];
    let c3 = [0x03u8; 32];
    let stakes = vec![
        StakeEntry::new(c1, 5000),
        StakeEntry::new(c2, 3000),
        StakeEntry::new(c3, 2000),
    ];
    let ballots = vec![
        ballot([0xA1; 32], c1, 100),
        ballot([0xA2; 32], c2, 100),
        ballot([0xA3; 32], c3, 100),
    ];
    let voters = vec![voter([0xA1; 32]), voter([0xA2; 32]), voter([0xA3; 32])];
    let tally = elect_coordinator(
        GovernanceModel::Dao,
        [0x01u8; 32],
        100,
        &ballots,
        &stakes,
        &voters,
        None,
    )
    .unwrap();
    assert_eq!(tally.winner, c1);
}

// TV-EL-2: Centralized creator-designates-first.
#[test]
fn tv_el_2_centralized_creator_designates_first() {
    let designator = [0xBBu8; 32];
    let tally = elect_coordinator(
        GovernanceModel::Centralized,
        [0x01u8; 32],
        100,
        &[],
        &[],
        &[],
        Some(designator),
    )
    .unwrap();
    assert_eq!(tally.winner, designator);
}

// TV-EL-3: Federated f+1 of 2f+1 Byzantine consensus (7 reps; f=3, f+1=4).
#[test]
fn tv_el_3_federated_byzantine_quorum() {
    let reps: Vec<[u8; 32]> = (1..=7u8).map(|i| [i; 32]).collect();
    let candidate = [0xFFu8; 32];
    let other = [0xAAu8; 32];
    let ballots: Vec<ElectionBallot> = reps
        .iter()
        .enumerate()
        .map(|(i, &r)| {
            if i < 4 {
                ballot(r, candidate, 100)
            } else {
                ballot(r, other, 100)
            }
        })
        .collect();
    let voters: Vec<VoterEligibility> = reps.iter().map(|&v| voter(v)).collect();
    let tally = elect_coordinator(
        GovernanceModel::Federated,
        [0x01u8; 32],
        100,
        &ballots,
        &[],
        &voters,
        None,
    )
    .unwrap();
    assert_eq!(tally.winner, candidate);
    assert_eq!(tally.votes_received, 4);
}

// TV-EL-4: Lex tie-break — 2 candidates with identical stake.
#[test]
fn tv_el_4_lex_tie_break_identical_stake() {
    let low = [0x01u8; 32];
    let high = [0xFFu8; 32];
    let stakes = vec![StakeEntry::new(low, 1000), StakeEntry::new(high, 1000)];
    let ballots = vec![ballot([0xA1; 32], low, 100), ballot([0xA2; 32], high, 100)];
    let voters = vec![voter([0xA1; 32]), voter([0xA2; 32])];
    let tally = elect_coordinator(
        GovernanceModel::Dao,
        [0x01u8; 32],
        100,
        &ballots,
        &stakes,
        &voters,
        None,
    )
    .unwrap();
    assert_eq!(tally.winner, low); // Lex-min coordinator wins.
}

// TV-EL-5: Quorum failure (Federated below Byzantine threshold).
// 5 reps, quorum = floor(5/2)+1 = 3. Split votes 2/2/1 → no candidate
// reaches quorum → ElectionTimeout.
#[test]
fn tv_el_5_federated_quorum_timeout() {
    let reps: Vec<[u8; 32]> = (1..=5u8).map(|i| [i; 32]).collect();
    let ballots: Vec<ElectionBallot> = reps
        .iter()
        .enumerate()
        .map(|(i, &r)| {
            // 2 vote for A, 2 vote for B, 1 votes for C → no quorum for anyone.
            let candidate = match i {
                0..=1 => [0xAAu8; 32],
                2..=3 => [0xBBu8; 32],
                _ => [0xCCu8; 32],
            };
            ballot(r, candidate, 100)
        })
        .collect();
    let voters: Vec<VoterEligibility> = reps.iter().map(|&v| voter(v)).collect();
    let err = elect_coordinator(
        GovernanceModel::Federated,
        [0x01u8; 32],
        100,
        &ballots,
        &[],
        &voters,
        None,
    )
    .unwrap_err();
    assert!(matches!(err, CoordinatorError::ElectionTimeout { .. }));
}

// TV-EL-6: Empty ballot set → NoCandidates.
#[test]
fn tv_el_6_empty_ballot_set_no_candidates() {
    let err = elect_coordinator(GovernanceModel::Dao, [0x01u8; 32], 100, &[], &[], &[], None)
        .unwrap_err();
    assert_eq!(err, CoordinatorError::NoCandidates);
}

#[test]
fn tv_el_aux_tie_break_lex_byte_ordering() {
    let a = [0x11u8; 32];
    let b = [0x22u8; 32];
    let c = [0x33u8; 32];
    assert_eq!(tie_break(&[c, a, b]), a);
}
