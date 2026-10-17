//! TV-0206-A10 — `register_roundtrip.rs` for
//! `octo-cap-macaroon-vault-storage`.
//!
//! Per RFC-0206 §Test Vectors, each of the 5 adapter crates
//! exposes a `tests/register_roundtrip.rs` that exercises the
//! canonical `register → execute_checked → typed SELECT` round-trip.
//! For the vault-lookup adapter the round-trip is a `VaultLookup`
//! lookup against an empty DB (no row → `None`).

use std::sync::Arc;

use octo_cap_macaroon::VaultLookup;
use octo_cap_macaroon_vault_storage::{
    build_allowlist, register, VaultLookupAdapter, ADAPTER_ID, TABLE_VAULTS,
};
use octo_storage_core::typed_statement::{DdlOperation, DdlTemplate, SqlSelect};
use octo_storage_core::{AdapterAllowlist, AdapterId, Database, SubstrateError, TypedStatement};
use octo_vault_storage::VaultStore;

#[test]
fn register_roundtrip_allowlist_table_and_ddl_match() {
    let db = Arc::new(Database::open_in_memory().expect("open_in_memory"));
    let vault_store = Arc::new(VaultStore::new(Arc::clone(&db)));
    let adapter = Arc::new(VaultLookupAdapter::new(Arc::clone(&db), vault_store));
    let allowlist = Arc::new(build_allowlist());

    let registered = register(allowlist, Arc::clone(&adapter));
    assert!(Arc::ptr_eq(&registered, &adapter));

    let select_stmt = TypedStatement::Select(SqlSelect {
        tables: vec![TABLE_VAULTS.to_owned()],
    });
    adapter
        .allowlist()
        .check(&select_stmt)
        .expect("select allowlist check passes");
}

#[test]
fn vault_lookup_on_empty_db_returns_none() {
    let db = Arc::new(Database::open_in_memory().expect("open_in_memory"));
    db.execute(
        "CREATE TABLE vaults (vault_id BLOB PRIMARY KEY, chain_id BLOB NOT NULL, state TEXT NOT NULL)",
        (),
    )
    .expect("CREATE TABLE vaults");

    let vault_store = Arc::new(VaultStore::new(Arc::clone(&db)));
    let adapter = VaultLookupAdapter::new(Arc::clone(&db), vault_store);

    let result = adapter.lookup_vault(&[0xAB; 32]);
    assert!(result.is_none());
}

#[test]
fn allowlist_check_rejects_unknown_table() {
    let db = Arc::new(Database::open_in_memory().expect("open_in_memory"));
    let vault_store = Arc::new(VaultStore::new(Arc::clone(&db)));
    let adapter = VaultLookupAdapter::new(Arc::clone(&db), vault_store);

    let stmt = TypedStatement::Select(SqlSelect {
        tables: vec!["not_vaults".to_owned()],
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
        id: "DROP TABLE vaults".to_owned(),
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
        [TABLE_VAULTS.to_owned()],
        [DdlTemplate {
            id: "vaults_vault_id_idx".to_owned(),
            operation: DdlOperation::CreateIndex,
        }],
    );
    assert_eq!(al.adapter().as_str(), ADAPTER_ID);
    assert!(al.tables().contains(TABLE_VAULTS));
}
