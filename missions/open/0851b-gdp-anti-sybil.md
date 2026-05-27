# Mission: GDP Anti-Sybil Mechanisms

## Status

Open

## RFC

RFC-0851: Gateway Discovery Protocol (GDP) — §11

## Summary

Implement anti-Sybil mechanisms for gateway discovery including stake-gated discovery, diversity constraints, and Proof-of-Reliability integration.

## Acceptance Criteria

- [ ] Stake-gated discovery: minimum stake required for global advertisement propagation
- [ ] Diversity constraints: transport, geographic, organizational, trust-source diversity
- [ ] Eclipse attack resistance via diversity requirements
- [ ] Integration with PoRelay (RFC-0860) for reliability weighting
- [ ] Sybil detection via stake verification
- [ ] Global vs regional stake requirements (GLOBAL = highest)
- [ ] Unit tests: 6+ tests covering stake gating, diversity enforcement, Sybil detection
- [ ] `cargo fmt -- --check` passes
- [ ] `cargo test -p octo-network` passes

## Location

`crates/octo-network/src/gdp/discovery.rs` (anti-sybil extensions)

## Complexity

High

## Prerequisites

- Mission 0851: GDP Gateway Discovery
- Mission 0851a: GDP Discovery Scopes and Lifecycle
- Mission 0860: PoRelay Proof-of-Relay

## Updated Acceptance Criteria (M-GDP-5 fix)

The original acceptance criteria were too vague. Updated with concrete values from RFC-0851 Section 11:

- [ ] Stake-gated discovery: minimum OCTO per scope (Local=0, Regional=500, Global=1000, Consensus=1000)
- [ ] OCTO-B role stake per scope (Local=0, Regional=50, Global=100, Consensus=200)
- [ ] Mission and Private scopes per RFC-0851 Section 11.1 (mission-defined / invite-only)
- [ ] Diversity constraint: `diversity_score = transport_diversity * 3 + geographic_diversity * 2 + trust_diversity * 1`
- [ ] Minimum diversity thresholds: Regional ≥ 2 transports, Global ≥ 3 transports + 2 regions
- [ ] Non-compliant gateways deprioritized (score = 0), not rejected
- [ ] Sybil cluster detection via correlated behavior analysis
- [ ] Integration with RFC-0860 Section 6 anti-sybil model
- [ ] 10+ tests covering stake gating, diversity enforcement, Sybil detection

## Implementation Notes

- Stake requirements scale with visibility scope (LOCAL=0, REGIONAL=0.5, MISSION=1.0, GLOBAL=2.0)
- Diversity constraints prevent eclipse attacks (multiple transports, geographic regions)
- PoRelay integration provides trust scores for discovery weighting

## Reference

- RFC-0851 §11: Anti-Sybil Mechanisms
