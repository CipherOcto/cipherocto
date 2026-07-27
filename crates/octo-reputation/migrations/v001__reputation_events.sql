-- v001 — reputation_events + reputation_aggregates (RFC-0968 §3).
--
-- Schema per RFC-0968 §22 + amendments 1-58. The aggregate row carries
-- exactly nine canonical fields: did, kind, layer, score_ewma, samples,
-- severity_total, last_event_id, last_event_unix, updated_at_unix.
--
-- Blob columns store:
--   recorder_did         BLOB(52)  — RecorderDid (CID-style)
--   controller_id        BLOB(32)  — ControllerId
--   event_id             BLOB(8)   — EventId (u64 big-endian)
--   score_delta / ewma   BLOB(24)  — DfpEncoding::to_bytes()
--
-- The Dfp encoding is bit-deterministic per RFC-0104, so byte equality on
-- BLOB columns is sufficient for ordering, equality, and aggregation.

CREATE TABLE reputation_events (
    recorder_did          BLOB NOT NULL,
    event_id              BLOB NOT NULL,
    controller_id         BLOB NOT NULL,
    signal_kind           INTEGER NOT NULL,
    layer                 INTEGER NOT NULL,
    score_delta           BLOB NOT NULL,
    recorded_at_unix      INTEGER NOT NULL,
    rotation_provenance   BLOB,
    audit_ref             BLOB,
    PRIMARY KEY (recorder_did, event_id)
);

CREATE INDEX reputation_events_did_layer_kind_time
    ON reputation_events (recorder_did, layer, signal_kind, recorded_at_unix);

CREATE INDEX reputation_events_recorded_at
    ON reputation_events (recorded_at_unix);

CREATE TABLE reputation_aggregates (
    recorder_did          BLOB NOT NULL,
    signal_kind           INTEGER NOT NULL,
    layer                 INTEGER NOT NULL,
    score_ewma            BLOB NOT NULL,
    samples               INTEGER NOT NULL,
    severity_total        INTEGER NOT NULL,
    last_event_id         BLOB NOT NULL,
    last_event_unix       INTEGER NOT NULL,
    updated_at_unix       INTEGER NOT NULL,
    PRIMARY KEY (recorder_did, signal_kind, layer)
);