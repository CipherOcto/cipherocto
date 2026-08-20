//! Mission 0871b-storage-idempotent-alter-hardening — ADD COLUMN
//! retry-safety TV.
//!
//! The cipherocto-side migration runner records the migration
//! version AFTER all statements in the migration run. If a process
//! crashes mid-`apply_pending` between two ADD COLUMN statements of
//! the same migration, the version row is un-inserted; a retry
//! re-runs the migration from statement 1 and the second ADD COLUMN
//! would have hit a "duplicate column" error.
//!
//! The hardening (`migrations::run_one`) catches
//! `Error::DuplicateColumn` on ADD COLUMN statements and treats
//! them as no-op, so the retry succeeds.
//!
//! **NOTE:** Tests serialize via Mutex because stoolap `memory://`
//! shares global catalog state across threads (PK collisions on
//! concurrent `apply_pending`).

use std::sync::Mutex;

use quota_router_storage::migrations;

static MIGRATION_LOCK: Mutex<()> = Mutex::new(());

/// Simulates a crash between two ADD COLUMN statements of the same
/// migration: v010 ADD COLUMN verification_methods succeeds, ADD
/// COLUMN capability_delegations is the hypothetical crash point.
///
/// Approximation: apply fresh DB through v009 (so all earlier
/// statements succeed), then directly insert the v010 column #1
/// manually (simulating "statement 1 succeeded before crash") while
/// leaving v010 unrecorded in cipherocto_schema_version. Re-running
/// apply_pending should catch statement 1's "duplicate column" (no-op),
/// succeed at statement 2, then record v010.
#[test]
fn apply_pending_is_idempotent_on_re_run_after_partial_migration() {
    let _guard = MIGRATION_LOCK.lock().unwrap_or_else(|p| p.into_inner());
    let db = octo_storage_core::open_in_memory().expect("open in-memory");
    migrations::apply_pending(&db).expect("first apply");

    // Verify v010 was applied (catalog max = 10).
    let version_after_first: i64 = db
        .query("SELECT MAX(version) FROM cipherocto_schema_version", ())
        .expect("version query")
        .next()
        .expect("row")
        .expect("row ok")
        .get(0)
        .unwrap_or(0);
    assert_eq!(
        version_after_first, 16,
        "first apply reaches v016 (current max migration version; v016 = 0959-c1 settlement_chain_vault)"
    );

    // Simulate partial-v010 crash: drop the v010 + v011 + v012 + v013 + v014 + v015 + v016 version rows
    // (so re-apply_pending will attempt v010–v016 again) AND drop
    // capability_delegations column (so v010's first statement —
    // ADD COLUMN verification_methods — fails as "duplicate
    // column" → caught as no-op, second statement re-adds
    // capability_delegations successfully).
    db.execute(
        "DELETE FROM cipherocto_schema_version WHERE version IN (10, 11, 12, 13, 14, 15, 16)",
        (),
    )
    .expect("delete v010 + v011 + v012 + v013 + v014 + v015 + v016 version rows");

    // Drop the capability_delegations column so v010 statement 2
    // can re-add it after the "duplicate column" catch on
    // statement 1.
    db.execute(
        "ALTER TABLE did_registry DROP COLUMN capability_delegations",
        (),
    )
    .expect("drop cap_delegations to force re-add");

    // Re-run apply_pending. With hardening: v011 runs first (re-adds
    // chain_id column + backfill + reindex — all no-ops for existing
    // data + re-creates the unique index); v010 runs second —
    // ADD COLUMN verification_methods fails (dup) → no-op → ADD
    // COLUMN capability_delegations succeeds (column was just
    // dropped) → version 11 re-recorded.
    migrations::apply_pending(&db).expect("retry apply_pending");

    // Catalog must record v010 again.
    let version_after_retry: i64 = db
        .query("SELECT MAX(version) FROM cipherocto_schema_version", ())
        .expect("version query")
        .next()
        .expect("row")
        .expect("row ok")
        .get(0)
        .unwrap_or(0);
    assert_eq!(
        version_after_retry, 16,
        "retry apply_pending must record v016 (current max; v016 = 0959-c1 settlement_chain_vault)"
    );

    // Both v010 columns must be present and queryable.
    for col in ["verification_methods", "capability_delegations"] {
        let sql = format!("SELECT {col} FROM did_registry WHERE 1 = 0");
        db.query(&sql, ())
            .unwrap_or_else(|e| panic!("column {col} missing after retry: {e}"));
    }
}

/// A second TV that directly exercises the catch on a column that
/// already exists from a prior migration (no DROP COLUMN needed).
/// Drops the v009 version row (not the columns) so re-apply attempts
/// v009 again — both ADD COLUMNs hit "duplicate column" (no-op).
#[test]
fn apply_pending_swallows_v009_dup_column_on_retry() {
    let _guard = MIGRATION_LOCK.lock().unwrap_or_else(|p| p.into_inner());
    let db = octo_storage_core::open_in_memory().expect("open in-memory");
    migrations::apply_pending(&db).expect("first apply");

    // Drop v009 version row — re-apply will attempt v009 again.
    // The two ADD COLUMNs (service_endpoints, controllers) will both
    // hit "duplicate column". Pre-hardening: MigrationFailed.
    // Post-hardening: swallowed, v009 re-recorded.
    db.execute(
        "DELETE FROM cipherocto_schema_version WHERE version = 9",
        (),
    )
    .expect("delete v009 version row");

    migrations::apply_pending(&db).expect("retry after v009 dup");

    let version: i64 = db
        .query("SELECT MAX(version) FROM cipherocto_schema_version", ())
        .expect("query")
        .next()
        .expect("row")
        .expect("row ok")
        .get(0)
        .unwrap_or(0);
    assert_eq!(
        version, 16,
        "v009 dup must be swallowed; catalog still at v016 (current max)"
    );
}
