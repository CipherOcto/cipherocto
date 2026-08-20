//! TV-0206-A11 — drop_table_negative.rs for octo-reputation-storage.
//!
//! Per RFC-0206 v2.1 §Test Vectors.
//!
//! DdlRegistered(DropTable(...)) against a non-allowlisted DDL
//! template id → SubstrateError::DdlNotInAllowlist.

use octo_reputation_storage::build_allowlist;
use octo_storage_core::typed_statement::{DdlOperation, DdlTemplate, TypedStatement};
use octo_storage_core::SubstrateError;

#[test]
fn drop_table_template_not_in_allowlist_rejected() {
    let al = build_allowlist();
    let stmt = TypedStatement::DdlRegistered(DdlTemplate {
        id: "DROP TABLE reputation_signals".to_owned(),
        operation: DdlOperation::Drop,
    });
    let err = al
        .check(&stmt)
        .expect_err("DROP TABLE template must fail allowlist check");
    assert!(
        matches!(err, SubstrateError::DdlNotInAllowlist { .. }),
        "expected DdlNotInAllowlist, got {err:?}",
    );
}

#[test]
fn known_create_index_template_in_allowlist_passes() {
    let al = build_allowlist();
    let stmt = TypedStatement::DdlRegistered(DdlTemplate {
        id: "reputation_event_id_idx".to_owned(),
        operation: DdlOperation::CreateIndex,
    });
    al.check(&stmt).expect("registered DDL template must pass");
}

#[test]
fn create_table_template_not_in_allowlist_rejected() {
    let al = build_allowlist();
    let stmt = TypedStatement::DdlRegistered(DdlTemplate {
        id: "create_reputation_signals_v2".to_owned(),
        operation: DdlOperation::CreateTable,
    });
    let err = al
        .check(&stmt)
        .expect_err("unknown CREATE TABLE template must fail");
    assert!(matches!(err, SubstrateError::DdlNotInAllowlist { .. }));
}
