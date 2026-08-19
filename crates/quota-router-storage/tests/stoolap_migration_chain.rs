//! Mission 0010-f8-rich-did-storage + 0010-f2-registry-namespacing +
//! 0871e-slasher — migration chain TV.
//!
//! Verifies that `apply_pending` brings a fresh DB up through v012
//! (the current HEAD: includes v011 chain_id column from 0010-f2 and
//! v012 slash_ledger table from 0871e-slasher — the latter is in
//! its OWN table per the slashing-persistence architecture, but the
//! catalog version row records it as v012). And that the
//! `did_registry` schema carries the expected 9 columns
//! (4 legacy + 4 rich BLOB columns from v009/v010 + 1 chain_id BLOB
//! column from v011).
//!
//! **NOTE:** Tests run sequentially via a Mutex because stoolap's
//! `memory://` DSN appears to share state across test threads in the
//! same process (catalog PK collisions). Each test uses a fresh DB
//! handle but the underlying catalog table is global. Single-threaded
//! execution via the Mutex eliminates the race.

use std::sync::Mutex;

use quota_router_storage::migrations;

static MIGRATION_LOCK: Mutex<()> = Mutex::new(());

#[test]
fn migration_chain_reaches_v012_on_fresh_db() {
    let _guard = MIGRATION_LOCK.lock().unwrap_or_else(|p| p.into_inner());
    let db = stoolap::Database::open("memory://").expect("open in-memory");
    migrations::apply_pending(&db).expect("apply_pending");
    let mut rows = db
        .query("SELECT MAX(version) FROM cipherocto_schema_version", ())
        .expect("version query");
    let version: i64 = rows
        .next()
        .expect("row present")
        .expect("row ok")
        .get(0)
        .unwrap_or(0);
    assert_eq!(
        version, 15,
        "catalog must reach v015 (current max migration version)"
    );
}

#[test]
fn migration_chain_creates_all_4_rich_columns() {
    let _guard = MIGRATION_LOCK.lock().unwrap_or_else(|p| p.into_inner());
    let db = stoolap::Database::open("memory://").expect("open in-memory");
    migrations::apply_pending(&db).expect("apply_pending");
    for col in [
        "service_endpoints",
        "controllers",
        "verification_methods",
        "capability_delegations",
    ] {
        let sql = format!("SELECT {col} FROM did_registry WHERE 1 = 0");
        db.query(&sql, ())
            .unwrap_or_else(|e| panic!("column {col} missing: {e}"));
    }
}

#[test]
fn migration_chain_reaches_v012_with_legacy_then_rich_then_chain_columns() {
    let _guard = MIGRATION_LOCK.lock().unwrap_or_else(|p| p.into_inner());
    // Single end-to-end: open fresh DB, apply all 12 migrations,
    // verify the legacy 4 columns still exist (canonical_hash,
    // public_key, revoked, updated_at_unix_ms) + the 4 rich BLOB
    // columns from v009 + v010 + the chain_id BLOB column from v011.
    let db = stoolap::Database::open("memory://").expect("open in-memory");
    migrations::apply_pending(&db).expect("apply_pending");
    for col in [
        "canonical_hash",
        "public_key",
        "revoked",
        "updated_at_unix_ms",
        "service_endpoints",
        "controllers",
        "verification_methods",
        "capability_delegations",
        "chain_id",
    ] {
        let sql = format!("SELECT {col} FROM did_registry WHERE 1 = 0");
        db.query(&sql, ())
            .unwrap_or_else(|e| panic!("column {col} missing: {e}"));
    }
}
