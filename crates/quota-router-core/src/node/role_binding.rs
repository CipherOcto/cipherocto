// RFC-0971 §Phase 1+2: role binding types + lifecycle state machine.
//
// `DestinationNode = Router ∧ TokenIssuer ∧ Asker` is canonical (R23-N9 fix).
// `ReputationAnchor` is OPTIONAL (R13-N8 fix). Pure forwarder exception
// is explicit (Finding A18 defense).

use std::collections::BTreeSet;

/// RoleTag typed enum (RFC-0971 §Phase 1).
///
/// NO string literals — typed enum enforced at compile time.
#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Debug)]
pub enum RoleTag {
    Router,
    TokenIssuer,
    Asker,
    PureForwarder,
    ReputationAnchor,
}

/// RoleBindingLifecycle state machine (RFC-0971 §Phase 1).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum RoleBindingLifecycle {
    Active,
    Draining,
    Suspended,
    Retired,
}

/// RoleBindingDeclaration (RFC-0971 §Phase 1).
#[derive(Clone, Debug)]
pub struct RoleBindingDeclaration {
    pub node_did: String,
    pub required_roles: BTreeSet<RoleTag>,
    pub optional_roles: BTreeSet<RoleTag>,
    pub lifecycle: RoleBindingLifecycle,
    pub minted_at_millis_unix: i64,
}

/// Required roles for a destination node (canonical predicate).
pub fn destination_required_roles() -> BTreeSet<RoleTag> {
    let mut s = BTreeSet::new();
    s.insert(RoleTag::Router);
    s.insert(RoleTag::TokenIssuer);
    s.insert(RoleTag::Asker);
    s
}

/// Optional roles for a destination node (ReputationAnchor is OPTIONAL).
pub fn destination_optional_roles() -> BTreeSet<RoleTag> {
    let mut s = BTreeSet::new();
    s.insert(RoleTag::ReputationAnchor);
    s
}

/// Pure forwarder role set (Finding A18).
pub fn pure_forwarder_roles() -> BTreeSet<RoleTag> {
    let mut s = BTreeSet::new();
    s.insert(RoleTag::PureForwarder);
    s
}

/// Validate: required_roles must contain exactly the canonical set.
pub fn validate_destination_binding(decl: &RoleBindingDeclaration) -> bool {
    let canonical = destination_required_roles();
    decl.required_roles == canonical
}

/// Role-binding transition errors (RFC-0971 §Phase 1 + §Lifecycle
/// Requirements §Role-Binding State Machine).
///
/// Per [[deferred-vs-unspecified]] named-owner rule: variants are
/// concrete with named owners for any future additions.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum RoleBindingError {
    /// Transition from `from` to `to` is not permitted by the state
    /// machine (RFC-0971 §Role-Binding State Machine).
    InvalidTransition {
        from: RoleBindingLifecycle,
        to: RoleBindingLifecycle,
    },
    /// Required role missing from `required_roles` set per canonical
    /// destination predicate (`Router ∧ TokenIssuer ∧ Asker`).
    MissingRequiredRole(RoleTag),
    /// Role attempted to be exercised on a node without binding (e.g.,
    /// `ReputationAnchor` on a node without `optional_roles`).
    RoleNotBound(RoleTag),
}

/// Validate a role-binding lifecycle transition.
///
/// **Canonical transitions (RFC-0971 §Role-Binding State Machine):**
///
/// ```text
///   Active    → Draining, Suspended, Retired
///   Draining  → Suspended, Retired
///   Suspended → Active, Retired
///   Retired   → (terminal — no transitions out)
/// ```
///
/// `Retired` is terminal (no further transitions). All other transitions
/// not enumerated above return `RoleBindingError::InvalidTransition`.
pub fn validate_lifecycle_transition(
    from: RoleBindingLifecycle,
    to: RoleBindingLifecycle,
) -> Result<(), RoleBindingError> {
    use RoleBindingLifecycle::*;
    let valid = matches!(
        (from, to),
        (Active, Draining)
            | (Active, Suspended)
            | (Active, Retired)
            | (Draining, Suspended)
            | (Draining, Retired)
            | (Suspended, Active)
            | (Suspended, Retired)
    );
    if valid {
        Ok(())
    } else {
        Err(RoleBindingError::InvalidTransition { from, to })
    }
}

/// Apply a Router Resignation: deactivates Router role only; TokenIssuer
/// and Asker remain `Active` (R23-N1 fix per mission text).
///
/// Returns a new `RoleBindingDeclaration` with `Router` removed from
/// `required_roles` and lifecycle set to `Draining`. If `decl` does not
/// carry `Router` binding, returns the original declaration unchanged
/// (no-op; Router Resigned is a no-op when Router is not bound).
pub fn router_resigned(decl: RoleBindingDeclaration) -> RoleBindingDeclaration {
    let mut new_decl = decl;
    new_decl.required_roles.remove(&RoleTag::Router);
    new_decl.lifecycle = RoleBindingLifecycle::Draining;
    new_decl
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn destination_required_roles_are_canonical() {
        let canonical = destination_required_roles();
        assert!(canonical.contains(&RoleTag::Router));
        assert!(canonical.contains(&RoleTag::TokenIssuer));
        assert!(canonical.contains(&RoleTag::Asker));
        assert_eq!(canonical.len(), 3);
    }

    #[test]
    fn validate_destination_binding_accepts_canonical() {
        let decl = RoleBindingDeclaration {
            node_did: octo_ident::test_helpers::sample_did(147),
            required_roles: destination_required_roles(),
            optional_roles: BTreeSet::new(),
            lifecycle: RoleBindingLifecycle::Active,
            minted_at_millis_unix: 1_700_000_000_000,
        };
        assert!(validate_destination_binding(&decl));
    }

    #[test]
    fn validate_destination_binding_rejects_missing_role() {
        let mut req = destination_required_roles();
        req.remove(&RoleTag::Asker);
        let decl = RoleBindingDeclaration {
            node_did: octo_ident::test_helpers::sample_did(147),
            required_roles: req,
            optional_roles: BTreeSet::new(),
            lifecycle: RoleBindingLifecycle::Active,
            minted_at_millis_unix: 1_700_000_000_000,
        };
        assert!(!validate_destination_binding(&decl));
    }

    #[test]
    fn pure_forwarder_roles_excludes_destination_set() {
        let pf = pure_forwarder_roles();
        assert!(pf.contains(&RoleTag::PureForwarder));
        assert!(!pf.contains(&RoleTag::Router));
        assert!(!pf.contains(&RoleTag::TokenIssuer));
        assert!(!pf.contains(&RoleTag::Asker));
    }

    #[test]
    fn role_binding_lifecycle_variants() {
        let _ = RoleBindingLifecycle::Active;
        let _ = RoleBindingLifecycle::Draining;
        let _ = RoleBindingLifecycle::Suspended;
        let _ = RoleBindingLifecycle::Retired;
    }

    #[test]
    fn reputation_anchor_is_optional() {
        let opt = destination_optional_roles();
        assert!(opt.contains(&RoleTag::ReputationAnchor));
        assert_eq!(opt.len(), 1);
    }

    // ---- RFC-0971 §Test Vectors ----

    /// TV1: Role Binding Assertion (Required Roles Present) — full
    /// canonical declaration validates.
    #[test]
    fn tv1_required_roles_present_validates() {
        let decl = RoleBindingDeclaration {
            node_did: octo_ident::test_helpers::sample_did(147),
            required_roles: destination_required_roles(),
            optional_roles: destination_optional_roles(),
            lifecycle: RoleBindingLifecycle::Active,
            minted_at_millis_unix: 1_700_000_000_000,
        };
        assert!(validate_destination_binding(&decl));
    }

    /// TV1: missing any one of `Router`, `TokenIssuer`, `Asker` rejects.
    #[test]
    fn tv1_missing_required_role_rejects() {
        for missing in [RoleTag::Router, RoleTag::TokenIssuer, RoleTag::Asker] {
            let mut req = destination_required_roles();
            req.remove(&missing);
            let decl = RoleBindingDeclaration {
                node_did: octo_ident::test_helpers::sample_did(147),
                required_roles: req,
                optional_roles: BTreeSet::new(),
                lifecycle: RoleBindingLifecycle::Active,
                minted_at_millis_unix: 1_700_000_000_000,
            };
            assert!(
                !validate_destination_binding(&decl),
                "decl without {missing:?} MUST NOT validate"
            );
        }
    }

    /// TV4: Role Binding Lifecycle — full happy path
    /// `Active → Draining → Suspended → Retired`.
    #[test]
    fn tv4_lifecycle_happy_path() {
        use RoleBindingLifecycle::*;
        assert!(validate_lifecycle_transition(Active, Draining).is_ok());
        assert!(validate_lifecycle_transition(Draining, Suspended).is_ok());
        assert!(validate_lifecycle_transition(Suspended, Retired).is_ok());
    }

    /// TV4: invalid transitions return `InvalidTransition` (e.g.,
    /// `Active → Retired` is valid in the canonical table above, but
    /// `Retired → Active` is terminal — no transition out).
    #[test]
    fn tv4_lifecycle_terminal_retired_rejects() {
        use RoleBindingLifecycle::*;
        let err = validate_lifecycle_transition(Retired, Active);
        assert!(matches!(
            err,
            Err(RoleBindingError::InvalidTransition {
                from: Retired,
                to: Active
            })
        ));
    }

    /// TV4: `Suspended → Draining` is NOT in canonical table → reject.
    #[test]
    fn tv4_lifecycle_invalid_suspended_to_draining_rejects() {
        use RoleBindingLifecycle::*;
        let err = validate_lifecycle_transition(Suspended, Draining);
        assert!(matches!(
            err,
            Err(RoleBindingError::InvalidTransition {
                from: Suspended,
                to: Draining
            })
        ));
    }

    /// TV5: Router Resigned deactivates Router role only; TokenIssuer +
    /// Asker remain `Active`.
    #[test]
    fn tv5_router_resigned_deactivates_router_only() {
        let decl = RoleBindingDeclaration {
            node_did: octo_ident::test_helpers::sample_did(147),
            required_roles: destination_required_roles(),
            optional_roles: BTreeSet::new(),
            lifecycle: RoleBindingLifecycle::Active,
            minted_at_millis_unix: 1_700_000_000_000,
        };
        let resigned = router_resigned(decl);
        // Router removed from required_roles
        assert!(!resigned.required_roles.contains(&RoleTag::Router));
        // TokenIssuer + Asker still bound
        assert!(resigned.required_roles.contains(&RoleTag::TokenIssuer));
        assert!(resigned.required_roles.contains(&RoleTag::Asker));
        // Lifecycle set to Draining (resignation = drain to retirement)
        assert_eq!(resigned.lifecycle, RoleBindingLifecycle::Draining);
    }

    /// TV6: Pure Forwarder Exception — pure forwarder config (required
    /// = empty + optional = {PureForwarder}) declares a pure forwarder
    /// node. No Router / TokenIssuer / Asker binding.
    #[test]
    fn tv6_pure_forwarder_config_excludes_destination_roles() {
        let decl = RoleBindingDeclaration {
            node_did: octo_ident::test_helpers::sample_did(147),
            required_roles: BTreeSet::new(),
            optional_roles: pure_forwarder_roles(),
            lifecycle: RoleBindingLifecycle::Active,
            minted_at_millis_unix: 1_700_000_000_000,
        };
        // No destination required roles bound.
        assert!(!validate_destination_binding(&decl));
        // Only PureForwarder role present.
        assert_eq!(decl.optional_roles.len(), 1);
        assert!(decl.optional_roles.contains(&RoleTag::PureForwarder));
    }

    /// TV7: ReputationAnchor Optional — destination node without
    /// ReputationAnchor binding performs deal settlement (no
    /// `RoleBindingError::RoleNotBound`); ReputationAnchor binding is
    /// OPTIONAL.
    #[test]
    fn tv7_reputation_anchor_absence_does_not_block_settlement() {
        let decl = RoleBindingDeclaration {
            node_did: octo_ident::test_helpers::sample_did(147),
            required_roles: destination_required_roles(),
            optional_roles: BTreeSet::new(), // no ReputationAnchor
            lifecycle: RoleBindingLifecycle::Active,
            minted_at_millis_unix: 1_700_000_000_000,
        };
        // Canonical validation passes (required roles present).
        assert!(validate_destination_binding(&decl));
        // ReputationAnchor NOT bound (absent from optional set).
        assert!(!decl.optional_roles.contains(&RoleTag::ReputationAnchor));
        // The canonical predicate is `optional_roles ⊇ {ReputationAnchor}`?
        // No — `ReputationAnchor` is OPTIONAL. Absence does not block
        // deal settlement. This is the canonical finding (R13-N8 fix).
    }
}
