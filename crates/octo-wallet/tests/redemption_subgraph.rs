//! Subgraph check at capability redemption (RFC-0967 §5).
//!
//! `is_subgraph` ensures `capability ⊆ policy` — a capability cannot
//! exceed the policy it claims to authorize. Without this check, an
//! attacker with a `PolicyReference` caveat could mint capabilities that
//! violate the parent policy.

use octo_wallet::capability::caveat::Caveat;
use octo_wallet::capability::macaroon::{CapabilityCatalog, Macaroon};
use octo_wallet::capability::redemption::{redeem_capability, PolicyCatalog, RedemptionError};
use octo_wallet::capability::CapabilityToken;
use octo_wallet::identity::IdentityKey;

use cipherocto_policy::{PolicyObject, PolicySurface};

use std::collections::{HashMap, HashSet};

/// Empty `CapabilityCatalog` for tests that don't use `WrappedOnly` caveats.
/// `CapabilityCatalog` is in scope but `InMemoryCatalog` is `cfg(test)`-only
/// (unit tests inside the crate), so we inline a stub for integration tests.
#[derive(Debug, Default)]
struct EmptyCatalog;

impl CapabilityCatalog for EmptyCatalog {
    fn get(&self, _id: &[u8; 32]) -> Option<&Macaroon> {
        None
    }
}

/// In-memory `PolicyCatalog` for integration tests. Symmetric to the
/// `cfg(test)`-only `InMemoryPolicyCatalog` in the lib (also gated) — the
/// integration test cannot reach into the crate's `cfg(test)` items, so we
/// inline a stub.
#[derive(Debug, Default)]
struct TestPolicyCatalog {
    by_id: HashMap<[u8; 32], PolicyObject>,
}

impl TestPolicyCatalog {
    fn insert(&mut self, policy: PolicyObject) {
        self.by_id.insert(policy.policy_id, policy);
    }
}

impl PolicyCatalog for TestPolicyCatalog {
    fn get(&self, policy_id: &[u8; 32]) -> Option<&PolicyObject> {
        self.by_id.get(policy_id)
    }
}

// -----------------------------------------------------------------------------
// Helpers — build a fresh capability + policy catalog per test.
// -----------------------------------------------------------------------------

fn fresh_root_secret() -> [u8; 32] {
    [0x42; 32]
}

fn fresh_holder() -> IdentityKey {
    IdentityKey::generate().expect("identity key gen")
}

fn build_capability(caveats: Vec<Caveat>) -> CapabilityToken {
    let holder = fresh_holder();
    let catalog = EmptyCatalog;
    CapabilityToken::mint(
        &fresh_root_secret(),
        &holder,
        "did:octo:test",
        caveats,
        &catalog,
    )
    .expect("mint capability")
}

fn policy_with_max_total(max: u128) -> PolicyObject {
    let surface = PolicySurface {
        allowed_models: None,
        allowed_providers: None,
        per_axis_caps: Vec::new(),
        max_total_spend: Some(max),
        audit_window_secs: 0,
        allowed_destinations: None,
    };
    PolicyObject::mint_surface(surface, [0u8; 32], 1_700_000_000)
}

fn policy_with_models(models: &[&str]) -> PolicyObject {
    let surface = PolicySurface {
        allowed_models: Some(
            models
                .iter()
                .map(ToString::to_string)
                .collect::<HashSet<_>>(),
        ),
        allowed_providers: None,
        per_axis_caps: Vec::new(),
        max_total_spend: None,
        audit_window_secs: 0,
        allowed_destinations: None,
    };
    PolicyObject::mint_surface(surface, [0u8; 32], 1_700_000_000)
}

// -----------------------------------------------------------------------------
// Task 1.2 — redeem_capability behavior tests.
// -----------------------------------------------------------------------------

#[test]
fn redeem_rejects_capability_exceeding_policy() {
    // Policy caps total spend at 100_000; capability claims 1_000_000.
    // is_subgraph should reject (cap_exceeds_policy).
    let org_policy = policy_with_max_total(100_000);
    let org_id = org_policy.policy_id;

    let cap = build_capability(vec![
        Caveat::PolicyReference {
            policy_id: org_id,
            policy_version_seq: 1,
            attenuation_witness: [0u8; 64],
        },
        Caveat::AmountMax(1_000_000),
    ]);

    let mut catalog = TestPolicyCatalog::default();
    catalog.insert(org_policy);

    let err = redeem_capability(&cap, &catalog).unwrap_err();
    assert_eq!(
        err,
        RedemptionError::PolicyNotSuperseded {
            cap_id: cap.macaroon.id,
            policy_id: org_id,
        }
    );
}

#[test]
fn redeem_accepts_capability_within_policy() {
    // Policy caps total spend at 1_000_000; capability claims 500_000
    // (narrower ⇒ subgraph holds).
    let org_policy = policy_with_max_total(1_000_000);
    let org_id = org_policy.policy_id;

    let cap = build_capability(vec![
        Caveat::PolicyReference {
            policy_id: org_id,
            policy_version_seq: 1,
            attenuation_witness: [0u8; 64],
        },
        Caveat::AmountMax(500_000),
    ]);

    let mut catalog = TestPolicyCatalog::default();
    catalog.insert(org_policy);

    redeem_capability(&cap, &catalog).expect("within-policy capability should redeem");
}

#[test]
fn redeem_rejects_missing_policy_reference() {
    // No PolicyReference caveat ⇒ error.
    let cap = build_capability(vec![Caveat::AmountMax(1_000)]);

    let catalog = TestPolicyCatalog::default();
    let err = redeem_capability(&cap, &catalog).unwrap_err();
    assert_eq!(err, RedemptionError::MissingPolicyReference);
}

#[test]
fn redeem_rejects_policy_not_found_in_catalog() {
    let bogus_policy_id = [0xab; 32];

    let cap = build_capability(vec![Caveat::PolicyReference {
        policy_id: bogus_policy_id,
        policy_version_seq: 1,
        attenuation_witness: [0u8; 64],
    }]);

    let catalog = TestPolicyCatalog::default();
    let err = redeem_capability(&cap, &catalog).unwrap_err();
    assert_eq!(
        err,
        RedemptionError::PolicyNotFound {
            policy_id: bogus_policy_id,
        }
    );
}

#[test]
fn redeem_rejects_model_not_in_policy() {
    // Policy allows gpt-4 only; capability claims claude-3.
    let org_policy = policy_with_models(&["gpt-4"]);
    let org_id = org_policy.policy_id;

    let cap = build_capability(vec![
        Caveat::PolicyReference {
            policy_id: org_id,
            policy_version_seq: 1,
            attenuation_witness: [0u8; 64],
        },
        Caveat::Model("claude-3".to_owned()),
    ]);

    let mut catalog = TestPolicyCatalog::default();
    catalog.insert(org_policy);

    let err = redeem_capability(&cap, &catalog).unwrap_err();
    assert_eq!(
        err,
        RedemptionError::PolicyNotSuperseded {
            cap_id: cap.macaroon.id,
            policy_id: org_id,
        }
    );
}

#[test]
fn redeem_accepts_model_in_policy() {
    let org_policy = policy_with_models(&["gpt-4", "claude-3"]);
    let org_id = org_policy.policy_id;

    let cap = build_capability(vec![
        Caveat::PolicyReference {
            policy_id: org_id,
            policy_version_seq: 1,
            attenuation_witness: [0u8; 64],
        },
        Caveat::Model("gpt-4".to_owned()),
    ]);

    let mut catalog = TestPolicyCatalog::default();
    catalog.insert(org_policy);

    redeem_capability(&cap, &catalog).expect("model in policy should redeem");
}

// -----------------------------------------------------------------------------
// Task 1.3 — end-to-end via CapabilityToken::redeem (holder sig + subgraph).
// -----------------------------------------------------------------------------

#[test]
fn capability_redeem_runs_holder_sig_then_subgraph_check() {
    let org_policy = policy_with_max_total(100_000);
    let org_id = org_policy.policy_id;

    // Build a capability whose AmountMax exceeds the policy.
    let cap = build_capability(vec![
        Caveat::PolicyReference {
            policy_id: org_id,
            policy_version_seq: 1,
            attenuation_witness: [0u8; 64],
        },
        Caveat::AmountMax(1_000_000),
    ]);

    let mut catalog = TestPolicyCatalog::default();
    catalog.insert(org_policy);

    let err = cap.redeem(&catalog).unwrap_err();
    assert_eq!(
        err,
        RedemptionError::PolicyNotSuperseded {
            cap_id: cap.macaroon.id,
            policy_id: org_id,
        }
    );
}

#[test]
fn capability_redeem_accepts_in_policy_capability() {
    let org_policy = policy_with_max_total(1_000_000);
    let org_id = org_policy.policy_id;

    let cap = build_capability(vec![
        Caveat::PolicyReference {
            policy_id: org_id,
            policy_version_seq: 1,
            attenuation_witness: [0u8; 64],
        },
        Caveat::AmountMax(500_000),
    ]);

    let mut catalog = TestPolicyCatalog::default();
    catalog.insert(org_policy);

    cap.redeem(&catalog)
        .expect("in-policy capability should redeem");
}
