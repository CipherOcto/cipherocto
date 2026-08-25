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
        version, 19,
        "catalog must reach v019 (current max migration version; v018 = RFC-0903-D1 litellm_users + litellm_keys + scim_users + scim_groups + scim_group_members; v019 = litellm_user_vault_link bridge table)"
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

// ─────────────────────────────────────────────────────────────────────
// C3 — v017 → v018 incremental migration + apply_pending idempotency
// coverage. Per adversarial review of commit 39b42b05.
// ─────────────────────────────────────────────────────────────────────

/// Helper: count tables in the catalog (user tables; EXCLUDES the
/// `cipherocto_schema_version` migration-tracker table so the
/// delta between versions reflects only schema-table growth).
///
/// Stoolap fork exposes table introspection via
/// `sqlite_schema`-like `DBA_TABLES`-style queries through
/// `octo_storage_core::stoolap::TableInfo`. For the in-memory fork
/// (verified 2026-08-24 recon), the simpler approach is to count the
/// known canonical names directly. We use a fixed canonical set per
/// migration max (so the count is monotonic across v001..v018).
fn count_canonical_tables(db: &octo_storage_core::Database) -> usize {
    // Canonical user-table set across v001..v019 inclusive.
    // Names that exist at v019 but did NOT exist at v018 are the
    // v019 additions (1 litellm user→vault link table). Names
    // that exist at v018 but not earlier are the v018 additions
    // (5 litellm/scim tables). The set is closed (no further
    // tables land after v019 in the current catalog).
    const TABLES: &[&str] = &[
        // v001-v008 (asks, indexes, consumed_receipt_index,
        // settlement_events, holder_registry, outbox, spend_ledger,
        // did_registry).
        "asks",
        "consumed_receipt_index",
        "settlement_events",
        "holder_registry",
        "outbox",
        "spend_ledger",
        "did_registry",
        // v009-v011 (rich BLOB columns + chain_id). Same tables.
        // v012 (slash_ledger).
        "slash_ledger",
        // v015 (chain_aware_slash_ledger alters; no NEW tables).
        // v016 (settlement_chain_vault alters; no NEW tables).
        // v017 (4 NEW tables).
        "ledger_chain_registry",
        "chain_metadata",
        "policy_registry",
        "policy_kind_authority",
        // v018 (5 NEW tables per RFC-0903-D1 §2).
        "litellm_users",
        "litellm_keys",
        "scim_users",
        "scim_groups",
        "scim_group_members",
        // v019 (1 NEW table per RFC-0903-D1 follow-up).
        "litellm_user_vault_link",
    ];
    let mut count = 0usize;
    for t in TABLES {
        // Stoolap fork: SELECT 1 from a missing table → Err. Catch
        // the error and DON'T count. The query succeeds if the table
        // exists.
        let probe = db.query(&format!("SELECT 1 FROM {t} WHERE 1 = 0"), ());
        if probe.is_ok() {
            count += 1;
        }
    }
    count
}

#[test]
fn migration_chain_v017_then_v018_additive() {
    let _guard = MIGRATION_LOCK.lock().unwrap_or_else(|p| p.into_inner());
    let db = octo_storage_core::open_in_memory().expect("open in-memory");

    // 1. Snapshot BEFORE applying any migrations: empty catalog.
    let count_at_zero = count_canonical_tables(&db);
    assert_eq!(count_at_zero, 0, "fresh DB has zero user tables");

    // 2. First apply_pending: brings DB up through the current
    //    catalog (v019 inclusive; v018 was the prior catalog max).
    //    Both v017 and v018 and v019 tables land in the same
    //    apply_pending call because catalog_max = 19.
    migrations::apply_pending(&db).expect("first apply_pending");

    // 3. Snapshot AFTER: all canonical tables (v001..v019 inclusive)
    //    are present (substrate-recon 2026-08-24).
    let count_at_final = count_canonical_tables(&db);

    // Verify the v018 tables are present (regression guard: v018
    // tables must not regress when v019 lands).
    for table in [
        "litellm_users",
        "litellm_keys",
        "scim_users",
        "scim_groups",
        "scim_group_members",
    ] {
        db.query(&format!("SELECT 1 FROM {table} WHERE 1 = 0"), ())
            .unwrap_or_else(|e| panic!("v018 table {table} missing: {e}"));
    }

    // Verify the v017 tables are present (regression guard: v017
    // must not regress when v018 lands).
    for table in [
        "ledger_chain_registry",
        "chain_metadata",
        "policy_registry",
        "policy_kind_authority",
    ] {
        db.query(&format!("SELECT 1 FROM {table} WHERE 1 = 0"), ())
            .unwrap_or_else(|e| panic!("v017 table {table} regressed: {e}"));
    }

    // Verify the v019 table is present (the "additive" property
    // for this migration set: v019 strictly ADDS 1 NEW table).
    db.query("SELECT 1 FROM litellm_user_vault_link WHERE 1 = 0", ())
        .expect("v019 table litellm_user_vault_link missing");

    // Snapshot the schema_version: every applied migration must be
    // recorded EXACTLY once. The catalog is monotonic — fresh DBs
    // see all migrations up to catalog_max (= 19).
    let rows = db
        .query(
            "SELECT version FROM cipherocto_schema_version ORDER BY version",
            (),
        )
        .expect("schema_version");
    let mut versions: Vec<i64> = Vec::new();
    for r in rows {
        let v: i64 = r.expect("ok").get(0).unwrap_or(0);
        versions.push(v);
    }
    assert!(
        versions.contains(&17),
        "v017 must be applied (got versions {versions:?})"
    );
    assert!(
        versions.contains(&18),
        "v018 must be applied (got versions {versions:?})"
    );
    assert!(
        versions.contains(&19),
        "v019 must be applied (got versions {versions:?})"
    );

    // Cumulative count of canonical tables = 18 (v001-v008: 7 +
    // v012: 1 + v017: 4 + v018: 5 + v019: 1 = 18).
    assert_eq!(
        count_at_final, 18,
        "v001..v019 inclusive should yield 18 canonical user tables (got {count_at_final})"
    );

    // Additive check: count strictly increased v017 → v018 (no
    // DROP semantics). We can't actually run apply_pending twice
    // (idempotent), but the additive property is enforced by the
    // CREATE TABLE IF NOT EXISTS pattern in each migration's DDL.
    // For this check we verify that the v017 tables survived the
    // v018 + v019 ADDs (already verified above) AND that no DROP
    // statements are present in the v018 / v019 SQL surfaces.
    let v018_sql = include_str!("../migrations/v018__litellm_scim_persistence.sql");
    let v019_sql = include_str!("../migrations/v019__litellm_user_vault_link.sql");
    assert!(
        !v018_sql.to_uppercase().contains("DROP TABLE"),
        "v018 must be ADDITIVE (no DROP TABLE clauses); check the migration SQL"
    );
    assert!(
        !v019_sql.to_uppercase().contains("DROP TABLE"),
        "v019 must be ADDITIVE (no DROP TABLE clauses); check the migration SQL"
    );
}

#[test]
fn apply_pending_idempotent() {
    let _guard = MIGRATION_LOCK.lock().unwrap_or_else(|p| p.into_inner());
    let db = octo_storage_core::open_in_memory().expect("open in-memory");

    // First apply: brings DB to v018.
    migrations::apply_pending(&db).expect("first apply_pending");
    // Snapshot the schema_version table after first apply.
    let mut rows = db
        .query("SELECT COUNT(*) FROM cipherocto_schema_version", ())
        .expect("count1");
    let first_count: i64 = rows.next().expect("row").expect("ok").get(0).unwrap_or(0);
    let first_max = {
        let mut r = db
            .query("SELECT MAX(version) FROM cipherocto_schema_version", ())
            .expect("max1");
        r.next().expect("row").expect("ok").get(0).unwrap_or(0)
    };

    // Second apply: idempotent — no new migration is applied.
    migrations::apply_pending(&db).expect("second apply_pending (idempotent)");
    let mut rows = db
        .query("SELECT COUNT(*) FROM cipherocto_schema_version", ())
        .expect("count2");
    let second_count: i64 = rows.next().expect("row").expect("ok").get(0).unwrap_or(0);
    let second_max = {
        let mut r = db
            .query("SELECT MAX(version) FROM cipherocto_schema_version", ())
            .expect("max2");
        r.next().expect("row").expect("ok").get(0).unwrap_or(0)
    };

    assert_eq!(
        first_count, second_count,
        "schema_version row count must be unchanged after second apply_pending (idempotent); \
         first={first_count}, second={second_count}"
    );
    assert_eq!(
        first_max, second_max,
        "schema_version max must be unchanged after second apply_pending (idempotent); \
         first={first_max}, second={second_max}"
    );

    // Third apply for paranoia.
    migrations::apply_pending(&db).expect("third apply_pending (idempotent)");
    let mut rows = db
        .query("SELECT COUNT(*) FROM cipherocto_schema_version", ())
        .expect("count3");
    let third_count: i64 = rows.next().expect("row").expect("ok").get(0).unwrap_or(0);
    assert_eq!(
        first_count, third_count,
        "third apply_pending must also be idempotent; first={first_count}, third={third_count}"
    );

    // Schema-version row count must equal the number of distinct
    // migration files in the catalog (the apply_pending guard
    // refuses to re-record an already-applied version). The catalog
    // has 17 migration files (v001-v012 = 12 + v015-v019 = 5 = 17)
    // but max version is 19 because v013 + v014 were reserved /
    // skipped. So COUNT = 17, MAX = 19 is the substrate-truth.
    // The load-bearing property is that COUNT stays constant across
    // N idempotent apply_pending calls (already asserted above).
    assert_eq!(
        first_count, 17,
        "schema_version row count must equal the number of distinct migration files (17); \
         got {first_count}"
    );
    assert_eq!(
        first_max, 19,
        "schema_version max must equal catalog_max (19); got {first_max}"
    );
}
