# Mission: RFC-0909 tokenizer_id_to_version DB Lookup Implementation

## Status

Completed (v4)

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

## Complexity

Low — single DB query + optional upsert

---

**Mission Type:** Implementation
**Priority:** Medium
**Phase:** RFC-0909 Phase 1 Core (follow-on from 0909-f)

## Changelog

| Version | Date | Changes |
|---------|------|---------|
| v4 | 2026-04-21 | BLUEPRINT.md compliance fixes: I-B1 (Status → Completed v4), I-B2 (add Claimant), I-B3 (add Pull Request), I-B4 (add Notes section), I-B5 (fix Dependencies), I-B6 (rename Reference → Notes), I-B7 (clarify stub scope), I-B8 (changelog detail), I-B9 (add Key Files to Modify) |
| v3 | 2026-04-21 | IMPLEMENTED: tokenizers table added to schema.rs; resolve_tokenizer + ensure_tokenizer implemented on KeyStorage trait; 4 new unit tests; clippy clean (0 warnings); 128 tests pass |
| v2 | 2026-04-20 | Updated RFC references: RFC-0909 v62, RFC-0910 v15; added BLOCKED status; updated RFC-0903-C1 reference |
| v1 | 2026-04-20 | Initial draft |