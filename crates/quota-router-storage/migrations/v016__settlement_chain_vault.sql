-- Migration v016: settlement_events chain + vault row binding
-- (RFC-0959 §Wire Format + review §20.7).
--
-- Mission 0959-c1-wire-format-amendment (S6e sub-session per
-- `docs/plans/2026-08-16-storage-layer-restructuring-execution-plan.md`
-- §3 row 6 Stream A.1). Audit hard-recommendation #4 (Risk #5
-- HIGH + Risk #6 MED partial). Closes the parallel-model risk:
-- the same vault-row UNIQUE INDEX lookup pattern serves both
-- capability verify-time (S5 LANDED) AND settlement-time (this
-- mission) via the shared `VaultLookup` trait.
--
-- v016 is purely ADDITIVE — 2 new NULL columns + 1 new UNIQUE INDEX:
--   1. ADD COLUMN cost_vault_id BLOB(32) NULL
--   2. ADD COLUMN chain_id BLOB(32) NULL
--   3. CREATE UNIQUE INDEX idx_se_cost_vault_id ON
--      settlement_events(cost_vault_id)
--
-- Why NULL (not DEFAULT): the stoolap fork's parser rejects
-- `x'...'` hex literals in DEFAULT clauses (per recon documented
-- in v011 + v015). Legacy v004 rows are NOT backfilled — the
-- settlement-time verifier treats NULL cost_vault_id as
-- `SettlementError::CostVaultIdMissing` (per RFC-0959
-- §Cross-Chain Settlement Reject). This is the migration gate:
-- pre-v2.0 settlement_events rows are gated out of the v2.0
-- verify path until a follow-on migration populates them.
--
-- Why cost_micro_octo_w is UNCHANGED: the column already stores
-- 16-byte BE u128 (per v004 comment) which is byte-equivalent to
-- the canonical `DqaEncoding` 16-byte BE wire form at scale=0
-- (per RFC-0960 §Vault Substrate + 0900-d AC-2 sub-clause). The
-- S4 codemod added `dqa_to_i64` / `i64_to_dqa` bridge helpers
-- so the Rust type is `Dqa` while the SQL column stays `BLOB`
-- (Stoolap fork has no native Dqa driver per 0900-d recon).
--
-- UNIQUE INDEX rationale: per §20.7 audit query "show all
-- settlements against vault X" — the index is the lookup key.
-- A NULL cost_vault_id row cannot collide with another NULL
-- (SQLite/stoolap UNIQUE INDEX semantics — multiple NULLs are
-- allowed in a UNIQUE column).

ALTER TABLE settlement_events ADD COLUMN cost_vault_id BLOB(32);
ALTER TABLE settlement_events ADD COLUMN chain_id BLOB(32);

-- Backfill guard: legacy v004 rows have NULL cost_vault_id +
-- NULL chain_id. The settlement-time verifier in
-- `crates/quota-router-storage/src/settlement_verify.rs` rejects
-- these with `SettlementError::CostVaultIdMissing` per RFC-0959
-- v2.0 §Cross-Chain Settlement Reject. No UPDATE backfill
-- required — the v2.0 wire form is forward-only.
--
-- (UPDATE ... SET cost_vault_id = ... WHERE cost_vault_id IS NULL)
-- intentionally OMITTED: there is no canonical derivation of
-- cost_vault_id from a v1.0 SettlementEnvelope (the field did
-- not exist), so backfill is a no-op. Pre-v2.0 rows are
-- recognized as "v1.0 legacy" by the verifier and rejected.

-- UNIQUE INDEX keyed on cost_vault_id for audit query
-- "show all settlements against vault X" (per §20.7).
-- Stoolap fork allows multiple NULLs in a UNIQUE column, so the
-- legacy v004 rows do NOT collide with each other.
CREATE UNIQUE INDEX IF NOT EXISTS idx_se_cost_vault_id
    ON settlement_events(cost_vault_id);

-- Composite index for the v2.0 wire form audit join:
-- "show all settlements for vault X on chain Y" (per §20.7).
-- Cheap additive index — both columns are BLOB(32) so the index
-- size is bounded (64 bytes + row overhead per entry).
CREATE INDEX IF NOT EXISTS idx_se_cost_vault_id_chain_id
    ON settlement_events(cost_vault_id, chain_id);
