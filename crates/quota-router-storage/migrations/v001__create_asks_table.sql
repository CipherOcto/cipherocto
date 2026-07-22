-- Migration v001: Create `asks` table for per-node Ask pricing (RFC-0959 v1.0).
--
-- cipherocto-side migration per [[stoolap-general-purpose-db]] principle.
-- The stoolap fork stays a general-purpose DB; consumer schema lives here.

CREATE TABLE IF NOT EXISTS asks (
    -- Synthetic row ID (stoolap requires INTEGER PRIMARY KEY).
    row_id            INTEGER PRIMARY KEY,

    -- BLAKE3 32-byte content hash (AskId). UNIQUE for content-addressable lookup.
    -- Cannot be PRIMARY KEY directly (stoolap PRIMARY KEY must be INTEGER).
    ask_id            BLOB NOT NULL UNIQUE,

    -- Publisher DID (e.g., "did:octo:asker1")
    asker_did         TEXT NOT NULL,

    -- Model reference (e.g., "openai/gpt-4")
    model             TEXT NOT NULL,

    -- Per-model rate table (ModelRateTable serialized as JSON)
    rates_json        BLOB NOT NULL,

    -- Nonce for content-addressable AskId uniqueness
    nonce             BLOB NOT NULL,

    -- Expiry timestamp (Unix seconds; inclusive)
    expires_at_unix   INTEGER NOT NULL,

    -- Creation timestamp (Unix seconds)
    created_at_unix   INTEGER NOT NULL
);