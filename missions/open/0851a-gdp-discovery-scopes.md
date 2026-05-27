# Mission: GDP Discovery Scopes and Lifecycle

## Status

Open

## RFC

RFC-0851: Gateway Discovery Protocol (GDP) — §2, §8

## Summary

Implement discovery scopes (LOCAL, REGIONAL, MISSION, GLOBAL, PRIVATE), discovery lifecycle (bootstrap → expansion → stabilization), and discovery plane separation from data plane.

## Acceptance Criteria

- [ ] `DiscoveryScope` enum: Local, Regional, Mission, Global, Private
- [ ] Discovery plane separated from data plane (no recursive routing)
- [ ] Bootstrap phase: static seed list, QR/bootstrap blob, local broadcast, trusted peers
- [ ] Expansion phase: gateway advertises peers, peer graph expands recursively
- [ ] Stabilization phase: preferred gateways, trust-weighted neighbors, route diversity
- [ ] Scope-based filtering: gateways only visible within their scope
- [ ] Mission-scoped discovery: temporary overlay bootstrap
- [ ] Unit tests: 8+ tests covering scope filtering, lifecycle transitions
- [ ] `cargo fmt -- --check` passes
- [ ] `cargo test -p octo-network` passes

## Location

`crates/octo-network/src/gdp/discovery.rs`

## Complexity

Medium

## Prerequisites

- Mission 0851: GDP Gateway Discovery

> **Note (H-GDP-7):** Mission 0851a implements GDP's 6 discovery scopes (Local, Regional, Mission, Global, Private, Consensus). RFC-0855 MON defines a separate `MissionDiscoveryScope` enum (starting at 0x0100) for mission-specific visibility. The mapping between GDP scopes and MON scopes is defined in RFC-0851 Section 2.

## Implementation Notes

- Discovery plane handles visibility/topology; data plane handles envelope routing
- Bootstrap methods: static seeds (hardcoded), QR blob (human transfer), LAN broadcast, existing DOT domain, trusted peers, mission invitation
- Stabilization maintains anti-eclipse diversity constraints

## Reference

- RFC-0851 §2: Design Goals
- RFC-0851 §8: Discovery Lifecycle
