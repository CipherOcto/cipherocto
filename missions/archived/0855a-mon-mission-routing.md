# Mission: MON Mission Routing

## Status

Implemented (2 files, 26 tests: drs/mission_routing.rs + mon/reconciliation.rs)

## RFC

RFC-0855: Mission Overlay Networks (MON) — §6

## Summary

Implement mission-scoped routing where mission traffic is isolated from other missions, with deterministic route selection within mission boundaries.

## Acceptance Criteria

- [x] Mission-scoped route tables: Mission A routes separate from Mission B
- [x] Deterministic route selection within mission (DRS integration)
- [x] Route isolation: mission traffic MUST NOT leak to non-mission gateways
- [x] Mission route commitment: BLAKE3-256(mission_id || route_sequence || epoch)
- [x] Route table Merkle commitment for deterministic replay
- [x] Integration with DRS (RFC-0856) for route computation
- [x] Unit tests: 8+ tests covering route isolation, deterministic selection
- [x] `cargo fmt -- --check` passes
- [x] `cargo test -p octo-network` passes

## Claimant

@agent (Jcode)

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
