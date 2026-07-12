//! Idempotent SQL DDL for the query layer.
//!
//! All `CREATE` statements use `IF NOT EXISTS` so `migrate()` is safe
//! to run on every boot. New tables / columns are added by appending
//! new statements; never modify a statement once it ships in a tagged
//! release (operators may have older tables).
//!
//! See `docs/plans/2026-07-11-whatsapp-query-layer-design.md` Schema
//! section for the full design rationale.

use stoolap::Database;

/// Bumped when a non-additive schema change lands; reset to 0 for a
/// fresh table on first install. Replays of the same `SCHEMA_VERSION`
/// do NOT trigger a rebuild — only mismatches do.
pub const SCHEMA_VERSION: u32 = 1;

const CREATE_EVENTS_TABLE: &str = r#"
CREATE TABLE IF NOT EXISTS events (
    id           INTEGER PRIMARY KEY,
    ts_unix_ms   INTEGER  NOT NULL,
    ts_mono_ns   INTEGER  NOT NULL,
    kind         TEXT     NOT NULL,
    variant      TEXT,
    peer         TEXT,
    sender       TEXT,
    chat_jid     TEXT,
    payload      TEXT     NOT NULL
)
"#;

const CREATE_EVENTS_INDEXES: &[&str] = &[
    "CREATE INDEX IF NOT EXISTS idx_events_kind_ts ON events(kind, ts_unix_ms)",
    "CREATE INDEX IF NOT EXISTS idx_events_peer_ts ON events(peer, ts_unix_ms)",
    "CREATE INDEX IF NOT EXISTS idx_events_chat_ts ON events(chat_jid, ts_unix_ms)",
];

const CREATE_MESSAGES_TABLE: &str = r#"
CREATE TABLE IF NOT EXISTS messages (
    event_id     INTEGER PRIMARY KEY,
    peer         TEXT     NOT NULL,
    sender       TEXT     NOT NULL,
    ts_unix_ms   INTEGER  NOT NULL,
    kind         TEXT     NOT NULL,
    text         TEXT     NOT NULL,
    media_token  TEXT,
    from_me      INTEGER  NOT NULL,
    is_group     INTEGER  NOT NULL,
    FOREIGN KEY (event_id) REFERENCES events(id) ON DELETE CASCADE
)
"#;

const CREATE_MESSAGES_INDEXES: &[&str] = &[
    "CREATE INDEX IF NOT EXISTS idx_messages_peer_ts ON messages(peer, ts_unix_ms)",
    // `messages` has no `chat_jid` column — for Message variants the
    // ingester sets `peer = chat_jid`, so `idx_messages_peer_ts`
    // covers that query path.
    "CREATE INDEX IF NOT EXISTS idx_messages_ts ON messages(ts_unix_ms)",
    "CREATE INDEX IF NOT EXISTS idx_messages_kind_ts ON messages(kind, ts_unix_ms)",
    "CREATE INDEX IF NOT EXISTS idx_messages_sender ON messages(sender, ts_unix_ms)",
];

const CREATE_EMBEDDINGS_TABLE: &str = r#"
CREATE TABLE IF NOT EXISTS embeddings (
    event_id     INTEGER PRIMARY KEY,
    model_id     TEXT     NOT NULL,
    dims         INTEGER  NOT NULL,
    provider     TEXT     NOT NULL,
    vec          VECTOR   NOT NULL,
    ts_embed_ms  INTEGER  NOT NULL,
    FOREIGN KEY (event_id) REFERENCES messages(event_id) ON DELETE CASCADE
)
"#;

// HNSW intentionally NOT created in v1:
// - Brute-force search is the actual implementation per fork TODOs at
//   stoolap/src/storage/vector/search.rs:79,93,139.
// - HNSW needs fixed dimensions in the column type (`VECTOR(N)`),
//   which would lock us to a single model. The local MiniLM-L6-v2 is
//   384d, but remote embedders can return 768/1536/3072 — flexibility
//   wins for now. Brute-force is correct up to ~500k embeddings
//   (30-50ms per top-200 query), so this is acceptable.
// Revisit when either (a) the fork ships the HNSW integration path
// that doesn't require fixed-dim column types, or (b) we standardize
// on a single model end-to-end.

const CREATE_META_TABLE: &str = r#"
CREATE TABLE IF NOT EXISTS query_meta (
    id        INTEGER PRIMARY KEY,
    meta_key  TEXT    NOT NULL UNIQUE,
    value     TEXT    NOT NULL
)
"#;

/// Run the full bootstrap DDL. Safe to call multiple times.
pub fn migrate(db: &Database) -> Result<(), stoolap::Error> {
    db.execute(CREATE_EVENTS_TABLE, ())?;
    for stmt in CREATE_EVENTS_INDEXES {
        db.execute(stmt, ())?;
    }
    db.execute(CREATE_MESSAGES_TABLE, ())?;
    for stmt in CREATE_MESSAGES_INDEXES {
        db.execute(stmt, ())?;
    }
    db.execute(CREATE_EMBEDDINGS_TABLE, ())?;
    db.execute(
        "CREATE INDEX IF NOT EXISTS idx_embeddings_model ON embeddings(model_id)",
        (),
    )?;
    db.execute(CREATE_META_TABLE, ())?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn migrate_idempotent_on_empty_db() {
        let db = Database::open_in_memory().expect("open in-memory");
        migrate(&db).expect("first migrate");
        migrate(&db).expect("second migrate (idempotent)");
    }

    #[test]
    fn migrate_creates_expected_tables() {
        let db = Database::open_in_memory().expect("open in-memory");
        migrate(&db).expect("migrate");
        let names: Vec<String> = db
            .query("SHOW TABLES", ())
            .expect("show tables")
            .map(|row| row.and_then(|r| r.get::<String>(0)).expect("name"))
            .collect();
        for tbl in ["events", "messages", "embeddings", "query_meta"] {
            assert!(
                names.iter().any(|n| n == tbl),
                "table `{tbl}` missing from {names:?}"
            );
        }
    }
}
