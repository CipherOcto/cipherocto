# Mission: MON Mission Discovery and Gossip

## Status

Implemented (2 new files: discovery.rs + gossip.rs, 27 tests)

## RFC

RFC-0855: Mission Overlay Networks (MON) — §8, §9

## Summary

Implement mission discovery with 5 scopes (Public, Invite-only, Stealth, Federated, Ephemeral) and mission gossip with 7 propagation classes.

## Acceptance Criteria

- [x] `MissionDiscoveryScope` enum: Public, InviteOnly, Stealth, Federated, Ephemeral
- [x] Public missions: discoverable via GDP global scope
- [x] Invite-only missions: require invitation from Coordinator
- [x] Stealth missions: encrypted advertisements, only capability key holders can decrypt
- [x] Mission gossip integration with DGP (RFC-0852)
- [x] 7 propagation classes: Emergency, Consensus, Coordination, Execution, Ai, Standard, Archive
- [x] Mission-scoped gossip isolation (Mission A gossip separate from Mission B)
- [x] Unit tests: 10+ tests covering each discovery scope, gossip class isolation
- [x] `cargo fmt -- --check` passes
- [x] `cargo test -p octo-network` passes

## Claimant

@agent (Jcode)

## Location

`crates/octo-network/src/mon/mod.rs` (discovery, gossip)

## Complexity

High

## Prerequisites

- Mission 0855: MON Mission Overlay Networks
- Mission 0855a: MON Mission Routing
- Mission 0851: GDP Gateway Discovery
- Mission 0852: DGP Deterministic Gossip

## Implementation Notes

- Stealth missions use encrypted advertisements (only holders of discovery capability keys can decrypt)
- Mission gossip reuses DGP infrastructure with mission-scoped domains
- Invite-only missions require Coordinator signature on invitation

## Reference

- RFC-0855 §8: Mission Discovery
- RFC-0855 §9: Mission Gossip
