//! TV-0206-A12 — namespace_guard.rs for octo-policy-storage.
//!
//! Per RFC-0206 §Test Vectors.
//!
//! Workspace query targeting a table outside this adapter's
//! namespace → SubstrateError::TableNotInNamespace.

use octo_policy_storage::{build_allowlist, TABLE_POLICY_OBJECTS};
use octo_storage_core::typed_statement::{
    SqlDelete, SqlInsert, SqlSelect, SqlUpdate, TypedStatement,
};
use octo_storage_core::SubstrateError;

#[test]
fn select_outside_namespace_rejected() {
    let al = build_allowlist();
    let stmt = TypedStatement::Select(SqlSelect {
        tables: vec!["not_policy_objects".to_owned()],
    });
    let err = al
        .check(&stmt)
        .expect_err("table outside adapter namespace must fail");
    assert!(matches!(err, SubstrateError::TableNotInNamespace { .. }));
}

#[test]
fn insert_outside_namespace_rejected() {
    let al = build_allowlist();
    let stmt = TypedStatement::Insert(SqlInsert {
        table: "secrets_table".to_owned(),
    });
    let err = al
        .check(&stmt)
        .expect_err("insert outside namespace must fail");
    assert!(matches!(err, SubstrateError::TableNotInNamespace { .. }));
}

#[test]
fn update_outside_namespace_rejected() {
    let al = build_allowlist();
    let stmt = TypedStatement::Update(SqlUpdate {
        table: "user_pii".to_owned(),
    });
    let err = al
        .check(&stmt)
        .expect_err("update outside namespace must fail");
    assert!(matches!(err, SubstrateError::TableNotInNamespace { .. }));
}

#[test]
fn delete_outside_namespace_rejected() {
    let al = build_allowlist();
    let stmt = TypedStatement::Delete(SqlDelete {
        table: "audit_log".to_owned(),
    });
    let err = al
        .check(&stmt)
        .expect_err("delete outside namespace must fail");
    assert!(matches!(err, SubstrateError::TableNotInNamespace { .. }));
}

#[test]
fn table_in_namespace_passes() {
    let al = build_allowlist();
    let stmt = TypedStatement::Select(SqlSelect {
        tables: vec![TABLE_POLICY_OBJECTS.to_owned()],
    });
    al.check(&stmt).expect("registered table must pass");
}
