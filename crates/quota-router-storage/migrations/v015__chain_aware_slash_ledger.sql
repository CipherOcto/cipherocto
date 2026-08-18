-- Mission 0900-d: chain-aware slash ledger + DQA(12) bridge.
--
-- Audit verdict 2026-08-17 Risk #2 CRITICAL: slash_ledger substrate
-- diverges from §20.3 Model B chain-aware vault substrate (PK =
-- `(chain_id, owner_did, asset_id)`). v012 PK = `(row_id, provider_id
-- UNIQUE)` carries no chain dimension; same provider_id in two chains
-- silently overwrites stakes.
--
-- v015 promotes slash_ledger to chain-aware:
--   1. ADD COLUMN chain_id BLOB (no DEFAULT — see note below)
--   2. Backfill legacy v012 rows with the default zero namespace
--      (32 bytes of 0x00, RFC-0010 v1.4 ChainId::default)
--   3. DROP provider_id UNIQUE constraint
--   4. CREATE UNIQUE INDEX slash_ledger_chain_provider_idx
--      ON slash_ledger (chain_id, provider_id)
--
-- The same provider_id may carry one row per chain (cross-chain
-- stake partitioning). Mirrors vault v013 PK pattern.
--
-- NOTE on DEFAULT clause: the stoolap fork's parser rejects
-- `x'...'` hex literals in DEFAULT clauses (per recon documented in
-- v011). The column is therefore added WITHOUT a DEFAULT; legacy
-- v012 rows are backfilled via UPDATE to the canonical 32-zero-byte
-- namespace. Combined with the `is_idempotent_already_applied`
-- guard from mission 0871b-storage-idempotent-alter-hardening, the
-- entire v015 migration is retry-safe across mid-apply crashes.
--
-- NOTE on §20.3 + RFC-0960 v3.0 §Vault Substrate (CROSS-REF 0900-d):
-- DQA(12) for amount-bearing columns. Stoolap fork does NOT expose a
-- native Dqa driver (verified 2026-08-18: only `r.get::<i64>()` for
-- amount columns). The substrate invariant per mission 0900-d AC-2
-- sub-clause "fall back to i64 bridge at scale=0 with documented
-- invariant" holds: columns remain BIGINT (i64) at scale=0, encoded
-- via `dqa_to_i64` / `i64_to_dqa` helpers in
-- `crates/quota-router-storage/src/slash_store.rs`. The bridge text
-- form is identical to the canonical `DqaEncoding` 16-byte BE at
-- scale=0 (i64 zero-extended). Stoolap native DQA(12) adoption is
-- deferred to a separate mission tied to the upstream fork Dqa
-- driver.

-- 1. Add chain_id column (NULLABLE during migration window; backfilled
--    in step 2). Stoolap fork does not parse `x'...'` literals in
--    DEFAULT clauses (per v011 recon).
ALTER TABLE slash_ledger ADD COLUMN chain_id BLOB;

-- 2. Backfill legacy v012 rows with the default zero namespace.
--    32 bytes of 0x00 encoded as a 64-char hex string literal; the
--    fork's parser casts string → BLOB via CAST(... AS BLOB).
--    No-op on re-run (chain_id no longer NULL after first apply).
UPDATE slash_ledger
    SET chain_id = CAST('0000000000000000000000000000000000000000000000000000000000000000' AS BLOB)
    WHERE chain_id IS NULL;

-- 3. Drop the unique constraint on provider_id (the singleton PK
--    for the global slash ledger). Stoolap fork requires
--    table-qualified DROP INDEX syntax: `DROP INDEX [IF EXISTS]
--    <idx> ON <table>`. The fork names the column-level UNIQUE
--    autoindex `unique_slash_ledger_provider_id` (NOT the SQLite
--    convention `sqlite_autoindex_slash_ledger_1`). Drop both the
--    autoindex + the conventional autoindex name idempotently.
DROP INDEX IF EXISTS unique_slash_ledger_provider_id ON slash_ledger;
DROP INDEX IF EXISTS slash_ledger_provider_id_unique ON slash_ledger;
DROP INDEX IF EXISTS sqlite_autoindex_slash_ledger_1 ON slash_ledger;

-- 4. Composite UNIQUE INDEX keyed on (chain_id, provider_id).
--    Mirrors the vault v013 PK pattern (`chain_id, owner_did,
--    asset_id`) per §20.3 Model B lattice.
CREATE UNIQUE INDEX IF NOT EXISTS slash_ledger_chain_provider_idx
    ON slash_ledger (chain_id, provider_id);