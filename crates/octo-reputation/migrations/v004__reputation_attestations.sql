-- v004 — reputation_attestations + reputation_attestors (RFC-0968 §12 +
-- amendments 22, 28, 29).
--
-- Attestors are replication peers that sign `Attestation` records
-- indicating they have observed a `SignalEvent` gossiped from another
-- node. The attestor's signature is non-authoritative transport metadata
-- (amendment 28) — only the recorder's signature carries authority.
--
-- The attestor registry holds the per-attestor pubkey + peer-set id used
-- for signature verification at gossip ingress.
--
-- Blob columns store:
--   attestor_did        BLOB(52)  — AttestorId (CID-style, same as RecorderDid)
--   recorder_did        BLOB(52)  — RecorderDid of the original recorder
--   event_id            BLOB(8)   — EventId (u64 big-endian)
--   signature           BLOB      — ed25519 signature from the attestor
--   attestor_pubkey     BLOB(32)  — ed25519 public key of the attestor
--   peer_set_id         BLOB(32)  — libp2p peer-set identifier

CREATE TABLE reputation_attestors (
    attestor_did         BLOB NOT NULL,
    attestor_pubkey      BLOB NOT NULL,
    peer_set_id          BLOB NOT NULL,
    requested_at_unix    INTEGER NOT NULL,
    registered_at_unix   INTEGER NOT NULL,
    PRIMARY KEY (attestor_did)
);

CREATE TABLE reputation_attestations (
    attestation_id       INTEGER PRIMARY KEY,
    attestor_did         BLOB NOT NULL,
    recorder_did         BLOB NOT NULL,
    event_id             BLOB NOT NULL,
    signature            BLOB NOT NULL,
    observed_at_unix     INTEGER NOT NULL,
    received_at_unix     INTEGER NOT NULL,
    source_mission       TEXT NOT NULL,
    source_domain        TEXT NOT NULL
);

CREATE INDEX reputation_attestations_event
    ON reputation_attestations (event_id);

CREATE INDEX reputation_attestations_attestor
    ON reputation_attestations (attestor_did);

CREATE INDEX reputation_attestations_recorder
    ON reputation_attestations (recorder_did, observed_at_unix);
