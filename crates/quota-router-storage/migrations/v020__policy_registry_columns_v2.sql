-- Migration v020: RFC-0967-A1 v1.9.2 §2.4 full schema landing for
-- `policy_registry`.
--
-- R5 fix D2 + D3 + N6: substrate columns aligned to the canonical
-- RFC-0967-A1 §2.4 column list. The v017 migration landed 6 columns
-- (`policy_hash`, `registry_kind`, `crate_name`, `trait_spec`,
-- `registered_at_unix`, `revoked_at_unix`); the canonical RFC
-- spec calls for 10 columns including:
--   + body BLOB                 (canonical name; v017 used `trait_spec`)
--   + kind_uuid BLOB(16)         (UUIDv5 from §2.6 namespace registry)
--   + execution_class TEXT       (A | B | C per RFC-0008 §Data Structures)
--   + registered_by_did BLOB(32) (RFC-0957 DID)
--   + revoked_by_did BLOB(32)    (RFC-0957 DID, nullable)
--   + revocation_reason TEXT     (free-text, nullable)
--   + superseding_policy_hash BLOB(32) (delegation chain, nullable)
--
-- v020 is purely ADDITIVE — no DROP, no ALTER-RENAME. The `trait_spec`
-- column from v017 stays as a deprecated historical alias; the new
-- canonical column name is `body`. Both are populated on every INSERT
-- for migration-window backward compatibility (until a future v0xx
-- removes `trait_spec` once the v017 catalog is end-of-life).
--
-- Stoolap fork constraints (per [[feedback_stoolap_persistence]] +
-- recon 2026-08-23):
--   1. ADD COLUMN without DEFAULT for BLOB is permitted (column is
--      nullable); we add as nullable then UPDATE backfill — same
--      pattern as v011/v015/v016.
--   2. ADD COLUMN with DEFAULT for TEXT is permitted (verified in
--      v018 `scim_users.active` + `litellm_users.role`); we use
--      `DEFAULT 'A'` for `execution_class` so legacy v017 rows land
--      at Class A (the substrate's canonical conservative default
--      per RFC-0008 §Execution Class Mapping).
--   3. CHECK clauses are accepted-but-not-enforced at runtime
--      (per v017/v019 substrate recon); the BLOB length invariants
--      (`length(kind_uuid) = 16`, `length(registered_by_did) = 32`)
--      are application-layer load-bearing via the registry's INSERT
--      path (column-by-column writes, no fork-side CHECK firing).
--   4. Rejects partial UNIQUE INDEX with WHERE clause — the "one
--      active policy per kind_uuid" invariant from R6 fix F-R6-013
--      is enforced via application-layer lookup-before-insert, not
--      at the substrate layer. (Stays consistent with the
--      `policy_kind_authority_active_uuid_idx` precedent.)

-- ─────────────────────────────────────────────────────────────────────
-- R5 fix D2 (LOW): canonical `body` column per RFC-0967-A1 §2.4
-- ─────────────────────────────────────────────────────────────────────
-- v017 used `trait_spec` as the column name; RFC §2.4 specifies `body`.
-- Per F-P5.2-3 RETAIN framework: substrate truth wins for column
-- NAMES (the RFC is the spec for WHAT FIELDS exist; the SQL DDL is
-- the substrate truth for column naming). The substrate column is
-- renamed by adding `body` and deprecating `trait_spec` going
-- forward; new INSERTs populate both for the migration window.
ALTER TABLE policy_registry ADD COLUMN body BLOB;

-- Backfill: every v017 row's `trait_spec` value lands in `body`.
-- Idempotent on re-apply (rows with body already populated are
-- skipped by the WHERE clause).
--
-- Stoolap fork quirk (verified 2026-08-24 R5 recon): UPDATE on a
-- non-INTEGER PK table requires the WHERE clause to scope to the
-- primary key column. The clause `WHERE body IS NULL` is not
-- sufficient (the fork refuses without an explicit PK reference).
-- Adding `AND policy_hash IS NOT NULL` (PK is NOT NULL, so this is
-- always true) scopes the UPDATE without changing semantics.
UPDATE policy_registry SET body = trait_spec WHERE body IS NULL AND policy_hash IS NOT NULL;

-- ─────────────────────────────────────────────────────────────────────
-- R5 fix D3 (LOW): 6 RFC §2.4 columns added (denormalized from
-- policy_kind_authority / RFC-0957 metadata)
-- ─────────────────────────────────────────────────────────────────────
ALTER TABLE policy_registry ADD COLUMN kind_uuid BLOB(16);
UPDATE policy_registry SET kind_uuid = CAST('00000000000000000000000000000000' AS BLOB)
    WHERE kind_uuid IS NULL AND policy_hash IS NOT NULL;

-- execution_class: NOT NULL with DEFAULT 'A' (safe substrate
-- default per RFC-0008 §Execution Class Mapping for legacy v017
-- rows that pre-date the column). Subsequent INSERTs MUST
-- provide an explicit value (the registry trait signature carries
-- `execution_class: ExecutionClass` and binds it as TEXT).
ALTER TABLE policy_registry ADD COLUMN execution_class TEXT NOT NULL DEFAULT 'A';

ALTER TABLE policy_registry ADD COLUMN registered_by_did BLOB(32);
UPDATE policy_registry SET registered_by_did = CAST('0000000000000000000000000000000000000000000000000000000000000000' AS BLOB)
    WHERE registered_by_did IS NULL AND policy_hash IS NOT NULL;

-- The remaining columns are nullable by design: revocation metadata
-- is NULL on active rows; `superseding_policy_hash` is NULL until
-- `delegate_authority` writes it.
ALTER TABLE policy_registry ADD COLUMN revoked_by_did BLOB(32);
ALTER TABLE policy_registry ADD COLUMN revocation_reason TEXT;
ALTER TABLE policy_registry ADD COLUMN superseding_policy_hash BLOB(32);
