//! Subgraph check at capability redemption (RFC-0967 §5).
//!
//! `is_subgraph` ensures `capability ⊆ policy` — a capability cannot
//! exceed the policy it claims to authorize. Without this check, an
//! attacker with a `PolicyReference` caveat could mint capabilities that
//! violate the parent policy.

use octo_determin::Dqa;
use octo_wallet::capability::caveat::Caveat;
use octo_wallet::capability::redemption::{
    redeem_capability, redeem_capability_at, redeem_capability_token, PolicyCatalog,
    RedemptionError,
};
use octo_wallet::capability::CapabilityToken;
use octo_wallet::identity::IdentityKey;

use octo_policy::{PolicyObject, PolicySurface};

use std::collections::{HashMap, HashSet};

use octo_ident::test_helpers::sample_did;

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
    let mut k = IdentityKey::generate().expect("identity key gen");
    // Newly-generated identities start in `Designated` state; `mint()` requires
    // `Active` so the holder can sign (per RFC-0009 §Lifecycle row 1).
    k.activate(1_700_000_000).expect("activate");
    k
}

fn build_capability(caveats: &[Caveat]) -> CapabilityToken {
    let holder = fresh_holder();
    CapabilityToken::mint(&fresh_root_secret(), &holder, &sample_did(81), caveats)
        .expect("mint capability")
}

fn policy_with_max_total(max: Dqa) -> PolicyObject {
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
    let org_policy = policy_with_max_total(Dqa::new(100_000, 0).unwrap());
    let org_id = org_policy.policy_id;

    let cap = build_capability(&[
        Caveat::PolicyReference {
            policy_id: org_id,
            policy_version_seq: 1,
            attenuation_witness: [0u8; 64],
        },
        Caveat::AmountMax(Dqa::new(1_000_000, 0).unwrap()),
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
    let org_policy = policy_with_max_total(Dqa::new(1_000_000, 0).unwrap());
    let org_id = org_policy.policy_id;

    let cap = build_capability(&[
        Caveat::PolicyReference {
            policy_id: org_id,
            policy_version_seq: 1,
            attenuation_witness: [0u8; 64],
        },
        Caveat::AmountMax(Dqa::new(500_000, 0).unwrap()),
    ]);

    let mut catalog = TestPolicyCatalog::default();
    catalog.insert(org_policy);

    redeem_capability(&cap, &catalog).expect("within-policy capability should redeem");
}

#[test]
fn redeem_rejects_capability_exceeding_policy_across_scales() {
    // The policy operator authored a cap of 100.0. A cap is a quantity with a
    // scale, so 100.0 is `100_000 @ scale 3`. The capability claims
    // `100_000 @ scale 0` = 100,000.0 — a thousand times the policy.
    //
    // Note both sides are the same *numerator*, 100_000. A scale-blind
    // comparison sees 100_000 > 100_000, decides the child fits, and the
    // capability redeems. Only the scale distinguishes them.
    let org_policy = policy_with_max_total(Dqa::new(100_000, 3).unwrap());
    let org_id = org_policy.policy_id;

    let cap = build_capability(&[
        Caveat::PolicyReference {
            policy_id: org_id,
            policy_version_seq: 1,
            attenuation_witness: [0u8; 64],
        },
        Caveat::AmountMax(Dqa::new(100_000, 0).unwrap()),
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
fn redeem_accepts_capability_within_policy_across_scales() {
    // The mirror image: same two scales, but the *capability* is the tighter
    // one. `100 @ scale 1` = 10.0 fits inside the policy's `100_000 @
    // scale 3` = 100.0, so redemption must succeed. A scale-blind
    // comparison would also pass this one, so it is here to prove the
    // fix is not simply "reject everything that spans scales".
    let org_policy = policy_with_max_total(Dqa::new(100_000, 3).unwrap());
    let org_id = org_policy.policy_id;

    let cap = build_capability(&[
        Caveat::PolicyReference {
            policy_id: org_id,
            policy_version_seq: 1,
            attenuation_witness: [0u8; 64],
        },
        Caveat::AmountMax(Dqa::new(100, 1).unwrap()),
    ]);

    let mut catalog = TestPolicyCatalog::default();
    catalog.insert(org_policy);

    redeem_capability(&cap, &catalog).expect("10.0 fits inside the policy's 100.0");
}

#[test]
fn redeem_applies_the_tighter_of_two_amount_max_caveats_across_scales() {
    // A capability carrying two `AmountMax` caveats has its spend ceiling
    // narrowed to the *smaller quantity*. `1_000 @ scale 3` = 1.0 is tighter
    // than `2 @ scale 0` = 2.0, even though its numerator is five hundred
    // times larger.
    //
    // The narrowing used to take the smaller bare numerator, which picked
    // 2.0. This policy caps at 1.5, which sits deliberately *between* the
    // two: redemption succeeds only if the effective ceiling is 1.0.
    let org_policy = policy_with_max_total(Dqa::new(15, 1).unwrap());
    let org_id = org_policy.policy_id;

    let cap = build_capability(&[
        Caveat::PolicyReference {
            policy_id: org_id,
            policy_version_seq: 1,
            attenuation_witness: [0u8; 64],
        },
        Caveat::AmountMax(Dqa::new(1_000, 3).unwrap()),
        Caveat::AmountMax(Dqa::new(2, 0).unwrap()),
    ]);

    let mut catalog = TestPolicyCatalog::default();
    catalog.insert(org_policy);

    redeem_capability(&cap, &catalog).expect("narrowed ceiling 1.0 fits inside the policy's 1.5");
}

#[test]
fn redeem_rejects_missing_policy_reference() {
    // No PolicyReference caveat ⇒ error.
    let cap = build_capability(&[Caveat::AmountMax(Dqa::new(1_000, 0).unwrap())]);

    let catalog = TestPolicyCatalog::default();
    let err = redeem_capability(&cap, &catalog).unwrap_err();
    assert_eq!(err, RedemptionError::MissingPolicyReference);
}

#[test]
fn redeem_rejects_policy_not_found_in_catalog() {
    let bogus_policy_id = [0xab; 32];

    let cap = build_capability(&[Caveat::PolicyReference {
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

    let cap = build_capability(&[
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

    let cap = build_capability(&[
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
    let org_policy = policy_with_max_total(Dqa::new(100_000, 0).unwrap());
    let org_id = org_policy.policy_id;

    // Build a capability whose AmountMax exceeds the policy.
    let cap = build_capability(&[
        Caveat::PolicyReference {
            policy_id: org_id,
            policy_version_seq: 1,
            attenuation_witness: [0u8; 64],
        },
        Caveat::AmountMax(Dqa::new(1_000_000, 0).unwrap()),
    ]);

    let mut catalog = TestPolicyCatalog::default();
    catalog.insert(org_policy);

    let err = redeem_capability_token(&cap, &catalog).unwrap_err();
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
    let org_policy = policy_with_max_total(Dqa::new(1_000_000, 0).unwrap());
    let org_id = org_policy.policy_id;

    let cap = build_capability(&[
        Caveat::PolicyReference {
            policy_id: org_id,
            policy_version_seq: 1,
            attenuation_witness: [0u8; 64],
        },
        Caveat::AmountMax(Dqa::new(500_000, 0).unwrap()),
    ]);

    let mut catalog = TestPolicyCatalog::default();
    catalog.insert(org_policy);

    redeem_capability_token(&cap, &catalog).expect("in-policy capability should redeem");
}

// -----------------------------------------------------------------------------
// Task H-4 — time caveats are evaluated at redemption.
//
// Before this, `redeem_capability` had no clock, so a capability whose
// `ValidRange` had elapsed still redeemed. The exploit is the second test:
// an EXPIRED capability redeeming successfully is the whole finding.
// -----------------------------------------------------------------------------

fn time_capability(policy_id: [u8; 32], caveats: &[Caveat]) -> CapabilityToken {
    let mut all: Vec<Caveat> = vec![Caveat::PolicyReference {
        policy_id,
        policy_version_seq: 1,
        attenuation_witness: [0u8; 64],
    }];
    all.extend_from_slice(caveats);
    build_capability(&all)
}

#[test]
fn h4_expired_capability_no_longer_redeems() {
    // The finding, verbatim: window [1000, 2000], redeemed at 5000.
    let org = policy_with_max_total(Dqa::new(1_000_000, 0).unwrap());
    let cap = time_capability(
        org.policy_id,
        &[Caveat::ValidRange {
            valid_after_unix: 1_000,
            valid_until_unix: 2_000,
        }],
    );
    let mut catalog = TestPolicyCatalog::default();
    catalog.insert(org);

    let err = redeem_capability_at(&cap, &catalog, 5_000).unwrap_err();
    assert!(
        matches!(err, RedemptionError::OutsideValidityWindow { .. }),
        "an expired capability redeemed: {err:?}"
    );
    assert!(
        err.to_string().contains("expired"),
        "the error must say it expired, got: {err}"
    );
}

#[test]
fn h4_capability_before_its_window_does_not_redeem() {
    let org = policy_with_max_total(Dqa::new(1_000_000, 0).unwrap());
    let cap = time_capability(
        org.policy_id,
        &[Caveat::ValidRange {
            valid_after_unix: 1_000,
            valid_until_unix: 2_000,
        }],
    );
    let mut catalog = TestPolicyCatalog::default();
    catalog.insert(org);

    let err = redeem_capability_at(&cap, &catalog, 500).unwrap_err();
    assert!(
        matches!(err, RedemptionError::OutsideValidityWindow { .. }),
        "a not-yet-valid capability redeemed: {err:?}"
    );
}

#[test]
fn h4_capability_inside_its_window_redeems() {
    // Anti-vacuity: the clock-aware path must still ACCEPT a live capability,
    // otherwise the guard above proves nothing.
    let org = policy_with_max_total(Dqa::new(1_000_000, 0).unwrap());
    let cap = time_capability(
        org.policy_id,
        &[Caveat::ValidRange {
            valid_after_unix: 1_000,
            valid_until_unix: 2_000,
        }],
    );
    let mut catalog = TestPolicyCatalog::default();
    catalog.insert(org);

    redeem_capability_at(&cap, &catalog, 1_500).expect("in-window capability must redeem");
    // Boundaries are inclusive on both ends.
    redeem_capability_at(&cap, &catalog, 1_000).expect("start boundary is inclusive");
    redeem_capability_at(&cap, &catalog, 2_000).expect("end boundary is inclusive");
}

#[test]
fn h4_valid_after_is_enforced() {
    let org = policy_with_max_total(Dqa::new(1_000_000, 0).unwrap());
    let cap = time_capability(
        org.policy_id,
        &[Caveat::ValidAfter {
            not_before_unix: 10_000,
        }],
    );
    let mut catalog = TestPolicyCatalog::default();
    catalog.insert(org);

    assert!(
        redeem_capability_at(&cap, &catalog, 9_999).is_err(),
        "ValidAfter was not enforced"
    );
    redeem_capability_at(&cap, &catalog, 10_000).expect("boundary is inclusive");
}

#[test]
fn h4_unsatisfiable_range_is_rejected_at_every_instant() {
    // RFC-0960 §R5-F2: after > until is unsatisfiable, not wrapping.
    let org = policy_with_max_total(Dqa::new(1_000_000, 0).unwrap());
    let cap = time_capability(
        org.policy_id,
        &[Caveat::ValidRange {
            valid_after_unix: 2_000,
            valid_until_unix: 1_000,
        }],
    );
    let mut catalog = TestPolicyCatalog::default();
    catalog.insert(org);

    for now in [0u64, 1_000, 1_500, 2_000, 9_999] {
        assert!(
            redeem_capability_at(&cap, &catalog, now).is_err(),
            "an unsatisfiable ValidRange redeemed at {now}"
        );
    }
}

#[test]
fn h4_clock_less_redemption_refuses_a_time_bearing_capability() {
    // Fail-closed: without a `now`, there is no honest way to answer "is this
    // still valid?", so the clock-less entry point must refuse rather than
    // wave it through.
    //
    // The window deliberately CONTAINS 0. This matters: `redeem_capability`
    // delegates to `redeem_capability_at(cap, catalog, 0)`, and a window like
    // [1000, 2000] is already rejected at now=0 ("not yet valid"), so such a
    // test would pass even with the explicit refusal removed. Only a window
    // spanning 0 exercises the guard itself — and the mutation check is what
    // found that the weaker version of this test was passing for the wrong
    // reason.
    let org = policy_with_max_total(Dqa::new(1_000_000, 0).unwrap());
    let cap = time_capability(
        org.policy_id,
        &[Caveat::ValidRange {
            valid_after_unix: 0,
            valid_until_unix: u64::MAX,
        }],
    );
    let mut catalog = TestPolicyCatalog::default();
    catalog.insert(org);

    // Anti-vacuity for the guard: the clock-aware path DOES accept this window
    // at now=0, so the clock-less refusal is doing real work.
    redeem_capability_at(&cap, &catalog, 0)
        .expect("the clock-aware path must accept a window containing 0");

    let err = redeem_capability(&cap, &catalog).unwrap_err();
    assert!(
        matches!(err, RedemptionError::OutsideValidityWindow { .. }),
        "the clock-less path accepted a time-bearing capability: {err:?}"
    );
}

#[test]
fn h4_clock_less_redemption_still_works_without_time_caveats() {
    // Anti-vacuity for the refusal above: a capability with nothing
    // time-dependent must redeem exactly as it did before this change.
    let org = policy_with_max_total(Dqa::new(1_000_000, 0).unwrap());
    let cap = build_capability(&[
        Caveat::PolicyReference {
            policy_id: org.policy_id,
            policy_version_seq: 1,
            attenuation_witness: [0u8; 64],
        },
        Caveat::AmountMax(Dqa::new(1_000, 0).unwrap()),
    ]);
    let mut catalog = TestPolicyCatalog::default();
    catalog.insert(org);

    redeem_capability(&cap, &catalog).expect("a non-time capability must still redeem");
}
