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
    let db = octo_storage_core::open_in_memory().expect("open in-memory");
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
        version, 17,
        "catalog must reach v017 (current max migration version; v017 = 0010-v17 chain_metadata + policy_registry + ledger_chain_registry + policy_kind_authority)"
    );
}

#[test]
fn migration_chain_creates_all_4_rich_columns() {
    let _guard = MIGRATION_LOCK.lock().unwrap_or_else(|p| p.into_inner());
    let db = octo_storage_core::open_in_memory().expect("open in-memory");
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
    let db = octo_storage_core::open_in_memory().expect("open in-memory");
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

// Mission 0010-v17-chain-id-registration-authority + RFC-0967-A1 v1.9.2:
// v017 combined migration TV (4 tables: ledger_chain_registry +
// chain_metadata + policy_registry + policy_kind_authority).
#[test]
fn migration_chain_v017_creates_4_tables() {
    let _guard = MIGRATION_LOCK.lock().unwrap_or_else(|p| p.into_inner());
    let db = octo_storage_core::open_in_memory().expect("open in-memory");
    migrations::apply_pending(&db).expect("apply_pending");
    for table in [
        "ledger_chain_registry",
        "chain_metadata",
        "policy_registry",
        "policy_kind_authority",
    ] {
        let sql = format!("SELECT 1 FROM {table} WHERE 1 = 0");
        db.query(&sql, ())
            .unwrap_or_else(|e| panic!("table {table} missing after v017: {e}"));
    }
}

#[test]
fn migration_chain_v017_ledger_chain_registry_columns() {
    let _guard = MIGRATION_LOCK.lock().unwrap_or_else(|p| p.into_inner());
    let db = octo_storage_core::open_in_memory().expect("open in-memory");
    migrations::apply_pending(&db).expect("apply_pending");
    for col in [
        "chain_id",
        "chain_namespace",
        "operator_did",
        "operator_signature",
        "registration_body",
        "registered_at_unix",
        "revoked_at_unix",
    ] {
        let sql = format!("SELECT {col} FROM ledger_chain_registry WHERE 1 = 0");
        db.query(&sql, ())
            .unwrap_or_else(|e| panic!("ledger_chain_registry.{col} missing: {e}"));
    }
}

#[test]
fn migration_chain_v017_chain_metadata_columns() {
    let _guard = MIGRATION_LOCK.lock().unwrap_or_else(|p| p.into_inner());
    let db = octo_storage_core::open_in_memory().expect("open in-memory");
    migrations::apply_pending(&db).expect("apply_pending");
    for col in [
        "chain_id",
        "workflow_kind_hashes",
        "interop_policy_hash",
        "audit_policy_hash",
        "burn_policy_hash",
        "admin_pubkey",
        "composite_depth",
        "registered_at_unix",
        "revoked_at_unix",
    ] {
        let sql = format!("SELECT {col} FROM chain_metadata WHERE 1 = 0");
        db.query(&sql, ())
            .unwrap_or_else(|e| panic!("chain_metadata.{col} missing: {e}"));
    }
}

#[test]
fn migration_chain_v017_policy_registry_columns() {
    let _guard = MIGRATION_LOCK.lock().unwrap_or_else(|p| p.into_inner());
    let db = octo_storage_core::open_in_memory().expect("open in-memory");
    migrations::apply_pending(&db).expect("apply_pending");
    for col in [
        "policy_hash",
        "registry_kind",
        "crate_name",
        "trait_spec",
        "registered_at_unix",
        "revoked_at_unix",
    ] {
        let sql = format!("SELECT {col} FROM policy_registry WHERE 1 = 0");
        db.query(&sql, ())
            .unwrap_or_else(|e| panic!("policy_registry.{col} missing: {e}"));
    }
}

#[test]
fn migration_chain_v017_policy_kind_authority_columns() {
    let _guard = MIGRATION_LOCK.lock().unwrap_or_else(|p| p.into_inner());
    let db = octo_storage_core::open_in_memory().expect("open in-memory");
    migrations::apply_pending(&db).expect("apply_pending");
    for col in [
        "policy_kind_uuid",
        "policy_hash",
        "registrant_did",
        "registrant_signature",
        "registration_body",
        "registered_at_unix",
        "revoked_at_unix",
    ] {
        let sql = format!("SELECT {col} FROM policy_kind_authority WHERE 1 = 0");
        db.query(&sql, ())
            .unwrap_or_else(|e| panic!("policy_kind_authority.{col} missing: {e}"));
    }
}

#[test]
fn migration_chain_v017_insert_and_query_round_trip() {
    let _guard = MIGRATION_LOCK.lock().unwrap_or_else(|p| p.into_inner());
    let db = octo_storage_core::open_in_memory().expect("open in-memory");
    migrations::apply_pending(&db).expect("apply_pending");

    db.execute(
        "INSERT INTO ledger_chain_registry \
         (chain_id, chain_namespace, operator_did, operator_signature, \
          registration_body, registered_at_unix, revoked_at_unix) \
         VALUES (?, ?, ?, ?, ?, ?, NULL)",
        (
            vec![0xAA_u8; 32].as_slice(),
            vec![0x01_u8].as_slice(),
            vec![0xBB_u8; 32].as_slice(),
            vec![0xCC_u8; 64].as_slice(),
            vec![0xDD_u8; 16].as_slice(),
            1_700_000_000_i64,
        ),
    )
    .expect("insert ledger_chain_registry");

    db.execute(
        "INSERT INTO chain_metadata \
         (chain_id, workflow_kind_hashes, interop_policy_hash, audit_policy_hash, \
          burn_policy_hash, admin_pubkey, composite_depth, registered_at_unix, revoked_at_unix) \
         VALUES (?, ?, NULL, NULL, NULL, ?, 0, ?, NULL)",
        (
            vec![0xAA_u8; 32].as_slice(),
            vec![0xEE_u8; 4].as_slice(),
            vec![0xFF_u8; 32].as_slice(),
            1_700_000_001_i64,
        ),
    )
    .expect("insert chain_metadata");

    db.execute(
        "INSERT INTO policy_registry \
         (policy_hash, registry_kind, crate_name, trait_spec, registered_at_unix, revoked_at_unix) \
         VALUES (?, ?, ?, ?, ?, NULL)",
        (
            vec![0x11_u8; 32].as_slice(),
            1_i64,
            "octo_authority_capability",
            vec![0x22_u8; 8].as_slice(),
            1_700_000_002_i64,
        ),
    )
    .expect("insert policy_registry");

    db.execute(
        "INSERT INTO policy_kind_authority \
         (policy_kind_uuid, policy_hash, registrant_did, registrant_signature, \
          registration_body, registered_at_unix, revoked_at_unix) \
         VALUES (?, ?, ?, ?, ?, ?, NULL)",
        (
            vec![0x33_u8; 16].as_slice(),
            vec![0x11_u8; 32].as_slice(),
            vec![0x44_u8; 32].as_slice(),
            vec![0x55_u8; 64].as_slice(),
            vec![0x66_u8; 16].as_slice(),
            1_700_000_003_i64,
        ),
    )
    .expect("insert policy_kind_authority");

    let mut rows = db
        .query("SELECT COUNT(*) FROM ledger_chain_registry", ())
        .expect("count lcr");
    let count: i64 = rows.next().expect("row").expect("ok").get(0).unwrap_or(0);
    assert_eq!(count, 1, "ledger_chain_registry must have 1 row");

    let mut rows = db
        .query("SELECT COUNT(*) FROM chain_metadata", ())
        .expect("count cm");
    let count: i64 = rows.next().expect("row").expect("ok").get(0).unwrap_or(0);
    assert_eq!(count, 1, "chain_metadata must have 1 row");

    let mut rows = db
        .query("SELECT COUNT(*) FROM policy_registry", ())
        .expect("count pr");
    let count: i64 = rows.next().expect("row").expect("ok").get(0).unwrap_or(0);
    assert_eq!(count, 1, "policy_registry must have 1 row");

    let mut rows = db
        .query("SELECT COUNT(*) FROM policy_kind_authority", ())
        .expect("count pka");
    let count: i64 = rows.next().expect("row").expect("ok").get(0).unwrap_or(0);
    assert_eq!(count, 1, "policy_kind_authority must have 1 row");
}
