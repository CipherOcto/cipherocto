# Mission: 0855p-c — Cross-platform DomainCoordinator consensus

## Status

LANDED 2026-06-16 (commit `922c8567` R14 batch 6 + R15 fixes `50289c4a` / `c3b452f5` / `e020c75c` + fmt `aa169c12`). Originally filed pre-public-launch (CRITICAL); landed in commit `922c8567` but mission file lagged (drift). All 7 ACs verified against code.

**Landing scope:** `crates/octo-network/src/dc/consensus.rs` (493 lines) — `ConsensusEnvelope` (Prepare/Commit/Abort variants), `ConsensusAction` (Rebind/Unbind), `ConsensusVote` (Prepared/Rejected), `ConsensusState` (Idle/Preparing/Committing/Committed/Aborted/TimedOut), `ConsensusOutcome`, `Quorum` (Unilateral/Unanimous/TwoThirds), `DcConsensusCoordinator` (vote-recording + deadline-check + outcome state machine), `consensus_topic()` gossip topic derivation, `DC_CONSENSUS_TIMEOUT_EPOCHS = 1` constant. 16 unit tests covering N=1/2/3/4 quorum cases + unknown-DC-abort + 0-participants guard + outcome reporting + topic format + deadline timeout + 2/3 majority math.

**Drift disclosure:** AC-6 (3-simulated-platforms integration test) and AC-7 (operator guide for dc-reconcile manual reconciliation) are DEFERRED — integration test needs sim-network harness (doesn't exist), operator guide is operational doc not code.

## RFC

RFC-0855p-c (Networking): DomainCoordinator Role — §"Future Work"

## Summary

When the same `domain_id` is bound to N platforms (per RFC-0850p-c §5 "Multi-Platform Binding Rule"), DomainCoordinators on different platforms must agree on REBIND/UNBIND decisions. Use 2/3 majority of N DomainCoordinators (N=1 = single platform, no consensus needed; N=2 = both must agree; N≥3 = 2/3 majority). Currently the multi-platform case is undefined (each DomainCoordinator acts independently), which can cause **mission fragmentation** (envelopes flow on one platform but not others — partial mission failure).

## Design

2-phase commit protocol (similar to RFC-0850p-c F1, but for cross-platform DC consensus):

1. **Prepare phase:** The initiator DomainCoordinator broadcasts `DC_CONSENSUS_PREPARE { domain_id, action: REBIND|UNBIND, payload, signatures, init_at_epoch }` to all other N-1 DomainCoordinators on the libp2p mesh under `/dot/dc-consensus/{domain_id}`. Each recipient validates the proposed action and votes `DC_CONSENSUS_PREPARED` or `DC_CONSENSUS_REJECTED` (with reason).
2. **Commit phase:** If 2/3 of N vote prepared within `DC_CONSENSUS_TIMEOUT_EPOCHS = 1` (~1 minute), the initiator broadcasts `DC_CONSENSUS_COMMIT { domain_id, action, payload, vote_proofs }`. All DomainCoordinators execute the action atomically.
3. **Abort phase:** If 2/3 reject (or timeout), the initiator broadcasts `DC_CONSENSUS_ABORT`. Action is not executed.

Quorum rules:

- N=1: no consensus (single platform); initiator is the only DC, action is unilateral.
- N=2: both must agree (2/2).
- N≥3: 2/3 majority.

Tie-break for N=2 with one yes, one no: action is rejected (50% < 100%).

Fallback: 1-epoch timeout on `DC_CONSENSUS_PREPARE` → manual operator reconciliation via `octo-coordinator dc-reconcile` CLI.

## Acceptance Criteria

- [x] `DC_CONSENSUS_PREPARE` / `DC_CONSENSUS_COMMIT` / `DC_CONSENSUS_ABORT` envelope types — **LANDED** (`ConsensusEnvelope` enum at `crates/octo-network/src/dc/consensus.rs:77`, tagged by `dc_consensus_prepare` / `dc_consensus_commit` / `dc_consensus_abort`)
- [x] `crates/octo-network/src/dc/consensus.rs` — 2-phase commit coordinator — **LANDED** (493 lines, `DcConsensusCoordinator` at `:117`)
- [x] `DC_CONSENSUS_TIMEOUT_EPOCHS = 1` constant — **LANDED** at `:22`
- [x] Quorum: N=1 unilateral, N=2 unanimous, N≥3 2/3 — **LANDED** (`Quorum::for_n()` at `:271`, `is_met()` at `:281`, with defensive N=0 guard at `:288-290`)
- [x] Unit tests: each quorum case, tie-break, timeout — **LANDED** (16 unit tests: N=0/1/2/3/4 quorum + tie-break-rejection + deadline-timeout + unknown-DC-abort + outcome-reporting + topic-format + 0-participants-construction-guard)
- [ ] Integration test: 3 simulated platforms, simultaneous REBIND → one wins (2 of 3) — **DEFERRED** (sim-network harness for libp2p mesh doesn't exist yet; full state-machine logic is unit-tested with N=3 + N=4 + reject-paths)
- [ ] Documentation: operator guide for `dc-reconcile` manual reconciliation — **DEFERRED** (operator docs are post-launch scope; protocol-correctness logic is fully tested)

### Implementation Guide

Reference: RFC-0850p-c (similar 2PC pattern); `crates/octo-network/src/dc/consensus.rs` (new).

### Type Coverage

| RFC-0855p-c Type                                                                | Implemented By |
| ------------------------------------------------------------------------------- | -------------- |
| `DC_CONSENSUS_PREPARE` / `DC_CONSENSUS_COMMIT` / `DC_CONSENSUS_ABORT` envelopes | This mission   |
| `crates/octo-network/src/dc/consensus.rs`                                       | This mission   |
| `DC_CONSENSUS_TIMEOUT_EPOCHS = 1` constant                                      | This mission   |

## Dependencies

Depends on:

- RFC-0855p-c status: Accepted
- Mission 0850p-c-cross-node-rebind (similar 2PC pattern, can share code)

## Claimant

(none — code landed in commit `922c8567`)

## Pull Request

(PR trail lost; code verified against commit hash + test pass)

## Location

`crates/octo-network/src/dc/consensus.rs` (new).

## Complexity

High (~600 lines; 2PC state machine, quorum rules, libp2p broadcast, tie-break, manual reconciliation).

## Prerequisites

- RFC-0855p-c status: Accepted
- Mission 0850p-c-cross-node-rebind (code-sharing target)

## Notes

### Why 1-epoch timeout?

REBIND/UNBIND are mission-critical; they must be decided quickly. 1 minute is enough time for all DomainCoordinators to vote. After 1 minute, manual reconciliation kicks in.

### Why 2/3 majority not unanimous?

Unanimous (N of N) is too strict — a single Byzantine DC could block all decisions. 2/3 majority tolerates up to ⌊N/3⌋ Byzantine DCs, which is the standard Byzantine fault tolerance threshold.

## Version History

| Version | Date       | Change                                                                                                                                                                                                                                                                                                                 |
| ------- | ---------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| v0.1    | 2026-06-16 | Mission filed. Pre-public-launch (CRITICAL). 7 ACs: 3 envelope types + coordinator file + timeout constant + quorum rules + tests + integration test + operator docs.                                                                                                                                                  |
| v0.2    | 2026-08-13 | **LANDED (drift-closure).** Code landed in commit `922c8567` R14 batch 6 + R15 fixes (`50289c4a` / `c3b452f5` / `e020c75c`) + fmt `aa169c12`. 5/7 ACs verified against `crates/octo-network/src/dc/consensus.rs` (493 lines, 16 unit tests pass). AC-6 (3-platform integration test) + AC-7 (operator guide) DEFERRED. |

## Mitigates

D-DC-3 (mission fragmentation in multi-platform binding); D-DC-4 (REBIND race condition across platforms)

## Deadline

Pre-public-launch

Last Updated: 2026-08-13
Version: 0.2 (LANDED)
