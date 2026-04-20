# Mission: RFC-0909 api_keys + teams BLOB Schema Migration

## Status

Open

## RFC

RFC-0909 v59 (Economics): Deterministic Quota Accounting

## Summary

Migrate `api_keys` and `teams` tables to BLOB storage for UUID primary/foreign keys per RFC-0903-C1. This is required for FK consistency: `spend_ledger.key_id` (BLOB(16)) references `api_keys.key_id` (must also be BLOB(16)).

## Acceptance Criteria

- [ ] `api_keys.key_id`: TEXT (UUID string) → BLOB(16) (raw UUID bytes, 16 bytes)
- [ ] `api_keys.team_id`: TEXT (UUID string, nullable) → BLOB(16) (raw UUID bytes, nullable)
- [ ] `teams.team_id`: TEXT (UUID string) → BLOB(16) (raw UUID bytes)
- [ ] Update `idx_api_keys_team_id` index on BLOB(16) column
- [ ] Update `idx_teams_team_id` index on BLOB(16) column
- [ ] Storage boundary helpers: use `uuid_to_blob_16()` / `blob_16_to_uuid()` for all UUID columns
- [ ] All existing tests pass after migration
- [ ] FK chain consistent: `spend_ledger.key_id` (BLOB(16)) → `api_keys.key_id` (BLOB(16)) ✓

## Implementation Notes

- Location: `crates/quota-router-core/src/schema.rs`
- Migration: shadow column pattern (same as Mission 0909-g)
- `teams.team_id` is NOT NULL → always use `uuid_to_blob_16()` (non-nullable)
- `api_keys.team_id` is nullable → use `Option<uuid::Uuid>` → `Option<Vec<u8>>` at storage boundary
- `api_keys.key_id` is NOT NULL → always use `uuid_to_blob_16()` (non-nullable)
- After migration: `row.get::<_, Vec<u8>>("key_id")` → convert to `uuid::Uuid::from_bytes`
- `lookup_by_hash()` in `storage.rs` uses `key_hash` (BYTEA, unchanged) — not affected by this migration
- `create_key()` in `storage.rs` MUST be updated to use `uuid_to_blob_16()` for key_id
- `get_team()` and `create_team()` in `storage.rs` MUST be updated to use BLOB helpers for team_id

## Reference

- RFC-0903-C1 §Schema Amendments (api_keys.key_id, api_keys.team_id, teams.team_id → BLOB(16))
- RFC-0909 §Relationship to RFC-0903 (FK consistency requirement)
- RFC-0909 §spend_ledger FK constraints (key_id → api_keys.key_id, team_id → teams.team_id)

## Complexity

High — requires migration of hot tables (high-frequency reads/writes) and all affected storage functions

---
**Mission Type:** Implementation
**Priority:** Critical
**Phase:** RFC-0909 Phase 1 Core
**Blocked By:** Mission 0909-g (spend_ledger BLOB migration should happen first or concurrently — FK consistency requires all three tables to use BLOB(16) simultaneously)
