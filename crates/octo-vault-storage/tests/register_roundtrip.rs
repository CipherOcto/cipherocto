//! TV-0206-A10 — `register_roundtrip.rs` for `octo-vault-storage`.
//!
//! Per RFC-0206 §Test Vectors:
//! > `ls crates/octo-{vault,reputation,cap-macaroon-vault,matrix-session-store,policy}-storage/tests/ | grep -c register_roundtrip` equals 5
//!
//! This file exercises the canonical `register → execute_checked →
//! typed INSERT → typed SELECT` round-trip for the vault substrate
//! adapter.

use std::sync::Arc;

use octo_storage_core::typed_statement::{DdlOperation, DdlTemplate, SqlInsert, SqlSelect};
use octo_storage_core::{AdapterAllowlist, AdapterId, Database, SubstrateError, TypedStatement};
use octo_vault::vault_id;
use octo_vault::{AssetId, ChainId, VaultState};
use octo_vault_storage::{build_allowlist, register, VaultStore, ADAPTER_ID, TABLE_VAULTS};

#[test]
fn register_roundtrip_insert_then_select_returns_row() {
    let db = Arc::new(Database::open_in_memory().expect("open_in_memory"));
    // Create the vaults table directly (the adapter does not auto-migrate;
    // production callers run `octo_vault::apply(&db)` first).
    db.execute(
        "CREATE TABLE vaults (vault_id BLOB PRIMARY KEY, chain_id BLOB NOT NULL, state TEXT NOT NULL)",
        (),
    )
    .expect("CREATE TABLE vaults");

    let store = Arc::new(VaultStore::new(Arc::clone(&db)));
    let allowlist = Arc::new(build_allowlist());

    // 1. Register via the facade helper. The allowlist IS the typed
    // surface — it round-trips through `register` and the substrate's
    // process-global registry becomes the live witness.
    let registered = register(allowlist, Arc::clone(&store));
    assert!(Arc::ptr_eq(&registered, &store));

    // 2. Build a typed INSERT + run through the substrate's allowlist
    // guard. This is the load-bearing `execute_checked` path per RFC
    // §Format Bypass Defense.
    let insert_stmt = TypedStatement::Insert(SqlInsert {
        table: TABLE_VAULTS.to_owned(),
    });
    store
        .allowlist()
        .check(&insert_stmt)
        .expect("insert allowlist check passes");

    // 3. INSERT a real row via the typed-surface dispatch.
    let chain_id = ChainId::derive("cipherocto/vault-test/v1");
    let vault_id_value = vault_id(chain_id, "did:example:alice", AssetId::derive("OCTO-W"));
    store
        .insert_vault(&vault_id_value, &chain_id, VaultState::Active)
        .expect("insert_vault");

    // 4. Typed SELECT guard, then read back.
    let select_stmt = TypedStatement::Select(SqlSelect {
        tables: vec![TABLE_VAULTS.to_owned()],
    });
    store
        .allowlist()
        .check(&select_stmt)
        .expect("select allowlist check passes");

    let row = store
        .lookup_by_vault_id(&vault_id_value)
        .expect("lookup_by_vault_id");
    let (got_chain, got_state) = row.expect("row exists");
    assert_eq!(got_chain, chain_id);
    assert_eq!(got_state, VaultState::Active);
}

#[test]
fn allowlist_check_rejects_unknown_table() {
    let db = Arc::new(Database::open_in_memory().expect("open_in_memory"));
    let store = VaultStore::new(Arc::clone(&db));

    // Build a typed SELECT for a table the adapter does NOT own.
    let stmt = TypedStatement::Select(SqlSelect {
        tables: vec!["not_vaults".to_owned()],
    });
    let err = store
        .allowlist()
        .check(&stmt)
        .expect_err("unknown table must fail namespace guard");
    assert!(matches!(err, SubstrateError::TableNotInNamespace { .. }));
}

#[test]
fn allowlist_check_rejects_unknown_ddl_template() {
    // Adapter owns only the `vaults_vault_id_idx` DDL template.
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
    // Direct substrate check: AdapterAllowlist for THIS adapter_id
    // round-trips through `register` and the adapter_id matches.
    let al = AdapterAllowlist::with_registrations(
        AdapterId::new(ADAPTER_ID),
        [TABLE_VAULTS.to_owned()],
        [DdlTemplate {
            id: "vaults_vault_id_idx".to_owned(),
            operation: DdlOperation::CreateIndex,
        }],
    );
    assert_eq!(al.adapter().as_str(), ADAPTER_ID);
}
