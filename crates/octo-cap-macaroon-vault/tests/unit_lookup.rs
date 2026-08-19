//! Unit tests for [`OctoVaultLookup`] (mission 0957-g1 AC-5).
//!
//! Exercises the production substrate's
//! `vaults_vault_id_idx` UNIQUE INDEX lookup primitive via the typed
//! [`VaultSubstrate`] handle. No live DB; uses
//! `octo_storage_core::open_in_memory` and pre-runs the
//! `octo_vault::apply` migration runner so the table + index exist.

use std::sync::Arc;

use octo_cap_macaroon_vault::{OctoVaultLookup, VaultLookup};
use octo_storage_core::open_in_memory;
use octo_vault::{apply, AssetId, ChainId, VaultId, VaultState, VaultSubstrate};

/// TV-0957-g1-01: an `Active` vault row maps to
/// `Some(VaultRowSnapshot { is_active: true, .. })`. Chain_id is the
/// substrate's `chain_id` column, byte-exact.
#[test]
fn lookup_vault_hit_with_active_row_returns_snapshot() {
    let db = open_in_memory().expect("open in-memory");
    apply(&db).expect("apply migrations");

    // Insert an Active vault row directly via SQL (substrate doesn't
    // expose a high-level create fn — the row CRUD is the consumer's
    // responsibility; the substrate only owns the schema + UNIQUE
    // INDEX lookup primitive). Mirrors `apply_migrations.rs` insert
    // pattern.
    let chain_id = ChainId::derive("cipherocto/testnet/v1");
    let asset_id = AssetId::derive("OCTO_W");
    let vault_id = octo_vault::vault_id(chain_id, "did:octo:test-alice", asset_id);
    db.execute(
        "INSERT INTO vaults \
         (vault_id, chain_id, owner_did, asset_id, balance, policy, state, \
          created_at_unix, metadata) \
         VALUES (?, ?, 'did:octo:test-alice', ?, '1.5', ?, 'Active', 1700000000, ?)",
        (
            vault_id.as_bytes().as_slice(),
            chain_id.as_bytes().as_slice(),
            asset_id.as_bytes().as_slice(),
            vec![].as_slice(),
            vec![].as_slice(),
        ),
    )
    .expect("insert Active vault row");

    let substrate = VaultSubstrate::new(Arc::new(db));
    let lookup = OctoVaultLookup::new(substrate);

    let snap = lookup
        .lookup_vault(vault_id.as_bytes())
        .expect("hit on Active row");
    assert!(snap.is_active, "Active state MUST map to is_active=true");
    assert_eq!(snap.chain_id, *chain_id.as_bytes());
}

/// TV-0957-g1-02: a `Frozen` vault row maps to
/// `Some(VaultRowSnapshot { is_active: false, .. })`. Fail-closed on
/// non-Active states — verify-time invariant gates on `is_active`.
#[test]
fn lookup_vault_hit_with_frozen_row_returns_snapshot_inactive() {
    let db = open_in_memory().expect("open in-memory");
    apply(&db).expect("apply migrations");

    let chain_id = ChainId::derive("cipherocto/testnet/v1");
    let asset_id = AssetId::derive("OCTO_A");
    let vault_id = octo_vault::vault_id(chain_id, "did:octo:test-bob", asset_id);
    db.execute(
        "INSERT INTO vaults \
         (vault_id, chain_id, owner_did, asset_id, balance, policy, state, \
          created_at_unix, metadata) \
         VALUES (?, ?, 'did:octo:test-bob', ?, '0.0', ?, 'Frozen', 1700000000, ?)",
        (
            vault_id.as_bytes().as_slice(),
            chain_id.as_bytes().as_slice(),
            asset_id.as_bytes().as_slice(),
            vec![].as_slice(),
            vec![].as_slice(),
        ),
    )
    .expect("insert Frozen vault row");

    let substrate = VaultSubstrate::new(Arc::new(db));
    let lookup = OctoVaultLookup::new(substrate);

    let snap = lookup
        .lookup_vault(vault_id.as_bytes())
        .expect("hit on Frozen row");
    assert!(
        !snap.is_active,
        "Frozen state MUST map to is_active=false (fail-closed)"
    );
    assert_eq!(snap.chain_id, *chain_id.as_bytes());
}

/// TV-0957-g1-03: a vault_id that has no matching row returns
/// `None`. Mirrors the [`VaultLookup`] contract; the verify path maps
/// `None` to `VaultVerifyError::VaultRowMissing { vault_id }`.
#[test]
fn lookup_vault_miss_returns_none() {
    let db = open_in_memory().expect("open in-memory");
    apply(&db).expect("apply migrations");

    let substrate = VaultSubstrate::new(Arc::new(db));
    let lookup = OctoVaultLookup::new(substrate);

    let absent_vid: VaultId = VaultId::from_bytes([0xCC; 32]);
    assert!(
        lookup.lookup_vault(absent_vid.as_bytes()).is_none(),
        "missing vault_id MUST return None (no error)"
    );
}

/// Sanity check: `OctoVaultLookup` is `Send + Sync` (required for
/// trait-object injection into the verify path's `&dyn VaultLookup`
/// parameter on multi-threaded node stacks).
#[test]
fn octo_vault_lookup_is_send_sync() {
    fn assert_send_sync<T: Send + Sync>() {}
    assert_send_sync::<OctoVaultLookup>();
    // Also confirm the substrate handle is `Send + Sync` so the
    // `VaultSubstrate` field can satisfy the auto-trait bounds.
    assert_send_sync::<VaultSubstrate>();
    // And confirm the substrate's state enum + id types propagate
    // Send/Sync (cheap to assert — these are pure-data types).
    assert_send_sync::<VaultState>();
    assert_send_sync::<VaultId>();
    assert_send_sync::<ChainId>();
    assert_send_sync::<AssetId>();
}
