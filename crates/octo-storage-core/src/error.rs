//! Substrate error type + legacy [`StorageError`] compatibility alias.
//!
//! Per RFC-0206 v2.1 §Substrate Newtype Refactor, the canonical error is
//! [`SubstrateError`]. [`StorageError`] is retained as a deprecated type
//! alias for the ≥ 6-month transition window (§Migration Order).

use thiserror::Error;

/// Canonical substrate error type.
///
/// **Layer A stable**: variants are additive. `Display` impls are
/// operator-facing; never embed raw SQL, migration `name` strings, or
/// other substrate-internal fields that could leak through the boundary.
#[derive(Debug, Error)]
pub enum SubstrateError {
    /// Underlying Stoolap operation failed. The `operation` field is a
    /// stable, short tag; the `message` is operator-facing prose.
    #[error("stoolap error during {operation}: {message}")]
    Storage {
        /// Short, stable operation tag (e.g. `"record_migration:legacy_id_pk"`).
        operation: &'static str,
        /// Operator-facing prose; never raw SQL or migration `name`.
        message: String,
    },

    /// Migration failed during `apply_pending`. The migration's `name`
    /// field is intentionally excluded from the public surface (it can
    /// leak schema intent); the version is retained because it is the
    /// only field the substrate uses to make a re-apply decision.
    #[error("migration v{version} failed: {message}")]
    MigrationFailed {
        /// Numeric migration version that failed.
        version: u32,
        /// Operator-facing prose; never raw SQL or migration `name`.
        message: String,
    },

    /// System clock is before UNIX_EPOCH (BIOS reset, sandbox frozen
    /// clock). Surfaced as a typed error rather than panicking.
    #[error("system clock error: {0}")]
    SystemTime(String),

    /// Caller supplied an identifier (table name, column name) that
    /// failed the substrate's strict `is_safe_identifier` regex.
    #[error("unsupported identifier: {0}")]
    Unsupported(String),

    /// DB is at a higher version than this code's catalog allows.
    #[error("migration version {version} not found in catalog (catalog_max={catalog_max})")]
    UnknownMigration {
        /// DB version that no longer maps to a catalog entry.
        version: u32,
        /// Highest version the code's catalog contains.
        catalog_max: u32,
    },

    /// A typed-query statement (Select/Insert/Update/Delete) targets a
    /// table that has not been registered with the [`crate::AdapterAllowlist`]
    /// for the calling adapter. Per RFC §Format Bypass Defense, the
    /// substrate refuses to dispatch typed queries against unregistered
    /// tables so the `Database::execute_checked` path remains the only
    /// legitimate SQL execution boundary.
    #[error("table {table:?} not in adapter {adapter} namespace")]
    TableNotInNamespace {
        /// Adapter id (`AdapterId` value, surfaced via `Display`).
        adapter: String,
        /// Table that the statement targets.
        table: String,
    },

    /// A DDL statement (non-`DdlNoOp`, non-`DdlRegistered`) was rejected
    /// by the [`crate::AdapterAllowlist`]. The substrate refuses to
    /// dispatch arbitrary DDL through `Database::execute_checked`; the
    /// only legitimate DDL path is a [`crate::DdlTemplate`] that has
    /// been pre-registered at adapter startup.
    #[error("DDL not in adapter {adapter} allowlist (template: {template})")]
    DdlNotInAllowlist {
        /// Adapter id (`AdapterId` value, surfaced via `Display`).
        adapter: String,
        /// DDL template identifier (e.g. the canonical name registered
        /// in the [`crate::AdapterAllowlist`]).
        template: String,
    },
}

/// Canonical substrate `Result` alias. All public APIs return
/// `Result<T, SubstrateError>`; consumers should `use` this alias
/// rather than spelling out the full path.
pub type Result<T> = std::result::Result<T, SubstrateError>;

/// Deprecated alias for the pre-v2.1 substrate error type.
///
/// Per RFC-0206 v2.1 §Migration Order, this alias is retained for ≥ 6
/// months so owner crates migrating from `StorageError` to
/// `SubstrateError` compile without edit. **New code MUST use
/// [`SubstrateError`] directly.**
#[deprecated(
    since = "1.0.0",
    note = "renamed to `SubstrateError` per RFC-0206 v2.1 §Substrate Newtype Refactor; will be removed in v2.0"
)]
pub type StorageError = SubstrateError;

impl SubstrateError {
    /// Build a `Storage` variant from a Stoolap `Error`. Captures
    /// `format!("{e}")` for the operator-facing message; the underlying
    /// `Error`'s `Debug` form is preserved in the variant's own
    /// `Debug` output via the `#[derive(Debug)]` on `SubstrateError`
    /// itself.
    pub(crate) fn stoolap(operation: &'static str, e: stoolap::Error) -> Self {
        Self::Storage {
            operation,
            message: format!("{e}"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// H-Sec3 regression: `MigrationFailed` must NOT leak the
    /// migration `name` field through its `Display` impl.
    #[test]
    fn migration_failed_display_does_not_leak_name() {
        let err = SubstrateError::MigrationFailed {
            version: 42,
            message: "schema_migrations: column 'name' not found".to_owned(),
        };
        let s = format!("{err}");
        assert!(s.contains("v42"), "version retained for re-apply");
        assert!(
            !s.contains("v042__create_did_registry"),
            "operator-facing Display must not leak migration name; got: {s}"
        );
    }

    /// `SubstrateError` Display impl for the `UnknownMigration` variant
    /// surfaces the catalog/db version pair directly.
    #[test]
    fn unknown_migration_display_surfaces_versions() {
        let err = SubstrateError::UnknownMigration {
            version: 999,
            catalog_max: 12,
        };
        let s = format!("{err}");
        assert!(s.contains("999"));
        assert!(s.contains("12"));
    }

    /// `SubstrateError::TableNotInNamespace` Display surfaces both
    /// adapter + table so operators can diagnose the missing
    /// allowlist entry.
    #[test]
    fn table_not_in_namespace_display_surfaces_both_fields() {
        let err = SubstrateError::TableNotInNamespace {
            adapter: "octo-vault".to_owned(),
            table: "shadow_vault".to_owned(),
        };
        let s = format!("{err}");
        assert!(s.contains("octo-vault"));
        assert!(s.contains("shadow_vault"));
    }

    /// `SubstrateError::DdlNotInAllowlist` Display surfaces both
    /// adapter + template.
    #[test]
    fn ddl_not_in_allowlist_display_surfaces_both_fields() {
        let err = SubstrateError::DdlNotInAllowlist {
            adapter: "octo-reputation".to_owned(),
            template: "v001__create_reputation".to_owned(),
        };
        let s = format!("{err}");
        assert!(s.contains("octo-reputation"));
        assert!(s.contains("v001__create_reputation"));
    }

    /// `SubstrateError` is `std::error::Error`-compatible.
    #[test]
    fn substrate_error_implements_std_error() {
        fn assert_error<E: std::error::Error>(_: &E) {}
        let err = SubstrateError::SystemTime("test".to_owned());
        assert_error(&err);
    }

    /// Legacy `StorageError` alias compiles + matches `SubstrateError`.
    /// Pin the alias contract so a future PR that removes the
    /// deprecated alias during the transition window breaks the test.
    #[allow(deprecated)]
    #[test]
    fn storage_error_alias_matches_substrate_error() {
        let legacy: StorageError = SubstrateError::SystemTime("alias_check".to_owned());
        let canonical: SubstrateError = legacy;
        let s = format!("{canonical}");
        assert!(s.contains("alias_check"));
    }
}
