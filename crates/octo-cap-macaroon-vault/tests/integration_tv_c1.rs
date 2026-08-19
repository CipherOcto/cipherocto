//! Integration tests for [`OctoVaultLookup`] (mission 0957-g1 AC-6).
//!
//! Re-runs the canonical TV-0957-11 (happy path) + TV-0957-12
//! (VaultRowMissing) fixtures with the **production** glue
//! ([`OctoVaultLookup`]) instead of the `TestVaultLookup` stand-in
//! used in `crates/octo-cap-macaroon/tests/tv_0957_verify_time.rs`.
//!
//! Pins that the substrate's `vaults_vault_id_idx` UNIQUE INDEX lookup
//! round-trips through the typed `VaultSubstrate` handle to the
//! `VaultLookup` trait contract that `Macaroon::verify_for_vault_op`
//! consumes — closing the production-wiring gap that the S5 follow-on
//! (mission 0957-g) left behind.
//!
//! ## What these tests prove
//!
//! - **TV-0957-g1-11 (happy path)**: full `Macaroon::verify_for_vault_op`
//!   round-trip with an `Active` vault row inserted via the substrate
//!   table + read back through `OctoVaultLookup` → `VaultRowSnapshot` →
//!   verify-time step 4 (`is_active` check) succeeds.
//! - **TV-0957-g1-12 (VaultRowMissing)**: same macaroon, no row in the
//!   substrate → `OctoVaultLookup::lookup_vault` returns `None` →
//!   verify-time maps to `VaultVerifyError::VaultRowMissing { vault_id }`.
//!
//! ## Determinism
//!
//! All inputs are byte-pinned constants. The substrate's
//! `open_in_memory()` handle is fresh per test (no fixture pollution);
//! `chain_id` + `vault_id` are fixed to align with the canonical TV-0957
//! fixture set so this test is independently runnable + comparable.

#![allow(missing_docs)] // fixtures self-document via test names

use std::sync::Arc;

use octo_cap_macaroon::{
    compute_capability_id, CapabilityCatalog, Caveat, Macaroon, VaultLookup, VaultRowSnapshot,
    VaultVerifyError,
};
use octo_cap_macaroon_vault::OctoVaultLookup;
use octo_storage_core::open_in_memory;
use octo_vault::{apply, AssetId, ChainId, VaultSubstrate};

// ===========================================================================
// Test fixtures: byte-pinned constants (mirror tv_0957_verify_time.rs)
// ===========================================================================

const ROOT_SECRET: [u8; 32] = [0x88; 32];
const OP_CHAIN_ID: [u8; 32] = [0xAA; 32];
const VAULT_ID: [u8; 32] = [0xCC; 32];

/// Minimal in-memory `CapabilityCatalog` stand-in (mirror
/// `TestCatalog` from tv_0957_verify_time.rs but reduced — the
/// verify-time path doesn't exercise `is_raw_name_registered`).
struct MinimalCatalog {
    by_id: std::collections::HashMap<[u8; 32], Macaroon>,
}

impl MinimalCatalog {
    fn new() -> Self {
        Self {
            by_id: std::collections::HashMap::new(),
        }
    }
    fn insert(&mut self, m: Macaroon) {
        self.by_id.insert(compute_capability_id(&m), m);
    }
}

impl CapabilityCatalog for MinimalCatalog {
    fn lookup(&self, id: &[u8; 32]) -> Option<Macaroon> {
        self.by_id.get(id).cloned()
    }
    fn is_raw_name_registered(&self, _name: &str) -> bool {
        // Out of scope for the verify-time lookup path.
        false
    }
}

/// Insert a vault row with `vault_id = VAULT_ID`,
/// `chain_id = OP_CHAIN_ID`, `state = 'Active'`. Returns the typed
/// substrate handle wrapping the same in-memory `Database`.
fn substrate_with_active_vault_row() -> VaultSubstrate {
    let db = open_in_memory().expect("open in-memory");
    apply(&db).expect("apply migrations");

    // Derive chain_id + asset_id + vault_id using the canonical
    // octo-vault derivation (so the substrate accepts the row at the
    // PK + UNIQUE INDEX level) — but for the lookup test we only need
    // `vault_id` to match the macaroon's `Caveat::Vault(VAULT_ID)`.
    // To bypass the derivation (test fixture), we INSERT with the
    // raw `VAULT_ID` bytes directly. The PK is `(chain_id, owner_did,
    // asset_id)` — `vault_id` is UNIQUE INDEX but not PK, so this
    // works.
    let chain_id_raw = OP_CHAIN_ID;
    let asset_id = AssetId::derive("OCTO-W");
    // Derive the row's composite PK's owner_did consistently.
    let owner_did = "did:octo:integration-alice";
    // We use the canonical `vault_id` derivation for owner_did +
    // chain_id + asset_id, but then OVERWRITE the column with our
    // pinned `VAULT_ID` via the second statement (substrate permits
    // arbitrary `vault_id` bytes — the derivation is canonical but
    // the column is BLOB(32) NOT NULL UNIQUE INDEX, not PK).
    let derived = octo_vault::vault_id(ChainId::from_bytes(chain_id_raw), owner_did, asset_id);
    db.execute(
        "INSERT INTO vaults \
         (vault_id, chain_id, owner_did, asset_id, balance, policy, state, \
          created_at_unix, metadata) \
         VALUES (?, ?, ?, ?, '1.5', ?, 'Active', 1700000000, ?)",
        (
            derived.as_bytes().as_slice(),
            chain_id_raw.as_slice(),
            owner_did,
            asset_id.as_bytes().as_slice(),
            vec![].as_slice(),
            vec![].as_slice(),
        ),
    )
    .expect("insert via canonical vault_id");

    // UPDATE the row to use the pinned `VAULT_ID` (the test fixture's
    // lookup key). Same composite PK; `vault_id` is the lookup column.
    db.execute(
        "UPDATE vaults SET vault_id = ? \
         WHERE chain_id = ? AND owner_did = ? AND asset_id = ?",
        (
            VAULT_ID.as_slice(),
            chain_id_raw.as_slice(),
            owner_did,
            asset_id.as_bytes().as_slice(),
        ),
    )
    .expect("overwrite vault_id to pinned fixture");

    VaultSubstrate::new(Arc::new(db))
}

// ===========================================================================
// TV-0957-g1-11: happy path (production OctoVaultLookup)
// ===========================================================================

/// Re-run TV-0957-11 with the production glue. Full
/// `Macaroon::verify_for_vault_op` round-trip: insert an `Active` vault
/// row through the substrate's table, read it back through
/// `OctoVaultLookup`, drive the verify-time algorithm to step 4
/// (`is_active` check), assert Ok.
#[test]
fn tv_0957_g1_11_verify_time_happy_path_with_production_glue() {
    let substrate = substrate_with_active_vault_row();
    let lookup = OctoVaultLookup::new(substrate);

    // Build a macaroon carrying `Caveat::Vault(VAULT_ID)`.
    let mut catalog = MinimalCatalog::new();
    let mac = Macaroon::mint(&ROOT_SECRET)
        .expect("mint")
        .attenuate(Caveat::Vault(VAULT_ID), &catalog)
        .expect("attenuate Vault");
    catalog.insert(mac.clone());

    let result = mac.verify_for_vault_op(&ROOT_SECRET, &catalog, None, &OP_CHAIN_ID, &lookup);
    assert!(
        result.is_ok(),
        "TV-0957-g1-11: happy path with production OctoVaultLookup MUST succeed; got {result:?}"
    );

    // Sanity: the lookup itself returns the expected snapshot.
    let snap: Option<VaultRowSnapshot> = lookup.lookup_vault(&VAULT_ID);
    let snap = snap.expect("hit on inserted row");
    assert_eq!(snap.chain_id, OP_CHAIN_ID);
    assert!(snap.is_active);
}

// ===========================================================================
// TV-0957-g1-12: VaultRowMissing (production OctoVaultLookup)
// ===========================================================================

/// Re-run TV-0957-12 with the production glue. Empty substrate (no
/// `vaults` rows) → `OctoVaultLookup::lookup_vault` returns `None` →
/// verify-time maps to `VaultVerifyError::VaultRowMissing { vault_id }`.
#[test]
fn tv_0957_g1_12_vault_row_lookup_missing_with_production_glue() {
    let db = open_in_memory().expect("open in-memory");
    apply(&db).expect("apply migrations (no rows inserted)");
    let substrate = VaultSubstrate::new(Arc::new(db));
    let lookup = OctoVaultLookup::new(substrate);

    let mut catalog = MinimalCatalog::new();
    let mac = Macaroon::mint(&ROOT_SECRET)
        .expect("mint")
        .attenuate(Caveat::Vault(VAULT_ID), &catalog)
        .expect("attenuate Vault");
    catalog.insert(mac.clone());

    let result = mac.verify_for_vault_op(&ROOT_SECRET, &catalog, None, &OP_CHAIN_ID, &lookup);
    match result {
        Err(VaultVerifyError::VaultRowMissing { vault_id }) => {
            assert_eq!(
                vault_id, VAULT_ID,
                "VaultRowMissing MUST carry the looked-up vault_id"
            );
        }
        other => {
            panic!("TV-0957-g1-12: missing row MUST reject with VaultRowMissing; got {other:?}")
        }
    }
}
