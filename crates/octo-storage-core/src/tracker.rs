//! Tracker-table helpers: ensure / read current version / record a version.
//!
//! Default tracker table name: [`crate::DEFAULT_TRACKER_TABLE`] = `"schema_migrations"`.
//! Owner crates can override via [`crate::ApplyConfig::with_tracker_table`].

use crate::error::StorageError;

/// Ensure the tracker table exists. Idempotent (`CREATE TABLE IF NOT EXISTS`).
///
/// DDL is hard-coded to match the long-standing convention used by
/// `octo-reputation` and `quota-router-sm-engine`. DDL is layered-A stable
/// (years) — if a future migration of THIS table is required, it must be
/// applied by a separate migration in the owner's catalog (NOT auto-baked
/// into the runtime).
///
/// # Errors
/// Returns `StorageError::stoolap` on `db.execute` failure.
pub fn ensure_tracker_table(db: &stoolap::Database, table_name: &str) -> Result<(), StorageError> {
    // Defensively validate table_name as a SQL identifier (lowercase
    // letters, digits, underscore — restrict to a safe subset so it
    // cannot be SQL-injected by a misconfigured owner).
    if !is_safe_identifier(table_name) {
        return Err(StorageError::Unsupported(format!(
            "tracker table name must match [a-z_][a-z0-9_]*; got {table_name:?}"
        )));
    }
    let ddl = format!(
        "CREATE TABLE IF NOT EXISTS {table_name} (\
         version INTEGER PRIMARY KEY, \
         name TEXT NOT NULL, \
         applied_at_unix INTEGER NOT NULL\
         )"
    );
    db.execute(&ddl, ())
        .map(|_| ())
        .map_err(|e| StorageError::stoolap("ensure_tracker_table", e))
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
}
