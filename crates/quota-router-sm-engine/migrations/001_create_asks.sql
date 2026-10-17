-- Asks table (RFC-0959 §Data Structures; cipherocto-owned per
-- [[stoolap-general-purpose-db]] Path B).
--
-- Schema owner: cipherocto (quota-router-sm-engine crate).
-- Records every ZK-verified capability that progresses through the
-- settlement state machine (Minted to Settled to Consumed).
--
-- Note: stoolap requires PRIMARY KEY to be INTEGER. We use ask_id (BLOB)
-- as a UNIQUE constraint; lookups use idx_asks_ask_id.
CREATE TABLE IF NOT EXISTS asks (
    ask_id BLOB NOT NULL UNIQUE,
    holder_did TEXT NOT NULL,
    axes_consumed BLOB NOT NULL,
    cap_root_hash BLOB NOT NULL,
    invocation_hash BLOB NOT NULL,
    current_unix_time INTEGER NOT NULL,
    output_hash BLOB,
    settlement_hash BLOB NOT NULL,
    state TEXT NOT NULL CHECK(state IN ('Minted', 'Settled', 'Consumed')),
    created_at INTEGER NOT NULL,
    settled_at INTEGER,
    consumed_at INTEGER
);
CREATE INDEX IF NOT EXISTS idx_asks_ask_id ON asks(ask_id);
CREATE INDEX IF NOT EXISTS idx_asks_state ON asks(state);
CREATE INDEX IF NOT EXISTS idx_asks_settlement_hash ON asks(settlement_hash);

