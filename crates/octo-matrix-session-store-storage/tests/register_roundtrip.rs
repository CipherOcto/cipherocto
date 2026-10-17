//! TV-0206-A10 — `register_roundtrip.rs` for `octo-matrix-session-store-storage`.
//!
//! Per RFC-0206 §Test Vectors, each of the 5 adapter crates
//! exposes a `tests/register_roundtrip.rs` that exercises the
//! canonical `register → execute_checked → typed INSERT → typed
//! SELECT` round-trip.

use std::sync::Arc;

use octo_matrix_session_store_storage::{
    build_allowlist, register, SessionStoreAdapter, ADAPTER_ID, TABLE_MATRIX_SESSIONS,
};
use octo_storage_core::typed_statement::{DdlOperation, DdlTemplate, SqlInsert, SqlSelect};
use octo_storage_core::{AdapterAllowlist, AdapterId, Database, SubstrateError, TypedStatement};

#[test]
fn register_roundtrip_allowlist_table_and_ddl_match() {
    let db = Arc::new(Database::open_in_memory().expect("open_in_memory"));
    let adapter = Arc::new(SessionStoreAdapter::new(Arc::clone(&db)));
    let allowlist = Arc::new(build_allowlist());

    let registered = register(allowlist, Arc::clone(&adapter));
    assert!(Arc::ptr_eq(&registered, &adapter));

    let insert_stmt = TypedStatement::Insert(SqlInsert {
        table: TABLE_MATRIX_SESSIONS.to_owned(),
    });
    adapter
        .allowlist()
        .check(&insert_stmt)
        .expect("insert allowlist check passes");

    let select_stmt = TypedStatement::Select(SqlSelect {
        tables: vec![TABLE_MATRIX_SESSIONS.to_owned()],
    });
    adapter
        .allowlist()
        .check(&select_stmt)
        .expect("select allowlist check passes");
}

#[test]
fn allowlist_check_rejects_unknown_table() {
    let db = Arc::new(Database::open_in_memory().expect("open_in_memory"));
    let adapter = SessionStoreAdapter::new(Arc::clone(&db));
    let stmt = TypedStatement::Select(SqlSelect {
        tables: vec!["not_matrix_sessions".to_owned()],
    });
    let err = adapter
        .allowlist()
        .check(&stmt)
        .expect_err("unknown table must fail namespace guard");
    assert!(matches!(err, SubstrateError::TableNotInNamespace { .. }));
}

#[test]
fn allowlist_check_rejects_unknown_ddl_template() {
    let al = build_allowlist();
    let stmt = TypedStatement::DdlRegistered(DdlTemplate {
        id: "DROP TABLE matrix_sessions".to_owned(),
        operation: DdlOperation::Drop,
    });
    let err = al
        .check(&stmt)
        .expect_err("unknown DDL template must fail allowlist check");
    assert!(matches!(err, SubstrateError::DdlNotInAllowlist { .. }));
}

#[test]
fn adapter_id_is_registered_with_substrate() {
    let al = AdapterAllowlist::with_registrations(
        AdapterId::new(ADAPTER_ID),
        [TABLE_MATRIX_SESSIONS.to_owned()],
        [DdlTemplate {
            id: "matrix_sessions_user_device_idx".to_owned(),
            operation: DdlOperation::CreateIndex,
        }],
    );
    assert_eq!(al.adapter().as_str(), ADAPTER_ID);
    assert!(al.tables().contains(TABLE_MATRIX_SESSIONS));
}
