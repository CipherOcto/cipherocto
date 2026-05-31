# Mission: DRS Onion-Compatible and Mission-Aware Routing

## Status

Open

## RFC

RFC-0856: Deterministic Route Selection (DRS) — §11, §13, §16, §17, §18

## Summary

Implement onion-compatible routing where route computation supports ORR (RFC-0858) layered encryption, and mission-aware routing with geographic isolation, trusted-only relays, and stealth modes.

## Acceptance Criteria

- [x] Onion-compatible route construction: compute routes that support layered encryption
- [x] Route computation includes per-hop key material
- [x] Mission-aware routing: geographic isolation (restrict to specific regions)
- [x] Trusted-only relays: filter routes by trust threshold
- [x] Low-bandwidth mode: prefer LoRa/Bluetooth for constrained environments
- [x] Stealth routing: minimize metadata leakage by preferring high-censorship-resistance carriers, avoiding known surveillance ASNs, randomizing hop selection within trust bounds (RFC §13)
- [x] Route persistence: cache routes for reuse (configurable TTL)
- [x] Partition resilience: automatic route recomputation on network partition (RFC §16)
- [x] Token economics integration: route cost calculation per RFC §17
- [x] AI-native routing: adaptive weight optimization per RFC §18
- [x] Integration with ORR (RFC-0858) for onion route construction
- [x] Unit tests: 19 tests covering onion compatibility, mission modes, partition recovery, cost, adaptive weights
- [x] `cargo fmt -- --check` passes
- [x] `cargo test -p octo-network` passes (636 tests)

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
- RFC-0856 §16: Partition Resilience
- RFC-0856 §17: Token Economics Integration
- RFC-0856 §18: AI-Native Routing
