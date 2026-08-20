//! Adversarial fixture per RFC-0206 v2.1 §Format Bypass Defense:
//! format!() injection in a DdlRegistered template id MUST be rejected —
//! the substrate's AdapterAllowlist uses exact string matching on the
//! template id, so any rendered template id that does not match a
//! registered entry must yield SubstrateError::DdlNotInAllowlist.

use octo_storage_core::typed_statement::{DdlOperation, DdlTemplate, TypedStatement};
use octo_storage_core::SubstrateError;
use octo_vault_storage::build_allowlist;

#[test]
fn format_injection_in_template_id_rejected() {
    let al = build_allowlist();
    // Simulate a caller trying to inject SQL via format!() into the
    // template id field. The rendered string must NOT match a registered
    // DDL template id; allowlist check should fail.
    let injected = format!("create_evil; DROP TABLE {} CASCADE--", "vaults");
    let stmt = TypedStatement::DdlRegistered(DdlTemplate {
        id: injected,
        operation: DdlOperation::CreateTable,
    });
    let err = al
        .check(&stmt)
        .expect_err("format!() injection MUST fail allowlist check");
    assert!(matches!(err, SubstrateError::DdlNotInAllowlist { .. }));
}

#[test]
fn backtick_and_quote_in_template_id_rejected() {
    let al = build_allowlist();
    let stmt = TypedStatement::DdlRegistered(DdlTemplate {
        id: "`; DROP TABLE vaults; --".to_owned(),
        operation: DdlOperation::Drop,
    });
    let err = al
        .check(&stmt)
        .expect_err("quote + DROP injection MUST fail allowlist check");
    assert!(matches!(err, SubstrateError::DdlNotInAllowlist { .. }));
}

#[test]
fn newline_injection_in_template_id_rejected() {
    let al = build_allowlist();
    let stmt = TypedStatement::DdlRegistered(DdlTemplate {
        id: "create_t\nDROP TABLE vaults".to_owned(),
        operation: DdlOperation::CreateTable,
    });
    let err = al
        .check(&stmt)
        .expect_err("newline injection MUST fail allowlist check");
    assert!(matches!(err, SubstrateError::DdlNotInAllowlist { .. }));
}
