# Mission: DGP Gossip Compression and Retention

## Status

Open

## RFC

RFC-0852: Deterministic Gossip Protocol (DGP) — §11, §13

## Summary

Implement gossip compression (Bloom filters, Merkle roots, bitmap summaries) for efficient state synchronization and retention classes for storage management.

## Acceptance Criteria

- [ ] Bloom filter summary for quick "is member" checks
- [ ] Merkle root summary for state verification
- [ ] Bitmap summary for range commitments
- [ ] `RetentionClass` enum: Ephemeral, Mission, Consensus, Archive (per RFC-0852 §13)
- [ ] Retention duration per class (configurable)
- [ ] Automatic cleanup of expired objects by retention class
- [ ] Anti-entropy integration: compression summaries usable in anti-entropy sync (Mission 0852a)
- [ ] Bloom filter hash uses BLAKE3-256 (not AHasher/SipHash) per RFC-0852 §11
- [ ] Large object fragmentation via `GossipFragment`
- [ ] Unit tests: 8+ tests covering compression, retention cleanup, fragmentation
- [ ] `cargo fmt -- --check` passes
- [ ] `cargo test -p octo-network` passes

## Location

`crates/octo-network/src/dgp/mod.rs` (compression, retention)

## Complexity

Medium

## Prerequisites

- Mission 0852: DGP Deterministic Gossip

## Implementation Notes

- Bloom filters for set membership (false positive acceptable, false negative not)
- Merkle roots for state verification (O(log n) proof size)
- Retention classes map to storage tiers (ephemeral = memory, permanent = disk)
- Fragmentation for objects larger than platform max payload

## Reference

- RFC-0852 §11: Gossip Compression
- RFC-0852 §13: Retention Classes
