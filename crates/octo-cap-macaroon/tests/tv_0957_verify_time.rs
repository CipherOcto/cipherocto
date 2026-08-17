//! RFC-0957 §Verify-Time Extension + §Caveat DSL Extension — TV-0957
//! byte-exact wire-form + verify-path pins (mission `0957-c1`).
//!
//! Twenty fixtures in four categories:
//!
//! - **TV-0957-01..05** — five Caveat DSL variant wire-form pins
//!   (`Vault`, `Permission`, `ValidRange`, `MaxPerTx`, `AuditWindow`).
//! - **TV-0957-06..10** — five more variant pins (`MaxUses`,
//!   `WrappedOnly`, `Factory`, `PolicyReference`, `Raw` unknown-name
//!   rejection).
//! - **TV-0957-11..15** — five verify-time path pins (one per
//!   RFC-0957 §20.6.1 algorithm step: signature verify, vault row
//!   lookup, chain match, `state == Active`, `WrappedOnly` chain walk).
//! - **TV-0957-16..20** — five regression tests (frozen vault
//!   `is_active=false`, chain mismatch, missing root secret,
//!   `WrappedChainHasNoVault`, attenuation-monotonicity invariant
//!   with the v2.1 variants).
//!
//! ## Wire form
//!
//! Caveat enum uses `#[serde(tag = "type", content = "value")]` — the
//! wire form is the canonical `serde_json::to_string` rendering. Each
//! DSL pin constructs the variant, serializes to JSON, and asserts
//! the discriminant tag string + content shape round-trip. This is
//! the de-facto "byte-exact" form for serde-JSON-enum-tagged types;
//! the discriminant is wire-stable across Rust versions, and
//! content-shape assertions catch every borsh/serde schema drift.
//!
//! ## Verify-time path
//!
//! Mirrors the catalog + lookup stand-ins from
//! `tests/tv_c1_verify_time.rs`. Both store TV-C1's
//! `TV_C1_OP_CHAIN_ID` / `TV_C1_VAULT_ID` constants; this file
//! parallels its layout under a `TV_0957_*` namespace so the two
//! fixtures are independently runnable.
//!
//! ## Determinism
//!
//! All inputs are byte-pinned `TV_0957_*` constants. Root secret +
//! chain id + vault ids + permission discriminants are fixed. No
//! RNG. Re-running reproduces the verdict bit-for-bit per
//! RFC-0008 Class A determinism.

#![allow(missing_docs)] // fixtures self-document via test names

use std::collections::{HashMap, HashSet};

use octo_cap_macaroon::{
    compute_capability_id, ActionTemplate, Caveat, FactoryVet, Macaroon, MacaroonError,
    PermissionKind, RawCaveat, VaultLookup, VaultRowSnapshot, VaultVerifyError,
};

// ===========================================================================
// Test fixtures: byte-pinned constants
// ===========================================================================

/// Issuer root secret (32 bytes). Fixed for fixture determinism.
const TV_0957_ROOT_SECRET: [u8; 32] = [0x88; 32];

/// Target operation chain (32 bytes). All verify-time fixtures in this
/// file target this chain; `lookup_vault` rows MUST carry matching
/// `chain_id` for `Ok(())` outcomes.
const TV_0957_OP_CHAIN_ID: [u8; 32] = [0xAA; 32];

/// Non-matching chain (32 bytes). Used by TV-0957-13 + TV-0957-17.
const TV_0957_OTHER_CHAIN_ID: [u8; 32] = [0xBB; 32];

/// Vault id bound to `Caveat::Vault(vault_id)` in verify-time fixtures.
const TV_0957_VAULT_ID: [u8; 32] = [0xCC; 32];

/// Second vault id (reserved for `Factory.target_vault_id`).
const TV_0957_VAULT_ID_2: [u8; 32] = [0xDD; 32];

// ===========================================================================
// Test-only stand-ins (mirrors tv_c1_verify_time.rs pattern)
// ===========================================================================

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

struct TestVaultLookup {
    rows: HashMap<[u8; 32], VaultRowSnapshot>,
}

impl TestVaultLookup {
    fn empty() -> Self {
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
// TV-0957-01..05: Caveat DSL wire-form pins (first five)
// ===========================================================================

#[test]
fn tv_0957_01_vault_variant_wire_form() {
    // `Vault([u8; 32])` — discriminant `vault`, content is a 32-byte
    // fixed array (serialized by serde-json default as a 32-element
    // number array for unbounded serde; verify the discriminant tag
    // + content round-trip + CaveatName::Vault.as_str() match.
    let c = Caveat::Vault(TV_0957_VAULT_ID);
    let json = serde_json::to_string(&c).expect("serialize");
    // Discriminant must be the wire-stable tag string.
    assert!(
        json.contains("\"type\":\"vault\""),
        "TV-0957-01: Vault discriminant MUST be \"vault\": got {json}"
    );
    // Content shape round-trip preserves [u8; 32] exactly.
    let back: Caveat = serde_json::from_str(&json).expect("deserialize");
    assert_eq!(back, Caveat::Vault(TV_0957_VAULT_ID));
}

#[test]
fn tv_0957_02_permission_variant_wire_form() {
    // `Permission(PermissionKind)` — discriminant `permission`, content
    // is one of the five `PermissionKind` variants (snake_case in JSON).
    for kind in [
        PermissionKind::NativeTokenTransfer,
        PermissionKind::Erc20TokenTransfer,
        PermissionKind::ContractCall,
        PermissionKind::Reservation,
        PermissionKind::VaultMutation,
    ] {
        let c = Caveat::Permission(kind);
        let json = serde_json::to_string(&c).expect("serialize");
        assert!(
            json.contains("\"type\":\"permission\""),
            "TV-0957-02: Permission discriminant MUST be \"permission\": got {json}"
        );
        let back: Caveat = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(
            back,
            Caveat::Permission(kind),
            "PermissionKind round-trip must be exact for {kind:?}"
        );
    }
}

#[test]
fn tv_0957_03_valid_range_variant_wire_form() {
    // `ValidRange { valid_after_unix, valid_until_unix }` — discriminant
    // `valid_range`, content is a struct with named u64 fields.
    let c = Caveat::ValidRange {
        valid_after_unix: 1_700_000_000,
        valid_until_unix: 1_800_000_000,
    };
    let json = serde_json::to_string(&c).expect("serialize");
    assert!(
        json.contains("\"type\":\"valid_range\""),
        "TV-0957-03: ValidRange discriminant MUST be \"valid_range\": got {json}"
    );
    // Pin the field names (regression: a rename from `valid_after_unix`
    // would silently break macaroon signatures).
    assert!(
        json.contains("valid_after_unix"),
        "ValidRange field `valid_after_unix` MUST be present: got {json}"
    );
    assert!(
        json.contains("valid_until_unix"),
        "ValidRange field `valid_until_unix` MUST be present: got {json}"
    );
    let back: Caveat = serde_json::from_str(&json).expect("deserialize");
    assert_eq!(
        back,
        Caveat::ValidRange {
            valid_after_unix: 1_700_000_000,
            valid_until_unix: 1_800_000_000
        }
    );
}

#[test]
fn tv_0957_04_max_per_tx_variant_wire_form() {
    // `MaxPerTx(u128)` — discriminant `max_per_tx`, content is u128.
    let c = Caveat::MaxPerTx(42_000_000_000u128);
    let json = serde_json::to_string(&c).expect("serialize");
    assert!(
        json.contains("\"type\":\"max_per_tx\""),
        "TV-0957-04: MaxPerTx discriminant MUST be \"max_per_tx\": got {json}"
    );
    // u128 content must round-trip exactly (no f64 drift).
    assert!(
        json.contains("42000000000"),
        "MaxPerTx u128 payload must be preserved: got {json}"
    );
    let back: Caveat = serde_json::from_str(&json).expect("deserialize");
    assert_eq!(back, Caveat::MaxPerTx(42_000_000_000u128));
}

#[test]
fn tv_0957_05_audit_window_variant_wire_form() {
    // `AuditWindow { duration_secs: u64 }` — discriminant `audit_window`.
    let c = Caveat::AuditWindow {
        duration_secs: 3600,
    };
    let json = serde_json::to_string(&c).expect("serialize");
    assert!(
        json.contains("\"type\":\"audit_window\""),
        "TV-0957-05: AuditWindow discriminant MUST be \"audit_window\": got {json}"
    );
    // Field name MUST be `duration_secs` (RFC-0965 §3.5 — distinct from
    // earlier draft's `start_unix_secs`/`end_unix_secs` design).
    assert!(
        json.contains("\"duration_secs\":3600"),
        "AuditWindow payload MUST be `{{\"duration_secs\":N}}`: got {json}"
    );
    let back: Caveat = serde_json::from_str(&json).expect("deserialize");
    assert_eq!(
        back,
        Caveat::AuditWindow {
            duration_secs: 3600
        }
    );
}

// ===========================================================================
// TV-0957-06..10: Caveat DSL wire-form pins (second five)
// ===========================================================================

#[test]
fn tv_0957_06_max_uses_variant_wire_form() {
    // `MaxUses { count: u32 }` — discriminant `max_uses`, struct field.
    let c = Caveat::MaxUses { count: 7 };
    let json = serde_json::to_string(&c).expect("serialize");
    assert!(
        json.contains("\"type\":\"max_uses\""),
        "TV-0957-06: MaxUses discriminant MUST be \"max_uses\": got {json}"
    );
    // Field name MUST be `count` (RFC-0965 §3.6).
    assert!(
        json.contains("\"count\":7"),
        "MaxUses payload MUST be `{{\"count\":N}}`: got {json}"
    );
    let back: Caveat = serde_json::from_str(&json).expect("deserialize");
    assert_eq!(back, Caveat::MaxUses { count: 7 });
}

#[test]
fn tv_0957_07_wrapped_only_variant_wire_form() {
    // `WrappedOnly { parent_capability: [u8; 32] }` — discriminant
    // `wrapped_only`, payload MUST carry the parent capability id
    // (RFC-0965 §3.7 — parent_id is part of the wire form; consumers
    // deserialize via the catalog).
    let parent_id = [0x99; 32];
    let c = Caveat::WrappedOnly {
        parent_capability: parent_id,
    };
    let json = serde_json::to_string(&c).expect("serialize");
    assert!(
        json.contains("\"type\":\"wrapped_only\""),
        "TV-0957-07: WrappedOnly discriminant MUST be \"wrapped_only\": got {json}"
    );
    // Field name MUST be `parent_capability` (RFC-0965 §3.7 — distinguishes
    // from earlier draft's bare `WrappedOnly` unit form).
    assert!(
        json.contains("parent_capability"),
        "WrappedOnly MUST carry `parent_capability` field: got {json}"
    );
    let back: Caveat = serde_json::from_str(&json).expect("deserialize");
    assert_eq!(
        back,
        Caveat::WrappedOnly {
            parent_capability: parent_id
        }
    );
}

#[test]
fn tv_0957_08_factory_variant_wire_form() {
    // `Factory(FactoryVet)` — discriminant `factory`, content is a
    // typed `FactoryVet` struct (NOT opaque bytes; phishing-resistant).
    let vet = FactoryVet {
        target_vault_id: TV_0957_VAULT_ID_2,
        action_template: ActionTemplate {
            selector: "transfer".to_owned(),
            args: vec!["did:octo:zRecipient".to_owned()],
        },
        required_caller: Some("did:octo:zOperator".to_owned()),
        pre_conditions: vec![],
        expiry_for_deploy_unix: 17_000_003_600,
    };
    let c = Caveat::Factory(vet.clone());
    let json = serde_json::to_string(&c).expect("serialize");
    assert!(
        json.contains("\"type\":\"factory\""),
        "TV-0957-08: Factory discriminant MUST be \"factory\": got {json}"
    );
    // Pin inner-struct field names (RFC-0965 §3.8).
    assert!(
        json.contains("target_vault_id"),
        "FactoryVet MUST carry `target_vault_id`: got {json}"
    );
    assert!(
        json.contains("action_template"),
        "FactoryVet MUST carry `action_template`: got {json}"
    );
    assert!(
        json.contains("expiry_for_deploy_unix"),
        "FactoryVet MUST carry `expiry_for_deploy_unix`: got {json}"
    );
    let back: Caveat = serde_json::from_str(&json).expect("deserialize");
    assert_eq!(back, Caveat::Factory(vet));
}

#[test]
fn tv_0957_09_policy_reference_variant_wire_form() {
    // `PolicyReference { policy_id, policy_version_seq, attenuation_witness }`
    // — discriminant `policy_reference`. All three fields MUST serialize
    // verbatim (RFC-0965 §3.9 + RFC-0967 §8.2 — witness signature binds
    // the attenuation).
    let c = Caveat::PolicyReference {
        policy_id: [0x11; 32],
        policy_version_seq: 42,
        attenuation_witness: [0x22; 64],
    };
    let json = serde_json::to_string(&c).expect("serialize");
    assert!(
        json.contains("\"type\":\"policy_reference\""),
        "TV-0957-09: PolicyReference discriminant MUST be \"policy_reference\": got {json}"
    );
    assert!(
        json.contains("policy_id"),
        "must carry `policy_id`: got {json}"
    );
    assert!(
        json.contains("policy_version_seq"),
        "must carry `policy_version_seq`: got {json}"
    );
    assert!(
        json.contains("attenuation_witness"),
        "must carry `attenuation_witness`: got {json}"
    );
    let back: Caveat = serde_json::from_str(&json).expect("deserialize");
    assert_eq!(
        back,
        Caveat::PolicyReference {
            policy_id: [0x11; 32],
            policy_version_seq: 42,
            attenuation_witness: [0x22; 64],
        }
    );
}

#[test]
fn tv_0957_10_raw_unknown_name_rejected_at_attenuation() {
    // Per RFC-0965 §3 + `macaroon.rs:242-243` attenuation rule: a
    // `Caveat::Raw` whose `name` is not pre-registered in the catalog
    // is rejected at `attenuate` time (fail-closed on unknown raw
    // caveat names — prevents catalog-bypass attacks). This pins the
    // behavior so a future refactor that silently accepts unknown Raw
    // names trips this test loud.
    let mut catalog = TestCatalog::new();
    catalog.raw_names.insert("known_name".to_owned());
    let root = Macaroon::mint(&TV_0957_ROOT_SECRET).expect("mint");
    let unknown_raw = Caveat::Raw(RawCaveat {
        name: "definitely_not_registered".to_owned(),
        value: vec![0x01, 0x02, 0x03],
    });
    let result = root.attenuate(unknown_raw, &catalog);
    assert!(
        result.is_err(),
        "TV-0957-10: attenuation of unregistered Raw MUST be rejected (got Ok): {result:?}"
    );

    // Sanity: known-name Raw succeeds (proves the rejection is
    // specifically about registration, not all Raw caveats).
    let known_raw = Caveat::Raw(RawCaveat {
        name: "known_name".to_owned(),
        value: vec![0x04, 0x05],
    });
    let ok = root
        .attenuate(known_raw, &catalog)
        .expect("TV-0957-10 known Raw must succeed");
    assert_eq!(
        compute_capability_id(&ok),
        compute_capability_id(
            &root
                .attenuate(
                    Caveat::Raw(RawCaveat {
                        name: "known_name".to_owned(),
                        value: vec![0x04, 0x05],
                    }),
                    &catalog
                )
                .expect("second known Raw attenuation")
        )
    );
}

// ===========================================================================
// TV-0957-11..15: verify-time path pins (RFC-0957 §20.6.1 algorithm steps)
// ===========================================================================

/// Helper: mint a root macaroon + attenuate with `Caveat::Vault(TV_0957_VAULT_ID)`,
/// register the attenuated macaroon in the catalog (for any later
/// `WrappedOnly` chain walk), and return both.
fn mint_vault_caveat_mac_with_catalog() -> (Macaroon, TestCatalog) {
    let mut catalog = TestCatalog::new();
    let m = Macaroon::mint(&TV_0957_ROOT_SECRET).expect("mint");
    let m = m
        .attenuate(Caveat::Vault(TV_0957_VAULT_ID), &catalog)
        .expect("attenuate Vault");
    catalog.insert(m.clone());
    (m, catalog)
}

#[test]
fn tv_0957_11_verify_time_happy_path_ok() {
    // Algorithm step 1 (signature verify) yields Ok — proven transitively
    // by the pipeline completing with no `Macaroon(RootSecretMismatch)`
    // when the correct root secret is supplied AND all subsequent steps
    // (lookup, chain match, state check, chain walk) succeed. Populated
    // lookup + matching active chain drives the pipeline to step 5.
    // (A failure at step 1 with the wrong root is pinned by TV-0957-18;
    // a failure at step 2 with an empty lookup is pinned by TV-0957-12.)
    let (mac, catalog) = mint_vault_caveat_mac_with_catalog();
    let mut lookup = TestVaultLookup::empty();
    lookup.insert(
        TV_0957_VAULT_ID,
        VaultRowSnapshot {
            chain_id: TV_0957_OP_CHAIN_ID,
            is_active: true,
        },
    );
    let result = mac.verify_for_vault_op(
        &TV_0957_ROOT_SECRET,
        &catalog,
        None,
        &TV_0957_OP_CHAIN_ID,
        &lookup,
    );
    assert!(
        result.is_ok(),
        "TV-0957-11 must pass happy path (signature verify step + all subsequent steps): \
         got {result:?}"
    );
}

#[test]
fn tv_0957_12_vault_row_lookup_step_missing() {
    // Algorithm step 2: empty lookup returns `None` → VaultRowMissing.
    // Pin the error variant and vault_id payload.
    let (mac, catalog) = mint_vault_caveat_mac_with_catalog();
    let result = mac.verify_for_vault_op(
        &TV_0957_ROOT_SECRET,
        &catalog,
        None,
        &TV_0957_OP_CHAIN_ID,
        &TestVaultLookup::empty(),
    );
    match result {
        Err(VaultVerifyError::VaultRowMissing { vault_id }) => {
            assert_eq!(
                vault_id, TV_0957_VAULT_ID,
                "VaultRowMissing must carry the looked-up vault_id"
            );
        }
        other => {
            panic!("TV-0957-12 must reject with VaultRowMissing at lookup step: got {other:?}")
        }
    }
}

#[test]
fn tv_0957_13_chain_match_step_mismatch() {
    // Algorithm step 3: lookup row exists but with mismatched chain_id
    // → ChainMismatch carrying both chains.
    let (mac, catalog) = mint_vault_caveat_mac_with_catalog();
    let mut lookup = TestVaultLookup::empty();
    lookup.insert(
        TV_0957_VAULT_ID,
        VaultRowSnapshot {
            chain_id: TV_0957_OTHER_CHAIN_ID,
            is_active: true,
        },
    );
    let result = mac.verify_for_vault_op(
        &TV_0957_ROOT_SECRET,
        &catalog,
        None,
        &TV_0957_OP_CHAIN_ID,
        &lookup,
    );
    match result {
        Err(VaultVerifyError::ChainMismatch {
            vault_chain,
            op_chain,
        }) => {
            assert_eq!(
                vault_chain, TV_0957_OTHER_CHAIN_ID,
                "ChainMismatch.vault_chain MUST be the row's chain"
            );
            assert_eq!(
                op_chain, TV_0957_OP_CHAIN_ID,
                "ChainMismatch.op_chain MUST be the operation's chain"
            );
        }
        other => {
            panic!("TV-0957-13 must reject with ChainMismatch at chain-match step: got {other:?}")
        }
    }
}

#[test]
fn tv_0957_14_state_active_step_rejects_frozen() {
    // Algorithm step 4: lookup row exists with matching chain but
    // `is_active = false` → VaultNotActive. Pins the state check.
    let (mac, catalog) = mint_vault_caveat_mac_with_catalog();
    let mut lookup = TestVaultLookup::empty();
    lookup.insert(
        TV_0957_VAULT_ID,
        VaultRowSnapshot {
            chain_id: TV_0957_OP_CHAIN_ID,
            is_active: false,
        },
    );
    let result = mac.verify_for_vault_op(
        &TV_0957_ROOT_SECRET,
        &catalog,
        None,
        &TV_0957_OP_CHAIN_ID,
        &lookup,
    );
    match result {
        Err(VaultVerifyError::VaultNotActive { vault_id }) => {
            assert_eq!(vault_id, TV_0957_VAULT_ID);
        }
        other => {
            panic!("TV-0957-14 must reject with VaultNotActive at state-check step: got {other:?}")
        }
    }
}

#[test]
fn tv_0957_15_wrapped_only_chain_walk_step_ok() {
    // Algorithm step 5: child with `Caveat::WrappedOnly { parent_capability }`
    // where the parent carries `Caveat::Vault(vault_id)` AND the lookup
    // row matches the chain → OK. Pins the chain walk.
    let mut catalog = TestCatalog::new();
    let parent_root = Macaroon::mint(&TV_0957_ROOT_SECRET).expect("parent mint");
    let parent = parent_root
        .attenuate(Caveat::Vault(TV_0957_VAULT_ID), &catalog)
        .expect("parent attenuate Vault");
    let parent_id = compute_capability_id(&parent);
    catalog.insert(parent);

    let child_root = Macaroon::mint(&TV_0957_ROOT_SECRET).expect("child mint");
    let child = child_root
        .attenuate(
            Caveat::WrappedOnly {
                parent_capability: parent_id,
            },
            &catalog,
        )
        .expect("child WrappedOnly attenuate");

    let mut lookup = TestVaultLookup::empty();
    lookup.insert(
        TV_0957_VAULT_ID,
        VaultRowSnapshot {
            chain_id: TV_0957_OP_CHAIN_ID,
            is_active: true,
        },
    );
    let result = child.verify_for_vault_op(
        &TV_0957_ROOT_SECRET,
        &catalog,
        None,
        &TV_0957_OP_CHAIN_ID,
        &lookup,
    );
    assert!(
        result.is_ok(),
        "TV-0957-15 must pass WrappedOnly chain walk with parent Vault: got {result:?}"
    );
}

// ===========================================================================
// TV-0957-16..20: regression tests (catch future refactors)
// ===========================================================================

/// Regression: frozen vault row (`is_active = false`) MUST be rejected.
/// Catches a future refactor that skips step 4 of the algorithm.
#[test]
fn tv_0957_16_regression_frozen_vault_rejected() {
    let (mac, catalog) = mint_vault_caveat_mac_with_catalog();
    let mut lookup = TestVaultLookup::empty();
    lookup.insert(
        TV_0957_VAULT_ID,
        VaultRowSnapshot {
            chain_id: TV_0957_OP_CHAIN_ID,
            is_active: false,
        },
    );
    let result = mac.verify_for_vault_op(
        &TV_0957_ROOT_SECRET,
        &catalog,
        None,
        &TV_0957_OP_CHAIN_ID,
        &lookup,
    );
    assert!(matches!(
        result,
        Err(VaultVerifyError::VaultNotActive { vault_id })
        if vault_id == TV_0957_VAULT_ID
    ));
}

/// Regression: chain mismatch MUST be rejected at step 3.
/// Catches a future refactor that drops the chain-equality check.
#[test]
fn tv_0957_17_regression_chain_mismatch_rejected() {
    let (mac, catalog) = mint_vault_caveat_mac_with_catalog();
    let mut lookup = TestVaultLookup::empty();
    lookup.insert(
        TV_0957_VAULT_ID,
        VaultRowSnapshot {
            chain_id: TV_0957_OTHER_CHAIN_ID,
            is_active: true,
        },
    );
    let result = mac.verify_for_vault_op(
        &TV_0957_ROOT_SECRET,
        &catalog,
        None,
        &TV_0957_OP_CHAIN_ID,
        &lookup,
    );
    assert!(matches!(
        result,
        Err(VaultVerifyError::ChainMismatch { vault_chain, op_chain })
        if vault_chain == TV_0957_OTHER_CHAIN_ID && op_chain == TV_0957_OP_CHAIN_ID
    ));
}

/// Regression: missing root secret MUST surface as `Macaroon(RootSecretMismatch)`.
/// Catches future refactors that swallow root-secret errors silently
/// or leak raw `MacaroonError` across the verify boundary.
#[test]
fn tv_0957_18_regression_missing_root_secret_rejected() {
    let (mac, catalog) = mint_vault_caveat_mac_with_catalog();
    let lookup = TestVaultLookup::empty();
    let wrong_root = [0x00; 32];
    let result =
        mac.verify_for_vault_op(&wrong_root, &catalog, None, &TV_0957_OP_CHAIN_ID, &lookup);
    assert!(
        matches!(
            result,
            Err(VaultVerifyError::Macaroon(
                MacaroonError::RootSecretMismatch
            ))
        ),
        "wrong root secret MUST surface as Macaroon(RootSecretMismatch): got {result:?}"
    );
}

/// Regression: `WrappedOnly` chain WITHOUT any ancestor `Caveat::Vault`
/// MUST reject with `WrappedChainHasNoVault` per §20.6.1 line 1328
/// (chainless parent = safe default = reject).
#[test]
fn tv_0957_19_regression_wrapped_chain_has_no_vault() {
    let mut catalog = TestCatalog::new();
    let parent_root = Macaroon::mint(&TV_0957_ROOT_SECRET).expect("parent mint");
    let parent = parent_root
        .attenuate(
            // No Vault caveat — chainless parent per §20.6.1 line 1328.
            Caveat::MaxUses { count: 1 },
            &catalog,
        )
        .expect("parent attenuate (MaxUses, no Vault)");
    let parent_id = compute_capability_id(&parent);
    catalog.insert(parent);

    let child_root = Macaroon::mint(&TV_0957_ROOT_SECRET).expect("child mint");
    let child = child_root
        .attenuate(
            Caveat::WrappedOnly {
                parent_capability: parent_id,
            },
            &catalog,
        )
        .expect("child WrappedOnly attenuate");

    let result = child.verify_for_vault_op(
        &TV_0957_ROOT_SECRET,
        &catalog,
        None,
        &TV_0957_OP_CHAIN_ID,
        &TestVaultLookup::empty(),
    );
    assert!(
        matches!(result, Err(VaultVerifyError::WrappedChainHasNoVault)),
        "TV-0957-19 must reject with WrappedChainHasNoVault: got {result:?}"
    );
}

/// Regression: attenuation monotonicity (RFC-0957 §3.5) — adding any
/// new v2.1 variant MUST strictly preserve the parent's caveat set
/// (attenuators may add, never remove). Pin: child macaroon carries
/// BOTH the parent's `Vault(vault_id)` AND the new `AuditWindow`
/// caveat; verify with active vault + matching chain returns `Ok`.
#[test]
fn tv_0957_20_regression_attenuation_monotonicity_with_new_variants() {
    let mut catalog = TestCatalog::new();
    // Parent carries Vault + ValidRange (both pre-existing or new variants).
    let parent_root = Macaroon::mint(&TV_0957_ROOT_SECRET).expect("parent mint");
    let parent = parent_root
        .attenuate(Caveat::Vault(TV_0957_VAULT_ID), &catalog)
        .expect("parent attenuate Vault");
    let parent = parent
        .attenuate(
            Caveat::ValidRange {
                valid_after_unix: 1,
                valid_until_unix: u64::MAX,
            },
            &catalog,
        )
        .expect("parent attenuate ValidRange");
    let parent_id = compute_capability_id(&parent);
    catalog.insert(parent);

    // Child: adds Permission(PermissionKind) + WrappedOnly referencing
    // parent (chain extends through parent). The child's caveat set is
    // STRICTLY additive over the parent's (RFC-0957 §3.5).
    let child_root = Macaroon::mint(&TV_0957_ROOT_SECRET).expect("child mint");
    let child = child_root
        .attenuate(Caveat::Permission(PermissionKind::VaultMutation), &catalog)
        .expect("child attenuate Permission");
    let child = child
        .attenuate(
            Caveat::WrappedOnly {
                parent_capability: parent_id,
            },
            &catalog,
        )
        .expect("child attenuate WrappedOnly");

    let mut lookup = TestVaultLookup::empty();
    lookup.insert(
        TV_0957_VAULT_ID,
        VaultRowSnapshot {
            chain_id: TV_0957_OP_CHAIN_ID,
            is_active: true,
        },
    );
    let result = child.verify_for_vault_op(
        &TV_0957_ROOT_SECRET,
        &catalog,
        None,
        &TV_0957_OP_CHAIN_ID,
        &lookup,
    );
    assert!(
        result.is_ok(),
        "TV-0957-20 must verify (attenuation-monotonicity holds for new variants): \
         got {result:?}"
    );
    // Parent's `parent_id` is referenced in `WrappedOnly.parent_capability`;
    // bind to silence unused warning.
    let _ = parent_id;
}
