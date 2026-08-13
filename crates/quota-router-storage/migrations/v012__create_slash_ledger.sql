-- Mission marketplace-slashing-persistence: slashing ledger persistence.
--
-- Persists ProviderStake state so banned providers remain banned across
-- process restarts (RFC-0900 §Slashing Model).
--
-- Schema:
--   slash_ledger (provider_id PK, stake_micro_octo_w, initial_stake_micro_octo_w,
--                 offense_count, cumulative_loss_pct_micro, last_updated_unix)
--
-- `cumulative_loss_pct` is encoded as integer micro-percent (1e6) to keep
-- the column Eq-comparable without f64 round-trip ambiguity. A value of
-- 500_000 = 50.0000%, 1_000_000 = 100.0000%.

CREATE TABLE IF NOT EXISTS slash_ledger (
    row_id INTEGER NOT NULL PRIMARY KEY,
    provider_id TEXT NOT NULL UNIQUE,
    stake_micro_octo_w BIGINT NOT NULL,
    initial_stake_micro_octo_w BIGINT NOT NULL,
    offense_count INTEGER NOT NULL,
    cumulative_loss_pct_micro BIGINT NOT NULL,
    last_updated_unix BIGINT NOT NULL
);

CREATE INDEX IF NOT EXISTS slash_ledger_updated_idx
    ON slash_ledger (last_updated_unix);
