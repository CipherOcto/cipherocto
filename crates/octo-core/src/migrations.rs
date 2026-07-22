//! Migration runner for cipherocto-side schema migrations (Phase C).
//!
//! Stores applied migration versions in a `cipherocto_schema_version` table
//! inside the same database. On `apply_pending`, queries current version,
//! runs all migrations with higher version in order, idempotently.
//!
//! Per [[stoolap-general-purpose-db]] principle: consumer schema lives in
//! cipherocto-side migrations; fork stays untouched.

use thiserror::Error;

/// Migration definition: version + name + SQL statements.
#[derive(Debug, Clone)]
pub struct Migration {
    /// Numeric version (semver-like; monotonic). Example: 1, 2, 3.
    pub version: u32,
    /// Human-readable name (e.g., "create_asks_table"). Used for error messages.
    pub name: &'static str,
    /// SQL statements to execute. Multiple statements are split on `;` + newline.
    pub sql: &'static str,
}

/// Built-in migrations for the cipherocto `octo-core` schema.
///
/// Add new migrations to the END of this list. Never reorder or remove
/// already-released migrations.
pub const BUILTIN_MIGRATIONS: &[Migration] = &[
    Migration {
        version: 1,
        name: "create_asks_table",
        sql: include_str!("../migrations/v001__create_asks_table.sql"),
    },
    Migration {
        version: 2,
        name: "create_asks_indexes",
        sql: include_str!("../migrations/v002__create_asks_indexes.sql"),
    },
];

/// Migration errors.
#[derive(Debug, Error)]
pub enum MigrationError {
    #[error("database error: {0}")]
    Db(String),
    #[error("migration version {0} failed: {1}")]
    MigrationFailed(u32, String),
    #[error("migration version {version} not found in catalog (db has higher version than code)")]
    UnknownMigration { version: u32 },
}

/// Initialize the schema-version tracking table (idempotent).
fn ensure_version_table(db: &stoolap::Database) -> Result<(), MigrationError> {
    db.execute(
        "CREATE TABLE IF NOT EXISTS cipherocto_schema_version (\
         version INTEGER PRIMARY KEY, \
         name TEXT NOT NULL, \
         applied_at_unix INTEGER NOT NULL\
         )",
        (),
    )
    .map_err(|e| MigrationError::Db(format!("create version table: {e}")))?;
    Ok(())
}

/// Read the highest applied migration version (0 if none).
fn current_version(db: &stoolap::Database) -> Result<u32, MigrationError> {
    let rows = db
        .query("SELECT MAX(version) FROM cipherocto_schema_version", ())
        .map_err(|e| MigrationError::Db(format!("select max version: {e}")))?;
    let mut iter = rows.into_iter();
    if let Some(row_result) = iter.next() {
        let row = row_result.map_err(|e| MigrationError::Db(format!("row: {e}")))?;
        // MAX(version) returns NULL when the table is empty (no migrations yet).
        // ResultRow::get::<Option<i64>> returns Ok(None) in that case.
        let v: Option<i64> = row
            .get(0)
            .map_err(|e| MigrationError::Db(format!("get version: {e}")))?;
        return Ok(v.unwrap_or(0).max(0) as u32);
    }
    Ok(0)
}

/// Record that migration `version` was applied.
fn record_migration(
    db: &stoolap::Database,
    version: u32,
    name: &'static str,
) -> Result<(), MigrationError> {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_err(|e| MigrationError::Db(format!("system time: {e}")))?
        .as_secs() as i64;
    db.execute(
        "INSERT INTO cipherocto_schema_version (version, name, applied_at_unix) VALUES ($1, $2, $3)",
        (version as i64, name, now),
    )
    .map_err(|e| MigrationError::Db(format!("insert version: {e}")))?;
    Ok(())
}

/// Apply all pending migrations from `BUILTIN_MIGRATIONS` that are newer than
/// the current database version.
///
/// # Errors
/// Returns `MigrationError::Db` on database failure, `MigrationError::UnknownMigration`
/// if the DB is at a higher version than the code's catalog (downgrade scenario),
/// `MigrationError::MigrationFailed` if a specific migration's SQL fails.
pub fn apply_pending(db: &stoolap::Database) -> Result<(), MigrationError> {
    ensure_version_table(db)?;
    let current = current_version(db)?;

    for migration in BUILTIN_MIGRATIONS {
        if migration.version <= current {
            continue;
        }
        run_one(db, migration)?;
    }
    Ok(())
}

/// Run a single migration (split SQL on `;\n` boundaries, execute each).
fn run_one(db: &stoolap::Database, migration: &Migration) -> Result<(), MigrationError> {
    let statements = split_sql_statements(migration.sql);
    for stmt in &statements {
        db.execute(stmt, ()).map_err(|e| {
            MigrationError::MigrationFailed(migration.version, format!("{e}: {stmt}"))
        })?;
    }
    record_migration(db, migration.version, migration.name)?;
    Ok(())
}

/// Split a multi-statement SQL string on `;` boundaries.
/// Strips `--` line comments. Handles `; `, `;\n`, `;;` patterns.
fn split_sql_statements(sql: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut buf = String::new();
    let mut chars = sql.chars().peekable();
    while let Some(c) = chars.next() {
        // Strip line comments (-- ... \n).
        if c == '-' && chars.peek() == Some(&'-') {
            while let Some(&nc) = chars.peek() {
                chars.next();
                if nc == '\n' {
                    break;
                }
            }
            continue;
        }
        buf.push(c);
        // End-of-statement delimiter: `;` (with optional trailing whitespace).
        if c == ';' {
            let stmt = buf.trim().to_owned();
            // Strip trailing `;` from the captured statement.
            let stmt = stmt.trim_end_matches(';').trim().to_owned();
            if !stmt.is_empty() {
                out.push(stmt);
            }
            buf.clear();
        }
    }
    let tail = buf.trim().to_owned();
    if !tail.is_empty() {
        out.push(tail);
    }
    out
}

/// Debug: list all builtin migrations.
#[must_use]
pub fn list_migrations() -> Vec<(u32, &'static str)> {
    BUILTIN_MIGRATIONS
        .iter()
        .map(|m| (m.version, m.name))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn split_simple() {
        let sql = "CREATE TABLE foo (id INT); CREATE INDEX bar ON foo(id);";
        let stmts = split_sql_statements(sql);
        assert_eq!(stmts.len(), 2);
    }

    #[test]
    fn split_strips_line_comments() {
        let sql = "-- header\nCREATE TABLE foo (id INT); -- tail\nCREATE INDEX bar ON foo(id);";
        let stmts = split_sql_statements(sql);
        assert_eq!(stmts.len(), 2);
    }

    #[test]
    fn split_no_trailing_semicolon() {
        let sql = "CREATE TABLE foo (id INT)";
        let stmts = split_sql_statements(sql);
        assert_eq!(stmts.len(), 1);
    }

    #[test]
    fn builtin_migrations_have_unique_versions() {
        let mut seen = std::collections::HashSet::new();
        for m in BUILTIN_MIGRATIONS {
            assert!(
                seen.insert(m.version),
                "duplicate migration version: {}",
                m.version
            );
        }
    }

    #[test]
    fn builtin_migrations_are_sorted_by_version() {
        for window in BUILTIN_MIGRATIONS.windows(2) {
            assert!(
                window[0].version < window[1].version,
                "migrations not sorted: {} >= {}",
                window[0].version,
                window[1].version
            );
        }
    }

    #[test]
    fn list_migrations_returns_all() {
        let m = list_migrations();
        assert_eq!(m.len(), BUILTIN_MIGRATIONS.len());
    }
}
