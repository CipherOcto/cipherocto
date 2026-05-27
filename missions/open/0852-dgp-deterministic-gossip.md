# Mission: DGP Deterministic Gossip

## Status

Open

## RFC

RFC-0852: Deterministic Gossip Protocol (DGP)

## Summary

Implement deterministic gossip with gossip objects, domains, canonical processing order, deduplication, replay cache, and multi-mode gossip (flood, incremental, directed). Anti-entropy synchronization is covered by Mission 0852a.

## Acceptance Criteria

- [ ] `GossipObject` with object_type, object_hash, object_size, domain_id, logical_timestamp, origin_gateway, ttl_hops, propagation_flags, payload_root, signature
- [ ] `GossipDomainId` with network_id, mission_id, scope
- [ ] Canonical processing order: (domain_id, logical_timestamp, object_hash)
- [ ] Deduplication by object_hash with FIRST_VALID_HASH_WINS conflict resolution
- [ ] GossipReplayCache with BTreeMap, deterministic eviction (RFC-0852 §12)
- [ ] Flood gossip mode for bootstrap
- [ ] Incremental gossip mode for normal operation
- [ ] Directed gossip mode for mission-scoped propagation
- [ ] `DgpError` enum with all error variants
- [ ] Unit tests: 10+ tests covering ordering, dedup, replay cache, modes
- [ ] `cargo fmt -- --check` passes
- [ ] `cargo test -p octo-network` passes

## Location

`crates/octo-network/src/dgp/`

## Complexity

High

## Prerequisites

- Mission 0850: DOT Core Envelope and Native P2P
- Mission 0851: GDP Gateway Discovery

## Implementation Notes

- See `docs/07-developers/networking-implementation-guide.md` for concrete Rust code
- Processing order is by (domain_id, logical_timestamp, object_hash) — NOT arrival order
- Deduplication uses HashSet<[u8; 32]> for O(1) lookup
- Replay cache uses BTreeMap for deterministic eviction (see RFC-0852 §12)
- GossipPriority: Critical > Consensus > Mission > Standard > Bulk > Archive

## Reference

- RFC-0852: Deterministic Gossip Protocol (§4, §5, §6, §8, §9, §12)
- `docs/07-developers/networking-implementation-guide.md` (Module Tree)
