# Mission: RFC-0909 build_merkle_tree — Cryptographic Spend Proofs

## Status

Open

## RFC

RFC-0909 v59 (Economics): Deterministic Quota Accounting

## Summary

Implement `build_merkle_tree()` — builds a Merkle tree from SpendEvents for cryptographic proof generation. Leaf = SHA256(event_id_hex_as_bytes || cost_amount). Internal nodes = SHA256(left_hash || right_hash). Returns root for publication.

## Acceptance Criteria

- [ ] `build_merkle_tree(events: &[SpendEvent]) -> Option<MerkleNode>` — returns root node or None if empty
- [ ] Sort events by event_id (hex string, ascending) — same as replay_events
- [ ] Leaf hash: `SHA256(event_id.as_bytes() || cost_amount.to_le_bytes())`
- [ ] Internal node hash: `SHA256(left_hash || right_hash)`
- [ ] Odd leaf count: pad by duplicating last leaf (deterministic, keeps tree balanced)
- [ ] Build bottom-up until single root remains
- [ ] Returns `Option<MerkleNode>` — `None` for empty events (no root to publish)
- [ ] `MerkleNode` struct: `{ hash: [u8; 32], left: Option<Box<MerkleNode>>, right: Option<Box<MerkleNode>> }`
- [ ] Multi-tenant safety: caller MUST filter events to single tenant scope before calling (RFC-0909 §Security Note — No Field Delimiters)

## Implementation Notes

- Location: `crates/quota-router-core/src/keys/models.rs` or new `merkle.rs` module
- Import: `use sha2::{Digest, Sha256};`
- Leaf hashing: `hasher.update(e.event_id.as_bytes())` then `hasher.update(e.cost_amount.to_le_bytes())`
- `event_id` is hex String in struct (64 ASCII chars) — hashing the raw bytes would produce different results; this uses the application-layer hex String
- Odd leaf padding: duplicate last element before chunking into pairs
- This function is NOT used for budget computation — only for cryptographic proof generation

## Reference

- RFC-0909 §build_merkle_tree
- RFC-0909 §Audit Proof Generation
- RFC-0909 §Canonical Merkle root (event_id-only ordering for external verification)
- RFC-0909 §Security Note — No Field Delimiters (multi-tenant caller filtering requirement)

## Complexity

Medium — recursive tree construction with SHA256 hashing

---
**Mission Type:** Implementation
**Priority:** High
**Phase:** RFC-0909 Phase 1 Core
