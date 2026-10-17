-- v014__create_transfer_events.sql (plan §B.3 / stream B.3; review §20.3.5 Model B)
--
-- Append-only event log; balances are event-projected, NOT stored as
-- mutable state (per RFC-0960 grand design §11).
--
-- Naming convention: bare table names (see v013 header).
--
-- Stoolap fork parser constraint: NO inline `--` comments mid-statement.
--
-- §20.3 line 1202 mandates the secondary index on (chain_id, occurred_at_unix)
-- for time-range audit queries; without it, full table scan at mainnet scale.

CREATE TABLE IF NOT EXISTS transfer_events (
    event_id         BLOB(32) NOT NULL,
    tx_id            BLOB     NOT NULL,
    chain_id         BLOB(32) NOT NULL,
    schema_version   INT      NOT NULL,
    visibility       TEXT     NOT NULL,
    occurred_at_unix BIGINT   NOT NULL,
    attributes       BLOB     NOT NULL,
    corrections      BLOB,
    signature        BLOB(64) NOT NULL,
    zk_proof         BLOB,
    from_vault_id    BLOB(32) NOT NULL,
    to_vault_id      BLOB(32) NOT NULL,
    amount           DQA(12)  NOT NULL,
    capability_id    BLOB(32),
    reason           TEXT,
    canonical_hash   BLOB(32) NOT NULL,
    settlement_ref   BLOB,
    PRIMARY KEY (chain_id, event_id)
);

CREATE INDEX IF NOT EXISTS idx_xfer_chain
    ON transfer_events(chain_id, occurred_at_unix);
