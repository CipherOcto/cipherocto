# Mission: RFC-0909 tokenizer_id_to_version DB Lookup Implementation

## Status

Completed (v6)

## RFC

RFC-0909 v62 (Economics): Deterministic Quota Accounting
RFC-0910 v15 (Economics): Pricing Table Registry

## Dependencies

- Mission 0909-f: tokenizer_version_to_id (completed)

## Acceptance Criteria

- [x] `tokenizers` table schema matches RFC-0910 (`tokenizer_id BLOB(16) PRIMARY KEY`, `UNIQUE(version, provider)`)
- [x] `resolve_tokenizer(id: &[u8; 16]) -> Result<Option<String>, KeyError>` — DB lookup returns `Ok(Some(version_string))` on match, `Ok(None)` on no match
- [x] `ensure_tokenizer(version: &str, provider: Option<&str>) -> Result<[u8; 16], KeyError>` — on-demand population, idempotent
- [x] Error handling: DB-level errors (connection failure, etc.) propagate via `Err(KeyError::Storage(...))`
- [x] Unit test: given tokenizer_id bytes `e3c8e8ff724411c6416dd4fb135368e3`, `SELECT version FROM tokenizers WHERE tokenizer_id = ?` returns `Ok(Some("tiktoken-cl100k_base-v1.2.3"))`
- [x] Idempotent ensure: calling `ensure_tokenizer` twice with same version returns same tokenizer_id

## Claimant

@mmacedoeu

## Pull Request

# (Direct commit to next per trunk-based workflow)

## Notes

### Implementation Scope

This mission implements `tokenizer_id_to_version` as two methods on the `KeyStorage` trait:

- `resolve_tokenizer(id: &[u8; 16])` — DB lookup (read), returns `Ok(Some(version))` or `Ok(None)`
- `ensure_tokenizer(version: &str, provider: Option<&str>)` — on-demand population (write), idempotent

The stub in `crates/quota-router-core/src/keys/mod.rs::tokenizer_id_to_version` is preserved for callers without DB access. The `KeyStorage` implementor (`StoolapKeyStorage`) provides the DB-backed version.

**Note:** The stub remains a stub (returns error) because `tokenizer_id_to_version` is in `keys/mod.rs` which is a pure-computation module with no DB access. Callers should use `KeyStorage::resolve_tokenizer` instead, which is the DB-backed implementation.

**Stoolap Aggregate Support FIXED (2026-04-21):** stoolap now supports aggregate functions (SUM, COUNT, AVG, MIN, MAX) inside MVCC transactions. The `convert_where_to_storage_expr` helper was fixed to properly resolve query parameters instead of creating an empty ExecutionContext. Tests `test_record_spend_ledger_populates_tokenizers` and `test_record_spend_ledger_provider_usage` are now re-enabled and passing.

### Schema

`tokenizers` table created in `crates/quota-router-core/src/schema.rs` via `init_database()`:
```sql
CREATE TABLE tokenizers (
    tokenizer_id BLOB(16) NOT NULL,
    version TEXT NOT NULL,
    vocab_size INTEGER,
    encoding_type TEXT,
    provider TEXT,
    PRIMARY KEY (tokenizer_id),
    UNIQUE(version, provider)
)
```

### BLUEPRINT.md Compliance Fixes (v4)

This version fixes all BLUEPRINT.md template violations from v3:
- I-B1: Status changed from `Open (v3) — Completed` to `Completed (v4)` per template values
- I-B2: Added `## Claimant` field
- I-B3: Added `## Pull Request` field
- I-B4: Added `## Notes` section (consolidated Background, Implementation Notes, Key Design Decision into Notes)
- I-B5: Dependencies reformatted to reference Mission 0909-f by mission name, not RFC
- I-B6: Renamed `## Reference` to `## Notes` per Blueprint template
- I-B7: Clarified stub scope in Notes — stub preserved for callers without DB access
- I-B8: Changelog v3 entry now includes I-C resolution detail
- I-B9: Added `## Key Files to Modify` table per Blueprint RFC template

## Key Files to Modify

| File | Change |
|------|--------|
| `crates/quota-router-core/src/schema.rs` | Added `CREATE TABLE tokenizers` with BLOB(16) PK, UNIQUE(version, provider) |
| `crates/quota-router-core/src/storage.rs` | Added `resolve_tokenizer()` and `ensure_tokenizer()` to `KeyStorage` trait and `StoolapKeyStorage` impl |
| `missions/open/0909-i-tokenizer-reverse-lookup.md` | Mission documentation (this file) |

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

## Additional Fixes (Round 4)

| ID | Severity | Finding | Fix |
|----|----------|---------|-----|
| II-D2 | HIGH | `ensure_tokenizer` not wired into `record_spend_ledger` | Added on-demand population call when token_source is CanonicalTokenizer |
| II-B1 | HIGH | `record_spend_ledger` used TEXT string for key_id query but api_keys.key_id is BLOB(16) | Changed query to use `stoolap::core::Value::blob(key_id_blob.to_vec())` |
| II-T1 | MEDIUM | No integration test for tokenizer auto-population | Disabled direct tests (stoolap tx limitation); verified via middleware test_record_spend |
| II-G1 | MEDIUM | `vocab_size` and `encoding_type` columns not populated | Documented as informational NULL columns — not populated by ensure_tokenizer |

## Complexity

Low — single DB query + optional upsert

---

**Mission Type:** Implementation
**Priority:** Medium
**Phase:** RFC-0909 Phase 1 Core (follow-on from 0909-f)

## Changelog

| Version | Date | Changes |
|---------|------|---------|
| v6 | 2026-04-21 | Stoolap aggregate limitation FIXED — convert_where_to_storage_expr now properly resolves query parameters; re-enabled test_record_spend_ledger_populates_tokenizers and test_record_spend_ledger_provider_usage tests |
| v5 | 2026-04-21 | Round 4 fixes: wire ensure_tokenizer into record_spend_ledger (on-demand CanonicalTokenizer population); fix key_id BLOB query parameter in record_spend_ledger; disable integration tests blocked by stoolap transaction aggregate limitation; add stoopap known-issue note |
| v4 | 2026-04-21 | BLUEPRINT.md compliance fixes: I-B1 (Status → Completed v4), I-B2 (add Claimant), I-B3 (add Pull Request), I-B4 (add Notes section), I-B5 (fix Dependencies), I-B6 (rename Reference → Notes), I-B7 (clarify stub scope), I-B8 (changelog detail), I-B9 (add Key Files to Modify) |
| v2 | 2026-04-20 | Updated RFC references: RFC-0909 v62, RFC-0910 v15; added BLOCKED status; updated RFC-0903-C1 reference |
| v1 | 2026-04-20 | Initial draft |