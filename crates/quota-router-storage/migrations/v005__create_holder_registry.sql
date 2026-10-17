-- HolderRegistry schema (RFC-0957-A1 §StoolapHolderRegistry Schema).
-- Cipherocto-side migration per [[stoolap-general-purpose-db]] red line.
-- Stoolap parser does NOT support `UNIQUE ... WHERE`; we rely on NULL
-- semantics: NULL `ask_id` rows are excluded from the UNIQUE constraint,
-- so multiple non-market Bearer/V1 records are allowed; market-bound
-- records (ask_id IS NOT NULL) are uniquely keyed by (ask_id, kind).

CREATE TABLE holder_registry (
    cap_root_hash       BLOB PRIMARY KEY,        -- 32 bytes
    kind                INTEGER NOT NULL,
    holder_did          TEXT NOT NULL,
    holder_pub          BLOB NOT NULL,           -- 32 bytes
    audience_did        TEXT NOT NULL,
    caveats_canonical   BLOB NOT NULL,
    ask_id              BLOB,                    -- 32 bytes nullable
    mint_at_millis_unix INTEGER NOT NULL,
    ttl_millis_unix     INTEGER NOT NULL,
    revoked_at_millis_unix INTEGER                -- nullable; Some = revoked
);

-- UNIQUE composite on (ask_id, kind) — NULL ask_id rows are excluded by
-- Stoolap's NULL semantics, so non-market records are allowed duplicates,
-- market-bound records (ask_id IS NOT NULL) are uniquely keyed.
CREATE UNIQUE INDEX idx_unique_ask_kind ON holder_registry(ask_id, kind);

CREATE INDEX idx_holder_pub ON holder_registry(holder_pub);
