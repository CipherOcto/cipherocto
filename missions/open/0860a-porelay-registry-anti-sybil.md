# Mission: PoRelay Trust Registry and Anti-Sybil

## Status

Open

## RFC

RFC-0860: Proof-of-Relay (PoRelay) — §5, §6

## Summary

Implement the trust registry for relay scores, anti-Sybil mechanisms (stake verification, diversity constraints), and recursive proof aggregation for relay proofs.

## Acceptance Criteria

- [ ] `TrustRegistry`: map gateway_id → RelayScore (RFC §5.1)
- [ ] Trust registry persistence (deterministic ordering)
- [ ] Trust score update: on new relay proof, recompute trust
- [ ] Anti-Sybil: stake-gated participation (minimum OCTO-B stake)
- [ ] Diversity constraints: prefer diverse gateway connections
- [ ] Sybil detection: identify clusters of gateways with correlated behavior
- [ ] Recursive relay proof aggregation: local proofs → regional → global
- [ ] Integration with DPS (RFC-0854a) for aggregation backend
- [ ] Gateway economics: monthly earnings calculation (OCTO-B + OCTO-N)
- [ ] Unit tests: 12+ tests covering registry, anti-Sybil, aggregation, economics
- [ ] `cargo fmt -- --check` passes
- [ ] `cargo test -p octo-network` passes

## Location

`crates/octo-network/src/porelay/mod.rs` (registry, anti-sybil)

## Complexity

High

## Prerequisites

- Mission 0860: PoRelay Proof-of-Relay
- Mission 0854: DPS Deterministic Proof Substrate
- Mission 0854a: DPS Recursive Proof Aggregation

## Implementation Notes

- Trust registry is deterministic: same proofs → same trust scores
- Anti-Sybil: stake verification + diversity constraints + correlation detection
- Recursive aggregation: local relay proofs → regional trust proofs → global overlay trust
- Gateway economics: monthly earnings = relay_bandwidth + uptime_bonus + diversity_premium

## Reference

- RFC-0860 §5: Trust Registry
- RFC-0860 §6: Anti-Sybil Mechanisms
- RFC-0860 §7: Economic Integration
