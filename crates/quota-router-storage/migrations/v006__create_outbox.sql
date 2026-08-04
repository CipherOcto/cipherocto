-- Outbox table (RFC-0957-A1 §Outbox).
-- Cipherocto-side migration per [[stoolap-general-purpose-db]] red line.
-- The outbox is in the same transaction as the holder_registry inserts +
-- settlement event append + chain_tip CAS. A crash between commit and
-- gossip leaves the outbox entry durable; the outbox worker (sub-mission
-- 0959-c) replays it on restart.

CREATE TABLE outbox (
    id            INTEGER PRIMARY KEY AUTOINCREMENT,
    buyer_did     TEXT    NOT NULL,
    payload       BLOB    NOT NULL,            -- canonical_ser(MarketDeliveryEnvelope)
    attempts      INTEGER NOT NULL DEFAULT 0,
    created_at_millis_unix INTEGER NOT NULL,
    last_attempt_millis_unix INTEGER,
    flagged_for_intervention INTEGER
);

CREATE INDEX idx_outbox_buyer ON outbox(buyer_did);
