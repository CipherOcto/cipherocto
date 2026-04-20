# Mission: RFC-0909 tokenizer_version_to_id + tokenizer_id_to_version

## Status

Open

## RFC

RFC-0909 v59 (Economics): Deterministic Quota Accounting

## Summary

Implement bidirectional tokenizer ID conversion: version string → BLAKE3-16 bytes (for storage), and BLAKE3-16 bytes → version string (for retrieval lookup). The reverse lookup requires a database query against the tokenizers table.

## Acceptance Criteria

- [ ] `tokenizer_version_to_id(version: &str) -> [u8; 16]` — BLAKE3(version.as_bytes()) truncated to 16 bytes
- [ ] Test vector: `"tiktoken-cl100k_base-v1.2.3"` → `"e3c8e8ff724411c6416dd4fb135368e3"` (16 bytes hex)
- [ ] `#[inline]` on both functions
- [ ] `tokenizer_id_to_version(id: &[u8; 16]) -> Result<Option<String>, &'static str>` — stub returning `Err("tokenizer_id_to_version: requires DB lookup implementation")`
- [ ] `tokenizer_id_to_version` full implementation: `SELECT version FROM tokenizers WHERE tokenizer_id = ?` → `Ok(Some(version))` on match, `Ok(None)` on no match

## Implementation Notes

- Location: `crates/quota-router-core/src/keys/models.rs` or `crates/quota-router-core/src/tokenizer.rs`
- `tokenizer_version_to_id`: Uses `blake3::Hasher::new()`, `hasher.update()`, `hasher.finalize()` → `[u8; 32]` → `bytes[..16].try_into().expect()`
- Truncation note: collision probability non-negligible after ~2^32 versions — acceptable for tokenizer versioning
- `tokenizer_id_to_version`: The stub always returns an error. The DB-backed version requires a Stoolap query against the `tokenizers` table (schema per RFC-0909 §Tokenizer Database Schema, defined in RFC-0910)
- The `tokenizers` table is populated on-demand at INSERT time; version string is stored in `provider_usage_json` field for audit

## Reference

- RFC-0909 §tokenizer_version_to_id
- RFC-0909 §tokenizer_id_to_version
- RFC-0909 §Truncation Note
- RFC-0910 §Tokenizer Database Schema (tokenizers table)
- RFC-0903-B1 §tokenizer_id (BLAKE3-16 derivation)

## Complexity

Low — BLAKE3 hashing + optional DB query

---
**Mission Type:** Implementation
**Priority:** High
**Phase:** RFC-0909 Phase 1 Core
