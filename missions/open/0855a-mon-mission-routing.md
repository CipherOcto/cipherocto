# Mission: MON Mission Routing

## Status

Open

## RFC

RFC-0855: Mission Overlay Networks (MON) — §6

## Summary

Implement mission-scoped routing where mission traffic is isolated from other missions, with deterministic route selection within mission boundaries.

## Acceptance Criteria

- [ ] Mission-scoped route tables: Mission A routes separate from Mission B
- [ ] Deterministic route selection within mission (DRS integration)
- [ ] Route isolation: mission traffic MUST NOT leak to non-mission gateways
- [ ] Mission route commitment: BLAKE3-256(mission_id || route_sequence || epoch)
- [ ] Route table Merkle commitment for deterministic replay
- [ ] Integration with DRS (RFC-0856) for route computation
- [ ] Unit tests: 8+ tests covering route isolation, deterministic selection
- [ ] `cargo fmt -- --check` passes
- [ ] `cargo test -p octo-network` passes

## Location

`crates/octo-network/src/mon/routing.rs`

## Complexity

High

## Prerequisites

- Mission 0855: MON Mission Overlay Networks
- Mission 0856: DRS Deterministic Route Selection

## Implementation Notes

- Mission routing uses DRS (RFC-0856) for deterministic route computation
- Route isolation enforced at the gateway level (reject non-mission traffic)
- Route commitment allows replay verification
- Route table is mission-scoped, not global

## Reference

- RFC-0855 §6: Mission Routing
- RFC-0856: Deterministic Route Selection
- RFC-0851: Gateway Discovery Protocol
