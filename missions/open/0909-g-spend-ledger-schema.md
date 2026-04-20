# Mission: RFC-0909 spend_ledger BLOB Schema Migration

## Status

Open (v4)

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
  2. Copy data with conversion: `event_id`: TEXT (64-char hex) → BLOB(32) via `hex_to_blob_32()`; `request_id`: TEXT (raw SHA256 binary, NOT hex) → BLOB(32) — type-cast only, no hex decode; `key_id`: TEXT (UUID string) → BLOB(16) via `uuid_to_blob_16()`; `team_id`: TEXT (UUID string) → BLOB(16) via `uuid_to_blob_16()`; `tokenizer_id`: TEXT (hex) → BLOB(16) via `hex::decode()` → `bytes[..16].try_into()` (BLAKE3-16 hex from text storage)
  3. Drop old TEXT columns
  4. Rename new columns to original names
  5. Create indexes on new BLOB columns: `idx_spend_ledger_event_id`, `idx_spend_ledger_key_created`, `idx_spend_ledger_pricing_hash`, `idx_spend_ledger_tokenizer`. Pre-existing `idx_spend_ledger_key_time` index is preserved (not part of this migration).
- Stoolap BLOB storage: use `stoolap::core::Value::blob(bytes)` at INSERT boundary
- Existing `record_spend_ledger()` and `record_spend_ledger_with_team()` in `storage.rs` MUST be updated to use BLOB helpers
- FK constraints for `tokenizer_id` reference the `tokenizers` table (defined in RFC-0910 schema)
- FK constraints for `key_id` reference `api_keys(key_id)` — api_keys.key_id must also be BLOB(16) per RFC-0903-C1 (see Mission 0909-h)
- **Post-migration verification (M3):** After copying data but before dropping old columns, verify: `SELECT hex_to_blob_32(event_id) == original_event_id_string` for a sample. Run existing integration tests only after confirmed data integrity.

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
| v4 | 2026-04-20 | Round 3 adversarial review fixes: fix L1 (add Dependencies section — hex and uuid crates used in migration but not previously listed) |
| v3 | 2026-04-20 | Round 2 adversarial review fixes: fix M1 (clarify AC5 — pre-existing idx_spend_ledger_key_time is preserved, not added) |
| v2 | 2026-04-20 | Round 1 adversarial review fixes: fix C1 (document request_id is raw binary, not hex — no hex conversion needed); fix C2 (add hex_to_blob_32 helper name for event_id conversion); fix H1 (add provider_usage_json TEXT column to AC); fix H2 (add pre-existing idx_spend_ledger_key_time to impl notes); fix M1 (pricing_hash BYTEA(32) not BLOB per RFC); fix M2 (detail step 5 recreate indexes with specific index list); fix M3 (add post-migration data verification step); fix L1 (add timestamp/created_at columns to AC); fix L2 (add provider/model columns to AC) |
