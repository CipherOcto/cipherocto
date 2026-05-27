# Mission: DRS Trust-Weighted and Multi-Path Routing

## Status

Open

## RFC

RFC-0856: Deterministic Route Selection (DRS) — §9, §12

## Summary

Implement trust-weighted routing with composite trust scores (historical uptime, PoR, stake weight, mission trust, consensus participation) and multi-path routing for high-priority traffic.

## Acceptance Criteria

- [ ] `TrustScore` with historical_uptime, proof_of_relay, stake_weight, mission_trust, consensus_participation (all u64) (RFC §9.1)
- [ ] Composite trust computation: weighted sum of trust factors
- [ ] Trust root Merkle commitment for deterministic verification
- [ ] Multi-path routing: simultaneous route utilization for high-priority traffic
- [ ] Traffic splitting: deterministic load balancing via packet_sequence_number % route_count (RFC §12.1). "Configurable" means policy enum selection (Failover vs Redundant vs LoadBalance), not numeric ratios.
- [ ] Path diversity: maximize transport diversity across paths
- [ ] Integration with PoRelay (RFC-0860) for proof-of-relay trust — PoRelay provides relay attestation proofs that map to `TrustScore.proof_of_relay`. The 1000-attestation cap in `compute_trust_score` prevents gaming.
- [ ] Unit tests: 10+ tests covering trust computation, multi-path selection, diversity
- [ ] `cargo fmt -- --check` passes
- [ ] `cargo test -p octo-network` passes

## Location

`crates/octo-network/src/drs/trust.rs`, `crates/octo-network/src/drs/route.rs`

## Complexity

High

## Prerequisites

- Mission 0856: DRS Deterministic Route Selection
- Mission 0860: PoRelay Proof-of-Relay

## Implementation Notes

- Trust score is Class A (deterministic) — all nodes compute identical scores from identical inputs
- Multi-path routing uses DRS scoring for each path independently
- Path diversity: prefer different transport types across paths
- Trust factors: uptime (continuous), PoR (per-relay), stake (economic), mission (context), consensus (participation)

## Reference

- RFC-0856 §9: Trust-Weighted Routing
- RFC-0856 §12: Multi-Path Routing
