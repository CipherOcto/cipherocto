# Mission: DRS Deterministic Route Selection

## Status

Open

## RFC

RFC-0856: Deterministic Route Selection (DRS)

## Summary

Implement deterministic route selection with canonical scoring (u64 arithmetic, saturating_mul), trust-weighted selection, route commitments, route cache with deterministic eviction, and governance-modifiable weight configuration.

## Acceptance Criteria

- [ ] `DeterministicRoute` with route_id, source_gateway, destination_gateway, next_hop, transport_vector_root, trust_score, bandwidth_class, latency_class, censorship_resistance_class, route_cost, route_epoch, ttl_hops, signature
- [ ] `TransportVector` with transport_type, transport_class, reliability_score, censorship_score, cost_class
- [ ] `RouteWeights` with trust, bandwidth, latency, censorship_resistance (u64 fields)
- [ ] `compute_route_score()` using u64 arithmetic with saturating_mul (no overflow)
- [ ] `canonical_route_cmp()`: (score DESC, epoch ASC, route_id ASC)
- [ ] `RouteCommitment` with BLAKE3-256(gateway_sequence_hash || weights_hash || epoch)
- [ ] `RouteCache` with BTreeMap, deterministic eviction
- [ ] Weight configuration: network-level constants at genesis, governance proposal for changes (RFC-0001, 2/3 vote)
- [ ] RFC-0008 execution class mapping table
- [ ] `DrsError` enum with all error variants
- [ ] Unit tests: 12+ tests covering scoring determinism, overflow safety, ordering, cache eviction
- [ ] `cargo fmt -- --check` passes
- [ ] `cargo test -p octo-network` passes

## Location

`crates/octo-network/src/drs/`

## Complexity

High

## Prerequisites

- Mission 0850: DOT Core Envelope and Native P2P
- Mission 0851: GDP Gateway Discovery
- Mission 0852: DGP Deterministic Gossip
- Mission 0853: OCrypt Overlay Cryptography
- Mission 0855: MON Mission Overlay Networks

## Implementation Notes

- See `docs/07-developers/networking-implementation-guide.md` for concrete Rust code
- Scoring formula uses u64 ONLY — no u32 intermediates (overflow risk)
- Weights are u64, not u32 — test with u64::MAX to verify saturating_mul
- Route ordering is deterministic: (score DESC, epoch ASC, route_id ASC)
- Forbidden inputs: latency measurements, local heuristics, wall-clock, CPU load

## Reference

- RFC-0856: Deterministic Route Selection (§4, §5, §6, §7, §8)
- `docs/07-developers/networking-implementation-guide.md` (Canonical Scoring)
