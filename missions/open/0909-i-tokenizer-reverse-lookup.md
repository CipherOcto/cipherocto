# Mission: RFC-0909 tokenizer_id_to_version DB Lookup Implementation

## Status

Open (v2) — BLOCKED by RFC-0910 v15 adoption

## RFC

RFC-0909 v62 (Economics): Deterministic Quota Accounting
RFC-0910 v15 (Economics): Pricing Table Registry

## Summary

Implement the DB-backed version of `tokenizer_id_to_version(id: &[u8; 16]) -> Result<Option<String>, KeyError>` that queries the `tokenizers` table to resolve a tokenizer_id (BLAKE3-16) back to its version string. The current stub always returns an error; the implementation requires a real Stoolap query.

## BLOCKED (2026-04-20)

This mission is BLOCKED pending RFC-0910 v15 adoption. The `tokenizers` table schema required by this mission is defined in RFC-0910 v15 which was recently updated to clarify Phase 1 independence from RFC-0914. Once RFC-0910 reaches Accepted status, this mission can proceed.

**Dependencies:**
- RFC-0910 v15 (or higher): defines `tokenizers` table schema with `tokenizer_id BLOB(16), version TEXT NOT NULL`
- Mission 0909-h: api_keys schema must be finalized (this mission does not depend on it directly but shares the DB schema context)

## Background

RFC-0909 §tokenizer_id_to_version defines this function as requiring a DB lookup:
```rust
pub fn tokenizer_id_to_version(id: &[u8; 16]) -> Result<Option<String>, &'static str> {
    // Current stub (always error):
    Err("tokenizer_id_to_version: requires DB lookup implementation")
}
```

RFC-0910 defines the `tokenizers` table schema:
```sql
CREATE TABLE tokenizers (
    tokenizer_id BLOB(16) NOT NULL,         -- Raw BLAKE3 hash of version string (16 bytes)
    version TEXT NOT NULL,                   -- e.g., "tiktoken-cl100k_base-v1.2.3"
    vocab_size INTEGER,
    encoding_type TEXT,                      -- e.g., "bpe", "sentencepiece"
    PRIMARY KEY (tokenizer_id),
    UNIQUE(version)                          -- Each version maps to exactly one tokenizer_id (per BLAKE3 derivation)
);
```

The implementation must use the raw 16-byte tokenizer_id as the lookup key:
```sql
SELECT version FROM tokenizers WHERE tokenizer_id = ?
```

## Acceptance Criteria

- [ ] `tokenizer_id_to_version(id: &[u8; 16]) -> Result<Option<String>, KeyError>` — DB lookup returns `Ok(Some(version_string))` on match, `Ok(None)` on no match
- [ ] Error handling: DB-level errors (connection failure, etc.) propagate via `Err(KeyError::Storage(...))` — the current `Err(&'static str)` return type should be changed to `KeyError` for consistency with the rest of the codebase
- [ ] Return type updated to `Result<Option<String>, KeyError>` (from `Result<Option<String>, &'static str>`)
- [ ] Unit test: given tokenizer_id bytes `e3c8e8ff724411c6416dd4fb135368e3`, `SELECT version FROM tokenizers WHERE tokenizer_id = ?` returns `Ok(Some("tiktoken-cl100k_base-v1.2.3"))`
- [ ] `tokenizers` table schema matches RFC-0910 (BLOB(16) primary key, UNIQUE on version)
- [ ] On-demand population: when a new tokenizer version is first used in a spend_ledger INSERT, the tokenizers table is upserted if needed

## Implementation Notes

- Location: `crates/quota-router-core/src/keys/mod.rs` (where the stub currently lives)
- The `tokenizers` table is populated on-demand at INSERT time (when a new tokenizer version is first used). The `ensure_tokenizer()` pattern from RFC-0903-B1:
  ```ignore
  fn ensure_tokenizer(db: &Database, version: &str) -> [u8; 16] {
      let tokenizer_id = tokenizer_version_to_id(version);
      db.execute(
          "INSERT OR IGNORE INTO tokenizers (tokenizer_id, version) VALUES (?, ?)",
          [tokenizer_id, version],
      )?;
      tokenizer_id
  }
  ```
- The tokenizer may already exist in the table (from a prior INSERT with `tokenizer_version` stored in `provider_usage_json` for audit). The DB lookup should find it regardless of how it was populated.
- **Important:** `tokenizer_id_to_version` is NOT called during normal spend recording — it is a read-only lookup function for verification/auditing purposes.

## Dependencies

- RFC-0909 Mission 0909-f (tokenizer_version_to_id) — already implemented
- RFC-0910 (tokenizers table schema)
- Stoolap DB access pattern from `storage.rs`

## Reference

- RFC-0909 §tokenizer_id_to_version (full specification)
- RFC-0910 §Tokenizer Database Schema
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
| v2 | 2026-04-20 | Updated RFC references: RFC-0909 v62, RFC-0910 v15; added BLOCKED status; updated RFC-0903-C1 reference |
| v1 | 2026-04-20 | Initial draft |
