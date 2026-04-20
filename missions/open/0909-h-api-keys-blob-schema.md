# Mission: RFC-0909 api_keys + teams BLOB Schema Migration

## Status

Open (v4)

## RFC

RFC-0909 v59 (Economics): Deterministic Quota Accounting

## Summary

Migrate `api_keys` and `teams` tables to BLOB storage for UUID primary/foreign keys per RFC-0903-C1. This is required for FK consistency: `spend_ledger.key_id` (BLOB(16)) references `api_keys.key_id` (must also be BLOB(16)).

## Acceptance Criteria

- [ ] `api_keys.key_id`: TEXT (UUID string) → BLOB(16) (raw UUID bytes, 16 bytes). **Note (C1):** RFC-0903-C1 defines `BLOB(16) NOT NULL` with no explicit PRIMARY KEY. Verify PRIMARY KEY constraint status — if required, add `PRIMARY KEY (key_id)` during migration.
- [ ] `api_keys.team_id`: TEXT (UUID string, nullable) → BLOB(16) (raw UUID bytes, nullable)
- [ ] `teams.team_id`: TEXT (UUID string) → BLOB(16) (raw UUID bytes)
- [ ] Recreate `idx_api_keys_team_id` on `team_id` BLOB(16) after column rename (M1)
- [ ] Recreate `idx_teams_team_id` on `team_id` BLOB(16) after column rename (M2)
- [ ] Storage boundary helpers: use `uuid_to_blob_16()` / `blob_16_to_uuid()` for all UUID columns
- [ ] All existing tests pass after migration
- [ ] FK chain consistent: `spend_ledger.key_id` (BLOB(16)) → `api_keys.key_id` (BLOB(16)) ✓
- [ ] All other `api_keys` columns unchanged (C2): `key_hash BYTEA(32)`, `key_prefix TEXT`, `budget_limit BIGINT`, `rpm_limit INTEGER`, `tpm_limit INTEGER`, `created_at INTEGER`, `expires_at INTEGER`, `revoked INTEGER`, `revoked_at INTEGER`, `revoked_by TEXT`, `revocation_reason TEXT`, `key_type TEXT`, `allowed_routes TEXT`, `auto_rotate INTEGER`, `rotation_interval_days INTEGER`, `description TEXT`, `metadata TEXT`
- [ ] All other `teams` columns unchanged (C3): `name TEXT NOT NULL`, `budget_limit BIGINT NOT NULL`, `created_at INTEGER NOT NULL`
- [ ] `idx_api_keys_key_hash_unique` UNIQUE on `key_hash BYTEA(32)` (pre-existing, preserved) (H2)
- [ ] `idx_api_keys_expires` on `expires_at` INTEGER (pre-existing, preserved) (H1)

## Implementation Notes

- Location: `crates/quota-router-core/src/schema.rs`
- Migration: shadow column pattern (same as Mission 0909-g). Follow the 5-step shadow column migration procedure defined in Mission 0909-g §Implementation Notes (H3): (1) Add new BLOB(16) columns with temporary names; (2) Copy data with `uuid_to_blob_16()` for UUID columns; (3) Drop old TEXT columns; (4) Rename new columns to original names; (5) Recreate indexes on new BLOB(16) columns.
- `teams.team_id` is NOT NULL → always use `uuid_to_blob_16()` (non-nullable)
- `api_keys.team_id` is nullable → use `Option<uuid::Uuid>` → `Option<Vec<u8>>` at storage boundary
- `api_keys.key_id` is NOT NULL → always use `uuid_to_blob_16()` (non-nullable)
- After migration: `row.get::<_, Vec<u8>>("key_id")` → `let bytes: [u8; 16] = raw.try_into().expect("key_id must be 16 bytes")` → `blob_16_to_uuid(&bytes)` (from Mission 0909-c). Do NOT call `uuid::Uuid::from_bytes` directly on `Vec<u8>` — the intermediate `try_into::<[u8; 16]>()` step is required.
- `lookup_by_hash()` in `storage.rs` uses `key_hash` (BYTEA, unchanged) — not affected by this migration
- `create_key()` in `storage.rs` MUST be updated to use `uuid_to_blob_16()` for key_id
- `lookup_by_key_id()` in `storage.rs` MUST be updated to use BLOB helpers for key_id (H1)
- `get_team()` and `create_team()` in `storage.rs` MUST be updated to use BLOB helpers for team_id
- **RFC-0903-C1 draft dependency (M3):** RFC-0903-C1 (currently Draft) must reach Accepted status before this migration is considered stable. Monitor RFC-0903-C1 for changes — if the RFC is amended before acceptance, this mission's schema targets may need adjustment.

## Reference

- RFC-0903-C1 §Schema Amendments (api_keys.key_id, api_keys.team_id, teams.team_id → BLOB(16))
- RFC-0909 §Relationship to RFC-0903 (FK consistency requirement)
- RFC-0909 §spend_ledger FK constraints (key_id → api_keys.key_id, team_id → teams.team_id)

## Dependencies

- `uuid = "1.x"` (already required from Mission 0909-c BLOB helpers)

## Complexity

High — requires migration of hot tables (high-frequency reads/writes) and all affected storage functions

---
**Mission Type:** Implementation
**Priority:** Critical
**Phase:** RFC-0909 Phase 1 Core
**Blocked By:** Mission 0909-g (spend_ledger BLOB migration should happen first or concurrently — FK consistency requires all three tables to use BLOB(16) simultaneously)

## Changelog

| Version | Date | Changes |
|---------|------|---------|
| v4 | 2026-04-20 | Round 3 adversarial review fixes: fix L1 (clarify row.get pattern — Vec<u8> requires try_into before blob_16_to_uuid; direct uuid::Uuid::from_bytes on Vec<u8> is a type error) |
| v3 | 2026-04-20 | Round 2 adversarial review fixes: fix H1 (add lookup_by_key_id to storage functions to update); fix M1 (add Dependencies section for consistency) |
| v2 | 2026-04-20 | Round 1 adversarial review fixes: fix C1 (document PRIMARY KEY ambiguity for api_keys.key_id); fix C2 (add all unchanged api_keys columns to AC); fix C3 (add all unchanged teams columns to AC); fix H1 (add pre-existing idx_api_keys_expires); fix H2 (add pre-existing idx_api_keys_key_hash_unique); fix H3 (reference 5-step shadow column procedure from mission 0909-g); fix M1 (clarify idx_api_keys_team_id recreate after rename); fix M2 (clarify idx_teams_team_id recreate after rename); fix M3 (add RFC-0903-C1 draft dependency risk note); add L1 (add changelog) |
