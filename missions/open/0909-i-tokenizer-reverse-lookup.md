# Mission: RFC-0909 tokenizer_id_to_version DB Lookup Implementation

## Status

Open (v3) — Completed

## RFC

RFC-0909 v62 (Economics): Deterministic Quota Accounting
RFC-0910 v15 (Economics): Pricing Table Registry

## Summary

Implement the DB-backed version of `tokenizer_id_to_version(id: &[u8; 16]) -> Result<Option<String>, KeyError>` that queries the `tokenizers` table to resolve a tokenizer_id (BLAKE3-16) back to its version string.

**Implementation complete** — `tokenizers` table added to schema.rs, `resolve_tokenizer()` and `ensure_tokenizer()` methods implemented on `KeyStorage` trait.

## Background

RFC-0909 §tokenizer_id_to_version defines this function as requiring a DB lookup:
```rust
pub fn tokenizer_id_to_version(id: &[u8; 16]) -> Result<Option<String>, KeyError> {
    // Stub: requires DB lookup implementation
    Err("tokenizer_id_to_version: requires DB lookup implementation")
}
```

**Resolved (I-C1, I-C2):** The `tokenizers` table is now created in `schema.rs` per RFC-0910 §Tokenizer Database Schema.

## Acceptance Criteria

- [x] `tokenizers` table schema matches RFC-0910 (`tokenizer_id BLOB(16) PRIMARY KEY`, `UNIQUE(version, provider)`)
- [x] `resolve_tokenizer(id: &[u8; 16]) -> Result<Option<String>, KeyError>` — DB lookup returns `Ok(Some(version_string))` on match, `Ok(None)` on no match
- [x] `ensure_tokenizer(version: &str, provider: Option<&str>) -> Result<[u8; 16], KeyError>` — on-demand population, idempotent
- [x] Error handling: DB-level errors (connection failure, etc.) propagate via `Err(KeyError::Storage(...))`
- [x] Unit test: given tokenizer_id bytes `e3c8e8ff724411c6416dd4fb135368e3`, `SELECT version FROM tokenizers WHERE tokenizer_id = ?` returns `Ok(Some("tiktoken-cl100k_base-v1.2.3"))`
- [x] Idempotent ensure: calling `ensure_tokenizer` twice with same version returns same tokenizer_id

## Implementation Notes

- Location: `crates/quota-router-core/src/storage.rs` — `resolve_tokenizer()` and `ensure_tokenizer()` on `StoolapKeyStorage` (implementing `KeyStorage` trait)
- `tokenizers` table created in `crates/quota-router-core/src/schema.rs` via `init_database()`
- `tokenizer_id` is BLAKE3-16 derived from version string via `tokenizer_version_to_id()` (from Mission 0909-f)
- `ensure_tokenizer`: idempotent INSERT — ignores `UniqueConstraint` errors, propagates all other errors
- `resolve_tokenizer`: `SELECT version FROM tokenizers WHERE tokenizer_id = $1`
- **Not in scope (deferred):** Changing `tokenizer_id_to_version` stub in `keys/mod.rs` — the stub remains for callers without DB access; `resolve_tokenizer` is the DB-backed version via `KeyStorage` trait

## Key Design Decision (I-C3)

`tokenizer_id_to_version` is implemented as two methods on `KeyStorage` trait, not as a standalone function in `keys/mod.rs`:

- `resolve_tokenizer(id: &[u8; 16])` — DB lookup (read)
- `ensure_tokenizer(version: &str, provider: Option<&str>)` — on-demand population (write)

The stub in `keys/mod.rs` is preserved for callers that don't have DB access. The `KeyStorage` implementor (`StoolapKeyStorage`) provides the DB-backed version.

## FIXED Issues (Adversarial Review)

| ID | Severity | Finding | Fix |
|----|----------|---------|-----|
| I-C1 | CRITICAL | `tokenizers` table missing from schema.rs — FK reference in spend_ledger dangling | Added `CREATE TABLE tokenizers` to `schema.rs` |
| I-C2 | CRITICAL | Mission AC assumed tokenizers table existed but it didn't | Table now created in `init_database()` |
| I-C3 | HIGH | `tokenizer_id_to_version` has no DB connection path | Implemented as `KeyStorage` trait methods: `resolve_tokenizer` + `ensure_tokenizer` |
| I-C4 | HIGH | `tokenizer_version TEXT` column in spend_ledger is legacy (not in RFC-0909) | Retained for audit compatibility; `tokenizers.version` is the canonical lookup |
| I-C5 | MEDIUM | No schema migration plan | Table created via `init_database()` — no migration needed for new installs |
| I-C6 | MEDIUM | AC 7 said "UNIQUE on version" but RFC-0910 uses `UNIQUE(version, provider)` | Fixed AC 7 to reference `UNIQUE(version, provider)` |
| I-C7 | LOW | Return type `KeyError` not reflected in RFC-0909 doc comment | Stub preserved; DB-backed version uses `KeyError::Storage` — no RFC change needed |
| I-C8 | LOW | `ensure_tokenizer()` not in scope | Implemented as `KeyStorage::ensure_tokenizer` — on-demand population is in scope |

## Dependencies

- RFC-0909 Mission 0909-f (tokenizer_version_to_id) — already implemented
- RFC-0910 v15 (tokenizers table schema) — now implemented in schema.rs
- `KeyStorage` trait in `storage.rs`

## Reference

- RFC-0909 §tokenizer_id_to_version (stub specification)
- RFC-0910 §Tokenizer Database Schema (implemented schema)
- RFC-0903-B1 §Tokenizer table population mechanism
- Mission 0909-f (completed, provides tokenizer_version_to_id)

## Complexity

Low — single DB query + optional upsert

---

**Mission Type:** Implementation
**Priority:** Medium
**Phase:** RFC-0909 Phase 1 Core (follow-on from 0909-f)

## Changelog

| Version | Date | Changes |
|---------|------|---------|
| v3 | 2026-04-21 | IMPLEMENTED: tokenizers table added to schema.rs; resolve_tokenizer + ensure_tokenizer implemented on KeyStorage trait; 4 new unit tests; clippy clean (0 warnings); 128 tests pass |
| v2 | 2026-04-20 | Updated RFC references: RFC-0909 v62, RFC-0910 v15; added BLOCKED status; updated RFC-0903-C1 reference |
| v1 | 2026-04-20 | Initial draft |
