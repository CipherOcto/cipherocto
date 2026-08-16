//! Schema migrations for the asks + consumed_receipt_index tables.
//!
//! Migrations are compile-time baked via `include_str!`. Single source of
//! truth; reproducible across builds. Cipherocto-owned schema per
//! [[stoolap-general-purpose-db]] Path B.
//!
//! Layer B (mission `octo-storage-split` S2): the underlying migration
//! runner is the Layer A substrate `octo_storage_core::apply_pending`.
//! The custom `MigratableDatabase` trait that historically shimmed the
//! Layer A interface is gone — substrate takes `&stoolap::Database`
//! directly, so the trait-and-impl shim is unnecessary indirection.

/// Migration table DDL + tracking table bootstrap.
pub const BOOTSTRAP_SQL: &str = include_str!("../migrations/000_bootstrap.sql");

/// Migration 001: create asks table.
pub const MIGRATION_001_SQL: &str = include_str!("../migrations/001_create_asks.sql");

/// Migration 002: create consumed_receipt_index table.
pub const MIGRATION_002_SQL: &str = include_str!("../migrations/002_create_receipt_index.sql");

/// Migration 003: per-shard event log (RFC-0963 §7).
pub const MIGRATION_003_SQL: &str = include_str!("../migrations/003_create_events_shard.sql");

/// Migration 004: shard registry + migration log (RFC-0963 §7 R3-F7/R4-F6).
pub const MIGRATION_004_SQL: &str = include_str!("../migrations/004_create_shard_registry.sql");

/// Migration 005: policy catalog (RFC-0967 §8).
pub const MIGRATION_005_SQL: &str = include_str!("../migrations/005_create_policy_catalog.sql");

/// Migration 006: consumed envelope index (RFC-0962 §6.3).
pub const MIGRATION_006_SQL: &str = include_str!("../migrations/006_create_consumed_envelopes.sql");

/// Static list of `(version, sql)` migrations applied in order.
pub const MIGRATIONS: &[(u32, &str)] = &[
    (1, MIGRATION_001_SQL),
    (2, MIGRATION_002_SQL),
    (3, MIGRATION_003_SQL),
    (4, MIGRATION_004_SQL),
    (5, MIGRATION_005_SQL),
    (6, MIGRATION_006_SQL),
];

/// Substrate-form migration catalog: numeric versions + canonical `v<NNN>`
/// labels, suitable for `octo_storage_core::apply_pending`. The labels
/// follow the substrate's convention so ops tooling that reads back
/// the `name` column from `cipherocto_migrations` sees stable
/// `v001__create_asks`-style strings.
pub(super) static BUILTIN_MIGRATION_CATALOG: &[&'static dyn octo_storage_core::Migration] = &[
    &octo_storage_core::StaticMigration::new(1, "v001__create_asks", MIGRATION_001_SQL),
    &octo_storage_core::StaticMigration::new(2, "v002__create_receipt_index", MIGRATION_002_SQL),
    &octo_storage_core::StaticMigration::new(3, "v003__create_events_shard", MIGRATION_003_SQL),
    &octo_storage_core::StaticMigration::new(4, "v004__create_shard_registry", MIGRATION_004_SQL),
    &octo_storage_core::StaticMigration::new(5, "v005__create_policy_catalog", MIGRATION_005_SQL),
    &octo_storage_core::StaticMigration::new(
        6,
        "v006__create_consumed_envelopes",
        MIGRATION_006_SQL,
    ),
];

/// Apply pending migrations to the given stoolap database.
///
/// Idempotent: re-running on an already-migrated DB is a no-op.
///
/// Thin delegation to the Layer A substrate
/// ([`octo_storage_core::apply_pending`]). The substrate's
/// `ensure_tracker_table` brings the historical
/// `cipherocto_migrations(version PK, applied_at)` table into the
/// substrate-friendly shape in-place (`name`, `applied_at_unix` columns
/// added; back-fill by step is implicitly safe because the substrate
/// only reads `MAX(version)` from the existing PK column and writes its
/// own marker rows via `name + applied_at_unix` — both columns ignored
/// by this crate's other queries that SELECT only `version`).
pub fn apply_migrations(db: &stoolap::Database) -> Result<(), octo_storage_core::StorageError> {
    octo_storage_core::apply_pending(
        db,
        BUILTIN_MIGRATION_CATALOG,
        octo_storage_core::ApplyConfig::default().with_tracker_table("cipherocto_migrations"),
    )
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
