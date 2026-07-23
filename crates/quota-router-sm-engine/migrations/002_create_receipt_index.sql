-- Consumed receipt index (RFC-0959 v1.0 §Algorithms `ConsumedReceiptIndex`).
--
-- Defense against in-flight proof replay: STARK proofs are not
-- nonce-deduped at ZK verify time; dedup happens at settlement time
-- via this unique index on receipt_id.
--
-- Note: stoolap requires PRIMARY KEY to be INTEGER. We use receipt_id (BLOB)
-- as a UNIQUE constraint; lookups use idx_consumed_receipt_id.
CREATE TABLE IF NOT EXISTS consumed_receipt_index (
    receipt_id BLOB NOT NULL UNIQUE,
    ask_id BLOB NOT NULL,
    consumed_at INTEGER NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_consumed_receipt_id ON consumed_receipt_index(receipt_id);
CREATE INDEX IF NOT EXISTS idx_consumed_receipt_ask_id ON consumed_receipt_index(ask_id);
