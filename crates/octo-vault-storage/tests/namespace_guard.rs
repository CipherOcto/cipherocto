//! TV-0206-A12 — `namespace_guard.rs` for `octo-vault-storage`.
//!
//! Per RFC-0206 v2.1 §Test Vectors:
//! > `ls crates/octo-{vault,reputation,cap-macaroon-vault,matrix-session-store,policy}-storage/tests/ | grep -c namespace_guard` equals 5
//!
//! Workspace query targeting a table outside this adapter's
//! namespace → `SubstrateError::TableNotInNamespace`.

use octo_storage_core::typed_statement::{
    SqlDelete, SqlInsert, SqlSelect, SqlUpdate, TypedStatement,
};
use octo_storage_core::SubstrateError;
use octo_vault_storage::build_allowlist;

#[test]
fn select_outside_namespace_rejected() {
    let al = build_allowlist();
    let stmt = TypedStatement::Select(SqlSelect {
        tables: vec!["not_vaults".to_owned()],
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
fn vaults_table_in_namespace_passes() {
    let al = build_allowlist();
    let stmt = TypedStatement::Select(SqlSelect {
        tables: vec!["vaults".to_owned()],
    });
    al.check(&stmt).expect("registered table must pass");
}
