//! Adversarial fixture per RFC-0206 v2.1 §Format Bypass Defense:
//! an AdapterAllowlist constructed with no registered tables or DDL
//! templates MUST reject every typed statement.

use octo_storage_core::typed_statement::{DdlOperation, DdlTemplate, SqlSelect, TypedStatement};
use octo_storage_core::{AdapterAllowlist, AdapterId, SubstrateError};

#[test]
fn empty_allowlist_rejects_select() {
    let al = AdapterAllowlist::new(AdapterId::new("octo-reputation-storage/v1"));
    let stmt = TypedStatement::Select(SqlSelect {
        tables: vec!["reputation_signals".to_owned()],
    });
    let err = al
        .check(&stmt)
        .expect_err("empty allowlist must reject SELECT");
    assert!(matches!(err, SubstrateError::TableNotInNamespace { .. }));
}

#[test]
fn empty_allowlist_rejects_any_ddl_template() {
    let al = AdapterAllowlist::new(AdapterId::new("octo-reputation-storage/v1"));
    let stmt = TypedStatement::DdlRegistered(DdlTemplate {
        id: "any_template_id".to_owned(),
        operation: DdlOperation::CreateTable,
    });
    let err = al
        .check(&stmt)
        .expect_err("empty allowlist must reject any DDL template");
    assert!(matches!(err, SubstrateError::DdlNotInAllowlist { .. }));
}

#[test]
fn empty_allowlist_passes_ddl_no_op() {
    // DdlNoOp is the substrate's typed-surface witness that no DDL was
    // attempted; an empty allowlist MUST let it through so legitimate
    // no-op dispatch (e.g. read-only queries that don't touch any
    // table) does not fail spuriously.
    let al = AdapterAllowlist::new(AdapterId::new("octo-reputation-storage/v1"));
    al.check(&TypedStatement::DdlNoOp)
        .expect("DdlNoOp must pass empty allowlist (no DDL attempted)");
}
