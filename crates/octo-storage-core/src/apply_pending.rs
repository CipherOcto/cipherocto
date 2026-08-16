//! [`apply_pending`] — unified migration runner.
//!
//! Owns the three behaviors its predecessor APIs shared individually:
//!
//! - **Idempotency** — re-running on an already-migrated DB is a no-op
//!   (guard via tracker table + sequential version guard).
//! - **Partial-application recovery** — a crashed mid-migration run that
//!   leaves ADD COLUMN statements partially applied is recoverable: see
//!   mission `0871b-storage-idempotent-alter-hardening` for the ADD
//!   COLUMN swallow rule.
//! - **Downgrade refusal** — DB at higher version than catalog →
//!   [`StorageError::UnknownMigration`] (no silent `apply_pending` on
//!   downgraded code).

use crate::error::StorageError;
use crate::migration::Migration;
use crate::sql_split::split_sql_statements;
use crate::tracker::{current_version, ensure_tracker_table, record_migration};

/// Configuration knobs for [`apply_pending`].
///
/// Defaults match the majority pre-substrate convention (`schema_migrations`
/// tracker table; idempotent ADD COLUMN swallow). Owner crates that need
/// a different tracker table name (`quota-router-storage`'s historical
/// `cipherocto_schema_version`) pass it via [`with_tracker_table`].
#[derive(Debug, Clone, Copy)]
pub struct ApplyConfig {
    /// Tracker table name. Default: [`crate::DEFAULT_TRACKER_TABLE`].
    pub tracker_table: &'static str,
}

impl Default for ApplyConfig {
    fn default() -> Self {
        Self {
            tracker_table: crate::DEFAULT_TRACKER_TABLE,
        }
    }
}

impl ApplyConfig {
    /// New config with default tracker-table name.
    pub const fn new() -> Self {
        Self {
            tracker_table: crate::DEFAULT_TRACKER_TABLE,
        }
    }

    /// Set a non-default tracker-table name.
    pub const fn with_tracker_table(mut self, name: &'static str) -> Self {
        self.tracker_table = name;
        self
    }
}

/// Apply all pending migrations from `migrations` that are newer than the
/// current database version. Idempotent: re-running is a no-op.
///
/// Migrations are executed in the order `migrations` appears in the
/// slice; each MUST carry a strictly-monotonic-increasing `version()`
/// (the runner does NOT sort — that's the caller's responsibility).
///
/// # Errors
/// - [`StorageError::Stoolap`]: tracker-table DDL / current-version read fails.
/// - [`StorageError::UnknownMigration`]: DB version exceeds catalog.
/// - [`StorageError::MigrationFailed`]: a migration's SQL failed (after
///   the ADD COLUMN swallow rule is applied — see
///   [`is_idempotent_already_applied`]). The migration is NOT recorded
///   in the tracker table; the caller may retry once the underlying
///   issue is fixed.
///
/// # Example
/// ```ignore
/// use octo_storage_core::{apply_pending, Migration, StaticMigration};
///
/// const MIGRATIONS: &[&dyn Migration] = &[
///     &StaticMigration::new(1, "create_asks", include_str!("../migrations/v001__create_asks.sql")),
///     &StaticMigration::new(2, "create_receipts", include_str!("../migrations/v002__create_receipts.sql")),
/// ];
///
/// let db = octo_storage_core::open_in_memory().unwrap();
/// apply_pending(&db, MIGRATIONS, ApplyConfig::default()).unwrap();
/// ```
pub fn apply_pending(
    db: &stoolap::Database,
    migrations: &[&'static dyn Migration],
    cfg: ApplyConfig,
) -> Result<(), StorageError> {
    ensure_tracker_table(db, cfg.tracker_table)?;
    let current = current_version(db, cfg.tracker_table)?;

    // Refuse to run if the DB has a higher version than our catalog
    // (downgrade or unknown-migration scenario).
    if let Some(highest) = migrations.iter().map(|m| m.version()).max() {
        if current > highest {
            return Err(StorageError::UnknownMigration {
                version: current,
                catalog_max: highest,
            });
        }
    }

    let mut last_applied: u32 = current;
    for slot in migrations {
        // Each `slot` is `&&'static dyn Migration` (slice of trait-object
        // refs). Deref once to get `&'static dyn Migration` — the
        // `Migration` trait's method dispatch only knows the trait object
        // type, not a reference to it.
        let migration: &'static dyn Migration = *slot;
        if migration.version() <= last_applied {
            continue;
        }
        run_one(db, cfg.tracker_table, migration)?;
        last_applied = migration.version();
    }
    Ok(())
}

/// Run a single migration. Splits SQL on `;` boundaries; each statement
/// is executed individually.
///
/// `ALTER TABLE ADD COLUMN` errors with `Error::DuplicateColumn`
/// (display: `"duplicate column"`) when the column already exists.
/// For ADD COLUMN statements only, this error is treated as a no-op
/// so a mid-`apply_pending` crash between two ADD COLUMNs of the same
/// migration does not brick the DB on retry. See mission
/// `0871b-storage-idempotent-alter-hardening`.
fn run_one(
    db: &stoolap::Database,
    tracker_table: &str,
    migration: &'static dyn Migration,
) -> Result<(), StorageError> {
    let statements = split_sql_statements(migration.sql());
    for stmt in &statements {
        match db.execute(stmt, ()) {
            Ok(_) => {}
            Err(e) => {
                let msg = format!("{e}");
                if is_idempotent_already_applied(&msg, stmt) {
                    // ADD COLUMN on a column that already exists = no-op.
                    // Migration was partially applied via a prior crash;
                    // remaining statements proceed.
                    continue;
                }
                return Err(StorageError::MigrationFailed {
                    version: migration.version(),
                    name: migration.name(),
                    message: format!("{e}: {stmt}"),
                });
            }
        }
    }
    record_migration(db, tracker_table, migration.version(), migration.name())?;
    Ok(())
}

/// Returns true when `err` represents an `ADD COLUMN` collision with a
/// pre-existing column. Restricts the swallow to ADD COLUMN statements
/// (not CREATE INDEX / CREATE TABLE — those use `IF NOT EXISTS` already)
/// AND to the fork's exact `DuplicateColumn` display string.
fn is_idempotent_already_applied(err: &str, stmt: &str) -> bool {
    let upper = stmt.to_ascii_uppercase();
    let is_add_column = upper.contains("ADD COLUMN") || upper.contains("ADD\tCOLUMN");
    is_add_column && err.contains("duplicate column")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::migration::StaticMigration;
    use crate::tracker::applied_version;

    const MIGRATIONS: &[&'static dyn Migration] = &[
        &StaticMigration::new(
            1,
            "create_x",
            "CREATE TABLE x (id INTEGER PRIMARY KEY, name TEXT NOT NULL)",
        ),
        &StaticMigration::new(2, "create_y", "CREATE TABLE y (id INTEGER PRIMARY KEY)"),
        &StaticMigration::new(
            3,
            "add_nickname_to_x",
            "ALTER TABLE x ADD COLUMN nickname TEXT; ALTER TABLE x ADD COLUMN score INTEGER",
        ),
    ];

    fn fresh_db() -> stoolap::Database {
        stoolap::Database::open_in_memory().unwrap()
    }

    #[test]
    fn applies_all_migrations_on_fresh_db() {
        let db = fresh_db();
        apply_pending(&db, MIGRATIONS, ApplyConfig::default()).unwrap();

        let applied = applied_version(&db, "schema_migrations").unwrap();
        assert_eq!(applied, [1_u32, 2, 3].into_iter().collect());

        // Sanity round-trip through table x.
        db.execute("INSERT INTO x (id, name) VALUES (1, 'alice')", ())
            .unwrap();
        let rows = db.query("SELECT name FROM x WHERE id = 1", ()).unwrap();
        for row in rows.into_iter() {
            let row = row.unwrap();
            let name: String = row.get(0).unwrap();
            assert_eq!(name, "alice");
        }
    }

    #[test]
    fn partially_applied_db_picks_up_remaining_migrations() {
        let db = fresh_db();
        // Simulate a partially-applied DB: v1 (create_x) was actually
        // applied (table exists, recorded), v2 + v3 not yet.
        ensure_tracker_table(&db, "schema_migrations").unwrap();
        db.execute(
            "CREATE TABLE x (id INTEGER PRIMARY KEY, name TEXT NOT NULL)",
            (),
        )
        .unwrap();
        crate::tracker::record_migration(&db, "schema_migrations", 1, "create_x").unwrap();

        // apply_pending now applies v2 (create_y) + v3 (ADD COLUMNs),
        // skipping v1 because the table already exists AND it's recorded.
        apply_pending(&db, MIGRATIONS, ApplyConfig::default()).unwrap();

        let applied = applied_version(&db, "schema_migrations").unwrap();
        assert_eq!(applied, [1_u32, 2, 3].into_iter().collect());

        // v3's ALTER TABLE ADD COLUMN (nickname) plus the second ADD COLUMN
        // (score) are now applied + recorded.
        db.execute(
            "INSERT INTO x (id, name, nickname, score) VALUES (5, 'bob', 'B', 99)",
            (),
        )
        .unwrap();
    }

    #[test]
    fn idempotent_second_call_is_noop() {
        let db = fresh_db();
        apply_pending(&db, MIGRATIONS, ApplyConfig::default()).unwrap();
        apply_pending(&db, MIGRATIONS, ApplyConfig::default()).unwrap();

        let applied = applied_version(&db, "schema_migrations").unwrap();
        assert_eq!(applied, [1_u32, 2, 3].into_iter().collect());
    }

    #[test]
    fn rejects_downgrade_db() {
        let db = fresh_db();
        apply_pending(&db, MIGRATIONS, ApplyConfig::default()).unwrap();

        // Manually record a higher version (simulating a newer DB than our catalog).
        db.execute(
            "INSERT INTO schema_migrations (version, name, applied_at_unix) VALUES (999, 'future', 0)",
            (),
        )
        .unwrap();

        let err = apply_pending(&db, MIGRATIONS, ApplyConfig::default()).unwrap_err();
        match err {
            StorageError::UnknownMigration {
                version,
                catalog_max,
            } => {
                assert_eq!(version, 999);
                assert_eq!(catalog_max, 3);
            }
            other => panic!("expected UnknownMigration, got {other:?}"),
        }
    }

    #[test]
    fn custom_tracker_table_name_used() {
        let db = fresh_db();
        let cfg = ApplyConfig::default().with_tracker_table("quasi_x_schema");
        apply_pending(&db, MIGRATIONS, cfg).unwrap();

        // Default tracker table is empty.
        ensure_tracker_table(&db, crate::DEFAULT_TRACKER_TABLE).unwrap();
        assert_eq!(
            current_version(&db, crate::DEFAULT_TRACKER_TABLE).unwrap(),
            0
        );
        // Custom tracker table has the records.
        assert_eq!(current_version(&db, "quasi_x_schema").unwrap(), 3);
    }

    #[test]
    fn partial_apply_recovers_when_add_column_already_exists() {
        // v3 is two ADD COLUMN statements. Pre-create the first column so the
        // "duplicate column" path is exercised.
        let db = fresh_db();
        ensure_tracker_table(&db, "schema_migrations").unwrap();
        // Apply v1 manually + create the table x with both pre-existing columns,
        // simulating v3 partially-applied-before-crash state.
        db.execute(
            "CREATE TABLE x (id INTEGER PRIMARY KEY, name TEXT NOT NULL, nickname TEXT)",
            (),
        )
        .unwrap();
        crate::tracker::record_migration(&db, "schema_migrations", 1, "create_x").unwrap();

        // apply_pending must: skip v1 (recorded, table exists), apply v2
        // (CREATE TABLE y), then run v3's ADD COLUMNs — first statement
        // (nickname) collides with existing column, gets swallowed; second
        // ADD COLUMN (score) proceeds; v3 finally recorded.
        apply_pending(&db, MIGRATIONS, ApplyConfig::default()).unwrap();

        let applied = applied_version(&db, "schema_migrations").unwrap();
        assert_eq!(applied, [1_u32, 2, 3].into_iter().collect());

        // Both columns must be usable.
        db.execute(
            "INSERT INTO x (id, name, nickname, score) VALUES (7, 'eve', 'N', 1)",
            (),
        )
        .unwrap();
    }
}
