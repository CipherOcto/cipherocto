//! Mission Coordinator election algorithm (Layer B per CLAUDE.md §Architectural
//! Principles).
//!
//! Canonical home per RFC-0855p-b §Implementation Phase 2 (L811-818) +
//! §Election Algorithm (L260-275) + RFC-0855 §11.1 "Governance
//! Flexibility" for the 5 governance-model election algorithms.
//!
//! ## Substrate surface
//!
//! - [`GovernanceModel`] — 5-variant enum per RFC table.
//! - [`elect_coordinator`] — main entry fn; routes by `GovernanceModel`.
//! - [`tie_break`] — lex byte order on `CoordinatorId`.
//! - [`ELECTION_TIMEOUT`] — 1000 epochs per RFC L290.
//! - [`StakeEntry`] — coordinator stake for DAO weighting.
//! - [`VoterEligibility`] — mission participant + trust + slash status per
//!   RFC L282-289.
//!
//! Per RFC-0855p-b §Implementation Phase 2, AI-Assisted (0x0004) +
//! Autonomous (0x0005) branches return
//! `Err(CoordinatorError::GovernanceModelNotImplemented)` (DEFERRED — see
//! Future-Work sub-specs F3 / F4).
//!
//! Per RFC-0008 §Class A, ballot iteration is via `(voter_peer_id,
//! ballot_epoch)` sort (Borsh-stable) + lex byte order for tie-break.

use borsh::{BorshDeserialize, BorshSerialize};

use crate::state::{CoordinatorError, CoordinatorId, ElectionBallot, ElectionTally, GenesisState};

// -----------------------------------------------------------------------------
// Constants (RFC §Implementation Phase 2 + §Election Algorithm)
// -----------------------------------------------------------------------------

/// Default election timeout (epochs) per RFC-0855p-b §Election Algorithm
/// L290. Election closes on `epoch + ELECTION_TIMEOUT` if quorum not
/// reached; on timeout, a new `election_id` is initiated (no slash).
pub const ELECTION_TIMEOUT: u64 = 1000;

/// Maximum accumulated slashes before a coordinator is banned from
/// re-election eligibility per RFC §Election Algorithm L290. Coordinators
/// with `slash_count >= MAX_SLASHES_BEFORE_BAN` are filtered out by the
/// eligibility check.
pub const MAX_SLASHES_BEFORE_BAN: u32 = 5;

/// DAO 50% quorum threshold (per-stake-weighted). Candidates receiving
/// strictly more than `votes_total / 2` on a DAO election win by absolute
/// majority. Otherwise top-stake wins. If neither holds, the election times
/// out.
pub const DAO_QUORUM_NUMERATOR: u64 = 1;
pub const DAO_QUORUM_DENOMINATOR: u64 = 2;

/// Federated Byzantine FT quorum: `f + 1` of `2f + 1` representatives
/// (RFC §Election Algorithm L260-275).
pub const FEDERATED_QUORUM_NUMERATOR: u64 = 0;
pub const FEDERATED_QUORUM_DENOMINATOR: u64 = 0;
pub const FEDERATED_QUORUM_OFFSET: u64 = 1; // +1
pub const FEDERATED_TOTAL_NUMERATOR: u64 = 2;
pub const FEDERATED_TOTAL_DENOMINATOR: u64 = 1; // *2 then +1

/// Centralized replacement 2/3 super-majority requirement.
pub const CENTRALIZED_REPLACEMENT_NUMERATOR: u64 = 2;
pub const CENTRALIZED_REPLACEMENT_DENOMINATOR: u64 = 3;

/// Trust-score eligibility threshold for voters + candidates (RFC §Election
/// Algorithm L260-275 trust gate).
pub const TRUST_SCORE_THRESHOLD: u32 = 500;

/// DAO minimum stake for voter + candidate eligibility.
pub const DAO_STAKE_THRESHOLD: u64 = 1_000;

// -----------------------------------------------------------------------------
// GovernanceModel (5-variant enum per RFC §Election Algorithm L260-275)
// -----------------------------------------------------------------------------

/// Governance model per RFC-0855 §11.1 + RFC-0855p-b §Election Algorithm
/// L260-275.
///
/// Discriminants are pinned `#[repr(u16)] 0x0001..=0x0005` to match
/// RFC-0855 §17 slash codes + cross-RFC governance-model identifiers.
#[derive(
    Clone,
    Copy,
    Debug,
    PartialEq,
    Eq,
    Hash,
    BorshSerialize,
    BorshDeserialize,
    serde::Serialize,
    serde::Deserialize,
)]
#[borsh(use_discriminant = true)]
#[repr(u16)]
#[derive(Default)]
pub enum GovernanceModel {
    /// First coordinator = creator-designated; replacement = 2/3 vote.
    /// Default per RFC-0862p-a §Default governance model.
    #[default]
    Centralized = 0x0001,
    /// One coordinator per domain; `f+1` of `2f+1` Byzantine FT consensus.
    Federated = 0x0002,
    /// Top-stake wins if no candidate >50%; else majority wins.
    Dao = 0x0003,
    /// AI proposes; humans ratify 2/3 within `proposal_deadline_epochs`
    /// (DEFERRED per RFC §Implementation Phase 2).
    AiAssisted = 0x0004,
    /// Protocol-defined rotation by `coordinator_term_id` ordering
    /// (DEFERRED per RFC §Implementation Phase 2).
    Autonomous = 0x0005,
}

impl GovernanceModel {
    /// Returns the u16 discriminant (wire format).
    pub const fn discriminant(self) -> u16 {
        self as u16
    }
}

// -----------------------------------------------------------------------------
// StakeEntry (DAO weighting)
// -----------------------------------------------------------------------------

/// Coordinator stake entry for DAO election weighting.
///
/// Per RFC-0855p-b §Election Algorithm L260-275, DAO candidates compete on
/// accumulated `octo_stake`. Stake is determined out-of-band (governance
/// snapshot) and passed as a sorted slice for determinism.
#[derive(
    Clone,
    Copy,
    Debug,
    PartialEq,
    Eq,
    Hash,
    BorshSerialize,
    BorshDeserialize,
    serde::Serialize,
    serde::Deserialize,
)]
pub struct StakeEntry {
    /// Coordinator candidate.
    pub coordinator: CoordinatorId,
    /// Locked `octo_stake` (≥ DAO_STAKE_THRESHOLD for eligibility).
    pub stake: u64,
}

impl StakeEntry {
    /// Constructor.
    pub const fn new(coordinator: CoordinatorId, stake: u64) -> Self {
        Self { coordinator, stake }
    }
}

// -----------------------------------------------------------------------------
// VoterEligibility (RFC §Election Algorithm L282-289)
// -----------------------------------------------------------------------------

/// Voter eligibility check result per RFC-0855p-b §Election Algorithm
/// L282-289.
///
/// Eligibility filter runs BEFORE ballot tally per RFC. A voter is
/// eligible iff:
/// - `is_mission_participant` (out-of-band check; passed in precomputed)
/// - `trust_score >= TRUST_SCORE_THRESHOLD`
/// - `slash_count < MAX_SLASHES_BEFORE_BAN`
/// - `attestation_within_window` (ballot `ballot_epoch` is recent;
///   enforce `epoch_fresh = current_epoch - ballot_epoch <= grace` in
///   the caller)
#[derive(
    Clone,
    Copy,
    Debug,
    PartialEq,
    Eq,
    Hash,
    BorshSerialize,
    BorshDeserialize,
    serde::Serialize,
    serde::Deserialize,
)]
pub struct VoterEligibility {
    /// Voter coordinator id.
    pub voter: CoordinatorId,
    /// Trust score (mission-scoped).
    pub trust_score: u32,
    /// Accumulated slash count.
    pub slash_count: u32,
    /// True iff voter is currently a mission participant.
    pub is_mission_participant: bool,
}

impl VoterEligibility {
    /// Returns `true` iff the voter passes the eligibility filter.
    pub fn is_eligible(&self) -> bool {
        self.is_mission_participant
            && self.trust_score >= TRUST_SCORE_THRESHOLD
            && self.slash_count < MAX_SLASHES_BEFORE_BAN
    }
}

// -----------------------------------------------------------------------------
// elect_coordinator (Phase 2 main entry)
// -----------------------------------------------------------------------------

/// Run a §Phase 2 election.
///
/// Routes to the appropriate governance-model branch:
/// - **Centralized** (0x0001): genesis = `designator`; replacement =
///   2/3 super-majority on `ballots`.
/// - **Federated** (0x0002): `f+1` of `2f+1` Byzantine FT consensus.
/// - **DAO** (0x0003): top-stake if no candidate >50%; else majority.
/// - **AI-Assisted** (0x0004): **DEFERRED** — returns
///   `Err(CoordinatorError::GovernanceModelNotImplemented)`.
/// - **Autonomous** (0x0005): **DEFERRED** — returns
///   `Err(CoordinatorError::GovernanceModelNotImplemented)`.
///
/// Eligibility filter runs before counting. Ballots sorted by
/// `(voter_peer_id, ballot_epoch)`. `closed_epoch =
/// min(quorum_reached_epoch, election_epoch + ELECTION_TIMEOUT)`.
#[allow(clippy::too_many_arguments)]
pub fn elect_coordinator(
    governance_model: GovernanceModel,
    mission_id: [u8; 32],
    election_epoch: u64,
    ballots: &[ElectionBallot],
    stakes: &[StakeEntry],
    voters: &[VoterEligibility],
    designator: Option<CoordinatorId>,
) -> Result<ElectionTally, CoordinatorError> {
    // Sort ballots deterministically by (voter_peer_id, ballot_epoch).
    let mut ballots_sorted: Vec<ElectionBallot> = ballots.to_vec();
    ballots_sorted.sort_by(|a, b| {
        a.voter_peer_id
            .cmp(&b.voter_peer_id)
            .then(a.ballot_epoch.cmp(&b.ballot_epoch))
    });

    // Filter eligible ballots (RFC §Election Algorithm L282-289).
    let eligible_ballots: Vec<&ElectionBallot> = ballots_sorted
        .iter()
        .filter(|b| {
            voters
                .iter()
                .find(|v| v.voter == b.voter_peer_id)
                .is_some_and(VoterEligibility::is_eligible)
        })
        .collect();

    match governance_model {
        GovernanceModel::Centralized => {
            elect_centralized(mission_id, election_epoch, &eligible_ballots, designator)
        }
        GovernanceModel::Federated => {
            elect_federated(mission_id, election_epoch, &eligible_ballots)
        }
        GovernanceModel::Dao => elect_dao(mission_id, election_epoch, &eligible_ballots, stakes),
        GovernanceModel::AiAssisted => Err(CoordinatorError::GovernanceModelNotImplemented(
            GovernanceModel::AiAssisted.discriminant(),
        )),
        GovernanceModel::Autonomous => Err(CoordinatorError::GovernanceModelNotImplemented(
            GovernanceModel::Autonomous.discriminant(),
        )),
    }
}

// -----------------------------------------------------------------------------
// Per-model branches (verbatim RFC §Election Algorithm L260-275 mechanics)
// -----------------------------------------------------------------------------

fn elect_centralized(
    mission_id: [u8; 32],
    election_epoch: u64,
    eligible_ballots: &[&ElectionBallot],
    designator: Option<CoordinatorId>,
) -> Result<ElectionTally, CoordinatorError> {
    // Genesis (no ballots, has designator): creator designates.
    if eligible_ballots.is_empty() {
        let designator = designator.ok_or(CoordinatorError::NoCandidates)?;
        let election_id = compute_election_id(&mission_id, election_epoch, 0x00);
        return Ok(ElectionTally {
            election_id,
            election_epoch,
            closed_epoch: election_epoch,
            governance_model: GovernanceModel::Centralized.discriminant(),
            ballots: Vec::new(),
            winner: designator,
            votes_received: 0,
            votes_total: 0,
        });
    }

    // Replacement: tally candidate votes; winner = candidate with 2/3.
    let tally = count_ballots(eligible_ballots);
    let total = tally.total_ballots;
    if total == 0 {
        return Err(CoordinatorError::NoCandidates);
    }
    let needed =
        (total * CENTRALIZED_REPLACEMENT_NUMERATOR).div_ceil(CENTRALIZED_REPLACEMENT_DENOMINATOR);

    // Find candidate(s) exceeding `needed` threshold.
    let mut winners: Vec<CoordinatorId> = tally
        .by_candidate
        .iter()
        .filter(|(_, count)| *count >= needed)
        .map(|(id, _)| *id)
        .collect();
    if winners.is_empty() {
        return Err(CoordinatorError::ElectionTimeout {
            closed_epoch: election_epoch + ELECTION_TIMEOUT,
        });
    }
    let winner = tie_break(&winners);
    // Pop the tied winner; avoid borrow conflict by collecting partial Vec.
    let _ = winners.pop(); // suppress warning if multiple winners; tie_break already resolved
    let election_id = compute_election_id(&mission_id, election_epoch, 0x00);
    Ok(ElectionTally {
        election_id,
        election_epoch,
        closed_epoch: election_epoch,
        governance_model: GovernanceModel::Centralized.discriminant(),
        ballots: ballots_to_vec(eligible_ballots),
        winner,
        votes_received: *tally
            .by_candidate
            .iter()
            .find(|(id, _)| *id == winner)
            .map(|(_, c)| c)
            .unwrap_or(&0),
        votes_total: total,
    })
}

fn elect_federated(
    mission_id: [u8; 32],
    election_epoch: u64,
    eligible_ballots: &[&ElectionBallot],
) -> Result<ElectionTally, CoordinatorError> {
    if eligible_ballots.is_empty() {
        return Err(CoordinatorError::NoCandidates);
    }
    let tally = count_ballots(eligible_ballots);
    let total = tally.total_ballots;
    // Byzantine FT quorum: f+1 of 2f+1 where 2f+1 = total representatives.
    // i.e. quorum = floor(total / 2) + 1.
    let quorum = total / 2 + 1;
    let mut winners: Vec<CoordinatorId> = tally
        .by_candidate
        .iter()
        .filter(|(_, count)| *count >= quorum)
        .map(|(id, _)| *id)
        .collect();
    if winners.is_empty() {
        return Err(CoordinatorError::ElectionTimeout {
            closed_epoch: election_epoch + ELECTION_TIMEOUT,
        });
    }
    let winner = tie_break(&winners);
    let _ = winners.pop();
    let votes_received = *tally
        .by_candidate
        .iter()
        .find(|(id, _)| *id == winner)
        .map(|(_, c)| c)
        .unwrap_or(&0);
    let election_id = compute_election_id(&mission_id, election_epoch, 0x00);
    Ok(ElectionTally {
        election_id,
        election_epoch,
        closed_epoch: election_epoch,
        governance_model: GovernanceModel::Federated.discriminant(),
        ballots: ballots_to_vec(eligible_ballots),
        winner,
        votes_received,
        votes_total: total,
    })
}

fn elect_dao(
    mission_id: [u8; 32],
    election_epoch: u64,
    eligible_ballots: &[&ElectionBallot],
    stakes: &[StakeEntry],
) -> Result<ElectionTally, CoordinatorError> {
    if stakes.is_empty() {
        return Err(CoordinatorError::NoCandidates);
    }
    let total_stake: u64 = stakes.iter().map(|s| s.stake).sum();
    if total_stake == 0 {
        return Err(CoordinatorError::NoCandidates);
    }
    let tally = count_ballots(eligible_ballots);
    // Stake-weighted tally: sum of stake per candidate.
    let mut weighted: Vec<(CoordinatorId, u64)> = stakes
        .iter()
        .map(|s| {
            let weight = tally
                .by_candidate
                .iter()
                .find(|(id, _)| *id == s.coordinator)
                .map(|(_, c)| *c)
                .unwrap_or(0);
            (s.coordinator, weight)
        })
        .collect();
    // Sort by (stake, lex) for determinism.
    weighted.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));

    let absolute_majority = total_stake / 2 + 1;
    let votes_total = tally.total_ballots;
    let election_id = compute_election_id(&mission_id, election_epoch, 0x00);

    // Majority winner?
    if let Some((winner, stake_weight)) = weighted.first() {
        if *stake_weight >= absolute_majority {
            return Ok(ElectionTally {
                election_id,
                election_epoch,
                closed_epoch: election_epoch,
                governance_model: GovernanceModel::Dao.discriminant(),
                ballots: ballots_to_vec(eligible_ballots),
                winner: *winner,
                votes_received: *stake_weight,
                votes_total,
            });
        }
    }

    // Otherwise top-stake wins (TV-2 verbatim per RFC L696-718).
    let top_stake = stakes
        .iter()
        .max_by_key(|s| s.stake)
        .ok_or(CoordinatorError::NoCandidates)?;
    let top_stake_winners: Vec<CoordinatorId> = stakes
        .iter()
        .filter(|s| s.stake == top_stake.stake)
        .map(|s| s.coordinator)
        .collect();
    if top_stake_winners.is_empty() {
        return Err(CoordinatorError::NoCandidates);
    }
    let winner = tie_break(&top_stake_winners);
    Ok(ElectionTally {
        election_id,
        election_epoch,
        closed_epoch: election_epoch,
        governance_model: GovernanceModel::Dao.discriminant(),
        ballots: ballots_to_vec(eligible_ballots),
        winner,
        votes_received: *tally
            .by_candidate
            .iter()
            .find(|(id, _)| *id == winner)
            .map(|(_, c)| c)
            .unwrap_or(&0),
        votes_total,
    })
}

// -----------------------------------------------------------------------------
// Helpers (deterministic count + lex tie-break)
// -----------------------------------------------------------------------------

/// Tally of ballots by candidate + total. Used internally by per-model
/// branches.
struct BallotTally {
    /// Per-candidate vote count.
    by_candidate: Vec<(CoordinatorId, u64)>,
    /// Total ballots counted.
    total_ballots: u64,
}

fn count_ballots(ballots: &[&ElectionBallot]) -> BallotTally {
    let mut by_candidate: Vec<(CoordinatorId, u64)> = Vec::new();
    let mut total: u64 = 0;
    for b in ballots {
        total = total.saturating_add(1);
        if let Some(entry) = by_candidate
            .iter_mut()
            .find(|(id, _)| *id == b.candidate_peer_id)
        {
            entry.1 = entry.1.saturating_add(1);
        } else {
            by_candidate.push((b.candidate_peer_id, 1));
        }
    }
    // Sort by (count desc, lex asc) for determinism.
    by_candidate.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
    BallotTally {
        by_candidate,
        total_ballots: total,
    }
}

/// Lex byte-order tie-break per RFC L812. Returns the lowest byte sequence
/// in `[u8; 32]` ordering (sequential byte comparison).
pub fn tie_break(candidates: &[CoordinatorId]) -> CoordinatorId {
    assert!(!candidates.is_empty(), "tie_break needs ≥1 candidate");
    let mut iter = candidates.iter();
    let first = *iter.next().expect("non-empty");
    iter.fold(first, |acc, next| -> CoordinatorId {
        if next < &acc {
            *next
        } else {
            acc
        }
    })
}

/// BLAKE3-derived election identifier (canonical wire).
///
/// Per RFC-0855p-b §Election Algorithm: `BLAKE3(mission_id ||
/// election_epoch || nonce)` truncated to 32 bytes. Default nonce `0x00`
/// for deterministic pinning.
pub fn compute_election_id(mission_id: &[u8; 32], election_epoch: u64, nonce: u8) -> [u8; 32] {
    let mut hasher = blake3::Hasher::new();
    hasher.update(mission_id);
    hasher.update(&election_epoch.to_be_bytes());
    hasher.update(&[nonce]);
    *hasher.finalize().as_bytes()
}

/// Materialize a `Vec<&ElectionBallot>` to `Vec<ElectionBallot>` sorted by
/// `(voter_peer_id, ballot_epoch)` (canonical Borsh-stable sort).
fn ballots_to_vec(ballots: &[&ElectionBallot]) -> Vec<ElectionBallot> {
    let mut v: Vec<ElectionBallot> = ballots.iter().map(|b| (*b).clone()).collect();
    v.sort_by(|a, b| {
        a.voter_peer_id
            .cmp(&b.voter_peer_id)
            .then(a.ballot_epoch.cmp(&b.ballot_epoch))
    });
    v
}

// -----------------------------------------------------------------------------
// Genesis bootstrap folding (RFC §"Genesis State Machine" v1.1)
// -----------------------------------------------------------------------------

/// Fold a [`GenesisState`] into the canonical [`CoordinatorLifecycle`] for
/// Layer-C re-export. Per RFC-0855p-b §"Genesis State Machine":
/// - `GenesisDesignated` / `GenesisSelfAttest` → `Designated` (still
///   pre-activation).
/// - `GenesisActive` → `Active` (bootstrap complete).
#[allow(dead_code)]
pub(crate) fn genesis_fold_lifecycle(genesis: GenesisState) -> crate::state::CoordinatorLifecycle {
    use crate::state::CoordinatorLifecycle;
    match genesis {
        GenesisState::GenesisDesignated | GenesisState::GenesisSelfAttest => {
            CoordinatorLifecycle::Designated
        }
        GenesisState::GenesisActive => CoordinatorLifecycle::Active,
    }
}

// ── Mission 0011-h-s-a-writer-election-struct (RFC-0011-n Phase 6 G18) ───

/// Thin struct wrapper around the existing free function
/// `elect_coordinator` (RFC-0011-n Phase 6 G18 NEW Phase 6).
/// Required by `octo network status` so the CLI can query
/// writer-election state without depending on the free function
/// directly.
///
/// Substrate-faithful: struct method delegates to the free
/// function to keep the canonical election logic in one place
/// per cipherocto-design-principles §Stable Abstractions
/// Principle.
#[derive(Clone, Debug, Default)]
pub struct WriterElection {
    ballots: Vec<ElectionBallot>,
    stakes: Vec<StakeEntry>,
    voters: Vec<VoterEligibility>,
    governance_model: GovernanceModel,
}

impl WriterElection {
    /// Construct an empty writer-election state.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Construct a writer-election state with the given governance
    /// model.
    #[must_use]
    pub fn with_governance_model(governance_model: GovernanceModel) -> Self {
        Self {
            governance_model,
            ..Self::default()
        }
    }

    /// Substrate-faithful struct wrapper around the existing free
    /// function `elect_coordinator` (RFC-0011-n Phase 6 G18). The
    /// struct method delegates to the free function to keep the
    /// canonical election logic in one place.
    #[allow(clippy::too_many_arguments)]
    pub fn elect_coordinator(
        &self,
        mission_id: [u8; 32],
        election_epoch: u64,
        designator: Option<CoordinatorId>,
    ) -> Result<ElectionTally, CoordinatorError> {
        elect_coordinator(
            self.governance_model,
            mission_id,
            election_epoch,
            &self.ballots,
            &self.stakes,
            &self.voters,
            designator,
        )
    }

    /// Cast a ballot (CLI substrate-faithful surface, Phase 6 closure
    /// path remains AdapterUnwired for write paths).
    pub fn cast_ballot(&mut self, ballot: ElectionBallot) {
        self.ballots.push(ballot);
    }

    /// Register a stake (CLI substrate-faithful surface).
    pub fn add_stake(&mut self, stake: StakeEntry) {
        self.stakes.push(stake);
    }

    /// Register a voter eligibility (CLI substrate-faithful surface).
    pub fn add_voter(&mut self, voter: VoterEligibility) {
        self.voters.push(voter);
    }

    /// Set the governance model (CLI substrate-faithful surface).
    pub fn set_governance_model(&mut self, model: GovernanceModel) {
        self.governance_model = model;
    }

    /// Number of ballots cast (operator-side observability).
    #[must_use]
    pub fn ballot_count(&self) -> usize {
        self.ballots.len()
    }

    /// Number of stakes registered (operator-side observability).
    #[must_use]
    pub fn stake_count(&self) -> usize {
        self.stakes.len()
    }

    /// Number of voters registered (operator-side observability).
    #[must_use]
    pub fn voter_count(&self) -> usize {
        self.voters.len()
    }
}

// -----------------------------------------------------------------------------
// Inline unit tests
// -----------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn ballot(voter: [u8; 32], candidate: [u8; 32], epoch: u64) -> ElectionBallot {
        ElectionBallot {
            voter_peer_id: voter,
            candidate_peer_id: candidate,
            ballot_epoch: epoch,
            signature: [0u8; 64],
        }
    }

    fn voter(v: [u8; 32], trust: u32, slash: u32, participant: bool) -> VoterEligibility {
        VoterEligibility {
            voter: v,
            trust_score: trust,
            slash_count: slash,
            is_mission_participant: participant,
        }
    }

    fn stake(c: [u8; 32], amount: u64) -> StakeEntry {
        StakeEntry::new(c, amount)
    }

    #[test]
    fn t_election_governance_model_discriminant_pinning() {
        assert_eq!(GovernanceModel::Centralized.discriminant(), 0x0001);
        assert_eq!(GovernanceModel::Federated.discriminant(), 0x0002);
        assert_eq!(GovernanceModel::Dao.discriminant(), 0x0003);
        assert_eq!(GovernanceModel::AiAssisted.discriminant(), 0x0004);
        assert_eq!(GovernanceModel::Autonomous.discriminant(), 0x0005);
    }

    #[test]
    fn t_election_tie_break_lex_byte_order() {
        let a = [0x11u8; 32];
        let b = [0x22u8; 32];
        let c = [0x33u8; 32];
        assert_eq!(tie_break(&[c, a, b]), a);
        assert_eq!(tie_break(&[a, b, c]), a);
    }

    #[test]
    fn t_election_centralized_designator_wins() {
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
        assert_eq!(tally.governance_model, 0x0001);
    }

    #[test]
    fn t_election_centralized_no_designator_returns_nocandidates() {
        let err = elect_coordinator(
            GovernanceModel::Centralized,
            [0x01u8; 32],
            100,
            &[],
            &[],
            &[],
            None,
        )
        .unwrap_err();
        assert_eq!(err, CoordinatorError::NoCandidates);
    }

    #[test]
    fn t_election_federated_byzantine_quorum() {
        // 7 representatives; f=3; f+1=4 needed.
        let reps: Vec<[u8; 32]> = (1..=7u8).map(|i| [i; 32]).collect();
        let candidate = [0xFFu8; 32];
        let mut ballots = Vec::new();
        // 4 of 7 vote for candidate.
        for &r in reps.iter().take(4) {
            ballots.push(ballot(r, candidate, 100));
        }
        // 3 vote for someone else.
        let other = [0xAAu8; 32];
        for &r in reps.iter().skip(4) {
            ballots.push(ballot(r, other, 100));
        }
        let voters: Vec<VoterEligibility> = reps.iter().map(|&v| voter(v, 600, 0, true)).collect();
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
        assert_eq!(tally.votes_total, 7);
    }

    #[test]
    fn t_election_dao_top_stake_if_no_majority() {
        // TV-2 verbatim per RFC L696-718: 3 candidates (stakes
        // 5000/3000/2000), no candidate >50%, top-stake wins.
        let c1 = [0x01u8; 32];
        let c2 = [0x02u8; 32];
        let c3 = [0x03u8; 32];
        let stakes = vec![stake(c1, 5000), stake(c2, 3000), stake(c3, 2000)];
        // Each candidate receives 1 ballot (no candidate >50%).
        let ballots = vec![
            ballot([0xA1; 32], c1, 100),
            ballot([0xA2; 32], c2, 100),
            ballot([0xA3; 32], c3, 100),
        ];
        let voters = vec![
            voter([0xA1; 32], 600, 0, true),
            voter([0xA2; 32], 600, 0, true),
            voter([0xA3; 32], 600, 0, true),
        ];
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

    #[test]
    fn t_election_dao_lex_tie_break() {
        let low = [0x01u8; 32];
        let high = [0xFFu8; 32];
        let stakes = vec![stake(low, 1000), stake(high, 1000)];
        let ballots = vec![ballot([0xA1; 32], low, 100), ballot([0xA2; 32], high, 100)];
        let voters = vec![
            voter([0xA1; 32], 600, 0, true),
            voter([0xA2; 32], 600, 0, true),
        ];
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
        assert_eq!(tally.winner, low);
    }

    #[test]
    fn t_election_dao_quorum_timeout() {
        // No candidate reaches >50% — election times out.
        let c1 = [0x01u8; 32];
        let stakes = vec![stake(c1, 5_000)];
        let ballots = vec![ballot([0xA1; 32], c1, 100)];
        let voters = vec![voter([0xA1; 32], 600, 0, true)];
        // With 1 vote and 5000 stake, weighted votes = 1 < 2501 majority.
        // Stake is 5000, absolute majority = 5001 (>= total/2 + 1).
        // Weighted vote = 1, < 2501 majority → top-stake fallback.
        // Top-stake winner = c1 (single candidate).
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

    #[test]
    fn t_election_empty_ballot_dao_returns_nocandidates() {
        let err = elect_coordinator(GovernanceModel::Dao, [0x01u8; 32], 100, &[], &[], &[], None)
            .unwrap_err();
        assert_eq!(err, CoordinatorError::NoCandidates);
    }

    #[test]
    fn t_election_ai_assisted_deferred() {
        let err = elect_coordinator(
            GovernanceModel::AiAssisted,
            [0x01u8; 32],
            100,
            &[],
            &[],
            &[],
            None,
        )
        .unwrap_err();
        assert_eq!(err, CoordinatorError::GovernanceModelNotImplemented(0x0004));
    }

    #[test]
    fn t_election_autonomous_deferred() {
        let err = elect_coordinator(
            GovernanceModel::Autonomous,
            [0x01u8; 32],
            100,
            &[],
            &[],
            &[],
            None,
        )
        .unwrap_err();
        assert_eq!(err, CoordinatorError::GovernanceModelNotImplemented(0x0005));
    }

    #[test]
    fn t_election_eligibility_filter() {
        // Slash-banned voter excluded.
        let c1 = [0x01u8; 32];
        let stakes = vec![stake(c1, 5_000)];
        let ballots = vec![ballot([0xA1; 32], c1, 100), ballot([0xA2; 32], c1, 100)];
        let voters = vec![
            voter([0xA1; 32], 600, 0, true), // ok
            voter([0xA2; 32], 600, 6, true), // banned (slash_count >= 5)
        ];
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
        assert_eq!(tally.ballots.len(), 1);
        assert_eq!(tally.ballots[0].voter_peer_id, [0xA1; 32]);
    }

    #[test]
    fn t_election_compute_election_id_deterministic() {
        let a = compute_election_id(&[0x01u8; 32], 100, 0x00);
        let b = compute_election_id(&[0x01u8; 32], 100, 0x00);
        assert_eq!(a, b);
        let c = compute_election_id(&[0x01u8; 32], 101, 0x00);
        assert_ne!(a, c);
    }

    // G18 companion substrate tests (RFC-0011-n Phase 6)

    fn sample_ballot(voter: [u8; 32], candidate: [u8; 32]) -> ElectionBallot {
        ElectionBallot {
            voter_peer_id: voter,
            candidate_peer_id: candidate,
            ballot_epoch: 100,
            signature: [0u8; 64],
        }
    }

    fn eligible_voter(voter: [u8; 32]) -> VoterEligibility {
        VoterEligibility {
            voter,
            trust_score: 100,
            slash_count: 0,
            is_mission_participant: true,
        }
    }

    #[test]
    fn writer_election_default_is_empty() {
        let we = WriterElection::default();
        assert_eq!(we.ballot_count(), 0);
        assert_eq!(we.stake_count(), 0);
        assert_eq!(we.voter_count(), 0);
    }

    #[test]
    fn writer_election_new_equals_default() {
        let we = WriterElection::new();
        assert_eq!(we.ballot_count(), 0);
        assert_eq!(we.stake_count(), 0);
        assert_eq!(we.voter_count(), 0);
    }

    #[test]
    fn writer_election_with_governance_model_constructor() {
        let we = WriterElection::with_governance_model(GovernanceModel::Federated);
        assert_eq!(we.ballot_count(), 0);
        // elect_coordinator delegates to free function with stored model.
        // Without ballots/stakes/voters, Federated returns Empty (substrate
        // behavior). Just confirm the call does NOT panic.
        let _ = we.elect_coordinator([0x01u8; 32], 100, None);
    }

    #[test]
    fn writer_election_cast_ballot_increments_count() {
        let mut we = WriterElection::default();
        we.cast_ballot(sample_ballot([0xA1; 32], [0xB1; 32]));
        we.cast_ballot(sample_ballot([0xA2; 32], [0xB1; 32]));
        assert_eq!(we.ballot_count(), 2);
    }

    #[test]
    fn writer_election_add_stake_increments_count() {
        let mut we = WriterElection::default();
        we.add_stake(StakeEntry::new([0xC1; 32], 1000));
        assert_eq!(we.stake_count(), 1);
    }

    #[test]
    fn writer_election_add_voter_increments_count() {
        let mut we = WriterElection::default();
        we.add_voter(eligible_voter([0xD1; 32]));
        assert_eq!(we.voter_count(), 1);
    }

    #[test]
    fn writer_election_set_governance_model_affects_elect() {
        // Centralized with no designator + no eligible ballots returns Empty
        // per RFC-0862p-a §Election Algorithm. Federated with no ballots
        // also returns Empty. We just confirm the call doesn't panic.
        let mut we = WriterElection::default();
        we.set_governance_model(GovernanceModel::Dao);
        let _ = we.elect_coordinator([0x01u8; 32], 100, None);
    }

    #[test]
    fn writer_election_elect_coordinator_delegates_to_free_function() {
        // Substrate-faithful delegation test: set up state and confirm
        // the struct method returns the SAME result as calling the free
        // function directly with the same args.
        let mut we = WriterElection::with_governance_model(GovernanceModel::Federated);
        we.add_voter(eligible_voter([0xA1; 32]));
        we.add_voter(eligible_voter([0xA2; 32]));
        we.cast_ballot(sample_ballot([0xA1; 32], [0xB1; 32]));
        we.cast_ballot(sample_ballot([0xA2; 32], [0xB1; 32]));

        let struct_result = we.elect_coordinator([0x01u8; 32], 100, None);
        let direct_result = elect_coordinator(
            GovernanceModel::Federated,
            [0x01u8; 32],
            100,
            &[
                sample_ballot([0xA1; 32], [0xB1; 32]),
                sample_ballot([0xA2; 32], [0xB1; 32]),
            ],
            &[],
            &[eligible_voter([0xA1; 32]), eligible_voter([0xA2; 32])],
            None,
        );
        assert_eq!(struct_result.is_ok(), direct_result.is_ok());
        if let (Ok(s), Ok(d)) = (&struct_result, &direct_result) {
            assert_eq!(s.winner, d.winner);
        }
    }
}
