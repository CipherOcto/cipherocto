//! Migration runner — applies the canonical `migrations/*.sql` files on
//! `StoolapReputationStore::open`. Idempotent: every migration is recorded
//! in `schema_migrations` after `CREATE TABLE IF NOT EXISTS` succeeds, so a
//! subsequent open is a no-op for already-applied versions.
//!
//! Per `feedback_stoolap-persistence.md` memory: stoolap-fork is the only
//! storage layer, and per [[stoolap-general-purpose-db]] the consumer schema
//! lives here, not in the fork. The runner pattern mirrors
//! `crates/quota-router-storage/src/migrations.rs` but stays local — no
//! quota-router-storage coupling.

use std::time::{SystemTime, UNIX_EPOCH};

#[cfg(feature = "stoolap")]
use crate::error::ReputationError;

/// All built-in migrations in version order. The runner bootstraps the
/// `schema_migrations` table (via `ensure_tracker_table`) BEFORE iterating
/// `BUILTIN_MIGRATIONS`, so the per-migration INSERT step can always find
/// a tracker table — including for the very first migration, `v001__`.
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

/// Bootstrap SQL for the tracker table — idempotent. The runner runs this
/// before iterating `BUILTIN_MIGRATIONS` so the per-migration
/// `INSERT OR IGNORE INTO schema_migrations` step can find a table.
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

#[cfg(feature = "stoolap")]
pub mod stoolap_runner {
    use super::{ReputationError, BUILTIN_MIGRATIONS, TRACKER_TABLE_SQL};
    use std::time::{SystemTime, UNIX_EPOCH};

    fn now_unix() -> u64 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs()
    }

    /// Apply every migration that has not yet been recorded in
    /// `schema_migrations`. Synchronous — the `StoolapReputationStore`
    /// constructor wraps this in `spawn_blocking` so the tokio reactor
    /// is never stalled for long.
    ///
    /// Order: bootstrap the `schema_migrations` tracker table first (so
    /// the per-migration INSERT step can find it), then iterate
    /// `BUILTIN_MIGRATIONS` in declared order.
    pub fn apply(db: &stoolap::Database) -> Result<(), ReputationError> {
        db.execute(TRACKER_TABLE_SQL, ())
            .map_err(|_e| ReputationError::ChainRefInvalid("migration:tracker_init"))?;
        for (version, sql) in BUILTIN_MIGRATIONS {
            // Idempotency guard: only run + record when this version is
            // not already in `schema_migrations`.
            let already_applied: bool = {
                let mut q = db
                    .query(
                        "SELECT name FROM schema_migrations WHERE name = $1",
                        vec![stoolap::Value::text((*version).to_string())],
                    )
                    .map_err(|_e| ReputationError::ChainRefInvalid("migration:guard_query"))?;
                matches!(q.next(), Some(Ok(_)))
            };
            if already_applied {
                continue;
            }
            let label = match *version {
                "v001__reputation_events" => "migration:v001",
                "v002__reputation_recorders" => "migration:v002",
                "v003__schema_migrations" => "migration:v003",
                "v004__reputation_attestations" => "migration:v004",
                "v005__reputation_gossip_seen" => "migration:v005",
                "v010__reputation_anchors" => "migration:v010",
                "v011__reputation_events_anchor" => "migration:v011",
                "v012__reputation_anchors_governance" => "migration:v012",
                _ => "migration:unknown",
            };
            db.execute(sql, ())
                .map_err(|_e| ReputationError::ChainRefInvalid(label))?;
            let ts = now_unix();
            // Idempotent guard: query first, only INSERT when missing.
            // Stoolap-fork does not support `INSERT OR IGNORE` (its parser
            // rejects the `OR IGNORE` modifier), so we roll the check at
            // the Rust boundary. The race window is bounded to a single-
            // process test; production deployments initialise the
            // schema_migrations table out of band.
            let already_applied: Option<String> = {
                let mut q = db
                    .query(
                        "SELECT name FROM schema_migrations WHERE name = $1",
                        vec![stoolap::Value::text((*version).to_string())],
                    )
                    .map_err(|_e| ReputationError::ChainRefInvalid("migration:guard_query"))?;
                match q.next() {
                    Some(Ok(row)) => row
                        .get_by_name("name")
                        .map(Some)
                        .map_err(|_e| ReputationError::ChainRefInvalid("migration:guard_get"))?,
                    Some(Err(_e)) => {
                        return Err(ReputationError::ChainRefInvalid("migration:guard_iter"))
                    }
                    None => None,
                }
            };
            if already_applied.is_none() {
                // Lookup next id from MAX(id)+1 so the PK is satisfied.
                let next_id: i64 = {
                    let mut q = db
                        .query("SELECT COALESCE(MAX(id), 0) + 1 FROM schema_migrations", ())
                        .map_err(|_e| ReputationError::ChainRefInvalid("migration:next_id"))?;
                    match q.next() {
                        Some(Ok(row)) => row.get(0).map_err(|_e| {
                            ReputationError::ChainRefInvalid("migration:next_id:get")
                        })?,
                        Some(Err(_e)) => {
                            return Err(ReputationError::ChainRefInvalid("migration:next_id:iter"))
                        }
                        None => 1i64,
                    }
                };
                db.execute(
                    "INSERT INTO schema_migrations(id, name, applied_at_unix) VALUES ($1, $2, $3)",
                    vec![
                        stoolap::Value::integer(next_id),
                        stoolap::Value::text((*version).to_string()),
                        stoolap::Value::integer(ts as i64),
                    ],
                )
                .map_err(|e| {
                    eprintln!("migration:record ERR: {e:?}");
                    ReputationError::ChainRefInvalid("migration:record")
                })?;
            }
        }
        Ok(())
    }

    /// Returns the list of version strings currently recorded as applied.
    pub fn applied_versions(db: &stoolap::Database) -> Result<Vec<String>, ReputationError> {
        let mut rows = db
            .query("SELECT name FROM schema_migrations ORDER BY name", ())
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
pub use stoolap_runner::{applied_versions, apply};

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
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();
        assert!((now as i64 - t as i64).abs() < 5);
    }
}
