-- v002 — reputation_recorders + governance_snapshots + governance_proofs
--         + auditor_nonces (RFC-0968 §3 + §21).
--
-- recorder row shape:
--   recorder_id           u64 PK
--   recorder_did          BLOB(52) UNIQUE
--   controller_id         BLOB(32)
--   octo_stake            u64
--   role_stake            u64
--   role_token_kind       u32
--   chain_id              u32
--   block_height          u64
--   tx_hash               BLOB(32)
--   lock_until_unix       u64
--   suspended             bool
--   slashed               bool
--   created_at_unix       u64
--   updated_at_unix       u64

CREATE TABLE reputation_recorders (
    recorder_id           INTEGER PRIMARY KEY,
    recorder_did          BLOB NOT NULL UNIQUE,
    controller_id         BLOB NOT NULL,
    octo_stake            INTEGER NOT NULL,
    role_stake            INTEGER NOT NULL,
    role_token_kind       INTEGER NOT NULL,
    chain_id              INTEGER NOT NULL,
    block_height          INTEGER NOT NULL,
    tx_hash               BLOB NOT NULL,
    lock_until_unix       INTEGER NOT NULL,
    suspended             INTEGER NOT NULL DEFAULT 0,
    slashed               INTEGER NOT NULL DEFAULT 0,
    created_at_unix       INTEGER NOT NULL,
    updated_at_unix       INTEGER NOT NULL
);

CREATE INDEX reputation_recorders_controller
    ON reputation_recorders (controller_id);

-- (recorder_did is already uniquely indexed via the UNIQUE constraint on
-- that column above; a second non-unique index would error with
-- "cannot create non-unique index: a unique index already exists".)

CREATE TABLE governance_snapshots (
    finalized_at_unix     INTEGER NOT NULL,
    governance_set_hash   BLOB NOT NULL,
    members_blob          BLOB NOT NULL,
    PRIMARY KEY (finalized_at_unix)
);

CREATE TABLE governance_proofs (
    proof_id              INTEGER PRIMARY KEY,
    recorder_id           INTEGER NOT NULL,
    reason_hash           BLOB NOT NULL,
    signature             BLOB NOT NULL,
    snapshot_unix         INTEGER NOT NULL,
    governance_set_hash   BLOB NOT NULL,
    slash_destination     INTEGER,
    slash_amount          INTEGER NOT NULL DEFAULT 0,
    slash_asset           INTEGER NOT NULL DEFAULT 0,
    created_at_unix       INTEGER NOT NULL
);

CREATE INDEX governance_proofs_recorder
    ON governance_proofs (recorder_id);

CREATE INDEX governance_proofs_snapshot
    ON governance_proofs (snapshot_unix);

CREATE TABLE auditor_nonces (
    id                    INTEGER PRIMARY KEY,
    nonce                 BLOB NOT NULL UNIQUE,
    issued_at_unix        INTEGER NOT NULL,
    expires_at_unix       INTEGER NOT NULL
);

CREATE INDEX auditor_nonces_expires
    ON auditor_nonces (expires_at_unix);