# Mission: MON Mission Discovery and Gossip

## Status

Open

## RFC

RFC-0855: Mission Overlay Networks (MON) — §8, §9

## Summary

Implement mission discovery with 5 scopes (Public, Invite-only, Stealth, Federated, Ephemeral) and mission gossip with 7 propagation classes.

## Acceptance Criteria

- [ ] `MissionDiscoveryScope` enum: Public, InviteOnly, Stealth, Federated, Ephemeral
- [ ] Public missions: discoverable via GDP global scope
- [ ] Invite-only missions: require invitation from Coordinator
- [ ] Stealth missions: encrypted advertisements, only capability key holders can decrypt
- [ ] Mission gossip integration with DGP (RFC-0852)
- [ ] 7 gossip classes: Coordination, Consensus, Execution, AI, Archive, Emergency, Standard
- [ ] Mission-scoped gossip isolation (Mission A gossip separate from Mission B)
- [ ] Unit tests: 10+ tests covering each discovery scope, gossip class isolation
- [ ] `cargo fmt -- --check` passes
- [ ] `cargo test -p octo-network` passes

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
