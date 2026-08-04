-- Migration v003: Create `consumed_receipt_index` table for replay defense
-- (RFC-0959 §Replay Defense; mission 0959-a §Anti-Fraud).
--
-- cipherocto-side migration per [[stoolap-general-purpose-db]] principle.
-- The stoolap fork stays a general-purpose DB; consumer schema lives here.
--
-- Replaces the in-memory `ConsumedReceiptIndex` (HashMap<[u8;32], ()>) used
-- by the CLI `settle-replay` + `SettlementEnvelope::verify` paths. With
-- this table, replay-defense state survives process restarts and is shared
-- across CLI invocations + mesh router nodes.
--
-- Indexes:
-- - idx_cri_nonce: PRIMARY lookup path (verify hits nonce first)
-- - idx_cri_asker: per-asker dashboard query (RFC-0959 §Monitoring)
-- - idx_cri_consumed_at: GC sweep over old entries (TTL on the table
--   itself is a follow-up; the index supports partitioned GC)

CREATE TABLE IF NOT EXISTS consumed_receipt_index (
    -- Synthetic row ID (stoolap requires INTEGER PRIMARY KEY).
    row_id            INTEGER PRIMARY KEY,

    -- BLAKE3 32-byte settlement_hash (canonical envelope hash).
    -- UNIQUE catches duplicate settlement attempts at the schema layer.
    settlement_hash   BLOB NOT NULL UNIQUE,

    -- Replay-defense nonce (32 bytes per SettlementEnvelope).
    -- UNIQUE catches replay attempts at the schema layer.
    nonce             BLOB NOT NULL UNIQUE,

    -- AskId (BLAKE3 canonical_ser payload). For audit / forensic queries.
    ask_id            BLOB NOT NULL,

    -- Asker DID (RFC-0009). For per-asker dashboard + GC fairness.
    asker_did         TEXT NOT NULL,

    -- Unix timestamp at which the nonce was inserted (seconds).
    -- Used by future GC / TTL sweep via idx_cri_consumed_at.
    consumed_at_unix  INTEGER NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_cri_nonce       ON consumed_receipt_index(nonce);
CREATE INDEX IF NOT EXISTS idx_cri_asker       ON consumed_receipt_index(asker_did);
CREATE INDEX IF NOT EXISTS idx_cri_consumed_at ON consumed_receipt_index(consumed_at_unix);
