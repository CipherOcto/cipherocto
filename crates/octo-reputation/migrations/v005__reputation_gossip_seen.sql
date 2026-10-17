-- v005 — reputation_gossip_seen (RFC-0968 §12, mission 0968 Phase 4)
--
-- Catch-up ledger. When an attestor joins late (or a node restarts), it
-- asks peers for envelopes newer than `since_event_id`. The peer responds
-- by re-publishing the events AND records an entry in this table so the
-- same catch-up request is not repeated on every restart.
--
-- The composite PK (recorder_did, event_id) means the catch-up ledger is
-- idempotent on the (recorder, event) pair — duplicate catch-up requests
-- for the same event from the same recorder never insert a second row.
--
-- Blob columns store:
--   recorder_did    BLOB(52)  — RecorderDid of the recorder whose event
--                              is being tracked in the catch-up ledger
--   event_id        BLOB(8)   — EventId (u64 big-endian)
--   attestor_did    BLOB(52)  — AttestorId of the asker
--   observed_at_unix INTEGER  — Unix seconds when the catch-up entry was
--                              recorded (request side)
--   peer_id         BLOB(32)  — libp2p peer id of the responder (or 32
--                              zero bytes for in-memory catch-up)

CREATE TABLE reputation_gossip_seen (
    recorder_did         BLOB NOT NULL,
    event_id             BLOB NOT NULL,
    attestor_did         BLOB NOT NULL,
    observed_at_unix     INTEGER NOT NULL,
    peer_id              BLOB NOT NULL,
    PRIMARY KEY (recorder_did, event_id)
);

CREATE INDEX reputation_gossip_seen_attestor
    ON reputation_gossip_seen (attestor_did, observed_at_unix);