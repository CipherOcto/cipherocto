# Mission: 0855p-b — Cross-mission coordinator reputation

## Status

Open (2026-06-16) — post-launch

## RFC

RFC-0855p-b (Networking): Coordinator Lifecycle — §"Future Work"

## Summary

Each `SlashEvent` per §"Slash Reason Codes" carries a per-mission `slash_count`. For cross-mission reputation, augment the local count with a global view fetched from a `SlashReputationStore` (a map `coordinator_pubkey -> Vec<SlashEvent>` from across all missions the coordinator has participated in). On election, candidates with a higher global slash count are deprioritized.

## Design

- `SlashReputationStore` is a key-value store mapping `coordinator_pubkey` to a list of `SlashEvent` references (one per mission).
- On election start, the election coordinator queries `SlashReputationStore` for each candidate's global slash count.
- Priority is computed: `priority = stake / (1 + global_slash_count)`. This is a soft penalty, not a hard disqualification.
- Candidates with `global_slash_count >= 5` are excluded from the election (hard threshold).
- The store is gossiped across the libp2p mesh under `/dot/reputation/{coordinator_pubkey}` topic, signed by the mission's coordinator.
- Privacy: slash events are referenced by hash, not included in full; the full event is fetched on demand.

## Acceptance Criteria

- [ ] `crates/octo-network/src/reputation/slash_store.rs` — `SlashReputationStore` type
- [ ] Gossip topic `/dot/reputation/{coordinator_pubkey}` in `crates/octo-network/src/gossip/reputation.rs`
- [ ] `priority = stake / (1 + global_slash_count)` election formula
- [ ] Hard threshold: `global_slash_count >= 5` → excluded
- [ ] Unit tests: priority calculation, threshold enforcement
- [ ] Integration test: gossip propagation of slash reputation
- [ ] Documentation: how slash reputation is computed and used in elections


### Implementation Guide

Reference: `crates/octo-network/src/reputation/slash_store.rs` (new); `crates/octo-network/src/gossip/reputation.rs` (new).


### Type Coverage

| RFC-0855p-b Type | Implemented By |
|-----------------|----------------|
| `SlashReputationStore` type | This mission |
| `/dot/reputation/{coordinator_pubkey}` gossip topic | This mission |
| Priority formula: `stake / (1 + global_slash_count)` | This mission |

## Dependencies

Depends on:
- RFC-0855p-b status: Accepted
- Mission 0855p-b (slash reason codes base implementation)
- The libp2p mesh (already operational)

## Claimant

(none — Open mission)

## Pull Request

(none — Open mission)

## Location

`crates/octo-network/src/reputation/slash_store.rs` (new); `crates/octo-network/src/gossip/reputation.rs` (new).

## Complexity

Medium (~400 lines; store type, gossip protocol, priority formula).

## Prerequisites

- RFC-0855p-b status: Accepted

## Notes

### Why a soft penalty?

A hard disqualification is a one-strike-and-out policy that is too aggressive. A slashed coordinator may have been a victim of platform misbehavior (e.g., admin key compromise by an attacker). The soft penalty (priority = stake / (1 + count)) reduces the chance of re-election but doesn't forbid it.

### Why a hard threshold at 5?

5 slashes is a strong signal of repeated misbehavior. Beyond this, the coordinator is excluded.

## Mitigates

D-CL-3 (re-election of repeatedly-misbehaving coordinators)

## Deadline

Post-launch
