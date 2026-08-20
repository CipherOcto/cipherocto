//! Migration catalog for [`octo_vault`].
//!
//! Layer B (per plan §B.3 / stream B.3): owns migrations for the vault
//! substrate. Two SQL files: `v013__create_vaults.sql` (vaults table +
//! unique vault_id index per review §20.3 Model B) +
//! `v014__create_transfer_events.sql` (append-only event log per §9.3).
//!
//! Catalog numbering continues from adjacent owner crates
//! (`quota-router-storage` last = v012). The substrate's `apply_pending`
//! owns ordering; the catalog here provides a typed slice sorted by
//! `version()` ascending per `octo_storage_core::_legacy_Migration::version()`.

/// All built-in migrations in version order as `(version, name, sql)` tuples.
/// Consumed by the in-module drift-detection tests below; `pub(crate)`
/// (not re-exported from the crate root) keeps it internal to the
/// migrations module.
#[allow(dead_code)] // drift tests in mod tests; rustc counts test usage separately
pub const BUILTIN_MIGRATIONS: &[(u32, &str, &str)] = &[
    (
        13,
        "v013__create_vaults",
        include_str!("../migrations/v013__create_vaults.sql"),
    ),
    (
        14,
        "v014__create_transfer_events",
        include_str!("../migrations/v014__create_transfer_events.sql"),
    ),
];

/// Substrate-form migration catalog: `&[&'static dyn Migration]`.
pub static BUILTIN_MIGRATION_CATALOG: &[&'static dyn octo_storage_core::_legacy_Migration] = &[
    &octo_storage_core::_legacy_StaticMigration::new(
        13,
        "v013__create_vaults",
        include_str!("../migrations/v013__create_vaults.sql"),
    ),
    &octo_storage_core::_legacy_StaticMigration::new(
        14,
        "v014__create_transfer_events",
        include_str!("../migrations/v014__create_transfer_events.sql"),
    ),
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builtin_migrations_are_unique_and_versioned() {
        let mut seen = std::collections::BTreeSet::new();
        for (v, name, _sql) in BUILTIN_MIGRATIONS {
            assert!(seen.insert(*v), "duplicate migration version: {v}");
            assert!(
                name.starts_with('v'),
                "migration name must start with 'v': {name}"
            );
            assert!(
                name.contains("__"),
                "migration name must contain '__': {name}"
            );
        }
    }

    #[test]
    fn catalog_versions_match_tuple_slice() {
        let tuple_versions: Vec<u32> = BUILTIN_MIGRATIONS.iter().map(|(v, _, _)| *v).collect();
        let catalog_versions: Vec<u32> = BUILTIN_MIGRATION_CATALOG
            .iter()
            .map(|m| m.version())
            .collect();
        assert_eq!(
            tuple_versions, catalog_versions,
            "tuple slice / catalog drift"
        );
        for (v, name, _sql) in BUILTIN_MIGRATIONS {
            let catalog_name = BUILTIN_MIGRATION_CATALOG
                .iter()
                .find(|m| m.version() == *v)
                .unwrap()
                .name();
            assert_eq!(*name, catalog_name, "name mismatch for v{v}");
        }
    }

    #[test]
    fn migrations_in_lex_order() {
        for w in BUILTIN_MIGRATIONS.windows(2) {
            assert!(
                w[0].0 < w[1].0,
                "migrations must be in order: v{} should come before v{}",
                w[0].0,
                w[1].0
            );
        }
    }
}
