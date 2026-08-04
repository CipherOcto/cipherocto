-- Migration v004: Create `settlement_events` table for event sourcing
-- (RFC-0959 §Implementation Phases; mission 0959-a S8).
--
-- cipherocto-side migration per [[stoolap-general-purpose-db]] principle.
-- The stoolap fork stays a general-purpose DB; consumer schema lives here.
--
-- Append-only event log of `SettlementEvent` records (RFC-0959 §Data
-- Structures). Each row is a router-signed attestation of a settlement:
-- 1. Router computes `settlement_hash = BLAKE3(DOMAIN || cap_root_hash ||
--    ask_id || invocation_hash || canonical_ser(axes_consumed))` from the
--    canonical preimage.
-- 2. Router signs the canonical event bytes with its Ed25519 identity key.
-- 3. Row is inserted into `settlement_events`; the settlement_hash is
--    UNIQUE so re-inserting the same event is idempotent (returns Ok(false)
--    via the DAO translation).
-- 4. The corresponding nonce is ALSO inserted into `consumed_receipt_index`
--    (v003) — replay defense + audit linkage.
--
-- This table is the canonical source for:
-- - Per-ask audit (idx_se_ask_id)
-- - Per-asker dashboard (idx_se_asker_did)
-- - Time-range queries (idx_se_settled_at)
-- - Forensic re-derivation of settlement_hash from canonical fields
--
-- Cost is stored as 16-byte big-endian (`BLOB`) since `u128` doesn't fit
-- in i64. Matches the canonical wire encoding in `SettlementEnvelope`.

CREATE TABLE IF NOT EXISTS settlement_events (
    -- Synthetic row ID (stoolap requires INTEGER PRIMARY KEY).
    row_id              INTEGER PRIMARY KEY,

    -- Canonical settlement_hash (BLAKE3 32 bytes). UNIQUE catches
    -- duplicate event inserts at the schema layer.
    settlement_hash     BLOB NOT NULL UNIQUE,

    -- BLAKE3 capability root (RFC-0957 §Algorithms cap_root_hash).
    cap_root_hash       BLOB NOT NULL,

    -- Content-addressable AskId (BLAKE3 canonical_ser payload).
    ask_id              BLOB NOT NULL,

    -- Asker DID (RFC-0009). Denormalized here so per-asker queries
    -- (mesh router dashboard, audit) don't need a JOIN against the
    -- `asks` table. Cost is +32 bytes per row; worth it for query speed.
    asker_did           TEXT NOT NULL,

    -- BLAKE3 of the invocation (provider-side; protocol boundary).
    invocation_hash     BLOB NOT NULL,

    -- Per-axis consumption (AxesConsumed serialized as JSON).
    -- BLOB (canonical JSON bytes) preserves BTreeMap ordering — serde_json
    -- deterministic encoding is the canonical form.
    axes_consumed_json  BLOB NOT NULL,

    -- Cost in micro-OCTO-W (16-byte big-endian u128).
    cost_micro_octo_w   BLOB NOT NULL,

    -- Settlement timestamp (Unix seconds).
    settled_at_unix     INTEGER NOT NULL,

    -- Router Ed25519 signature (64 bytes) over canonical_ser(event).
    -- NOT NULL for production (every settlement has a router signature).
    router_signature    BLOB NOT NULL,

    -- Replay-defense nonce (16 bytes; matches consumed_receipt_index.nonce).
    -- Useful for cross-table joins: settlement_events ⨝ consumed_receipt_index.
    nonce               BLOB NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_se_ask_id      ON settlement_events(ask_id);
CREATE INDEX IF NOT EXISTS idx_se_asker_did    ON settlement_events(cap_root_hash);
CREATE INDEX IF NOT EXISTS idx_se_settled_at  ON settlement_events(settled_at_unix);
