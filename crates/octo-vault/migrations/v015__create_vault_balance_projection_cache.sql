-- v015__create_vault_balance_projection_cache.sql (Mission A; RFC-0960 §3.1)
--
-- Cached projection table for the VaultBalanceProjection substrate.
-- PK (chain_id, vault_id) per §3.1.
--
-- Per-crate numbering per substrate state — octo-vault/migrations/ has
-- v013+v014; next free is v015. When the centralized migration runner
-- (§3.1) lands, this MUST be renumbered to global v017.
--
-- Stoolap fork parser constraint: NO inline `--` comments mid-statement.

CREATE TABLE IF NOT EXISTS vault_balance_projection_cache (
    chain_id                  BLOB(32) NOT NULL,
    vault_id                  BLOB(32) NOT NULL,
    asset_id                  BLOB(32) NOT NULL,
    projected_balance         DQA(12)  NOT NULL,
    projected_at_unix_seconds BIGINT,
    source_kind               INT      NOT NULL,
    registry_snapshot_epoch   BIGINT   NOT NULL,
    PRIMARY KEY (chain_id, vault_id)
);