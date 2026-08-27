//! TV-0206-A13: newtype round-trip `From<Database> for stoolap::Database`.
//!
//! Per RFC-0206 §Substrate Newtype Refactor + §Escape Hatch
//! Enumeration, the substrate's `Database` newtype exposes a one-way
//! `From<Database> for stoolap::Database` conversion so legacy
//! migration code paths can reach the underlying Stoolap engine. This
//! test pins the escape hatch contract: construct a `Database`, convert
//! to `stoolap::Database`, exercise a real query.

use octo_storage_core::{AdapterAllowlist, AdapterId, Database, TypedStatement};

#[test]
fn database_into_stoolap_database_roundtrip() {
    // 1. Construct the substrate newtype.
    let db: Database = Database::open_in_memory().expect("open_in_memory");

    // 2. Escape hatch: convert to stoolap::Database (substrate-internal
    //    only per §Escape Hatch Enumeration).
    let inner: stoolap::Database = db.into();

    // 3. Exercise the underlying engine with a real round-trip query.
    inner
        .execute(
            "CREATE TABLE escape_hatch_table (id INTEGER PRIMARY KEY, label TEXT NOT NULL)",
            (),
        )
        .expect("create table via escape hatch");
    inner
        .execute(
            "INSERT INTO escape_hatch_table (id, label) VALUES (1, 'alpha')",
            (),
        )
        .expect("insert via escape hatch");
    let rows = inner
        .query("SELECT label FROM escape_hatch_table WHERE id = 1", ())
        .expect("query via escape hatch");
    let mut got: Vec<String> = Vec::new();
    for r in rows.into_iter() {
        let row = r.expect("row decode");
        let label: String = row.get(0).expect("get label");
        got.push(label);
    }
    assert_eq!(
        got,
        vec!["alpha".to_owned()],
        "round-trip through From<Database> for stoolap::Database must read back the inserted row"
    );
}

#[test]
fn database_deref_reaches_stoolap_database() {
    // Deref is the non-consuming escape hatch: substrate code (and
    // ONLY substrate code, per §Escape Hatch Enumeration) can reach
    // the underlying Stoolap engine via `&db` without consuming the
    // newtype. Pin the Deref<Target = stoolap::Database> contract.
    let db = Database::open_in_memory().expect("open_in_memory");
    let stoolap_ref: &stoolap::Database = &db;
    stoolap_ref.execute("SELECT 1", ()).expect("trivial query");
}

#[test]
fn database_execute_checked_blocks_unregistered_table() {
    // The substrate's typed execution path is the canonical SQL
    // boundary. The escape hatch is for legacy paths ONLY; new code
    // MUST route through `Database::execute_checked` + an
    // `AdapterAllowlist`. This test pins the typed path's behavior
    // alongside the escape hatch tests.
    let db = Database::open_in_memory().expect("open_in_memory");
    let allowlist = AdapterAllowlist::with_registrations(
        AdapterId::new("tv-a13"),
        ["registered".to_owned()],
        std::iter::empty::<octo_storage_core::typed_statement::DdlTemplate>(),
    );
    let stmt = TypedStatement::Insert(octo_storage_core::typed_statement::SqlInsert {
        table: "unregistered".to_owned(),
    });
    let err = db
        .execute_checked(&allowlist, &stmt)
        .expect_err("unregistered table must be rejected");
    match err {
        octo_storage_core::SubstrateError::TableNotInNamespace { adapter, table } => {
            assert_eq!(adapter, "tv-a13");
            assert_eq!(table, "unregistered");
        }
        other => panic!("expected TableNotInNamespace, got {other:?}"),
    }
}
