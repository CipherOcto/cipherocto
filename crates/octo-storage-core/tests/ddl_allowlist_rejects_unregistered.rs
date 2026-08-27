//! TV-0206-A5: substrate-level DDL allowlist runtime enforcement.
//!
//! Per RFC-0206 §Format Bypass Defense + §Substrate Newtype Refactor,
//! the substrate refuses to dispatch arbitrary DDL through
//! `Database::execute_checked`; the only legitimate DDL path is a
//! pre-registered [`DdlTemplate`]. This test pins the runtime
//! enforcement contract: an unregistered `DdlRegistered` statement
//! must surface as `SubstrateError::DdlNotInAllowlist`.

use octo_storage_core::typed_statement::{DdlOperation, DdlTemplate, SqlSelect, TypedStatement};
use octo_storage_core::{AdapterAllowlist, AdapterId, Database};

#[test]
fn ddl_registered_against_unregistered_template_rejected() {
    let db = Database::open_in_memory().expect("open_in_memory");
    let allowlist = AdapterAllowlist::with_registrations(
        AdapterId::new("tv-a5"),
        ["registered_table".to_owned()],
        [DdlTemplate {
            id: "registered_ddl".to_owned(),
            operation: DdlOperation::CreateTable,
        }],
    );
    let stmt = TypedStatement::DdlRegistered(DdlTemplate {
        id: "rogue_ddl".to_owned(),
        operation: DdlOperation::Drop,
    });
    let err = db
        .execute_checked(&allowlist, &stmt)
        .expect_err("unregistered DDL template must be rejected");
    match err {
        octo_storage_core::SubstrateError::DdlNotInAllowlist { adapter, template } => {
            assert_eq!(adapter, "tv-a5");
            assert_eq!(template, "rogue_ddl");
        }
        other => panic!("expected DdlNotInAllowlist, got {other:?}"),
    }
}

#[test]
fn ddl_registered_with_matching_template_passes() {
    let db = Database::open_in_memory().expect("open_in_memory");
    let allowlist = AdapterAllowlist::with_registrations(
        AdapterId::new("tv-a5"),
        std::iter::empty::<String>(),
        [DdlTemplate {
            id: "create_x".to_owned(),
            operation: DdlOperation::CreateTable,
        }],
    );
    let stmt = TypedStatement::DdlRegistered(DdlTemplate {
        id: "create_x".to_owned(),
        operation: DdlOperation::CreateTable,
    });
    db.execute_checked(&allowlist, &stmt)
        .expect("registered DDL template must pass the allowlist");
}

#[test]
fn typed_query_against_unregistered_table_rejected() {
    // Companion test: the typed-query path enforces the same
    // per-adapter namespace contract. Unregistered table → rejected.
    let db = Database::open_in_memory().expect("open_in_memory");
    let allowlist = AdapterAllowlist::with_registrations(
        AdapterId::new("tv-a5"),
        ["registered_table".to_owned()],
        std::iter::empty::<DdlTemplate>(),
    );
    let stmt = TypedStatement::Select(SqlSelect {
        tables: vec!["unregistered_table".to_owned()],
    });
    let err = db
        .execute_checked(&allowlist, &stmt)
        .expect_err("unregistered table must be rejected");
    assert!(matches!(
        err,
        octo_storage_core::SubstrateError::TableNotInNamespace { .. }
    ));
}

#[test]
fn ddl_no_op_passes_without_registration() {
    // DdlNoOp is the safe-by-construction variant: the substrate
    // treats it as free to dispatch regardless of registration. This
    // pins the contract so a regression that gates DdlNoOp behind the
    // DDL allowlist would break legitimate idempotent no-op paths.
    let db = Database::open_in_memory().expect("open_in_memory");
    let allowlist = AdapterAllowlist::new(AdapterId::new("tv-a5"));
    db.execute_checked(&allowlist, &TypedStatement::DdlNoOp)
        .expect("DdlNoOp must pass without registration");
}
