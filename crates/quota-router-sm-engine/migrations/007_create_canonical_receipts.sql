-- Canonical receipt chain (RFC-0014 §Module Layout `sink`).
--
-- Append-only persistence path for the substrate-canonical Receipt
-- (RFC-0014 substrate: `receipt_id: u64`, distinct from the domain
-- Receipt's [u8;32] identifier at settlement consume-time).
-- Separate from `consumed_receipt_index` (002), which is the domain
-- replay-defense index; `canonical_receipts` is the canonical chain
-- used by `verify_receipt_chain`.
--
-- `receipt_id INTEGER PRIMARY KEY` provides the atomicity gate:
-- duplicate appends collide on the PK constraint and are surfaced as
-- `SettlementError::AlreadyExists` per RFC-0014 §Trait G3.
--
-- Stoolap requires PRIMARY KEY to be INTEGER. `receipt_id: u64` fits
-- that constraint natively.
CREATE TABLE IF NOT EXISTS canonical_receipts (
    receipt_id     INTEGER PRIMARY KEY,
    ask_id         BLOB    NOT NULL,
    settlement_hash BLOB   NOT NULL,
    router_id      TEXT    NOT NULL,
    router_sig     BLOB    NOT NULL,
    timestamp_unix INTEGER NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_canonical_receipts_ask_id ON canonical_receipts(ask_id);
