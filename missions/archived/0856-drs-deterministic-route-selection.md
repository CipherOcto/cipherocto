# Mission: DRS Deterministic Route Selection

## Status

Implemented (8 files, 61 tests)

## RFC

RFC-0856: Deterministic Route Selection (DRS)

## Summary

Implement deterministic route selection with canonical scoring (u64 arithmetic, saturating_mul), trust-weighted selection, route commitments, route cache with deterministic eviction, and governance-modifiable weight configuration.

## Acceptance Criteria

- [x] `DeterministicRoute` with route_id, source_gateway, destination_gateway, next_hop, transport_vector_root, trust_score, bandwidth_class, latency_class, censorship_resistance_class, route_cost, route_epoch, ttl_hops, signature
- [x] `TransportVector` with transport_type, transport_class, reliability_score, censorship_score, cost_class
- [x] `ScoringWeights` with trust_weight, bandwidth_weight, latency_weight, censorship_weight, cost_weight (u64 fields) (RFC §6.1)
- [x] `compute_route_score()` using u64 arithmetic with saturating_mul (no overflow)
- [x] `canonical_route_cmp()`: (score DESC, epoch ASC, route_id ASC)
- [x] `RouteCommitment` with BLAKE3-256(gateway_sequence_hash || weights_hash || epoch)
- [x] `RouteCache` with BTreeMap, deterministic eviction
- [x] Weight configuration: network-level constants at genesis, governance proposal for changes (RFC-0001, 2/3 vote)
- [x] RFC-0008 execution class mapping table
- [x] `DrsError` enum with all error variants (RFC Error Types section)
- [x] Unit tests: 12+ tests covering scoring determinism, overflow safety, ordering, cache eviction
- [x] `cargo fmt -- --check` passes
- [x] `cargo test -p octo-network` passes (638 total)

## Claimant

@agent (Jcode)

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
- RFC-0858 (optional): ORR Onion Relay Routing — for onion-compatible routing (Mission 0856b)

## Implementation Notes

- See `docs/07-developers/networking-implementation-guide.md` for concrete Rust code
- Scoring formula uses u64 ONLY — no u32 intermediates (overflow risk)
- All arithmetic uses saturating_mul, saturating_add, saturating_sub
- Weights are u64 — test with u64::MAX to verify saturating arithmetic
- Route ordering is deterministic: (score DESC, epoch ASC, route_id ASC)
- Forbidden inputs: latency measurements, local heuristics, wall-clock, CPU load

## Reference

- RFC-0856: Deterministic Route Selection (§4, §5, §6, §7, §8)
- `docs/07-developers/networking-implementation-guide.md` (Canonical Scoring)
