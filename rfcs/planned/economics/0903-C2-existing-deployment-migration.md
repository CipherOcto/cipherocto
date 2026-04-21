# RFC-0903-C2 (Economics): Existing Deployment Migration — api_keys/teams TEXT→BLOB(16)

## Status

Planned

## Authors

- Author: @mmacedoeu

## Summary

Migration procedure for existing deployments of RFC-0903 Final running TEXT-based `api_keys.key_id`, `api_keys.team_id`, and `teams.team_id` schemas. Amends these columns to `BLOB(16)` per RFC-0903-C1. Also adds the `failed_attempts`, `last_failed_at`, and `locked` columns identified as missing in the H3 finding (Round 4 adversarial review).

## Why Needed

RFC-0903-C1 Deployment Scope explicitly limits to greenfield: "This amendment applies to greenfield deployments only." Existing deployments (those already running RFC-0903 Final TEXT schema) require a separate migration procedure defined in this RFC.

## Scope

1. Shadow-column migration for `api_keys` and `teams` tables (hot tables — high-frequency reads/writes)
2. Migration must handle concurrent writes during population phase
3. Add `failed_attempts`, `last_failed_at`, `locked` columns to `api_keys`
4. Validation that post-migration FK relationships are type-consistent (BLOB(16) → BLOB(16))
5. Rollback procedure if migration fails mid-phase

## Dependencies

**Requires:**
- RFC-0903 Final v30 (base schema)
- RFC-0903-B1 (spend_ledger BLOB migration — must complete first)
- RFC-0903-C1 (api_keys/teams BLOB target schema)

**Required By:**
- RFC-0909: Deterministic Quota Accounting (needs consistent BLOB types across all tables)

## Key Decisions Needed

- [ ] Dual-write vs write-quiesce strategy for hot-table migration
- [ ] How to handle `failed_attempts`/`locked` during migration (default values?)
- [ ] PostgreSQL/MySQL vs SQLite specific syntax differences
- [ ] Index recreation after column swap