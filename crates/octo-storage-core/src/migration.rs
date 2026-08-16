//! [`Migration`] trait + [`StaticMigration`] zero-erased newtype.

use std::fmt::Debug;

/// A versioned, named SQL migration.
///
/// Implementations are typically `&'static` so they live in a const slice
/// (`BUILTIN_MIGRATIONS: &[&'static dyn Migration] = &[...&Foo, &Bar]`).
/// All three required methods return `&'static str` / `u32` — never
/// owned strings, never allocations.
///
/// Per `cipherocto-design-principles` Layer A row, this trait is
/// years-stable; new fields require a `MigrationV2` (see crate docs).
pub trait Migration: Send + Sync + Debug {
    /// Monotonically increasing version (`1..=u32::MAX`). Strictly
    /// increasing across the slice of migrations registered with
    /// [`crate::apply_pending`].
    fn version(&self) -> u32;

    /// Human-readable short name (`"create_asks"`, `"add_chain_id_namespace"`).
    /// Stable across releases; used as the `name` column value in the
    /// tracker table for diagnostics.
    fn name(&self) -> &'static str;

    /// SQL to execute. May contain multiple statements separated by `;`.
    /// Line comments (`-- ... \n`) are stripped by the runner; SQL
    /// itself is unchanged.
    fn sql(&self) -> &'static str;
}

/// Zero-erased newtype around a `Migration`. The most common shape
/// (matches `quota-router-storage` + `octo-reputation` + `quota-router-sm-engine`
/// pre-substrate conventions). Use `&StaticMigration` in
/// `BUILTIN_MIGRATIONS`.
//
// `include_str!` is the canonical way to embed a SQL file:
///
/// ```ignore
/// StaticMigration::new(3, "create_x", include_str!("../migrations/v003__create_x.sql"));
/// ```
#[derive(Debug)]
pub struct StaticMigration {
    /// Version number.
    version: u32,
    /// Migration name.
    name: &'static str,
    /// SQL body.
    sql: &'static str,
}

impl StaticMigration {
    /// Construct a new migration from `version`, `name`, and `sql`.
    pub const fn new(version: u32, name: &'static str, sql: &'static str) -> Self {
        Self { version, name, sql }
    }
}

impl Migration for StaticMigration {
    fn version(&self) -> u32 {
        self.version
    }
    fn name(&self) -> &'static str {
        self.name
    }
    fn sql(&self) -> &'static str {
        self.sql
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn static_migration_exposes_all_three_methods() {
        let m = StaticMigration::new(1, "create_asks", "CREATE TABLE asks (id INT);");
        assert_eq!(m.version(), 1);
        assert_eq!(m.name(), "create_asks");
        assert_eq!(m.sql(), "CREATE TABLE asks (id INT);");
    }

    #[test]
    fn static_migration_is_const_constructible() {
        const M: StaticMigration = StaticMigration::new(2, "x", "SELECT 1;");
        assert_eq!(M.version(), 2);
    }

    #[test]
    fn trait_object_form_compiles() {
        // Confirms the `dyn Migration` surface used by `apply_pending` is object-safe.
        fn _assert_object_safe(_m: &dyn Migration) {}
        let m = StaticMigration::new(3, "y", "");
        _assert_object_safe(&m);
    }

    #[test]
    fn debug_includes_fields() {
        let m = StaticMigration::new(7, "named", "SELECT 1");
        let s = format!("{m:?}");
        assert!(s.contains("7"));
        assert!(s.contains("named"));
    }
}
