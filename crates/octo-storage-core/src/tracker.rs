//! Tracker-table helpers: ensure / read current version / record a version.
//!
//! Default tracker table name: [`crate::DEFAULT_TRACKER_TABLE`] = `"schema_migrations"`.
//! Owner crates can override via [`crate::ApplyConfig::with_tracker_table`].

use crate::error::StorageError;

/// Ensure the tracker table exists, with the substrate's preferred
/// schema. Idempotent across fresh DBs and legacy DBs whose tracker
/// table predates the substrate.
///
/// **Fresh DB**: `CREATE TABLE IF NOT EXISTS` materializes the canonical
/// `(version PK, name NOT NULL, applied_at_unix NOT NULL)` shape.
///
/// **Legacy DB** (e.g. pre-S2 `octo-reputation` with
/// `(id PK, name UNIQUE, applied_at_unix)`, no `version` column): the
/// substrate adds the missing `version` column via `ALTER TABLE ADD
/// COLUMN` (idempotent — error swallowed) and backfills its value from
/// the legacy `name` column using the `v<NNN>__<label>` filename
/// convention. Subsequent reads via [`current_version`] then return
/// the historical `MAX(version)`, letting [`apply_pending`] skip
/// already-applied migrations cleanly.
///
/// Layered-A stable: the DDL is frozen, but the alignment step exists
/// to bridge pre-substrate legacy DBs (created by per-owner bespoke
/// runners) into the substrate model without requiring destructive
/// data migration. Owner crates write no v-alignment DDL themselves.
///
/// # Errors
/// Returns `StorageError::stoolap` or `StorageError::Unsupported` on
/// identifier-validation / DDL failure.
pub fn ensure_tracker_table(db: &stoolap::Database, table_name: &str) -> Result<(), StorageError> {
    // Defensively validate table_name as a SQL identifier (lowercase
    // letters, digits, underscore — restrict to a safe subset so it
    // cannot be SQL-injected by a misconfigured owner).
    if !is_safe_identifier(table_name) {
        return Err(StorageError::Unsupported(format!(
            "tracker table name must match [a-z_][a-z0-9_]*; got {table_name:?}"
        )));
    }

    // Step 1 — substrate-friendly schema (no-op when table already exists).
    let ddl = format!(
        "CREATE TABLE IF NOT EXISTS {table_name} (\
         version INTEGER PRIMARY KEY, \
         name TEXT NOT NULL, \
         applied_at_unix INTEGER NOT NULL\
         )"
    );
    db.execute(&ddl, ())
        .map(|_| ())
        .map_err(|e| StorageError::stoolap("ensure_tracker_table", e))?;

    // Step 2 — ensure `version` column exists (legacy DBs predating
    // the substrate may lack it). The fork's `Error::DuplicateColumn`
    // display is `"duplicate column"` — if the column already exists
    // (the fresh-DB path), swallow the error.
    let add_version = format!("ALTER TABLE {table_name} ADD COLUMN version INTEGER");
    match db.execute(&add_version, ()) {
        Ok(_) => {}
        Err(e) => {
            let msg = format!("{e}");
            if !msg.contains("duplicate column") {
                return Err(StorageError::stoolap("ensure_tracker_table:add_version", e));
            }
            // Already has version column — fine.
        }
    }

    // Step 3 — backfill `version` for legacy rows whose `version`
    // column was just added with default NULL. Derive from the
    // canonical `v<NNN>__<label>` filename stored in `name`.
    //
    // Tables that lack a `name` column (e.g. pre-substrate
    // `quota-router-sm-engine` legacy schema: `(version PK,
    // applied_at)` only) cannot satisfy `name LIKE 'v%'` — the
    // fork errors with `Column 'name' not found`. Swallow that
    // specific error; any other failure propagates.
    let backfill = format!(
        "UPDATE {table_name} \
         SET version = CAST(SUBSTR(name, 2, 3) AS INTEGER) \
         WHERE version IS NULL AND name LIKE 'v%'"
    );
    match db.execute(&backfill, ()) {
        Ok(_) => {}
        Err(e) => {
            let msg = format!("{e}");
            if msg.contains("not found") && msg.contains("name") {
                // Table lacks the `name` column — no-op backfill.
                // The substrate will still detect MAX(version) for
                // callers that own such tables (sm-engine path).
            } else {
                return Err(StorageError::stoolap("ensure_tracker_table:backfill", e));
            }
        }
    }

    Ok(())
}

/// Read the highest applied migration version from the tracker table.
/// Returns 0 if the table is empty.
///
/// # Errors
/// Returns `StorageError::stoolap` on `db.query` / row decode failure.
pub fn current_version(db: &stoolap::Database, table_name: &str) -> Result<u32, StorageError> {
    if !is_safe_identifier(table_name) {
        return Err(StorageError::Unsupported(format!(
            "tracker table name must match [a-z_][a-z0-9_]*; got {table_name:?}"
        )));
    }
    let sql = format!("SELECT MAX(version) FROM {table_name}");
    let rows = db
        .query(&sql, ())
        .map_err(|e| StorageError::stoolap("current_version", e))?;
    let mut iter = rows.into_iter();
    if let Some(row_result) = iter.next() {
        let row = row_result.map_err(|e| StorageError::stoolap("row decode", e))?;
        // MAX(version) returns NULL when the table is empty (no migrations yet).
        // `Row::get::<Option<i64>>` returns Ok(None) in that case.
        let v: Option<i64> = row
            .get(0)
            .map_err(|e| StorageError::stoolap("get version", e))?;
        return Ok(v.unwrap_or(0).max(0) as u32);
    }
    Ok(0)
}

/// Returns the set of all applied versions, not just the max. Used by
/// `quota-router-sm-engine`-style callers that operate on the set instead
/// of the max.
///
/// # Errors
/// Returns `StorageError::stoolap` on `db.query` / row decode failure.
pub fn applied_version(
    db: &stoolap::Database,
    table_name: &str,
) -> Result<std::collections::HashSet<u32>, StorageError> {
    if !is_safe_identifier(table_name) {
        return Err(StorageError::Unsupported(format!(
            "tracker table name must match [a-z_][a-z0-9_]*; got {table_name:?}"
        )));
    }
    let sql = format!("SELECT version FROM {table_name}");
    let rows = db
        .query(&sql, ())
        .map_err(|e| StorageError::stoolap("applied_version", e))?;
    let mut out = std::collections::HashSet::new();
    for row_result in rows.into_iter() {
        let row = row_result.map_err(|e| StorageError::stoolap("row decode", e))?;
        let v: i64 = row
            .get(0)
            .map_err(|e| StorageError::stoolap("get version", e))?;
        if v >= 0 {
            out.insert(v as u32);
        }
    }
    Ok(out)
}

/// Record that migration `version` was applied. Wall-clock time
/// (`SystemTime::now`) embedded as UNIX seconds.
///
/// # Errors
/// Returns `StorageError::SystemTime` or `StorageError::stoolap`.
pub fn record_migration(
    db: &stoolap::Database,
    table_name: &str,
    version: u32,
    name: &'static str,
) -> Result<(), StorageError> {
    if !is_safe_identifier(table_name) {
        return Err(StorageError::Unsupported(format!(
            "tracker table name must match [a-z_][a-z0-9_]*; got {table_name:?}"
        )));
    }
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_err(|e| StorageError::SystemTime(e.to_string()))?
        .as_secs() as i64;
    let sql =
        format!("INSERT INTO {table_name} (version, name, applied_at_unix) VALUES ($1, $2, $3)");
    db.execute(&sql, (version as i64, name, now))
        .map(|_| ())
        .map_err(|e| StorageError::stoolap("record_migration", e))
}

/// Strict SQL-identifier check: `^[a-z_][a-z0-9_]*$`.
///
/// Defensive guard against table-name interpolation in `format!`. Owner
/// crates that need additional characters (e.g. dashes) should pre-validate
/// before passing in.
fn is_safe_identifier(name: &str) -> bool {
    if name.is_empty() {
        return false;
    }
    let mut chars = name.chars();
    let first = chars.next().expect("non-empty");
    if !(first.is_ascii_lowercase() || first == '_') {
        return false;
    }
    chars.all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_')
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn safe_identifier_accepts_typical_names() {
        assert!(is_safe_identifier("schema_migrations"));
        assert!(is_safe_identifier("cipherocto_schema_version"));
        assert!(is_safe_identifier("_x"));
        assert!(is_safe_identifier("v1"));
    }

    #[test]
    fn safe_identifier_rejects_injection_attempts() {
        assert!(!is_safe_identifier(""));
        assert!(!is_safe_identifier("foo; DROP TABLE x; --"));
        assert!(!is_safe_identifier("foo bar"));
        assert!(!is_safe_identifier("foo-bar"));
        assert!(!is_safe_identifier("Foo"));
        assert!(!is_safe_identifier("1foo"));
        // Note: `select` matches the safe regex — we deliberately do NOT
        // filter SQL keywords. Defense-in-depth against format-string
        // injection only needs character-set restriction; a `select`
        // table name would error semantically at the SQL layer.
        assert!(is_safe_identifier("select"));
    }

    #[test]
    fn end_to_end_tracker_table() {
        let db = stoolap::Database::open_in_memory().unwrap();
        ensure_tracker_table(&db, "schema_migrations").unwrap();

        // Empty table → version 0
        assert_eq!(current_version(&db, "schema_migrations").unwrap(), 0);

        // Idempotent: calling twice still works.
        ensure_tracker_table(&db, "schema_migrations").unwrap();

        // Record version 5
        record_migration(&db, "schema_migrations", 5, "create_x").unwrap();
        assert_eq!(current_version(&db, "schema_migrations").unwrap(), 5);

        // Record version 3 (out of order; allowed).
        record_migration(&db, "schema_migrations", 3, "create_y").unwrap();
        assert_eq!(
            applied_version(&db, "schema_migrations").unwrap(),
            [3_u32, 5_u32].into_iter().collect()
        );

        // Calling ensure_tracker_table again does not drop data.
        ensure_tracker_table(&db, "schema_migrations").unwrap();
        assert_eq!(
            applied_version(&db, "schema_migrations").unwrap(),
            [3_u32, 5_u32].into_iter().collect()
        );
    }

    #[test]
    fn unsafe_table_name_returns_unsupported() {
        let db = stoolap::Database::open_in_memory().unwrap();
        let err = ensure_tracker_table(&db, "foo;DROP TABLE x;--").unwrap_err();
        assert!(matches!(err, StorageError::Unsupported(_)));

        let err = current_version(&db, "BAD!").unwrap_err();
        assert!(matches!(err, StorageError::Unsupported(_)));
    }

    #[test]
    fn legacy_schema_without_version_column_is_aligned() {
        // Pre-substrate `octo-reputation` legacy DB: tracker table created
        // without a `version` column, rows populated by `id PK + name +
        // applied_at_unix`. The substrate's `ensure_tracker_table` must
        // detect this and ADD the column + backfill `version` from the
        // canonical `v<NNN>__<label>` filename stored in `name`.
        let db = stoolap::Database::open_in_memory().unwrap();
        db.execute(
            "CREATE TABLE schema_migrations (\
             id INTEGER PRIMARY KEY, \
             name TEXT NOT NULL UNIQUE, \
             applied_at_unix INTEGER NOT NULL\
             )",
            (),
        )
        .unwrap();
        db.execute(
            "INSERT INTO schema_migrations (id, name, applied_at_unix) \
             VALUES (1, 'v001__legacy_create', 1000), (2, 'v005__legacy_alter', 2000)",
            (),
        )
        .unwrap();

        // Pre-alignment: `current_version` would fail (no version col) —
        // after alignment, version is backfilled to MAX(name-derived) = 5.
        ensure_tracker_table(&db, "schema_migrations").unwrap();
        assert_eq!(current_version(&db, "schema_migrations").unwrap(), 5);

        // Backfilled values are exactly the integer prefix of `name`.
        let rows = db
            .query(
                "SELECT name, version FROM schema_migrations ORDER BY id",
                (),
            )
            .unwrap();
        let mut collected: Vec<(String, i64)> = Vec::new();
        for row in rows.into_iter() {
            let row = row.unwrap();
            let name: String = row.get(0).unwrap();
            let v: i64 = row.get(1).unwrap();
            collected.push((name, v));
        }
        assert_eq!(
            collected,
            vec![
                ("v001__legacy_create".to_string(), 1),
                ("v005__legacy_alter".to_string(), 5),
            ]
        );
    }

    #[test]
    fn legacy_schema_with_version_pk_is_idempotent_noop() {
        // Pre-substrate `quota-router-sm-engine` legacy DB: tracker table
        // already has `version INTEGER PRIMARY KEY`. ensure_tracker_table
        // must not break this shape — it runs an idempotent ADD COLUMN
        // (which fails because the column exists, swallowed) and runs a
        // backfill UPDATE that touches zero rows (version is not NULL
        // on legacy rows because they were inserted with the column
        // already present).
        let db = stoolap::Database::open_in_memory().unwrap();
        db.execute(
            "CREATE TABLE cipherocto_migrations (\
             version INTEGER PRIMARY KEY, \
             applied_at INTEGER NOT NULL\
             )",
            (),
        )
        .unwrap();
        db.execute(
            "INSERT INTO cipherocto_migrations (version, applied_at) VALUES (1, 100), (2, 200)",
            (),
        )
        .unwrap();

        ensure_tracker_table(&db, "cipherocto_migrations").unwrap();
        // MAX(version) preserved.
        assert_eq!(current_version(&db, "cipherocto_migrations").unwrap(), 2);
        // Original columns preserved.
        let row_count: i64 = db
            .query("SELECT COUNT(*) FROM cipherocto_migrations", ())
            .unwrap()
            .next()
            .unwrap()
            .unwrap()
            .get(0)
            .unwrap();
        assert_eq!(row_count, 2);
    }
}
