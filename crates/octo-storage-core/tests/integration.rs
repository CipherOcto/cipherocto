//! Adapter trait TV fixtures for `octo-storage-core` per
//! `docs/plans/2026-08-16-storage-layer-restructuring-execution-plan.md`
//! §3 (S2 row) + §4 (S2 verification gates).
//!
//! 5 fixtures:
//! 1. `Migration` trait — idempotency invariant
//! 2. `Migration` trait — strict-version ordering invariant
//! 3. `apply_pending` — fresh-DB brings the DB to the catalog max
//! 4. `apply_pending` — partially-applied DB picks up remaining migrations
//! 5. `open` / `open_in_memory` — round-trip persistence + ephemerality

use octo_storage_core::{
    applied_version, apply_pending, current_version, ensure_tracker_table, open, open_in_memory,
    record_migration, ApplyConfig, Migration, StaticMigration, DEFAULT_TRACKER_TABLE,
};

const FIXTURES: &[&'static dyn Migration] = &[
    &StaticMigration::new(
        1,
        "create_sessions",
        "CREATE TABLE sessions (id INTEGER PRIMARY KEY, user_id TEXT NOT NULL, \
         started_at_unix INTEGER NOT NULL)",
    ),
    &StaticMigration::new(
        2,
        "create_invites",
        "CREATE TABLE invites (id INTEGER PRIMARY KEY, session_id INTEGER NOT NULL, \
         token TEXT NOT NULL UNIQUE, expires_at_unix INTEGER NOT NULL)",
    ),
    &StaticMigration::new(
        3,
        "add_email_to_sessions",
        "ALTER TABLE sessions ADD COLUMN email TEXT; ALTER TABLE sessions ADD COLUMN \
         phone TEXT",
    ),
];

// TV 1 — Migration trait idempotency invariant: applying the same slice
// twice leaves the database in the same terminal state.
#[test]
fn tv1_migration_trait_idempotency() {
    let db = open_in_memory().expect("ephemeral DB");
    apply_pending(&db, FIXTURES, ApplyConfig::default()).unwrap();
    let first = applied_version(&db, DEFAULT_TRACKER_TABLE).unwrap();

    apply_pending(&db, FIXTURES, ApplyConfig::default()).unwrap();
    let second = applied_version(&db, DEFAULT_TRACKER_TABLE).unwrap();

    assert_eq!(
        first, second,
        "idempotent re-application must produce identical tracker state"
    );
    assert_eq!(
        second,
        [1_u32, 2, 3].into_iter().collect(),
        "final state must include all 3 migrations exactly once"
    );
}

// TV 2 — Migration trait ordering invariant: each migration's version
// must exceed the previous one. Lifted from quota-router-storage's
// `builtin_migrations_are_sorted_by_version` test, generalized to the
// trait object form.
#[test]
fn tv2_migration_trait_ordering_invariant() {
    let mut last = 0_u32;
    for m in FIXTURES {
        assert!(
            m.version() > last,
            "migration version must be strictly increasing: {} <= {last}",
            m.version()
        );
        last = m.version();
    }
}

// TV 3 — apply_pending on a fresh DB brings it to catalog max.
#[test]
fn tv3_apply_pending_fresh_db_brings_to_catalog_max() {
    let db = open_in_memory().expect("ephemeral DB");
    apply_pending(&db, FIXTURES, ApplyConfig::default()).unwrap();

    let max_applied = current_version(&db, DEFAULT_TRACKER_TABLE).unwrap();
    let catalog_max = FIXTURES.iter().map(|m| m.version()).max().unwrap();
    assert_eq!(max_applied, catalog_max);

    // Spot-check: tables exist.
    db.execute(
        "INSERT INTO sessions (id, user_id, started_at_unix, email, phone) \
         VALUES (1, 'u1', 1700000000, 'a@b', '+1')",
        (),
    )
    .unwrap();
    db.execute(
        "INSERT INTO invites (id, session_id, token, expires_at_unix) VALUES (1, 1, 'tok', 0)",
        (),
    )
    .unwrap();
}

// TV 4 — partially-applied DB picks up remaining migrations.
#[test]
fn tv4_apply_pending_partial_db_picks_up_remaining() {
    let db = open_in_memory().expect("ephemeral DB");
    ensure_tracker_table(&db, DEFAULT_TRACKER_TABLE).unwrap();
    // Simulate a partially-applied DB: v1 (create_sessions) actually
    // applied (table exists, recorded); v2 + v3 not yet.
    db.execute(
        "CREATE TABLE sessions (id INTEGER PRIMARY KEY, user_id TEXT NOT NULL, \
         started_at_unix INTEGER NOT NULL)",
        (),
    )
    .unwrap();
    record_migration(&db, DEFAULT_TRACKER_TABLE, 1, "create_sessions").unwrap();

    // apply_pending should run v2 (create_invites, fresh CREATE TABLE)
    // and v3 (ADD COLUMNs on existing sessions table).
    apply_pending(&db, FIXTURES, ApplyConfig::default()).unwrap();

    let applied = applied_version(&db, DEFAULT_TRACKER_TABLE).unwrap();
    assert_eq!(applied, [1_u32, 2, 3].into_iter().collect());
}

// TV 5 — open/open_in_memory round-trip: ephemeral DB does NOT leak
// files on disk; open(path) does.
#[test]
fn tv5_open_and_open_in_memory_round_trip() {
    // open_in_memory: nothing on disk; queries return rows.
    {
        let db = open_in_memory().expect("ephemeral DB");
        db.execute("CREATE TABLE t (id INTEGER PRIMARY KEY, note TEXT)", ())
            .unwrap();
        db.execute("INSERT INTO t (id, note) VALUES (1, 'in-mem')", ())
            .unwrap();
        let rows = db.query("SELECT note FROM t WHERE id = 1", ()).unwrap();
        let row = rows.into_iter().next().unwrap().unwrap();
        let note: String = row.get(0).unwrap();
        assert_eq!(note, "in-mem");
    }

    // open(path): tempdir-roundtrip proves persistence.
    let dir = std::env::temp_dir().join(format!(
        "octo-storage-core-tv5-{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("round.db");
    let path_str = path.to_str().expect("tempdir path is utf8");

    {
        let db = open(path_str).expect("first open");
        db.execute("CREATE TABLE t (id INTEGER PRIMARY KEY, note TEXT)", ())
            .unwrap();
        db.execute("INSERT INTO t (id, note) VALUES (1, 'persisted')", ())
            .unwrap();
    }

    {
        let db = open(path_str).expect("reopen");
        let rows = db.query("SELECT note FROM t WHERE id = 1", ()).unwrap();
        let row = rows.into_iter().next().unwrap().unwrap();
        let note: String = row.get(0).unwrap();
        assert_eq!(note, "persisted");
    }

    let _ = std::fs::remove_dir_all(&dir);
}
