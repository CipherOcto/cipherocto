# 0011-h-s-a-writer-election-struct — Substrate additions for WriterElection struct wrapping elect_coordinator

## Status

Completed (2026-09-20) — Substrate slice LANDED at `next a58f2103`. CLI dispatch slice LANDED at `next 2ba4273b`. `WriterElection` struct + `elect_coordinator` method (delegating to free function) + `cast_ballot` + `add_stake` + `add_voter` + `set_governance_model` + `ballot_count` + `stake_count` + `voter_count` + `with_governance_model` constructor landed in `crates/octo-coordinator-types/src/election.rs` per RFC-0011-n Phase 6 G18 NEW companion mission. 8 substrate unit tests added (54 total octo-coordinator-types tests, was 46). `GovernanceModel` now derives `Default` with `Centralized` as the `#[default]` variant. Phase 6 CLI dispatch slice atop this substrate LANDED at `next 2ba4273b` (402/402 octo-cli tests, was 396). Phase 6 IMPLEMENTATION CLOSED for G18 per RFC-0011-n §Implementation Phases Phase 6 closure card.

## RFC

RFC-0011-n §Substrate-Additions Companion Missions row G18 + RFC-0862p-a Writer Election Bootstrap + RFC-0011-h §Substrate-Additions row G18

## Summary

Adds a thin `WriterElection` struct wrapper around the existing free function `elect_coordinator` at `octo-coordinator-types/src/election.rs:222` (verified substrate-faithful signature: 7 args returning `Result<ElectionTally, CoordinatorError>`). Required by `octo network status` (Phase 6 G18 substrate companion) so the CLI can query writer-election state without depending on the free function directly.

### Substrate additions target

```rust
// crates/octo-coordinator-types/src/election.rs (existing module extended)
#[derive(Clone, Debug, Default)]
pub struct WriterElection {
    ballots: Vec<ElectionBallot>,
    stakes: Vec<StakeEntry>,
    voters: Vec<VoterEligibility>,
    governance_model: GovernanceModel,
}

impl WriterElection {
    /// Substrate-faithful struct wrapper around the existing
    /// free function `elect_coordinator` (RFC-0011-n Phase 6
    /// G18). The struct method delegates to the free function
    /// to keep the canonical election logic in one place.
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

    /// Cast a ballot (CLI substrate-faithful surface; Phase 6
    /// closure path remains `AdapterUnwired` for write paths).
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
```

Layer B substrate additions land in `crates/octo-coordinator-types/src/election.rs` (existing module extended, NOT a new module). The struct wraps the existing free function `elect_coordinator` so the canonical election logic stays in one place per [[cipherocto-design-principles]] §Stable Abstractions Principle. The struct adds ballot + stake + voter + governance_model state that the CLI's `octo network status` can query for writer-election state aggregation.

The struct intentionally does NOT carry candidates: the `elect_coordinator` free function does not take a candidates list (verified substrate-faithful signature). Candidates are derived from ballots + stakes + voters per RFC-0862p-a election algorithm.

## Acceptance Criteria

- [x] `WriterElection` struct lands in `crates/octo-coordinator-types/src/election.rs` per RFC-0011-n §Substrate-Additions row G18 (next a58f2103)
- [x] `elect_coordinator(mission_id, election_epoch, designator) -> Result<ElectionTally, CoordinatorError>` struct method lands at same path (delegates to free function with 7 args)
- [x] `cast_ballot(ElectionBallot)` registry helper lands
- [x] `add_stake(StakeEntry)` registry helper lands
- [x] `add_voter(VoterEligibility)` registry helper lands
- [x] `set_governance_model(GovernanceModel)` registry helper lands
- [x] `ballot_count()` + `stake_count()` + `voter_count()` observability helpers land
- [x] `cargo clippy -p octo-coordinator-types --all-targets -- -D warnings` clean
- [x] `cargo test -p octo-coordinator-types --lib` green (54/54, +8 above Phase 5 baseline of 46)
- [x] Layer discipline preserved (Layer B only, zero Layer A change per [[cipherocto-design-principles]] §Stable Abstractions Principle)
- [x] ≥3 unit tests + ≥1 integration test (8 substrate unit tests pin: struct method delegates to free function + cast_ballot + add_stake + add_voter + set_governance_model + ballot_count + stake_count + voter_count + idempotent insert)

## Dependencies

- Hard sequencing: RFC-0011-h must be Accepted before this mission lands
- Hard sequencing: RFC-0011-n must be Accepted (Draft v0.2, DRY CLOSED)
- Soft sequencing: `elect_coordinator` free function at `octo-coordinator-types/src/election.rs:222` (LANDED) is consumed by the struct method (verified substrate-faithful 7-arg signature)

## Out of Scope

- CLI dispatch (paired CLI mission `0011-h-network-status` covers that surface in Phase 6 CLI dispatch slice)
- Wire format versioning (deferred to substrate-additions companion)
- Per-extension transport impl (deferred to per-extension crate pattern)
- Persistence adapter (Phase 6 follow-on companion `0011-h-s-a-writer-election-persistence`)
- Tie-break logic (consumed from existing `tie_break` free function at `octo-coordinator-types/src/election.rs:494`)
- Candidates list (NOT consumed by `elect_coordinator` free function; omitted from struct per substrate-faithfulness)

## Notes

Stub fill-in 2026-09-20 per RFC-0011-n closure card at `next` (DRY CLOSED v0.2). Substrate slice pending per directive sequencing (substrate coding is LAST). The struct method `WriterElection::elect_coordinator` DELEGATES to the free function `elect_coordinator` to keep the canonical election logic in one place — the struct is purely a state-aggregation surface for the CLI's `status` subcommand. The struct does NOT introduce parallel abstractions per [[cipherocto-design-principles]] §No parallel abstractions: the free function remains the authoritative election logic. Signature verified substrate-faithful (7-arg form returning `Result<ElectionTally, CoordinatorError>`) per direct read of `octo-coordinator-types/src/election.rs:222` during this stub fill-in.
