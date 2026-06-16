# Mission: 0850p-c — Cross-node REBIND atomicity

## Status

Open (2026-06-16) — post-launch follow-up

## RFC

RFC-0850p-c (Networking): Transport Group Binding — §"Future Work"

## Summary

When the same `domain_id` is bound to N physical groups on different platforms (multi-platform binding per §5 "Multi-Platform Binding Rule"), REBIND on one platform must coordinate with the others to maintain mission consistency. Currently REBIND is single-platform, which can cause mission fragmentation (different groups on different platforms end up with different `BIND` envelopes).

## Design

A 2-phase commit on the REBIND operation:

1. **Prepare phase:** The initiator broadcasts `REBIND_PREPARE` (signed with the DomainCoordinator's key) to all other N-1 platforms' DomainCoordinators. Each prepares the new binding locally (allocates resources, validates the new group) but does not commit.
2. **Commit phase:** If all N-1 vote `REBIND_PREPARED` within 30s, the initiator broadcasts `REBIND_COMMIT` and all parties switch atomically.
3. **Abort phase:** If any vote is `REBIND_ABORT` (or timeout), the initiator broadcasts `REBIND_ABORT` and all parties roll back.

Tie-break for concurrent REBINDs: lexicographic `domain_id` (lower first). A losing REBIND is rejected with `REBIND_LOST_TIE_BREAK`.

Fallback: 30s timeout on `REBIND_PREPARE` → manual operator reconciliation via `octo-coordinator reconcile` CLI (re-reads BIND logs from each platform and emits a new BIND envelope signed by governance).

## Acceptance Criteria

- [ ] `REBIND_PREPARE` / `REBIND_COMMIT` / `REBIND_ABORT` envelope types in `crates/octo-network/src/mon/bind_envelope.rs`
- [ ] 2-phase commit coordinator in `crates/octo-network/src/mon/rebind.rs`
- [ ] Tie-break: lex `domain_id` ordering
- [ ] 30s timeout with manual reconciliation fallback
- [ ] Unit tests: happy path, abort path, timeout path, tie-break
- [ ] Integration test: 2 simulated platforms, simultaneous REBIND → one wins, one rejects
- [ ] Documentation: operator guide for manual reconciliation

## Dependencies

Depends on RFC-0850p-c status: Accepted. No prerequisite missions; this is a coordinator-side addition to the existing REBIND path.

## Claimant

(none — Open mission)

## Pull Request

(none — Open mission)

## Location

`crates/octo-network/src/mon/bind_envelope.rs` (add envelope variants); `crates/octo-network/src/mon/rebind.rs` (add 2PC coordinator).

## Complexity

Medium (~400 lines; 3 envelope types, 2PC state machine, libp2p broadcast).

## Prerequisites

- RFC-0850p-c status: Accepted

## Notes

### Why 2-phase commit?

A single-platform REBIND is atomic by construction (one group, one admin). A cross-platform REBIND can leave the groups in inconsistent states if one succeeds and the other fails. 2PC is the standard way to coordinate multi-party state changes.

### Why 30s timeout?

30s is long enough for the slowest platform (typically Telegram supergroup) but short enough that operators don't wait forever. After 30s, manual reconciliation is the fallback.

### Type Coverage

| RFC-0850p-c Type | Implemented By |
|-----------------|----------------|
| `REBIND_PREPARE` / `REBIND_COMMIT` / `REBIND_ABORT` envelopes | This mission |
| 2-phase commit coordinator | This mission |
| Tie-break: lex `domain_id` ordering | This mission |

### Implementation Guide

Reference: `crates/octo-network/src/mon/rebind.rs` (existing single-platform REBIND); `crates/octo-network/src/mon/bind_envelope.rs` (existing envelope types).

## Mitigates

D-TGB-11 (cross-node REBIND atomicity in Adversary Analysis)

## Deadline

Post-launch
