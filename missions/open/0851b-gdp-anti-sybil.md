# Mission: GDP Anti-Sybil Mechanisms

## Status

Open

## RFC

RFC-0851: Gateway Discovery Protocol (GDP) — §11

## Summary

Implement anti-Sybil mechanisms for gateway discovery including stake-gated discovery, diversity constraints, and Proof-of-Reliability integration.

## Acceptance Criteria

### Stake Gating (RFC-0851 §11.1)
- [x] Minimum OCTO per scope: Local=0, Regional=500, Global=1000, Consensus=1000
- [x] OCTO-B role stake per scope: Local=0, Regional=50, Global=100, Consensus=200
- [x] Mission and Private scopes per RFC-0851 Section 11.1 (mission-defined / invite-only)
- [x] Insufficient stake → advertisement silently dropped
- [x] Stake verification uses integer arithmetic only (RFC-0008 Class A)

### Diversity Constraints (RFC-0851 §11.2)
- [x] Diversity formula: `diversity_score = transport_diversity * 3 + geographic_diversity * 2 + trust_diversity * 1`
- [x] Minimum thresholds: Regional ≥ 2 transports, Global ≥ 3 transports + 2 regions
- [x] Non-compliant gateways deprioritized (score = 0), not rejected
- [x] Eclipse attack resistance via diversity requirements

### Sybil Detection (RFC-0851 §11.3)
- [x] Sybil cluster detection via correlated behavior analysis
- [x] Integration with PoRelay (RFC-0860) for reliability weighting
- [x] Integration with RFC-0860 Section 6 anti-sybil model

### Verification
- [x] 10 tests covering stake gating, diversity enforcement, Sybil detection
- [x] `cargo fmt -- --check` passes
- [x] `cargo test -p octo-network` passes (643 tests)

## Implementation Notes

- Stake requirements scale with visibility scope (LOCAL=0, REGIONAL=500, MISSION=1000, GLOBAL=1000) — integer OCTO values per Section 11.1, RFC-0008 Class A compliant
- Diversity constraints prevent eclipse attacks (multiple transports, geographic regions)
- PoRelay integration provides trust scores for discovery weighting

## Reference

- RFC-0851 §11: Anti-Sybil Mechanisms
