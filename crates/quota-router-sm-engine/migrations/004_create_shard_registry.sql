-- Shard registry + migration log (RFC-0963 §7 + R3-F7 + R4-F6).
--
-- Stoolap limitation: PRIMARY KEY must be INTEGER. We use rowid + UNIQUE
-- BLOB shard_registry_id. Per RFC-0963 R4-F6 the migration state enum
-- is `Pending | DualWriting | Reading | Draining | Finalized | Aborted`
-- and `strategy` is `DrainRefill | LiveMigration`.

CREATE TABLE IF NOT EXISTS shard_registry (
    rowid                  INTEGER PRIMARY KEY AUTOINCREMENT,
    shard_registry_id     BLOB    NOT NULL UNIQUE,
    shard_id              INT     NOT NULL,
    state                 TEXT    NOT NULL,
    num_shards_at_creation INT     NOT NULL,
    current_num_shards     INT     NOT NULL,
    created_at_unix        BIGINT  NOT NULL,
    retired_at_unix        BIGINT,
    event_count            BIGINT  NOT NULL DEFAULT 0,
    last_root_k            BIGINT  NOT NULL DEFAULT 0,
    last_root_hash         BLOB    NOT NULL DEFAULT '',
    updated_at_unix        BIGINT  NOT NULL,
    cluster_node_count     INT     NOT NULL
);

CREATE TABLE IF NOT EXISTS shard_migration_log (
    rowid            INTEGER PRIMARY KEY AUTOINCREMENT,
    migration_id     BLOB    NOT NULL UNIQUE,
    shard_registry_id BLOB    NOT NULL,
    from_num_shards  INT     NOT NULL,
    to_num_shards    INT     NOT NULL,
    strategy         TEXT    NOT NULL,
    state            TEXT    NOT NULL,
    started_at_unix  BIGINT  NOT NULL,
    completed_at_unix BIGINT,
    events_migrated  BIGINT  NOT NULL DEFAULT 0
);

CREATE INDEX IF NOT EXISTS ix_shard_migration_log_state    ON shard_migration_log (state);
CREATE INDEX IF NOT EXISTS ix_shard_migration_log_registry ON shard_migration_log (shard_registry_id);
