//! Schema migrations for the asks + consumed_receipt_index tables.
//!
//! Migrations are compile-time baked via `include_str!`. Single source of
//! truth; reproducible across builds. Cipherocto-owned schema per
//! [[stoolap-general-purpose-db]] Path B.

/// Migration table DDL + tracking table bootstrap.
pub const BOOTSTRAP_SQL: &str = include_str!("../migrations/000_bootstrap.sql");

/// Migration 001: create asks table.
pub const MIGRATION_001_SQL: &str = include_str!("../migrations/001_create_asks.sql");

/// Migration 002: create consumed_receipt_index table.
pub const MIGRATION_002_SQL: &str = include_str!("../migrations/002_create_receipt_index.sql");

/// Static list of `(version, sql)` migrations applied in order.
pub const MIGRATIONS: &[(u32, &str)] = &[(1, MIGRATION_001_SQL), (2, MIGRATION_002_SQL)];

/// Errors from migration runner.
#[derive(Debug, thiserror::Error)]
pub enum MigrationError {
    #[error("stoolap error: {0}")]
    Stoolap(String),
    #[error("migration {0} not found in static list")]
    MissingMigration(u32),
    #[error("migration version mismatch: applied {applied}, expected {expected}")]
    VersionMismatch { applied: u32, expected: u32 },
}

/// Apply pending migrations to the given stoolap database.
///
/// Idempotent: re-running on an already-migrated DB is a no-op.
///
/// `db` is the cipherocto-side wrapper around `stoolap::Database`. We
/// accept it via a thin trait to avoid leaking stoolap types into the
/// public API (Path B keeps cipherocto as the schema owner; stoolap is
/// just a SQL engine).
pub fn apply_migrations(db: &mut impl MigratableDatabase) -> Result<(), MigrationError> {
    db.execute(BOOTSTRAP_SQL)
        .map_err(|e| MigrationError::Stoolap(e.to_string()))?;

    let applied = db
        .applied_versions()
        .map_err(|e| MigrationError::Stoolap(e.to_string()))?;

    for (version, sql) in MIGRATIONS {
        if applied.contains(version) {
            continue;
        }
        db.execute(sql)
            .map_err(|e| MigrationError::Stoolap(e.to_string()))?;
        db.record_migration(*version)
            .map_err(|e| MigrationError::Stoolap(e.to_string()))?;
    }
    Ok(())
}

/// Thin trait over the stoolap database for migration operations.
///
/// Cipherocto's `StoolapStore` implements this trait. Keeps stoolap types
/// out of the migration runner API.
pub trait MigratableDatabase {
    /// Execute a SQL statement (DDL or DML).
    fn execute(&mut self, sql: &str) -> Result<(), String>;
    /// Returns the set of applied migration versions.
    fn applied_versions(&mut self) -> Result<std::collections::HashSet<u32>, String>;
    /// Records a migration as applied.
    fn record_migration(&mut self, version: u32) -> Result<(), String>;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn migrations_list_is_ordered() {
        let mut last = 0;
        for (v, _) in MIGRATIONS {
            assert!(*v > last, "migration versions must be strictly increasing");
            last = *v;
        }
    }

    #[test]
    fn migrations_sql_is_non_empty() {
        for (v, sql) in MIGRATIONS {
            assert!(!sql.trim().is_empty(), "migration {v} SQL is empty");
        }
    }

    #[test]
    fn bootstrap_sql_is_non_empty() {
        assert!(!BOOTSTRAP_SQL.trim().is_empty());
    }
}
