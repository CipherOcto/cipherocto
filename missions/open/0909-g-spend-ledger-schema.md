# Mission: RFC-0909 spend_ledger BLOB Schema Migration

## Status

Open

## RFC

RFC-0909 v59 (Economics): Deterministic Quota Accounting

## Summary

Migrate `spend_ledger` table schema from TEXT storage to BLOB storage per RFC-0903-B1 and RFC-0903-C1 amendments. Also add missing indexes and extend schema with RFC-0909 required indexes.

## Acceptance Criteria

- [ ] `event_id`: TEXT → BLOB(32) (raw SHA256 binary, 32 bytes)
- [ ] `request_id`: TEXT → BLOB(32) (raw SHA256 binary, 32 bytes)
- [ ] `key_id`: TEXT → BLOB(16) (raw UUID binary, 16 bytes) — per RFC-0903-B1
- [ ] `team_id`: TEXT → BLOB(16) (raw UUID binary, 16 bytes) — per RFC-0903-C1
- [ ] `pricing_hash`: BLOB (already BLOB, unchanged)
- [ ] Add missing indexes per RFC-0909:
  - [ ] `idx_spend_ledger_event_id` on event_id
  - [ ] `idx_spend_ledger_key_created` on (key_id, created_at)
  - [ ] `idx_spend_ledger_pricing_hash` on pricing_hash
  - [ ] `idx_spend_ledger_tokenizer` on tokenizer_id (FK to tokenizers table)
- [ ] `tokenizer_id`: TEXT → BLOB(16) (raw BLAKE3 binary, 16 bytes) — per RFC-0903-B1
- [ ] Storage boundary helpers: use `hex_to_blob_32()` / `blob_32_to_hex()` for event_id at INSERT/SELECT
- [ ] Storage boundary helpers: use `uuid_to_blob_16()` / `blob_16_to_uuid()` for key_id at INSERT/SELECT
- [ ] `token_source` CHECK constraint unchanged: `'provider_usage', 'canonical_tokenizer'`
- [ ] `UNIQUE(key_id, request_id)` constraint maintained
- [ ] All existing tests pass after migration

## Implementation Notes

- Location: `crates/quota-router-core/src/schema.rs`
- Migration strategy: shadow column migration (SQLite does not support ALTER COLUMN TYPE)
  1. Add new BLOB columns with temporary names
  2. Copy data with conversion (hex→binary for event_id, UUID string→binary for key_id)
  3. Drop old TEXT columns
  4. Rename new columns to original names
  5. Recreate indexes
- Stoolap BLOB storage: use `stoolap::core::Value::blob(bytes)` at INSERT boundary
- Existing `record_spend_ledger()` and `record_spend_ledger_with_team()` in `storage.rs` MUST be updated to use BLOB helpers
- FK constraints for `tokenizer_id` reference the `tokenizers` table (defined in RFC-0910 schema)
- FK constraints for `key_id` reference `api_keys(key_id)` — api_keys.key_id must also be BLOB(16) per RFC-0903-C1 (see Mission 0909-h)

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
