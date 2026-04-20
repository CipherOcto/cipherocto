# Mission: RFC-0909 tokenizer_version_to_id + tokenizer_id_to_version

## Status

Open (v2)

## RFC

RFC-0909 v59 (Economics): Deterministic Quota Accounting

## Summary

Implement bidirectional tokenizer ID conversion: version string → BLAKE3-16 bytes (for storage), and BLAKE3-16 bytes → version string (for retrieval lookup). The reverse lookup requires a database query against the tokenizers table.

## Acceptance Criteria

- [ ] `tokenizer_version_to_id(version: &str) -> [u8; 16]` — BLAKE3(version.as_bytes()) truncated to 16 bytes
- [ ] Test vector: `"tiktoken-cl100k_base-v1.2.3"` → `"e3c8e8ff724411c6416dd4fb135368e3"` (16 bytes hex). Full BLAKE3 (for verification): `e3c8e8ff724411c6416dd4fb135368e36b5fdcec3ecc2cd13920767ed230b103`
- [ ] `#[inline]` on `tokenizer_version_to_id` (per RFC pseudocode; `#[inline]` on `tokenizer_id_to_version` is not in RFC but is acceptable)
- [ ] `tokenizer_id_to_version(id: &[u8; 16]) -> Result<Option<String>, &'static str>` — stub returning `Err("tokenizer_id_to_version: requires DB lookup implementation")`. DB-level errors (connection failure) propagate via a different error path — callers should substitute `Err(KeyError::Storage)` in the error arm until a unified error strategy is defined.
- [ ] `tokenizer_id_to_version` full implementation: `SELECT version FROM tokenizers WHERE tokenizer_id = ?` → `Ok(Some(version))` on match, `Ok(None)` on no match

## Implementation Notes

- Location: `crates/quota-router-core/src/keys/models.rs` or `crates/quota-router-core/src/tokenizer.rs`
- `tokenizer_version_to_id`: Uses `blake3::Hasher::new()`, `hasher.update()`, `hasher.finalize()` → `[u8; 32]` → `bytes[..16].try_into().unwrap()`
- Truncation note: collision probability non-negligible after ~2^32 versions — acceptable for tokenizer versioning
- `tokenizer_id_to_version`: The stub always returns an error. The DB-backed version requires a Stoolap query against the `tokenizers` table (schema per RFC-0910 §Tokenizer Database Schema)
- **Dependencies (C2):** Add `blake3 = "1.x"` to `Cargo.toml` dependencies
- **Tokenizer table population (M2):** The `tokenizers` table is populated on-demand at INSERT time (when a new tokenizer version is first used). The version string is stored in `provider_usage_json` for audit. When implementing `tokenizer_id_to_version`, the row may or may not exist depending on whether the tokenizer was used in a request that reached storage.
- **DB-backed test vector (L2):** When `tokenizer_id_to_version` DB implementation is complete, add test: given tokenizer_id bytes `e3c8e8ff724411c6416dd4fb135368e3`, `SELECT version FROM tokenizers WHERE tokenizer_id = ?` returns `Ok(Some("tiktoken-cl100k_base-v1.2.3"))`

## Reference

- RFC-0909 §tokenizer_version_to_id
- RFC-0909 §tokenizer_id_to_version
- RFC-0909 §Truncation Note
- RFC-0910 §Tokenizer Database Schema (tokenizers table — REQUIRED for tokenizer_id_to_version DB lookup)
- RFC-0903-B1 §tokenizer_id (BLAKE3-16 derivation)

## Complexity

Low — BLAKE3 hashing + optional DB query

---
**Mission Type:** Implementation
**Priority:** High
**Phase:** RFC-0909 Phase 1 Core

## Changelog

| Version | Date | Changes |
|---------|------|---------|
| v2 | 2026-04-20 | Round 1 adversarial review fixes: fix C1 (add RFC-0910 §Tokenizer Database Schema to references); fix C2 (add blake3 crate dependency); fix H1 (add full 32-byte BLAKE3 hash for test vector verification); fix H2 (clarify DB-level error propagation for stub); fix M1 (note #[inline] on tokenizer_id_to_version not in RFC but acceptable); fix M2 (document on-demand tokenizer table population); fix L1 (add collision probability note); fix L2 (add DB-backed test vector for tokenizer_id_to_version) |
