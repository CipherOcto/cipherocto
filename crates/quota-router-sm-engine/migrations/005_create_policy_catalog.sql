-- Policy catalog (RFC-0967 §9).
--
-- Stoolap limitation: PRIMARY KEY must be INTEGER. We use rowid + UNIQUE
-- BLOB policy_id. RFC-0967 §9 columns: policy_id, version_seq,
-- parent_policy_id (FK self-ref), graph_root (BLAKE3 of PolicyGraph),
-- audit_ref, timestamp_unix_ms, signature, lineage_id. Self-referential
-- FK is permitted because `lineage_id` separates the version chain from
-- the parent pointer.

CREATE TABLE IF NOT EXISTS policy_catalog (
    rowid              INTEGER PRIMARY KEY AUTOINCREMENT,
    policy_id          BLOB    NOT NULL,
    version_seq        BIGINT  NOT NULL,
    parent_policy_id   BLOB,
    graph_root         BLOB    NOT NULL,
    surface            BLOB    NOT NULL,
    lineage            BLOB    NOT NULL,
    audit_ref          BLOB    NOT NULL,
    timestamp_unix_ms  BIGINT  NOT NULL,
    signature          BLOB    NOT NULL,
    lineage_id         BLOB    NOT NULL,
    UNIQUE (policy_id, version_seq)
);

CREATE INDEX IF NOT EXISTS ix_policy_catalog_policy_id  ON policy_catalog (policy_id);
CREATE INDEX IF NOT EXISTS ix_policy_catalog_lineage_id ON policy_catalog (lineage_id, version_seq);
