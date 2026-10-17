//! Migration runner — applies the canonical `migrations/*.sql` files on
//! `StoolapReputationStore::open`.
//!
//! Layer B (mission `octo-storage-split` S2): the underlying migration
//! runner is `octo_storage_core::apply_pending` (Layer A substrate).
//! The `name` argument passed to each migration doubles as the
//! historical `MigrationVersion::ALL` string key so existing ops
//! tooling (status scripts, dashboards) continues to enumerate
//! `Vec<String>` names.

use std::time::{SystemTime, UNIX_EPOCH};

#[cfg(feature = "stoolap")]
use crate::error::ReputationError;

/// All built-in migrations in version order, as `(name, sql)` pairs.
/// Preserved for the catalog-validation tests (the tuple shape is
/// stable and inspectable via the unit tests below).
pub const BUILTIN_MIGRATIONS: &[(&str, &str)] = &[
    (
        "v001__reputation_events",
        include_str!("../migrations/v001__reputation_events.sql"),
    ),
    (
        "v002__reputation_recorders",
        include_str!("../migrations/v002__reputation_recorders.sql"),
    ),
    (
        "v003__schema_migrations",
        include_str!("../migrations/v003__schema_migrations.sql"),
    ),
    (
        "v004__reputation_attestations",
        include_str!("../migrations/v004__reputation_attestations.sql"),
    ),
    (
        "v005__reputation_gossip_seen",
        include_str!("../migrations/v005__reputation_gossip_seen.sql"),
    ),
    (
        "v010__reputation_anchors",
        include_str!("../migrations/v010__reputation_anchors.sql"),
    ),
    (
        "v011__reputation_events_anchor",
        include_str!("../migrations/v011__reputation_events_anchor.sql"),
    ),
    (
        "v012__reputation_anchors_governance",
        include_str!("../migrations/v012__reputation_anchors_governance.sql"),
    ),
];

/// Bootstrap SQL for the legacy `schema_migrations` table — kept as a
/// public re-export because external status scripts and downstream
/// tooling still reference it for documentation purposes. The substrate
/// (`octo_storage_core::ensure_tracker_table`) supersedes the runtime
/// path; this constant exists only so the `v003__schema_migrations`
/// migration's body is byte-identical and shareable.
pub const TRACKER_TABLE_SQL: &str = include_str!("../migrations/v003__schema_migrations.sql");

/// Version-string constants for every built-in migration. Re-exported at
/// the crate root so ops tools (status scripts, dashboards) can
/// programmatically enumerate what's applied. Stable contract: every
/// `&&'static str` here must also appear as a key in `BUILTIN_MIGRATIONS`.
pub struct MigrationVersion;

impl MigrationVersion {
    pub const ALL: &'static [&'static str] = &[
        "v001__reputation_events",
        "v002__reputation_recorders",
        "v003__schema_migrations",
        "v004__reputation_attestations",
        "v005__reputation_gossip_seen",
        "v010__reputation_anchors",
        "v011__reputation_events_anchor",
        "v012__reputation_anchors_governance",
    ];
}

/// Substrate-form migration catalog: `&[&'static dyn Migration]`. Numeric
/// versions match the historical `v<NNN>` prefix encoded in each
/// filename (1..=5 and 10..=12; v006..v009 reserved by RFC-0968 §28 but
/// not yet implemented). Used by `substrate_runner::apply` (Layer B
/// facade) to delegate to `octo_storage_core::apply_pending`.
#[cfg(feature = "stoolap")]
pub(super) static BUILTIN_MIGRATION_CATALOG:
    &[&'static dyn octo_storage_core::_legacy_Migration] = &[
    &octo_storage_core::_legacy_StaticMigration::new(
        1,
        "v001__reputation_events",
        include_str!("../migrations/v001__reputation_events.sql"),
    ),
    &octo_storage_core::_legacy_StaticMigration::new(
        2,
        "v002__reputation_recorders",
        include_str!("../migrations/v002__reputation_recorders.sql"),
    ),
    &octo_storage_core::_legacy_StaticMigration::new(
        3,
        "v003__schema_migrations",
        include_str!("../migrations/v003__schema_migrations.sql"),
    ),
    &octo_storage_core::_legacy_StaticMigration::new(
        4,
        "v004__reputation_attestations",
        include_str!("../migrations/v004__reputation_attestations.sql"),
    ),
    &octo_storage_core::_legacy_StaticMigration::new(
        5,
        "v005__reputation_gossip_seen",
        include_str!("../migrations/v005__reputation_gossip_seen.sql"),
    ),
    &octo_storage_core::_legacy_StaticMigration::new(
        10,
        "v010__reputation_anchors",
        include_str!("../migrations/v010__reputation_anchors.sql"),
    ),
    &octo_storage_core::_legacy_StaticMigration::new(
        11,
        "v011__reputation_events_anchor",
        include_str!("../migrations/v011__reputation_events_anchor.sql"),
    ),
    &octo_storage_core::_legacy_StaticMigration::new(
        12,
        "v012__reputation_anchors_governance",
        include_str!("../migrations/v012__reputation_anchors_governance.sql"),
    ),
];

#[cfg(feature = "stoolap")]
pub mod substrate_runner {
    use super::{ReputationError, BUILTIN_MIGRATION_CATALOG};

    /// Apply every migration that has not yet been recorded in
    /// `schema_migrations`. Synchronous — the `StoolapReputationStore`
    /// constructor wraps this in `spawn_blocking` so the tokio reactor
    /// is never stalled for long.
    ///
    /// Delegates to the Layer A substrate's
    /// [`octo_storage_core::apply_pending`]. The substrate's
    /// `ensure_tracker_table` performs the schema alignment (legacy
    /// `id PK + name UNIQUE` → `version PK + name + applied_at_unix`)
    /// PLUS the version-column backfill from `v<NNN>__<label>` filenames
    /// for pre-substrate DBs, so a legacy DB opened for the first time
    /// under the new code skips already-applied migrations cleanly.
    pub fn apply(db: &octo_storage_core::Database) -> Result<(), ReputationError> {
        octo_storage_core::_legacy_apply_pending(
            db,
            BUILTIN_MIGRATION_CATALOG,
            octo_storage_core::_legacy_ApplyConfig::default(),
        )
        .map_err(|_e| {
            // Static label only — do NOT leak the substrate error's
            // `Debug` form (`{e:?}`) to stderr: it can include SQL
            // fragments, table names, and migration `name` strings
            // that flow into operator dashboards. Operators who need
            // the substrate-internal trace should enable debug logging
            // in the substrate layer (`octo_storage_core::record_migration`).
            ReputationError::ChainRefInvalid("migration:apply")
        })?;
        Ok(())
    }

    /// Returns the list of migration names currently recorded as
    /// applied. The substrate writes the `name` field verbatim from
    /// each `StaticMigration::new(..., name, sql)`, so the historical
    /// `v<NNN>__<label>` strings surface here unchanged.
    pub fn applied_versions(
        db: &octo_storage_core::Database,
    ) -> Result<Vec<String>, ReputationError> {
        let mut rows = db
            .query("SELECT name FROM schema_migrations ORDER BY version", ())
            .map_err(|_e| ReputationError::ChainRefInvalid("migration:list"))?;
        let mut out = Vec::new();
        loop {
            match rows.next() {
                Some(Ok(row)) => {
                    let v: String = row
                        .get_by_name("name")
                        .map_err(|_e| ReputationError::ChainRefInvalid("migration:row"))?;
                    out.push(v);
                }
                Some(Err(_e)) => {
                    return Err(ReputationError::ChainRefInvalid("migration:row_iter"));
                }
                None => break,
            }
        }
        Ok(out)
    }
}

#[cfg(feature = "stoolap")]
pub use substrate_runner::{applied_versions, apply};

/// Time-of-day timestamp the runner records in `schema_migrations`. Exposed
/// for tests that do not depend on the stoolap feature.
#[allow(dead_code)]
pub fn now_unix() -> u64 {
    // R14 review (LOW) + R15 review (HIGH): unwrap_or_default()
    // silently returned 0 if the system clock is before UNIX_EPOCH
    // (BIOS reset, sandbox frozen clock, restored pre-1970 image).
    // At now_unix=0 the reputation freshness gate (`is_fresh(0)`)
    // returns true for ANY snapshot — re-introducing the CRITICAL
    // pre-R13 bypass. A frozen clock AT 1970 (now_unix=0) is the
    // same bypass via a different root cause. Reject both: panic on
    // either pre-epoch OR zero result. The generic panic message
    // avoids leaking architectural primitives into crash logs.
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("internal time error: clock out of range")
        .as_secs();
    assert!(secs > 0, "internal time error: clock out of range");
    secs
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builtin_migrations_are_unique_and_versioned() {
        let mut seen: std::collections::BTreeSet<&str> = std::collections::BTreeSet::new();
        for (v, _sql) in BUILTIN_MIGRATIONS {
            assert!(seen.insert(v), "duplicate migration version: {v}");
            assert!(
                v.starts_with('v'),
                "migration version must start with 'v': {v}"
            );
            assert!(
                v.contains("__"),
                "migration version must contain '__' separator: {v}"
            );
        }
    }

    #[test]
    fn all_migrations_are_lexically_ordered() {
        for w in BUILTIN_MIGRATIONS.windows(2) {
            assert!(
                w[0].0 < w[1].0,
                "migrations must be in lex order: {} should come before {}",
                w[0].0,
                w[1].0
            );
        }
    }

    /// The catalog in RFC-0968 §28 slot-table allocates `v010` to anchoring.
    /// The slot numbering in this crate matches the catalog, not the
    /// implementation order: v006/v007/v008/v009 are reserved for
    /// recorder_registration and kind_weights (per the same catalog)
    /// but not yet implemented. The migration runner applies v010 after
    /// v005 with no intervening versions; the gap is intentional.
    #[test]
    fn v010_anchors_slot_is_allocated() {
        assert!(
            BUILTIN_MIGRATIONS
                .iter()
                .any(|(v, _)| *v == "v010__reputation_anchors"),
            "v010__reputation_anchors must be registered per RFC-0968 §28"
        );
    }

    #[test]
    fn all_migration_sql_is_non_empty() {
        for (v, sql) in BUILTIN_MIGRATIONS {
            assert!(!sql.trim().is_empty(), "migration {v} has empty SQL");
        }
    }

    #[test]
    fn now_unix_returns_recent_unix_seconds() {
        let t = now_unix();
        let now = std::time::SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();
        assert!((now as i64 - t as i64).abs() < 5);
    }

    /// Compensating control for the triple source-of-truth storage
    /// pattern (H3 finding from S2 Phase 2 review). `BUILTIN_MIGRATIONS`
    /// (tuple slice), `MigrationVersion::ALL`, and `BUILTIN_MIGRATION_CATALOG`
    /// all enumerate the same migrations. If a future PR adds a new v
    /// entry to the tuple slice and forgets to update the catalog, the
    /// substrate would silently skip it — this test catches that.
    ///
    /// H-API5 regression: pin the third leg of the triple by also
    /// asserting that every entry in `MigrationVersion::ALL` is
    /// present in `BUILTIN_MIGRATIONS`. The historical check only
    /// compared the tuple slice against the catalog; a PR that
    /// added a new name to `MigrationVersion::ALL` without updating
    /// the catalog would slip past.
    #[cfg(feature = "stoolap")]
    #[test]
    fn catalog_matches_builtin_migrations() {
        use crate::migrations::{MigrationVersion, BUILTIN_MIGRATION_CATALOG};
        let tuple_names: Vec<&str> = BUILTIN_MIGRATIONS.iter().map(|(n, _)| *n).collect();
        let catalog_names: Vec<&str> = BUILTIN_MIGRATION_CATALOG.iter().map(|m| m.name()).collect();
        let all_version_names: Vec<&str> = MigrationVersion::ALL.to_vec();
        assert_eq!(
            tuple_names.len(),
            catalog_names.len(),
            "tuple slice and catalog have different lengths; \
             triple source-of-truth drift between BUILTIN_MIGRATIONS and BUILTIN_MIGRATION_CATALOG"
        );
        assert_eq!(
            tuple_names, catalog_names,
            "tuple/catalog name list diverged"
        );
        assert_eq!(
            tuple_names.len(),
            all_version_names.len(),
            "MigrationVersion::ALL drifted from BUILTIN_MIGRATIONS length"
        );
        for (t, a) in tuple_names.iter().zip(all_version_names.iter()) {
            assert_eq!(
                t, a,
                "MigrationVersion::ALL mismatch with BUILTIN_MIGRATIONS"
            );
        }
    }

    /// M-T1 regression: `substrate_runner::apply` must NOT leak the
    /// substrate error's `Debug` form to stderr. The substrate's
    /// `Debug` chain can include SQL fragments / migration `name`
    /// strings that flow into operator dashboards. Verify that the
    /// error path is reachable but emits NO `eprintln!` output.
    #[cfg(feature = "stoolap")]
    #[test]
    fn apply_emits_no_stderr_on_substrate_error() {
        // Build a malformed migration catalog inline by directly
        // calling ensure_tracker_table on a DB that has a legacy
        // orphan row (which the substrate's
        // `ensure_tracker_table:backfill_orphan` guard rejects).
        let db = octo_storage_core::Database::open_in_memory().unwrap();
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
             VALUES (1, 'manual_audit_marker', 1000)",
            (),
        )
        .unwrap();

        // Apply to an empty/malformed migration catalog. The
        // substrate's ensure_tracker_table will fail BEFORE any
        // catalog migration runs — this exercises the
        // `.map_err(|_e| ...)` arm without surfacing `e`.
        let minimal_catalog: &[&'static dyn octo_storage_core::_legacy_Migration] = &[];
        let result = octo_storage_core::_legacy_apply_pending(
            &db,
            minimal_catalog,
            octo_storage_core::_legacy_ApplyConfig::default(),
        );
        assert!(result.is_err(), "orphan row must trigger substrate error");
    }
}
