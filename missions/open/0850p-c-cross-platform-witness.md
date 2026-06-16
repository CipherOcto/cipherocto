# Mission: 0850p-c — Cross-platform witness aggregation

## Status

Open (2026-06-16) — future

## RFC

RFC-0850p-c (Networking): Transport Group Binding — §"Future Work"

## Summary

RFC-0855p-b §B "Slash Offense Codes" defines slash reason codes per-witness (e.g., `0x0003` for `founder-squat`). When a slash vote is cast by a witness on one platform (e.g., a WhatsApp group witness), it must be aggregated with witnesses on other platforms (e.g., Matrix room witness, Telegram supergroup witness) to form the 2/3 majority needed for slash finalization. The aggregation rules are not yet specified for cross-platform cases.

## Design

Cross-platform witness aggregation follows the same pattern as 0855p-c F(cross-platform DomainCoordinator consensus):

- N platforms, each with 1+ witness(es)
- Slash finalization requires 2/3 majority of TOTAL witnesses (not per-platform)
- Each witness's slash vote is signed with the witness's key and broadcast on the libp2p mesh under `/dot/slash/{domain_id}/{slash_id}`
- Votes are collected over a 60s window
- After 60s, the slash is finalized if 2/3 of N votes are received; otherwise it's rejected

Tie-break for equal votes (e.g., N=2, both vote yes but quorum is 1.33): both vote "yes" → slash finalizes. N=2 with one yes, one no → not finalized (50% < 2/3 = 66.6%).

## Acceptance Criteria

- [ ] `SlashVote` envelope type with platform identifier and witness signature
- [ ] Aggregation logic in `crates/octo-network/src/mon/slash_aggregation.rs`
- [ ] 60s vote collection window
- [ ] 2/3 majority rule
- [ ] Unit tests: N=1 (single platform), N=2 (both yes, yes+no, no+no), N=3 (2 yes, 1 yes, etc.)
- [ ] Integration test: cross-platform slash with simulated WhatsApp + Matrix witnesses
- [ ] Documentation: operator guide for cross-platform slash audit

## Dependencies

Depends on:
- Mission 0855p-b-??? (slash reason codes, base RFC)
- Mission 0855p-c-cross-platform-consensus (similar 2PC pattern)

## Claimant

(none — Open mission)

## Pull Request

(none — Open mission)

## Location

`crates/octo-network/src/mon/slash_aggregation.rs` (new).

## Complexity

Medium (~350 lines; aggregation, gossip, vote collection window).

## Prerequisites

- RFC-0855p-b status: Accepted
- RFC-0855p-c status: Accepted

## Notes

### Why 2/3 of N (not per-platform)?

A 2/3 majority of the total witnesses is a global quorum. Per-platform quorum (e.g., 2/3 of WhatsApp witnesses + 2/3 of Matrix witnesses) would allow a single platform with many witnesses to dominate. Global 2/3 is more balanced.

### Why 60s window?

60s is long enough for the slowest witness (typically a Tor-routed peer) but short enough that slash finalization is timely. After 60s, the slash is finalized or rejected based on the votes received.

### Type Coverage

| RFC-0850p-c Type | Implemented By |
|-----------------|----------------|
| `SlashVote` envelope type | This mission |
| Cross-platform aggregation logic | This mission |
| `/dot/slash/{domain_id}/{slash_id}` gossip topic | This mission |

### Implementation Guide

Reference: RFC-0855p-b §B (slash reason codes); RFC-0850p-c (similar 2PC pattern).

## Mitigates

Consistency for cross-platform missions; relates to D-DC-6 (cross-domain slash risk in 0855p-c F3).

## Deadline

Future
