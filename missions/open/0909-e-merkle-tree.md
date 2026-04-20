# Mission: RFC-0909 build_merkle_tree — Cryptographic Spend Proofs

## Status

Open (v5)

## RFC

RFC-0909 v59 (Economics): Deterministic Quota Accounting

## Summary

Implement `build_merkle_tree()` — builds a Merkle tree from SpendEvents for cryptographic proof generation. Leaf = SHA256(event_id_hex_as_bytes || cost_amount). Internal nodes = SHA256(left_hash || right_hash). Returns root for publication.

## Acceptance Criteria

- [ ] `build_merkle_tree(events: &[SpendEvent]) -> Option<MerkleNode>` — returns root node or None if empty
- [ ] Sort events by event_id (hex string, ascending) — same as replay_events
- [ ] Leaf hash: `SHA256(event_id.as_bytes() || cost_amount.to_le_bytes())` where `cost_amount: u64` (8-byte little-endian encoding required for cross-router determinism)
- [ ] Internal node hash: `SHA256(left_hash || right_hash)`
- [ ] Odd leaf count: pad by duplicating last leaf (deterministic, keeps tree balanced)
- [ ] Build bottom-up until single root remains
- [ ] Returns `Option<MerkleNode>` — `None` for empty events (no root to publish)
- [ ] `MerkleNode` struct: `#[derive(Debug, Clone)] pub struct MerkleNode { pub hash: [u8; 32], pub left: Option<Box<MerkleNode>>, pub right: Option<Box<MerkleNode>> }` (per RFC-0909 pseudocode)
- [ ] Multi-tenant safety: caller MUST filter events to single tenant scope before calling (RFC-0909 §Security Note — No Field Delimiters)

## Implementation Notes

- Location: `crates/quota-router-core/src/keys/models.rs` or new `merkle.rs` module
- Import: `use sha2::{Digest, Sha256};`
- Leaf hashing: `hasher.update(e.event_id.as_bytes())` then `hasher.update(e.cost_amount.to_le_bytes())`
- `event_id` is hex String in struct (64 ASCII chars) — hashing the raw bytes would produce different results; this uses the application-layer hex String
- Odd leaf padding: duplicate last element before chunking into pairs
- This function is NOT used for budget computation — only for cryptographic proof generation
- **DB→hex conversion (C2):** If building the tree from database rows (BLOB(32) storage), MUST convert event_id BLOB to hex via `blob_32_to_hex()` before hashing. Hashing raw BLOB bytes produces a different leaf hash than hashing the 64-char hex string — roots built from different representations will not match. Routers using in-memory `SpendEvent` structs already have hex String and are unaffected. See RFC-0909 §Audit Proof Generation.
- **Known limitation (H2):** If `record_spend()` has an internal bug producing duplicate logical events (same economic content, different request_id), the Merkle tree double-counts the cost with no error. Schema enforces `UNIQUE(key_id, request_id)` but cannot prevent same-cost duplicates from an application bug. Correct `record_spend` implementation is the caller's responsibility. `record_spend()` is defined in RFC-0903 Final §record_spend.
- **Test vectors (L1):** (1) empty events → `None`; (2) single event → root equals leaf hash; (3) two identical events → parent hash = SHA256(leaf_hash || leaf_hash); (4) odd count (3 leaves) → padded to 4, last leaf duplicated; (5) two different events (different hashes) → parent hash = SHA256(hash_A || hash_B) where hash_A ≠ hash_B.

## Dependencies

- `sha2 = "0.10"` for SHA256 hashing

## Reference

- RFC-0909 §build_merkle_tree
- RFC-0909 §Audit Proof Generation
- RFC-0909 §Canonical Merkle root (event_id-only ordering for external verification)
- RFC-0909 §Security Note — No Field Delimiters (multi-tenant caller filtering requirement)
- RFC-0903-B1 §SpendEvent (struct definition, BLOB(32) event_id encoding)

## Complexity

Medium — iterative bottom-up tree construction with SHA256 hashing

---
**Mission Type:** Implementation
**Priority:** Critical
**Phase:** RFC-0909 Phase 1 Core

## Changelog

| Version | Date | Changes |
|---------|------|---------|
| v5 | 2026-04-20 | Round 4 adversarial review fixes: fix L1 (Complexity section: "recursive" → "iterative bottom-up" — implementation is iterative, not recursive) |
| v4 | 2026-04-20 | Round 3 adversarial review fixes: fix M1 (specify cost_amount: u64 in leaf hash formula — 8-byte LE width required for cross-router determinism) |
| v3 | 2026-04-20 | Round 2 adversarial review fixes: fix H1 (add record_spend cross-reference to RFC-0903 Final §record_spend); fix M1 (move Dependencies before Reference for consistency); fix L1 (add two-different-hashes test vector) |
| v2 | 2026-04-20 | Round 1 adversarial review fixes: fix C1 (explicit MerkleNode struct AC with derive); fix C2 (add DB→hex conversion requirement); fix H1 (add sha2 crate dependency); fix H2 (document double-charge known limitation); fix M1 (clarify little-endian requirement in AC); fix M2 (Priority High→Critical); fix L1 (add test vectors); fix L2 (add RFC-0903-B1 §SpendEvent reference) |
