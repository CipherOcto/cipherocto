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
            node_did: octo_ident::test_helpers::sample_did(147).into(),
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
            node_did: octo_ident::test_helpers::sample_did(147).into(),
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
}
