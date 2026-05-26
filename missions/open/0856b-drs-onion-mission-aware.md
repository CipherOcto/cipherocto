# Mission: DRS Onion-Compatible and Mission-Aware Routing

## Status

Open

## RFC

RFC-0856: Deterministic Route Selection (DRS) — §11, §13

## Summary

Implement onion-compatible routing where route computation supports ORR (RFC-0858) layered encryption, and mission-aware routing with geographic isolation, trusted-only relays, and stealth modes.

## Acceptance Criteria

- [ ] Onion-compatible route construction: compute routes that support layered encryption
- [ ] Route computation includes per-hop key material
- [ ] Mission-aware routing: geographic isolation (restrict to specific regions)
- [ ] Trusted-only relays: filter routes by trust threshold
- [ ] Low-bandwidth mode: prefer LoRa/Bluetooth for constrained environments
- [ ] Stealth routing: minimize metadata leakage in route selection
- [ ] Route persistence: cache routes for reuse (configurable TTL)
- [ ] Partition resilience: automatic route recomputation on network partition
- [ ] Integration with ORR (RFC-0858) for onion route construction
- [ ] Unit tests: 10+ tests covering onion compatibility, mission modes, partition recovery
- [ ] `cargo fmt -- --check` passes
- [ ] `cargo test -p octo-network` passes

## Location

`crates/octo-network/src/drs/route.rs` (onion, mission-aware extensions)

## Complexity

High

## Prerequisites

- Mission 0856: DRS Deterministic Route Selection
- Mission 0856a: DRS Trust-Weighted and Multi-Path Routing
- Mission 0858: ORR Onion Relay Routing

## Implementation Notes

- Onion-compatible: route includes per-hop key derivation material
- Mission-aware: route selection respects mission constraints (geo, trust, bandwidth)
- Stealth routing: prefer high-censorship-resistance carriers
- Route persistence: deterministic cache with TTL-based expiration

## Reference

- RFC-0856 §11: Onion-Compatible Routing
- RFC-0856 §13: Mission-Aware Routing
- RFC-0856 §15: Partition Resilience
