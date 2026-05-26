# Mission: GDP Discovery Gossip Propagation

## Status

Open

## RFC

RFC-0851: Gateway Discovery Protocol (GDP) — §13

## Summary

Implement discovery gossip with flood (bootstrap), incremental (normal operation), anti-entropy (state healing), and directed (mission overlays) propagation modes.

## Acceptance Criteria

- [ ] Flood gossip: broadcast aggressively for bootstrap
- [ ] Incremental gossip: propagate only unseen advertisements
- [ ] Anti-entropy gossip: periodic Merkle summary reconciliation
- [ ] Directed gossip: targeted propagation for mission overlays
- [ ] Propagation limits: TTL hops to constrain graph explosion
- [ ] Advertisement deduplication by gateway_id + sequence
- [ ] Integration with DGP (RFC-0852) gossip infrastructure
- [ ] Unit tests: 8+ tests covering each gossip mode, TTL enforcement
- [ ] `cargo fmt -- --check` passes
- [ ] `cargo test -p octo-network` passes

## Location

`crates/octo-network/src/gdp/discovery.rs` (gossip extensions)

## Complexity

Medium

## Prerequisites

- Mission 0851: GDP Gateway Discovery
- Mission 0851a: GDP Discovery Scopes and Lifecycle
- Mission 0852: DGP Deterministic Gossip

## Implementation Notes

- Reuses DGP gossip infrastructure for propagation
- Flood mode for bootstrap only (bandwidth-expensive)
- Incremental mode for normal operation (only unseen objects)
- Anti-entropy for state healing when divergence detected

## Reference

- RFC-0851 §13: Discovery Gossip
