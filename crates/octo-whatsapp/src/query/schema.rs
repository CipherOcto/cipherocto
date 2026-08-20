//! Idempotent SQL DDL for the query layer.
//!
//! All `CREATE` statements use `IF NOT EXISTS` so `migrate()` is safe
//! to run on every boot. New tables / columns are added by appending
//! new statements; never modify a statement once it ships in a tagged
//! release (operators may have older tables).
//!
//! See `docs/plans/2026-07-11-whatsapp-query-layer-design.md` Schema
//! section for the full design rationale.

use octo_storage_core::Database;

/// Bumped when a non-additive schema change lands; reset to 0 for a
/// fresh table on first install. Replays of the same `SCHEMA_VERSION`
/// do NOT trigger a rebuild — only mismatches do.
///
/// v2 (2026-07-15, Phase 7.K): added `messages.view_once INTEGER NOT NULL
/// DEFAULT 0` + `messages.ephemeral_expires_at_seconds INTEGER` for
/// view-once media + disappearing-message flags, and new
/// `unavailable_messages` + `disappearing_mode_changes` tables for the
/// typed `Event::UndecryptableMessage` + `Event::DisappearingModeChanged`
/// bridges. All additive — `migrate()` is idempotent.
pub const SCHEMA_VERSION: u32 = 2;

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

/// Phase 7.K: one row per inbound `Unavailable` event — the WA server
/// fanout `<unavailable type="view_once|hosted|bot|...">`. `kind`
/// is the wire-format string from `wacore::types::events::UnavailableType`
/// (`unknown` / `view_once` / `hosted` / `bot`).
const CREATE_UNAVAILABLE_TABLE: &str = r#"
CREATE TABLE IF NOT EXISTS unavailable_messages (
    id             INTEGER PRIMARY KEY,
    ts_unix_ms     INTEGER  NOT NULL,
    ts_mono_ns     INTEGER  NOT NULL,
    kind           TEXT     NOT NULL,
    peer           TEXT     NOT NULL,
    sender         TEXT     NOT NULL,
    is_unavailable INTEGER  NOT NULL DEFAULT 1
)
"#;

const CREATE_UNAVAILABLE_INDEXES: &[&str] = &[
    "CREATE INDEX IF NOT EXISTS idx_unavailable_kind_ts ON unavailable_messages(kind, ts_unix_ms)",
    "CREATE INDEX IF NOT EXISTS idx_unavailable_peer_ts ON unavailable_messages(peer, ts_unix_ms)",
];

/// Phase 7.K: one row per inbound `DisappearingModeChanged` event
/// (`<notification type="disappearing_mode">`). `duration_seconds == 0`
/// means the timer is disabled.
const CREATE_DISAPPEARING_MODE_CHANGES_TABLE: &str = r#"
CREATE TABLE IF NOT EXISTS disappearing_mode_changes (
    id               INTEGER PRIMARY KEY,
    ts_unix_ms       INTEGER  NOT NULL,
    ts_mono_ns       INTEGER  NOT NULL,
    jid              TEXT     NOT NULL,
    duration_seconds INTEGER  NOT NULL
)
"#;

const CREATE_DISAPPEARING_MODE_CHANGES_INDEXES: &[&str] =
    &["CREATE INDEX IF NOT EXISTS idx_dmc_jid_ts ON disappearing_mode_changes(jid, ts_unix_ms)"];

/// Run the full bootstrap DDL. Safe to call multiple times.
pub fn migrate(db: &Database) -> Result<(), stoolap::Error> {
    migrate_v1(db)?;
    migrate_v2(db)?;
    Ok(())
}

/// Original (v1) schema bootstrap. Kept as a private function so the
/// public `migrate()` entrypoint always applies both v1 + v2 in order;
/// callers that want to inspect a v1-only state can still exercise
/// this path explicitly.
fn migrate_v1(db: &Database) -> Result<(), stoolap::Error> {
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

/// Phase 7.K v2 migration. Additive columns on `messages` plus two new
/// tables. Each step is gated by a column-existence probe so it's safe
/// to run on a v1-already-applied database OR a fresh-install one.
fn migrate_v2(db: &Database) -> Result<(), stoolap::Error> {
    // Probe the first new column: if `SELECT view_once FROM messages`
    // errors with `ColumnNotFound`, the column is absent and the ALTER
    // must run. Stoolap surfaces both ColumnNotFound and the parse
    // error on an unknown column reference, so we just look for either.
    if !has_column(db, "messages", "view_once")? {
        db.execute(
            "ALTER TABLE messages ADD COLUMN view_once INTEGER NOT NULL DEFAULT 0",
            (),
        )?;
    }
    if !has_column(db, "messages", "ephemeral_expires_at_seconds")? {
        db.execute(
            "ALTER TABLE messages ADD COLUMN ephemeral_expires_at_seconds INTEGER",
            (),
        )?;
    }
    if !has_column(db, "messages", "consumed_at_unix_ms")? {
        // T10: `messages.read_view_once` one-shot. NULL = unconsumed.
        db.execute(
            "ALTER TABLE messages ADD COLUMN consumed_at_unix_ms INTEGER",
            (),
        )?;
    }
    db.execute(CREATE_UNAVAILABLE_TABLE, ())?;
    for stmt in CREATE_UNAVAILABLE_INDEXES {
        db.execute(stmt, ())?;
    }
    db.execute(CREATE_DISAPPEARING_MODE_CHANGES_TABLE, ())?;
    for stmt in CREATE_DISAPPEARING_MODE_CHANGES_INDEXES {
        db.execute(stmt, ())?;
    }
    Ok(())
}

/// True if `SELECT <column> FROM <table> LIMIT 0` returns Ok.
fn has_column(db: &Database, table: &str, column: &str) -> Result<bool, stoolap::Error> {
    // Stoolap has no `PRAGMA table_info(...)` equivalent yet; the cheapest
    // probe is to try a query that references the column. Any error
    // (ColumnNotFound or a parse error) means the column is absent.
    let sql = format!("SELECT {column} FROM {table} LIMIT 0");
    match db.query(&sql, ()) {
        Ok(_) => Ok(true),
        Err(stoolap::Error::ColumnNotFound(_)) => Ok(false),
        Err(_) => Ok(false),
    }
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
        for tbl in [
            "events",
            "messages",
            "embeddings",
            "query_meta",
            "unavailable_messages",
            "disappearing_mode_changes",
        ] {
            assert!(
                names.iter().any(|n| n == tbl),
                "table `{tbl}` missing from {names:?}"
            );
        }
    }

    #[test]
    fn migrate_v2_adds_columns_and_tables() {
        let db = Database::open_in_memory().expect("open in-memory");
        migrate(&db).expect("migrate");
        // Probe the new columns. The v1->v2 upgrade is additive so the
        // columns must exist after a single migrate.
        assert!(has_column(&db, "messages", "view_once").unwrap());
        assert!(has_column(&db, "messages", "ephemeral_expires_at_seconds").unwrap());
        assert!(has_column(&db, "messages", "consumed_at_unix_ms").unwrap());
    }

    #[test]
    fn migrate_idempotent_after_v1_then_v2() {
        // Simulate an existing v1 install (no v2 columns) by manually
        // creating only the v1 tables, then running `migrate()` which
        // must apply v2 + the new tables without error.
        let db = Database::open_in_memory().expect("open in-memory");
        migrate_v1(&db).expect("v1 only");
        migrate(&db).expect("migrate v1+v2");
        migrate(&db).expect("migrate again (idempotent)");
        assert!(has_column(&db, "messages", "view_once").unwrap());
    }
}
