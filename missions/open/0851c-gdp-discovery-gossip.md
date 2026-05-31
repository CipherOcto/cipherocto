# Mission: GDP Discovery Gossip Propagation

## Status

Open

## RFC

RFC-0851: Gateway Discovery Protocol (GDP) — §13

## Summary

Implement discovery gossip with flood (bootstrap), incremental (normal operation), anti-entropy (state healing), and directed (mission overlays) propagation modes.

## Acceptance Criteria

- [x] Flood gossip: broadcast aggressively for bootstrap (DGP object_type = DiscoveryAdvertisement)
- [x] Incremental gossip: propagate only unseen advertisements
- [x] Anti-entropy gossip: periodic Merkle summary reconciliation (60s default)
- [x] Directed gossip: targeted propagation for mission overlays
- [x] Propagation limits: TTL hops per scope (Local=3, Regional=10, Mission=5, Global=20, Consensus=10)
- [x] Advertisement deduplication by gateway_id + sequence
- [x] GDP advertisements wrap as DGP `GossipObject` with `object_type = DiscoveryAdvertisement`
- [x] GDP DiscoveryScope maps to DGP GossipDomainId.scope (Local→LOCAL, Regional→REGIONAL, etc.)
- [x] Integration with DGP (RFC-0852) gossip infrastructure
- [x] Unit tests: 12 tests covering each gossip mode, TTL enforcement, deduplication
- [x] `cargo fmt -- --check` passes
- [x] `cargo test -p octo-network` passes (636 tests)

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
