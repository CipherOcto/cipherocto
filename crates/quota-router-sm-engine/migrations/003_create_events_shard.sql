-- Per-shard event log (RFC-0963 §2 + §7).
--
-- Each shard N owns a table `events_shard_{N}` containing the WAL events
-- routed to that shard by `shard_for_segment(wal_segment_id, num_shards)`.
-- vault_id is denormalized onto every row for single-shard balance reads
-- per RFC-0963 §2.
--
-- Stoolap limitation: PRIMARY KEY must be INTEGER. We use a rowid +
-- UNIQUE BLOB event_id (per RFC-0963 §2 §"events_shard_{N}" schema).

CREATE TABLE IF NOT EXISTS events_shard_0 (
    rowid        INTEGER PRIMARY KEY AUTOINCREMENT,
    event_id     BLOB    NOT NULL UNIQUE,
    event_type   TEXT    NOT NULL,
    tx_id        BLOB    NOT NULL,
    schema_version INT   NOT NULL,
    visibility   TEXT    NOT NULL,
    timestamp_unix BIGINT NOT NULL,
    attributes   BLOB    NOT NULL,
    corrections  BLOB,
    signature    BLOB    NOT NULL,
    zk_proof     BLOB,
    vault_id     BLOB    NOT NULL,
    segment_id   BLOB    NOT NULL,
    height       BIGINT  NOT NULL,
    producer     TEXT    NOT NULL,
    state_root   BLOB    NOT NULL DEFAULT '',
    shard_id     INT     NOT NULL DEFAULT 0
);

CREATE INDEX IF NOT EXISTS ix_events_shard_0_vault ON events_shard_0 (vault_id, event_id);
CREATE INDEX IF NOT EXISTS ix_events_shard_0_type  ON events_shard_0 (event_type, event_id);
CREATE INDEX IF NOT EXISTS ix_events_shard_0_event_id ON events_shard_0 (event_id);
CREATE INDEX IF NOT EXISTS ix_events_shard_0_height   ON events_shard_0 (height);
CREATE INDEX IF NOT EXISTS ix_events_shard_0_segment  ON events_shard_0 (segment_id);
