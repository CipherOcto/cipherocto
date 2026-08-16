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
/// `(version INTEGER PRIMARY KEY, name TEXT NOT NULL, applied_at_unix
/// INTEGER NOT NULL)` shape. All subsequent `ALTER TABLE ADD COLUMN`
/// steps are swallowed as `duplicate column` no-ops.
///
/// **Legacy DB** (pre-substrate):
/// - `octo-reputation`'s `(id INTEGER PRIMARY KEY, name TEXT NOT NULL
///   UNIQUE, applied_at_unix INTEGER NOT NULL)` — missing `version`
///   column. The substrate adds `version INTEGER` (idempotent) and
///   backfills its value from the legacy `name` column using the
///   canonical `v<NNN>__<label>` filename convention. `record_migration`
///   detects the legacy `id`-PK shape and supplies `id = MAX(id)+1`.
/// - `quota-router-sm-engine`'s `(version INTEGER PRIMARY KEY, applied_at
///   INTEGER NOT NULL)` — missing `name` and `applied_at_unix` columns.
///   The substrate adds both (idempotent) and they are populated by
///   later `record_migration` calls.
///
/// **Layer A stable**: the canonical DDL is frozen, but the alignment
/// step exists to bridge pre-substrate legacy DBs (created by per-owner
/// bespoke runners) into the substrate model without requiring
/// destructive data migration. Owner crates write no v-alignment DDL
/// themselves.
///
/// # Errors
/// Returns `StorageError::stoolap` or `StorageError::Unsupported` on
/// identifier-validation / DDL failure. Alignment-backfill failures
/// (a row whose `name` cannot be parsed into a version) are surfaced as
/// `StorageError::stoolap` with `operation: "ensure_tracker_table:backfill_orphan"`
/// to prevent silent partial-application.
pub fn ensure_tracker_table(db: &stoolap::Database, table_name: &str) -> Result<(), StorageError> {
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

    // Steps 2-4 — add the three substrate columns idempotently. Each
    // ADD COLUMN fails with `"duplicate column"` on the fresh-DB path
    // (where the CREATE already created them) and on legacy DBs that
    // pre-date the substrate. The fork's `Error::DuplicateColumn`
    // display string is `"duplicate column"` — match on that.
    add_column_idempotent(db, table_name, "name TEXT")?;
    add_column_idempotent(db, table_name, "applied_at_unix INTEGER")?;
    add_column_idempotent(db, table_name, "version INTEGER")?;

    // Step 5 — backfill `version` for legacy rows whose `version`
    // column was just added with default NULL. Derive from the
    // canonical `v<NNN>__<label>` filename stored in `name`. Variable-
    // length extraction: `SUBSTR(name, 2, INSTR(name, '__') - 2)` so
    // future versions >= 1000 (v1000, v1234) are parsed correctly
    // (SUBSTR with a fixed length 3 would truncate to "100"/"123").
    let backfill = format!(
        "UPDATE {table_name} \
         SET version = CAST(SUBSTR(name, 2, CASE WHEN INSTR(name, '__') > 0 \
             THEN INSTR(name, '__') - 2 ELSE LENGTH(name) - 1 END) AS INTEGER) \
         WHERE version IS NULL AND name LIKE 'v%'"
    );
    db.execute(&backfill, ())
        .map(|_| ())
        .map_err(|e| StorageError::stoolap("ensure_tracker_table:backfill", e))?;

    // Step 6 — post-backfill sanity check. If any row still has
    // `version IS NULL` after the backfill AND the table has a `name`
    // column, the legacy row had a non-`v<NNN>__<label>` name
    // (e.g. ops audit markers) that the substrate cannot interpret.
    // Surface this as a loud error rather than silently shipping a
    // half-aligned DB — `current_version` would otherwise read a
    // **wrong** MAX(version) and `apply_pending` would re-run
    // already-applied migrations.
    let orphans = format!("SELECT COUNT(*) FROM {table_name} WHERE version IS NULL");
    let rows = db
        .query(&orphans, ())
        .map_err(|e| StorageError::stoolap("ensure_tracker_table:orphan_count", e))?;
    let mut iter = rows.into_iter();
    if let Some(row_result) = iter.next() {
        let row = row_result.map_err(|e| StorageError::stoolap("orphan row", e))?;
        let n: i64 = row
            .get(0)
            .map_err(|e| StorageError::stoolap("orphan count get", e))?;
        if n > 0 {
            // Fetch up to 5 sample orphan names to surface in the error
            // message (avoids forcing the operator to re-query the DB).
            let samples_sql =
                format!("SELECT name FROM {table_name} WHERE version IS NULL LIMIT 5");
            let mut samples: Vec<String> = Vec::new();
            if let Ok(sample_rows) = db.query(&samples_sql, ()) {
                for sr in sample_rows.into_iter().take(5).flatten() {
                    if let Ok(s) = sr.get::<String>(0) {
                        samples.push(s);
                    }
                }
            }
            return Err(StorageError::Stoolap {
                operation: "ensure_tracker_table:backfill_orphan",
                message: format!(
                    "{n} rows in {table_name} have a name that does not match the v<NNN>__<label> convention \
                     (samples: {}); manual remediation required (rename rows or DROP TABLE)",
                    samples.join(", ")
                ),
            });
        }
    }

    Ok(())
}

/// `ALTER TABLE {table_name} ADD COLUMN {column_ddl}`, swallowing the
/// fork's `Error::DuplicateColumn` ("duplicate column") display string
/// for idempotent re-runs. Any other error propagates.
fn add_column_idempotent(
    db: &stoolap::Database,
    table_name: &str,
    column_ddl: &str,
) -> Result<(), StorageError> {
    let sql = format!("ALTER TABLE {table_name} ADD COLUMN {column_ddl}");
    match db.execute(&sql, ()) {
        Ok(_) => Ok(()),
        Err(e) => {
            let msg = format!("{e}");
            if msg.contains("duplicate column") {
                Ok(())
            } else {
                Err(StorageError::stoolap("ensure_tracker_table:add_column", e))
            }
        }
    }
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

/// Returns true if the given column exists in the given table. Used by
/// `record_migration` to detect the legacy `id`-PK shape (octo-reputation
/// pre-substrate) so it can supply an `id` value on INSERT. If the
/// probe fails for any other reason, returns false (the canonical
/// INSERT path will then fail and surface the real error).
fn has_column(db: &stoolap::Database, table_name: &str, column_name: &str) -> bool {
    // Sanitize: both args are validated `is_safe_identifier` per
    // ensure_tracker_table / record_migration callers, but defense
    // in depth here guards the SELECT literal.
    if !is_safe_identifier(table_name) || !is_safe_identifier(column_name) {
        return false;
    }
    let sql = format!("SELECT {column_name} FROM {table_name} LIMIT 0");
    db.query(&sql, ()).is_ok()
}

/// Record that migration `version` was applied. Wall-clock time
/// (`SystemTime::now`) embedded as UNIX seconds.
///
/// Three INSERT paths, dispatched by legacy-table shape probes:
///
/// 1. **Canonical** (substrate-created tracker or `quota-router-storage`
///    legacy `(version PK, name NOT NULL, applied_at_unix NOT NULL)`):
///    `INSERT INTO {table} (version, name, applied_at_unix) VALUES (?,
///    ?, ?)`.
///
/// 2. **octo-reputation legacy** (`id INTEGER PRIMARY KEY, name UNIQUE,
///    applied_at_unix` + `version` added by alignment): `INSERT INTO
///    {table} (id, version, name, applied_at_unix) VALUES (?, ?, ?, ?)`.
///    The fork's `id` PK is NOT auto-increment (verified at fork
///    `src/storage/mvcc/table.rs:1074`), so the caller must supply an
///    `id` value. We pre-fetch `MAX(id) + 1` because the Stoolap fork
///    does not support scalar subqueries in the `VALUES` clause of
///    parameterised INSERTs.
///
/// 3. **quota-router-sm-engine legacy** (`version PK, applied_at NOT
///    NULL` + `name` and `applied_at_unix` added by alignment):
///    `INSERT INTO {table} (version, applied_at, name, applied_at_unix)
///    VALUES (?, ?, ?, ?)`. The pre-substrate `applied_at` column is
///    NOT NULL, so we MUST supply it on every INSERT (the substrate
///    mirrors the value into both columns).
///
/// The probes are two cheap `SELECT col FROM {table} LIMIT 0` queries
/// per call; persistent caching is intentionally avoided because the
/// owner-crate hot path opens the DB once per process startup, and
/// schema shape invariants are owner concerns, not substrate concerns.
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

    if has_column(db, table_name, "id") {
        // octo-reputation legacy: pre-fetch the next id.
        let next_id_sql = format!("SELECT COALESCE(MAX(id), 0) + 1 FROM {table_name}");
        let rows = db
            .query(&next_id_sql, ())
            .map_err(|e| StorageError::stoolap("record_migration:next_id", e))?;
        let mut iter = rows.into_iter();
        let row = iter
            .next()
            .ok_or_else(|| StorageError::Stoolap {
                operation: "record_migration:next_id",
                message: "MAX(id) subquery returned no rows".to_owned(),
            })?
            .map_err(|e| StorageError::stoolap("record_migration:next_id_row", e))?;
        let next_id: i64 = row
            .get(0)
            .map_err(|e| StorageError::stoolap("record_migration:next_id_get", e))?;
        let sql = format!(
            "INSERT INTO {table_name} (id, version, name, applied_at_unix) \
             VALUES ($1, $2, $3, $4)"
        );
        db.execute(&sql, (next_id, version as i64, name, now))
            .map(|_| ())
            .map_err(|e| StorageError::stoolap("record_migration:legacy_id_pk", e))
    } else if has_column(db, table_name, "applied_at") {
        // quota-router-sm-engine legacy: must supply the legacy
        // `applied_at` NOT NULL column. Mirror the value into both.
        let sql = format!(
            "INSERT INTO {table_name} (version, applied_at, name, applied_at_unix) \
             VALUES ($1, $2, $3, $4)"
        );
        db.execute(&sql, (version as i64, now, name, now))
            .map(|_| ())
            .map_err(|e| StorageError::stoolap("record_migration:legacy_applied_at", e))
    } else {
        // Canonical: version PK.
        let sql = format!(
            "INSERT INTO {table_name} (version, name, applied_at_unix) VALUES ($1, $2, $3)"
        );
        db.execute(&sql, (version as i64, name, now))
            .map(|_| ())
            .map_err(|e| StorageError::stoolap("record_migration", e))
    }
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

    // === Legacy schema alignment ===

    #[test]
    fn legacy_octo_reputation_id_pk_recovers_end_to_end() {
        // Pre-substrate `octo-reputation` legacy DB: tracker table with
        // `id INTEGER PRIMARY KEY` (no AUTOINCREMENT in the fork),
        // `name UNIQUE`, `applied_at_unix`. Opening under S2 + then
        // recording a new migration must succeed (the bug fix: the
        // substrate supplies `id = MAX(id)+1` on the legacy path).
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
             VALUES (1, 'v001__legacy', 1000), (2, 'v005__legacy', 2000)",
            (),
        )
        .unwrap();

        ensure_tracker_table(&db, "schema_migrations").unwrap();
        // Backfilled version from canonical name.
        assert_eq!(current_version(&db, "schema_migrations").unwrap(), 5);

        // Now record a NEW migration (v006) — this is the path that
        // bug-d prior to the fix.
        record_migration(&db, "schema_migrations", 6, "v006__new").unwrap();

        // Both v005 and v006 are now recorded; v006 was inserted with id=3.
        let rows = db
            .query(
                "SELECT id, version, name FROM schema_migrations ORDER BY id",
                (),
            )
            .unwrap();
        let mut entries: Vec<(i64, i64, String)> = Vec::new();
        for row in rows.into_iter() {
            let row = row.unwrap();
            entries.push((
                row.get(0).unwrap(),
                row.get(1).unwrap(),
                row.get(2).unwrap(),
            ));
        }
        assert_eq!(entries.len(), 3, "v006 row was inserted");
        assert_eq!(entries[2].0, 3, "v006 id = MAX(id)+1 = 3");
        assert_eq!(entries[2].1, 6);
        assert_eq!(entries[2].2, "v006__new");
    }

    #[test]
    fn legacy_quota_router_sm_engine_lacks_name_and_applied_at_unix_recovers_end_to_end() {
        // Pre-substrate `quota-router-sm-engine` legacy DB: tracker table
        // with `version INTEGER PRIMARY KEY`, `applied_at INTEGER NOT
        // NULL`. No `name`, no `applied_at_unix`. The substrate's
        // `ensure_tracker_table` ADDs both (idempotent swallow), and
        // `record_migration` populates them on subsequent inserts.
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

        // New migration v3 — pre-fix this failed with
        // "column 'name' not found". Post-fix the substrate added
        // `name` and `applied_at_unix` columns during alignment, so
        // the INSERT succeeds.
        record_migration(&db, "cipherocto_migrations", 3, "v003__new").unwrap();

        let rows = db
            .query(
                "SELECT version, name, applied_at_unix FROM cipherocto_migrations ORDER BY version",
                (),
            )
            .unwrap();
        // Legacy rows (v1, v2) have `applied_at_unix = NULL` (the
        // legacy schema lacked the column; the substrate added it
        // nullable). Read as Option<i64> so the test does not falsely
        // fail on the legacy NULL.
        let mut entries: Vec<(i64, String, Option<i64>)> = Vec::new();
        for row in rows.into_iter() {
            let row = row.unwrap();
            entries.push((
                row.get(0).unwrap(),
                row.get(1).unwrap(),
                row.get(2).unwrap(),
            ));
        }
        assert_eq!(entries.len(), 3);
        assert_eq!(entries[2].0, 3);
        assert_eq!(entries[2].1, "v003__new");
        assert!(
            entries[2].2.unwrap_or(0) > 0,
            "v003 applied_at_unix was populated"
        );
    }

    #[test]
    fn legacy_substr_extracts_variable_length_versions() {
        // C3 regression: SUBSTR(name, 2, 3) used to truncate v1000+ to
        // 100. The fix uses INSTR(name, '__') to find the separator and
        // extract the variable-length version prefix.
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
             VALUES (1, 'v1000__big', 1000), (2, 'v1001__big', 2000), (3, 'v1234__big', 3000)",
            (),
        )
        .unwrap();
        ensure_tracker_table(&db, "schema_migrations").unwrap();

        let rows = db
            .query(
                "SELECT name, version FROM schema_migrations ORDER BY id",
                (),
            )
            .unwrap();
        let mut entries: Vec<(String, i64)> = Vec::new();
        for row in rows.into_iter() {
            let row = row.unwrap();
            entries.push((row.get(0).unwrap(), row.get(1).unwrap()));
        }
        assert_eq!(entries[0].1, 1000, "v1000 parsed as 1000 (not 100)");
        assert_eq!(entries[1].1, 1001, "v1001 parsed as 1001 (not 100)");
        assert_eq!(entries[2].1, 1234, "v1234 parsed as 1234 (not 123)");
    }

    #[test]
    fn legacy_orphan_name_row_triggers_backfill_orphan_error() {
        // H2/H7 regression: if a legacy row has a non-`v<NNN>__<label>`
        // name (e.g. an ops audit marker), the backfill UPDATE skips
        // it (WHERE name LIKE 'v%'). The post-backfill sanity check
        // must surface this LOUDLY rather than silently shipping a
        // half-aligned DB.
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
             VALUES (1, 'v001__legacy', 1000), (2, 'manual_audit_marker', 2000)",
            (),
        )
        .unwrap();

        let err = ensure_tracker_table(&db, "schema_migrations").unwrap_err();
        match err {
            StorageError::Stoolap { operation, message } => {
                assert_eq!(operation, "ensure_tracker_table:backfill_orphan");
                assert!(message.contains("manual_audit_marker"));
            }
            other => panic!("expected Stoolap backfill_orphan error, got {other:?}"),
        }
    }

    #[test]
    fn legacy_row_with_long_name_no_double_underscore_backfills_zero() {
        // Edge case: a legacy name without `__` (e.g. `v5`) is
        // tolerated by the variable-length SUBSTR (it falls back to
        // `LENGTH(name) - 1`). Version is parsed as the integer prefix.
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
            "INSERT INTO schema_migrations (id, name, applied_at_unix) VALUES (1, 'v5', 1000)",
            (),
        )
        .unwrap();
        ensure_tracker_table(&db, "schema_migrations").unwrap();
        let rows = db
            .query("SELECT version FROM schema_migrations", ())
            .unwrap();
        let mut iter = rows.into_iter();
        let row = iter.next().unwrap().unwrap();
        let v: i64 = row.get(0).unwrap();
        assert_eq!(v, 5, "v5 (no `__`) parsed as 5");
    }

    #[test]
    fn has_column_detects_id_pk_legacy_shape() {
        let db = stoolap::Database::open_in_memory().unwrap();
        // Tracker-shaped table with id PK.
        db.execute(
            "CREATE TABLE with_id (id INTEGER PRIMARY KEY, name TEXT NOT NULL)",
            (),
        )
        .unwrap();
        assert!(has_column(&db, "with_id", "id"));
        assert!(!has_column(&db, "with_id", "missing_col"));

        // Substrate-shaped table (version PK, no id).
        db.execute(
            "CREATE TABLE version_pk (version INTEGER PRIMARY KEY, name TEXT NOT NULL)",
            (),
        )
        .unwrap();
        assert!(!has_column(&db, "version_pk", "id"));
        assert!(has_column(&db, "version_pk", "version"));
    }
}
