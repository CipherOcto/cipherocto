-- Consumed envelope index (RFC-0962 §6.3).
--
-- Composite PK (signer_did, nonce) prevents replay across distinct signers
-- reusing the same nonce. Distinct from RFC-0959's ConsumedReceiptIndex
-- (which tracks ReceiptId per asker).
--
-- Stoolap limitation: PRIMARY KEY must be INTEGER. We use rowid +
-- UNIQUE (signer_did, nonce) for the RFC-0962 §6.3 composite key.

CREATE TABLE IF NOT EXISTS consumed_envelopes (
    rowid           INTEGER PRIMARY KEY AUTOINCREMENT,
    signer_did      BLOB    NOT NULL,
    nonce           BLOB    NOT NULL,
    envelope_id     BLOB    NOT NULL,
    seen_at_unix_ms BIGINT  NOT NULL,
    UNIQUE (signer_did, nonce)
);

CREATE INDEX IF NOT EXISTS ix_consumed_envelopes_envelope
    ON consumed_envelopes (envelope_id);