//! TV-C1 verify-time invariant fixtures (RFC-0957 §20.6.1, mission 0957-g).
//!
//! Four canonical scenarios for `Macaroon::verify_for_vault_op`:
//!
//! - **TV-C1-01** — `Caveat::Vault(vault_id)` verifies when the vault
//!   row exists in the lookup with matching `chain_id` and `is_active=true`.
//! - **TV-C1-02** — `Caveat::Vault(vault_id)` rejects with
//!   `VaultVerifyError::VaultRowMissing { vault_id }` when
//!   `lookup_vault` returns `None`.
//! - **TV-C1-03** — `WrappedOnly` chain WHERE the parent carries a
//!   `Caveat::Vault(vault_id)` verifies when the vault chain matches the
//!   operation's target chain (algorithm step 3 walks ancestors).
//! - **TV-C1-04** — `WrappedOnly` chain WITHOUT any ancestor `Caveat::Vault`
//!   (chainless parent) rejects with
//!   `VaultVerifyError::WrappedChainHasNoVault` (review doc §20.6.1
//!   — "safe default; chains must be explicit").
//!
//! ## Test catalog + lookup
//!
//! `InMemoryCatalog` + `InMemoryLookup` live behind `#[cfg(test)]` in
//! their source modules and are NOT accessible from integration tests.
//! These fixtures define local test-only stand-ins (mirroring the same
//! minimal interface). The production substrate-backed adapters
//! (`OctoVaultLookup` etc.) land in the S5.1 follow-on glue crate (see
//! memory card `mission-0957-g-verify-time-invariant-status`).
//!
//! ## Determinism
//!
//! All inputs are byte-pinned constants (`TV_C1_*`). No RNG; the issuer
//! root secret is a fixed `TV_C1_ROOT_SECRET`. macaroon_id + chain +
//! capability_id are derived deterministically per RFC-0957 §3.2.
//! Re-running the fixture reproduces the exact same `verify_for_vault_op`
//! verdict bit-for-bit.

#![allow(missing_docs)] // fixtures self-document via test names

use std::collections::{HashMap, HashSet};

use octo_cap_macaroon::{
    compute_capability_id, Caveat, Macaroon, MacaroonError, VaultLookup, VaultRowSnapshot,
    VaultVerifyError,
};
use octo_determin::Dqa;

// ===========================================================================
// Test fixtures: byte-pinned constants
// ===========================================================================

/// Issuer root secret (32 bytes). Fixed for fixture determinism.
const TV_C1_ROOT_SECRET: [u8; 32] = [0x77; 32];

/// Target operation chain (32 bytes). All TV-C1 fixtures target this
/// chain. `lookup_vault` rows MUST have matching `chain_id` for
/// `Ok(())` outcomes.
const TV_C1_OP_CHAIN_ID: [u8; 32] = [0xAA; 32];

/// Non-matching chain (32 bytes). Used by TV-C1-05 (deviation test) +
/// any future chain-mismatch fixture.
#[allow(dead_code)]
const TV_C1_OTHER_CHAIN_ID: [u8; 32] = [0xBB; 32];

/// Vault id bound to the `Caveat::Vault(vault_id)` in TV-C1-01..03.
const TV_C1_VAULT_ID: [u8; 32] = [0xCC; 32];

/// Second vault id (unused in current fixtures; reserved for future
/// "multiple Vault caveats" tests).
#[allow(dead_code)]
const TV_C1_VAULT_ID_2: [u8; 32] = [0xDD; 32];

// ===========================================================================
// Test-only stand-ins for InMemoryCatalog + VaultLookup
// ===========================================================================

/// Minimal `CapabilityCatalog` impl backed by a `HashMap`. Mirrors
/// `InMemoryCatalog` (defined behind `#[cfg(test)]` in
/// `crates/octo-cap-macaroon/src/macaroon.rs`). Used by TV-C1-03 + 04
/// to register the parent macaroon so `WrappedOnly` chain walks resolve.
struct TestCatalog {
    by_id: HashMap<[u8; 32], Macaroon>,
    raw_names: HashSet<String>,
}

impl TestCatalog {
    fn new() -> Self {
        Self {
            by_id: HashMap::new(),
            raw_names: HashSet::new(),
        }
    }
    fn insert(&mut self, m: Macaroon) {
        self.by_id.insert(compute_capability_id(&m), m);
    }
}

impl octo_cap_macaroon::CapabilityCatalog for TestCatalog {
    fn lookup(&self, id: &[u8; 32]) -> Option<Macaroon> {
        self.by_id.get(id).cloned()
    }
    fn is_raw_name_registered(&self, name: &str) -> bool {
        self.raw_names.contains(name)
    }
}

/// Minimal `VaultLookup` impl backed by a `HashMap`. Mirrors the
/// `InMemoryLookup` defined behind `#[cfg(test)]` in
/// `crates/octo-cap-macaroon/src/vault_lookup.rs`.
struct TestVaultLookup {
    rows: HashMap<[u8; 32], VaultRowSnapshot>,
}

impl TestVaultLookup {
    fn new() -> Self {
        Self {
            rows: HashMap::new(),
        }
    }
    fn insert(&mut self, vault_id: [u8; 32], snapshot: VaultRowSnapshot) {
        self.rows.insert(vault_id, snapshot);
    }
}

impl VaultLookup for TestVaultLookup {
    fn lookup_vault(&self, vault_id: &[u8; 32]) -> Option<VaultRowSnapshot> {
        self.rows.get(vault_id).copied()
    }
}

// ===========================================================================
// Fixture builders
// ===========================================================================

/// Mint a root macaroon + attenuate with `Caveat::Vault(TV_C1_VAULT_ID)`.
/// Returns the macaroon alongside a populated [`TestCatalog`] (used by
/// all four TV-C1 fixtures — even non-`WrappedOnly` ones pass an empty
/// catalog, which is sound because `Vault` doesn't reference a parent).
fn mint_vault_mac_with_catalog() -> (Macaroon, TestCatalog) {
    let mut catalog = TestCatalog::new();
    let m = Macaroon::mint(&TV_C1_ROOT_SECRET).expect("mint");
    // Note: Vault caveat has no parent reference; attenuate does not need
    // catalog insertion to succeed. We pass the empty catalog.
    let m = m
        .attenuate(Caveat::Vault(TV_C1_VAULT_ID), &catalog)
        .expect("attenuate with Vault");
    // Register the vault-bearing macaroon so any future WrappedOnly child
    // can resolve its parent id (TV-C1-03, 04).
    catalog.insert(m.clone());
    (m, catalog)
}

// ===========================================================================
// TV-C1-01: Vault caveat + lookup hit → Ok(())
// ===========================================================================

#[test]
fn tv_c1_01_vault_caveat_verifies_when_row_active_and_chain_matches() {
    let (mac, catalog) = mint_vault_mac_with_catalog();
    let mut lookup = TestVaultLookup::new();
    lookup.insert(
        TV_C1_VAULT_ID,
        VaultRowSnapshot {
            chain_id: TV_C1_OP_CHAIN_ID,
            is_active: true,
        },
    );
    let result = mac.verify_for_vault_op(
        &TV_C1_ROOT_SECRET,
        &catalog,
        None,
        &TV_C1_OP_CHAIN_ID,
        &lookup,
    );
    assert!(result.is_ok(), "TV-C1-01 must verify: got {result:?}");
}

// ===========================================================================
// TV-C1-02: Vault caveat + lookup miss → VaultRowMissing
// ===========================================================================

#[test]
fn tv_c1_02_vault_caveat_rejects_when_row_missing() {
    let (mac, catalog) = mint_vault_mac_with_catalog();
    let lookup = TestVaultLookup::new(); // empty — no rows
    let result = mac.verify_for_vault_op(
        &TV_C1_ROOT_SECRET,
        &catalog,
        None,
        &TV_C1_OP_CHAIN_ID,
        &lookup,
    );
    match result {
        Err(VaultVerifyError::VaultRowMissing { vault_id }) => {
            assert_eq!(vault_id, TV_C1_VAULT_ID);
        }
        other => panic!("TV-C1-02 must reject with VaultRowMissing: got {other:?}"),
    }
}

// ===========================================================================
// TV-C1-03: WrappedOnly chain WITH parent Vault → Ok(())
// ===========================================================================

#[test]
fn tv_c1_03_wrapped_only_chain_with_parent_vault_verifies() {
    // Step 1: build parent macaroon with Caveat::Vault(TV_C1_VAULT_ID).
    let mut catalog = TestCatalog::new();
    let parent_root = Macaroon::mint(&TV_C1_ROOT_SECRET).expect("parent mint");
    let parent_root_id = compute_capability_id(&parent_root);
    let parent = parent_root
        .attenuate(Caveat::Vault(TV_C1_VAULT_ID), &catalog)
        .expect("parent attenuate");
    let parent_id = compute_capability_id(&parent);
    catalog.insert(parent.clone());

    // Step 2: mint child + attenuate with WrappedOnly{parent_capability: parent_id}.
    // The child's root_secret_hash is independent of the parent's; the
    // chain walks via the parent's `id` only (not signature). We use the
    // same root secret for fixture simplicity (the catalog is the source
    // of truth for the parent's MACAR signature during verify_signature).
    let child_root = Macaroon::mint(&TV_C1_ROOT_SECRET).expect("child mint");
    let child = child_root
        .attenuate(
            Caveat::WrappedOnly {
                parent_capability: parent_id,
            },
            &catalog,
        )
        .expect("child attenuate with WrappedOnly");

    // Step 3: vault lookup hit + chain match.
    let mut lookup = TestVaultLookup::new();
    lookup.insert(
        TV_C1_VAULT_ID,
        VaultRowSnapshot {
            chain_id: TV_C1_OP_CHAIN_ID,
            is_active: true,
        },
    );
    let result = child.verify_for_vault_op(
        &TV_C1_ROOT_SECRET,
        &catalog,
        None,
        &TV_C1_OP_CHAIN_ID,
        &lookup,
    );
    assert!(
        result.is_ok(),
        "TV-C1-03 must verify (parent Vault binds the chain): got {result:?}"
    );
    // Reference unused root id to silence the warning.
    let _ = parent_root_id;
}

// ===========================================================================
// TV-C1-04: WrappedOnly chain WITHOUT parent Vault → WrappedChainHasNoVault
// ===========================================================================

#[test]
fn tv_c1_04_wrapped_only_chain_without_parent_vault_rejects() {
    // Step 1: parent has NO Caveat::Vault — only some other caveat
    // (AmountMax) to keep it a valid attenuation. The WrappedOnly parent
    // is "chainless" per review doc §20.6.1.
    let mut catalog = TestCatalog::new();
    let parent_root = Macaroon::mint(&TV_C1_ROOT_SECRET).expect("parent mint");
    let parent = parent_root
        .attenuate(
            Caveat::AmountMax(Dqa::new(1_000_000, 0).unwrap()), // some cap, no Vault binding
            &catalog,
        )
        .expect("parent attenuate (AmountMax, no Vault)");
    let parent_id = compute_capability_id(&parent);
    catalog.insert(parent.clone());

    // Step 2: child with WrappedOnly → chainless parent.
    let child_root = Macaroon::mint(&TV_C1_ROOT_SECRET).expect("child mint");
    let child = child_root
        .attenuate(
            Caveat::WrappedOnly {
                parent_capability: parent_id,
            },
            &catalog,
        )
        .expect("child attenuate with WrappedOnly");

    // Step 3: lookup is irrelevant — algorithm step 3 walks the chain
    // BEFORE consulting the lookup, and finds NO `Caveat::Vault`
    // anywhere. Should short-circuit with `WrappedChainHasNoVault`.
    let lookup = TestVaultLookup::new();
    let result = child.verify_for_vault_op(
        &TV_C1_ROOT_SECRET,
        &catalog,
        None,
        &TV_C1_OP_CHAIN_ID,
        &lookup,
    );
    match result {
        Err(VaultVerifyError::WrappedChainHasNoVault) => {
            // expected
        }
        other => panic!("TV-C1-04 must reject with WrappedChainHasNoVault: got {other:?}"),
    }
}

// ===========================================================================
// Companion tests: regression coverage for adjacent invariants
// ===========================================================================

/// Regression: `verify_for_vault_op` rejects a non-Active vault row
/// (`is_active = false`). Catches future refactors that might skip
/// the state check.
#[test]
fn verify_for_vault_op_rejects_frozen_vault() {
    let (mac, catalog) = mint_vault_mac_with_catalog();
    let mut lookup = TestVaultLookup::new();
    lookup.insert(
        TV_C1_VAULT_ID,
        VaultRowSnapshot {
            chain_id: TV_C1_OP_CHAIN_ID,
            is_active: false,
        },
    );
    let result = mac.verify_for_vault_op(
        &TV_C1_ROOT_SECRET,
        &catalog,
        None,
        &TV_C1_OP_CHAIN_ID,
        &lookup,
    );
    assert!(matches!(
        result,
        Err(VaultVerifyError::VaultNotActive { vault_id })
        if vault_id == TV_C1_VAULT_ID
    ));
}

/// Regression: `verify_for_vault_op` rejects a vault row whose `chain_id`
/// does not match the operation's target chain. Catches future refactors
/// that might drop the chain-equality check.
#[test]
fn verify_for_vault_op_rejects_chain_mismatch() {
    let (mac, catalog) = mint_vault_mac_with_catalog();
    let mut lookup = TestVaultLookup::new();
    lookup.insert(
        TV_C1_VAULT_ID,
        VaultRowSnapshot {
            chain_id: TV_C1_OTHER_CHAIN_ID,
            is_active: true,
        },
    );
    let result = mac.verify_for_vault_op(
        &TV_C1_ROOT_SECRET,
        &catalog,
        None,
        &TV_C1_OP_CHAIN_ID,
        &lookup,
    );
    assert!(matches!(
        result,
        Err(VaultVerifyError::ChainMismatch { vault_chain, op_chain })
        if vault_chain == TV_C1_OTHER_CHAIN_ID && op_chain == TV_C1_OP_CHAIN_ID
    ));
}

/// Regression: `verify_for_vault_op` surfaces `MacaroonError` variants
/// (e.g., wrong root secret) as `VaultVerifyError::Macaroon(...)`. Catches
/// future refactors that might leak `MacaroonError` directly across the
/// verify boundary.
#[test]
fn verify_for_vault_op_wraps_macaroon_errors() {
    let (mac, catalog) = mint_vault_mac_with_catalog();
    let lookup = TestVaultLookup::new();
    let wrong_root = [0x00; 32];
    let result = mac.verify_for_vault_op(&wrong_root, &catalog, None, &TV_C1_OP_CHAIN_ID, &lookup);
    assert!(
        matches!(
            result,
            Err(VaultVerifyError::Macaroon(
                MacaroonError::RootSecretMismatch
            ))
        ),
        "wrong root secret must surface as Macaroon(RootSecretMismatch): got {result:?}"
    );
}
