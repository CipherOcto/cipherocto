---
name: 0855p-b-mission-coordinator-election
description: Mission Coordinator election algorithm per RFC-0855p-b §Implementation Phase 2. Wire per-governance-model election logic (Centralized / Federated / DAO / AI-Assisted / Autonomous per RFC §Election Algorithm table L260-275) on top of `CoordinatorRecord` + `ElectionTally` + `ElectionBallot` types from `0855p-b-state-machine-types`. Land `transition_valid` Designated→Elected + Elected→Active paths + lex-order tie-break + per-model scoring/winner logic + `ELECTION_TIMEOUT = 1000` epoch closed-epoch rule + eligibility filter (RFC L282-289) at `crates/octo-coordinator-types/src/election.rs` with re-export shim at `crates/octo-network/src/mon/election.rs`. Algorithm is RFC-0008 Class A determinism (Borsh-sort ballots, lex-order winner tie-break). AI-Assisted + Autonomous branches return `Err(CoordinatorError::GovernanceModelNotImplemented)` (DEFERRED per §Implementation Phases). 6 canonical election vectors added to `tests/canonical_election_blobs.rs`. BLOCKED by `0855p-b-state-machine-types`. Distinct from Future-Work sub-specs F3 (VDF election, `0855p-b-vdf-election`) + F4 (stake-weighted quadratic, `0855p-b-stake-weighted-quadratic`); this mission owns ONLY the §Phase 2 base election algorithm that F3/F4 will extend.
metadata:
  node_type: substrate-coordinator
  type: substrate-election-algorithm
  rfc_source: RFC-0855p-b
  rfc_section: Implementation Phases §Phase 2 (L811-818) + Election Algorithm (L260-275) + Roles §Mission Coordinator (L72-90) + Round 2 adversarial review (L818)
  substrate_home:
    canonical: crates/octo-coordinator-types/src/election.rs
    re_export: crates/octo-network/src/mon/election.rs
    tests: crates/octo-coordinator-types/tests/canonical_election_blobs.rs
  execution_class: A
  deterministic: true
  depends_on:
    - RFC-0855p-b
    - RFC-0008
    - mission:0855p-b-state-machine-types
  blocks:
    - 0855p-b-mission-coordinator-liveness
    - 0855p-b-mission-coordinator-slashing
status: Claimed
claimed_by: mmacedoeu
claimed_at: 2026-09-03
v: "1.0"
created: 2026-09-03
---

# Mission `0855p-b-mission-coordinator-election` v1.0 — RFC-0855p-b §Phase 2

## Status

Claimed (2026-09-03) by @mmacedoeu — blocked on `0855p-b-state-machine-types` (Phase 1 substrate). RFC-0855p-b §Implementation Phase 2 promises per-governance-model election + tie-break + quorum; **0 election algorithms defined** in `crates/`.

## RFC

RFC-0855p-b §Implementation Phase 2 (L811-818) + §Election Algorithm (L258+) + §Roles §Mission Coordinator (L72-90). Per-governance-model election per RFC-0855 §11.1 "Governance Flexibility".

## Summary

Land the 5 governance-model election algorithms (Centralized / Federated / DAO / AI-Assisted / Autonomous per RFC §Election Algorithm L260-275; AI-Assisted + Autonomous DEFERRED via `GovernanceModelNotImplemented` error) on top of the Phase 1 state-machine substrate. Each algorithm: (a) accepts ballots / stake / `governance_model`; (b) sorts ballots by `(voter_peer_id, ballot_epoch)` for determinism (Borsh-stable); (c) applies per-model scoring per RFC table (DAO top-stake if no candidate `>50%`; Federated `f+1 of 2f+1`; Centralized creator designates first + 2/3 vote for replacement; AI-Assisted + Autonomous DEFERRED); (d) emits `ElectionTally` with `winner: CoordinatorId` + `votes_received` + `votes_total` + `closed_epoch`. Tie-break on equal score: lex-order on `CoordinatorId`. `closed_epoch = min(quorum_reached_epoch, election_epoch + ELECTION_TIMEOUT)` with `ELECTION_TIMEOUT = 1000` epochs. Eligibility filter runs BEFORE ballot tally per RFC L282-289. All Class A deterministic per RFC-0008.

The mission owns ONLY the §Phase 2 base algorithm. Future-Work sub-specs F3 (VDF beacon election — `0855p-b-vdf-election`) and F4 (stake-weighted quadratic — `0855p-b-stake-weighted-quadratic`) compose on top of this base by extending the scoring fn; they do NOT re-implement quorum + tie-break.

## Substrate work scope

### Step 1 — Per-governance-model election

**New file `crates/octo-coordinator-types/src/election.rs`** with:

```rust
#[repr(u8)]
pub enum GovernanceModel {
    Centralized   = 0x0001,
    Federated     = 0x0002,
    Dao           = 0x0003,
    AiAssisted    = 0x0004,
    Autonomous    = 0x0005,
}

pub fn elect_coordinator(
    governance_model: GovernanceModel,
    mission_id: [u8; 32],
    election_epoch: u64,
    ballots: &[ElectionBallot],
    eligible_voters: &[CoordinatorId],
    designator: Option<CoordinatorId>,     // for Centralized
) -> Result<ElectionTally, CoordinatorError>;
```

Returns an `ElectionTally` per RFC L188-206:
- `election_id: BLAKE3(mission_id || election_epoch || nonce)` (nonce = `0x00` for deterministic pinning).
- `ballots` sorted by `(voter_peer_id, ballot_epoch)` (RFC L196).
- `winner` = governance-model-specific (see Step 2) + lex tie-break on `CoordinatorId`.
- `votes_received` + `votes_total` (only emitted for ballot-driven models; Centralized / Autonomous emit empty ballot set).

### Step 2 — Per-model mechanics (verbatim RFC table L260-275)

| Model | Election rule | Tie-Break | Eligibility |
|-------|---------------|-----------|-------------|
| **Centralized** | First coordinator: creator designates. Replacement: 2/3 vote. | n/a (designated) | `trust_score >= 500` (RFC-0855 §4.2) |
| **DAO** | Top-stake candidate wins if no candidate receives `>50%`. Otherwise top-stake wins. Re-election every `term_epochs`. | Lex `peer_id` ascending | `octo_stake >= 1000` + `trust_score >= 500` |
| **Federated** | One per organizational domain; consensus from `f+1` of `2f+1` domain representatives. | Domain index then `peer_id` | `domain_reputation >= threshold` |
| **AI-Assisted** | AI proposes; humans ratify 2/3 within `proposal_deadline_epochs`. | n/a (proposed) | AI selection + human ratification |
| **Autonomous** | No election; protocol-defined rotation by `coordinator_term_id` ordering. Mission genesis names a deterministic order (e.g., BLAKE3-ordered `peer_id` list). | BLAKE3 of `(mission_id, slot_index)` | n/a |

Phase-2 mission scope per RFC §Implementation Phases (Phase 2 is "Election Algorithm (per governance model)") — implement the **5 branches**, but mark AI-Assisted (0x0004) and Autonomous (0x0005) as **DEFERRED** at the implementation level (return `Err(CoordinatorError::GovernanceModelNotImplemented)` per `lib.rs:140`-style reserved-range pattern; do NOT silently fail).

Election closed-epoch rule (RFC L290): `closed_epoch = min(quorum_reached_epoch, election_epoch + ELECTION_TIMEOUT)` where `ELECTION_TIMEOUT = 1000` epochs. If `closed_epoch = election_epoch + ELECTION_TIMEOUT` (quorum not reached), the election fails and a new `election_id` is initiated.

Election eligibility filter (RFC L282-289): MUST run BEFORE counting any ballot. Reject voters who are not current mission participants, voters whose trust score is below the governance model's threshold, candidates failing eligibility, candidates on the slash blacklist (`slash_count >= MAX_SLASHES_BEFORE_BAN = 5`), and ballots with invalid signature or stale `ballot_epoch`.

### Step 3 — Lex tie-break

`pub fn tie_break(candidates: &[CoordinatorId]) -> CoordinatorId` — lowest lex byte order on `[u8; 32]` per RFC L812.

### Step 4 — Canonical election vectors

**New file `crates/octo-coordinator-types/tests/canonical_election_blobs.rs`** pinning 6 vectors:
- DAO: 3 candidates (stakes 5000/3000/2000), no candidate >50%, top-stake wins (TV-2 verbatim per RFC L696-718).
- Centralized: creator designates 0xBBBB, subsequent replacement requires 2/3 vote.
- Federated: 3-of-7 representatives (f+1 of 2f+1 where f=3), Byzantine FT consensus.
- Lex tie-break: 2 candidates with identical stake; lowest lex `peer_id` wins.
- Quorum failure: DAO election reaches `ELECTION_TIMEOUT` (1000 epochs) without `>50%` majority → returns `Err(CoordinatorError::ElectionTimeout)` (no slash, new election initiated).
- Empty ballot set → `Err(CoordinatorError::NoCandidates)`.

### Step 5 — Re-export shim

**New file `crates/octo-network/src/mon/election.rs`** with `pub use octo_coordinator_types::election::*;`. Honors RFC §Key Files L848 verbatim at Layer C.

## Acceptance Criteria

- [ ] `crates/octo-coordinator-types/src/election.rs` exists with `GovernanceModel` (5 discriminants per RFC table), `elect_coordinator`, `tie_break`, 5 governance-model branches.
- [ ] DAO: top-stake wins if no candidate `>50%`; no 2/3 vote requirement.
- [ ] Federated: `f+1 of 2f+1` Byzantine consensus (not 2/3 of multi-sig).
- [ ] Centralized: first coordinator = creator-designated; replacement requires 2/3 vote (NOT first-replacement = designator pick).
- [ ] AI-Assisted + Autonomous branches return `Err(CoordinatorError::GovernanceModelNotImplemented)` with explicit DEFERRED marker; do NOT silently default.
- [ ] `closed_epoch = min(quorum_reached_epoch, election_epoch + ELECTION_TIMEOUT)` with `ELECTION_TIMEOUT = 1000` epochs.
- [ ] Election eligibility filter runs BEFORE ballot tally (RFC L282-289).
- [ ] Ballots sorted by `(voter_peer_id, ballot_epoch)` deterministically (Borsh-stable).
- [ ] Tie-break uses `CoordinatorId` lex byte order.
- [ ] `crates/octo-network/src/mon/election.rs` re-exports surface.
- [ ] 6 canonical-blob vectors pinned in `tests/canonical_election_blobs.rs`; all PASS.
- [ ] All-inline `#[cfg(test)]` unit tests PASS.
- [ ] `cargo clippy -p octo-coordinator-types --all-targets -- -D warnings` clean.
- [ ] No regression in `octo-coordinator-types` lib test suite.

## Out of scope

- VDF election (Future-Work F3 — `0855p-b-vdf-election.md`).
- Stake-weighted quadratic-cost voting (Future-Work F4 — `0855p-b-stake-weighted-quadratic.md`).
- Heartbeat integration (Phase 3).
- Handover envelope (Phase 4).

## Cross-references

- RFC-0855p-b §Implementation Phase 2 (L811-818)
- RFC-0855p-b §Election Algorithm (L258+)
- RFC-0855 §11.1 "Governance Flexibility"
- RFC-0855 §3 "Mission Lifecycle"
- Mission `0855p-b-state-machine-types` (BLOCKER — Phase 1 types)

## Dependencies

**Requires:**
- RFC-0855p-b (accepted)
- RFC-0855 §11.1 (governance flexibility contract)
- Mission `0855p-b-state-machine-types` (Phase 1 — ElectionTally, ElectionBallot, CoordinatorId types)

**Optional:**
- Mission `0855p-b-vdf-election.md` (F3 — VDF election composes on this base)
- Mission `0855p-b-stake-weighted-quadratic.md` (F4 — quadratic voting composes on this base)

## Version History

| Version | Date | Change |
| ------- | ---- | ------ |
| v1.0    | 2026-09-03 | Initial filing per RFC-0855p-b §Implementation Phase 2 + user audit (2026-09-03). |
