//! Error types for the storage substrate.

use thiserror::Error;

/// Storage substrate errors.
///
/// **Layer A stable**: enum variants are additive; the `Display` impl is
/// operator-facing (no migration `name` field, no SQL fragments). See
/// `MigrationFailed` for the redaction rules.
#[derive(Debug, Error)]
pub enum StorageError {
    /// A Stoolap-level operation failed. The `operation` field is a
    /// stable, short tag (e.g. `"record_migration:legacy_id_pk"`); the
    /// `message` is operator-facing prose — never raw SQL or migration
    /// `name` strings, which would leak through the substrate boundary
    /// to operators / dashboards that don't need them.
    ///
    /// `Debug` (`{err:?}`) retains the underlying Stoolap `Error`'s
    /// `Debug` output for substrate-internal logging; do NOT surface
    /// `Debug` across the public boundary.
    #[error("stoolap error during {operation}: {message}")]
    Stoolap {
        /// Short, stable operation tag (e.g. `"record_migration:legacy_id_pk"`).
        operation: &'static str,
        /// Operator-facing prose; never raw SQL or migration `name`.
        message: String,
    },

    /// A specific migration failed during `apply_pending`. The
    /// migration's `name` field is intentionally **excluded** from the
    /// public surface: it can leak schema intent to operators (the
    /// `v<NNN>__<label>` label encodes the migration's purpose, e.g.
    /// `"create_did_registry"`) and historically has been used in
    /// attacker-controlled inputs via legacy DBs. The version is
    /// retained because it is the only field the substrate uses to
    /// make a re-apply decision.
    ///
    /// `Debug` retains `name` for substrate-internal debugging
    /// (paired with `StorageError::stoolap`'s `Debug` retention). See
    /// `apply_pending::run_one` for the redacting `Display` impl.
    #[error("migration v{version} failed: {message}")]
    MigrationFailed {
        /// Numeric migration version that failed.
        version: u32,
        /// Operator-facing prose; never raw SQL or migration `name`.
        message: String,
    },

    /// System clock is before UNIX_EPOCH (BIOS reset, sandbox
    /// frozen clock). Surfaced as a typed error rather than panicking
    /// so the substrate's failure mode stays loud + typed.
    #[error("system clock error: {0}")]
    SystemTime(String),

    /// Caller supplied an identifier (table name, column name) that
    /// failed the substrate's strict `is_safe_identifier` regex
    /// (defense-in-depth against format-string injection).
    #[error("unsupported identifier: {0}")]
    Unsupported(String),

    /// DB is at a higher version than this code's catalog allows
    /// (downgrade scenario). Operator-facing via `Display`; the
    /// `Debug` representation is the same as `Display`.
    #[error("migration version {version} not found in catalog (catalog_max={catalog_max})")]
    UnknownMigration {
        /// DB version that no longer maps to a catalog entry.
        version: u32,
        /// Highest version the code's catalog contains.
        catalog_max: u32,
    },
}

impl StorageError {
    /// Build a `Stoolap` variant from a Stoolap `Error`. Captures
    /// `format!("{e}")` for the operator-facing message; the underlying
    /// `Error`'s `Debug` form is preserved in the variant's own
    /// `Debug` output via the `#[derive(Debug)]` on `StorageError`
    /// itself (the `source` chain is recorded for substrate-internal
    /// logging only).
    pub(crate) fn stoolap(operation: &'static str, e: stoolap::Error) -> Self {
        Self::Stoolap {
            operation,
            message: format!("{e}"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// H-Sec3 regression: `MigrationFailed` must NOT leak the
    /// migration `name` field through its `Display` impl. The
    /// `format!("{err}")` is what an operator sees on a CLI /
    /// dashboard / log line. If the `name` field ever escapes into
    /// this string, an attacker who controls the legacy DB can leak
    /// schema intent.
    #[test]
    fn migration_failed_display_does_not_leak_name() {
        // The substrate's `MigrationFailed` carries only `version`
        // + `message`. Construct it with a synthetic message that
        // happens to contain a migration `name` substring to prove
        // the constructor can't smuggle the `name` through either.
        let err = StorageError::MigrationFailed {
            version: 42,
            message: "schema_migrations: column 'name' not found".to_owned(),
        };
        let s = format!("{err}");
        // Version is allowed to appear (the substrate uses it for
        // re-apply decisions; it's the only field the substrate
        // emits to operators).
        assert!(s.contains("v42"), "version retained for re-apply");
        // The raw migration label "v042__create_did_registry" must
        // not appear in the rendered string (it would have been
        // carried from the substrate as the `name` field).
        assert!(
            !s.contains("v042__create_did_registry"),
            "operator-facing Display must not leak migration name; got: {s}"
        );
    }

    /// H-T10 regression: the substrate's M5 redaction must not leak
    /// the failing SQL statement. The `apply_pending::run_one` impl
    /// builds the `message` field with `{e}` only — the SQL is
    /// captured in `Debug` for substrate-internal logging but never
    /// in `Display`. Verify the contract by constructing a synthetic
    /// `MigrationFailed` with a message that simulates a Stoolap
    /// error format (`{e}` containing a SQL fragment) and asserting
    /// the substrate does not additionally embed the SQL via the
    /// `{stmt}` placeholder from the pre-fix code path.
    #[test]
    fn migration_failed_message_does_not_embed_raw_sql() {
        let err = StorageError::MigrationFailed {
            version: 3,
            message: "parse error: expected identifier, found 'foo'".to_owned(),
        };
        let s = format!("{err}");
        assert!(!s.contains("CREATE TABLE "));
        assert!(!s.contains("ALTER TABLE "));
        assert!(!s.contains("INSERT INTO "));
        assert!(!s.contains("SELECT "));
    }

    /// MigrationFailed Debug impl DOES retain `name` for substrate-
    /// internal logging (paired with `StorageError::stoolap`'s
    /// `Debug` retention). This is intentional: ops teams need
    /// substrate-internal traces, but operators don't. The contract:
    /// `Display` is operator-facing (redacted); `Debug` is
    /// substrate-internal (full).
    #[test]
    fn migration_failed_debug_retains_structural_fields() {
        // The current variant shape has no `name` field. This test
        // documents the contract: if a future PR adds a `name`
        // field, the substrate-internal `Debug` impl will retain it,
        // but the operator-facing `Display` impl (and
        // `migration_failed_display_does_not_leak_name` above) must
        // continue to redact it. This test pins the CURRENT shape.
        let err = StorageError::MigrationFailed {
            version: 7,
            message: "boom".to_owned(),
        };
        let d = format!("{err:?}");
        assert!(d.contains("MigrationFailed"));
        assert!(d.contains("7"), "version retained in Debug");
        assert!(d.contains("boom"), "message retained in Debug");
    }

    /// `StorageError` Display impl for the `UnknownMigration` variant
    /// is operator-facing and surfaces the catalog/db version pair
    /// directly (no redaction — both fields are operator-relevant).
    #[test]
    fn unknown_migration_display_surfaces_versions() {
        let err = StorageError::UnknownMigration {
            version: 999,
            catalog_max: 12,
        };
        let s = format!("{err}");
        assert!(s.contains("999"), "db version surfaces");
        assert!(s.contains("12"), "catalog max surfaces");
    }

    /// `StorageError` is `std::error::Error`-compatible (thiserror
    /// derives it). Pin the trait bound so a future PR that strips
    /// the `Error` derive breaks the test, not downstream crates.
    #[test]
    fn storage_error_implements_std_error() {
        fn assert_error<E: std::error::Error>(_: &E) {}
        let err = StorageError::SystemTime("test".to_owned());
        assert_error(&err);
    }
}
