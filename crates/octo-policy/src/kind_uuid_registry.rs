//! 30 per-policy-kind UUIDv5 namespace string registry (RFC-0967-A1 v1.9.2 §2.6).
//!
//! Layer A substrate — frozen; semver-major only.
//!
//! Each entry is the canonical `octo/<category>/<kind>/v1/` namespace
//! string used to derive a deterministic UUIDv5 via
//! `uuid::Uuid::new_v5(&CIPHEROCTO_KIND_ROOT, ns_string.as_bytes())`.
//!
//! ## Private root namespace
//!
//! [`CIPHEROCTO_KIND_ROOT`] is a project-private UUID derived once via
//! `Uuid::new_v5(&NAMESPACE_OID, b"cipherocto-octopolicy-root-v1")` and
//! frozen as bytes. Using a private root instead of the RFC-4122 public
//! OID namespace prevents anyone who knows the public `NAMESPACE_OID`
//! plus the public `KIND_NAMESPACE_STRINGS` from reproducing every
//! `kind_uuid`. The kind UUIDs are content-addressable identifiers for
//! our internal substrate, not public RFC-4122-derived UUIDs.
//!
//! ## RFC §2.6 ordering (verbatim)
//!
//! Authority (6): singlekey, multisig, capability, governance, hsm, hybrid
//! Membership (7): didattestation, invitationtoken, merklelist, teamsproxy,
//!                  corpmemberstable, capabilitygated, scimbridge
//! Interop (4):    none, swap, wrap, hybrid
//! Burn (3):       timelock, immediate, multisig
//! Workflow (4):   capability, litellm, scim, composite
//! Audit (3):      testnet, mainnet, ab
//! Selector (3):   bychain, byasset, byamountthreshold

/// Project-private root UUID for all kind-UUIDv5 derivations.
///
/// Derived once via `Uuid::new_v5(&Uuid::NAMESPACE_OID, b"cipherocto-octopolicy-root-v1")`
/// and frozen as bytes. Do NOT re-derive at runtime — the byte literal
/// below is the single source of truth for all 30 kind UUIDs.
pub const CIPHEROCTO_KIND_ROOT_BYTES: [u8; 16] = [
    0xc9, 0x6a, 0xcc, 0xc1, 0x32, 0x74, 0x5a, 0x21, 0x92, 0x39, 0xb5, 0xcd, 0x23, 0xbd, 0xa3, 0xe3,
];

/// [`uuid::Uuid`] view of [`CIPHEROCTO_KIND_ROOT_BYTES`].
pub const CIPHEROCTO_KIND_ROOT: uuid::Uuid = uuid::Uuid::from_bytes(CIPHEROCTO_KIND_ROOT_BYTES);

/// Total count of per-policy-kind namespace strings (per RFC-0967-A1 §2.6).
pub const KIND_COUNT: usize = 30;

/// Canonical per-policy-kind UUIDv5 namespace strings (30 entries, verbatim
/// from RFC-0967-A1 §2.6).
pub const KIND_NAMESPACE_STRINGS: [&str; KIND_COUNT] = [
    // Authority (6) — RFC-0967-A1 §2.6
    "octo/auth/singlekey/v1",
    "octo/auth/multisig/v1",
    "octo/auth/capability/v1",
    "octo/auth/governance/v1",
    "octo/auth/hsm/v1",
    "octo/auth/hybrid/v1",
    // Membership (7) — RFC-0967-A1 §2.6
    "octo/membership/didattestation/v1",
    "octo/membership/invitationtoken/v1",
    "octo/membership/merklelist/v1",
    "octo/membership/teamsproxy/v1",
    "octo/membership/corpmemberstable/v1",
    "octo/membership/capabilitygated/v1",
    "octo/membership/scimbridge/v1",
    // Interop (4) — RFC-0967-A1 §2.6
    "octo/interop/none/v1",
    "octo/interop/swap/v1",
    "octo/interop/wrap/v1",
    "octo/interop/hybrid/v1",
    // Burn (3) — RFC-0967-A1 §2.6
    "octo/burn/timelock/v1",
    "octo/burn/immediate/v1",
    "octo/burn/multisig/v1",
    // Workflow (4) — RFC-0967-A1 §2.6
    "octo/workflow/capability/v1",
    "octo/workflow/litellm/v1",
    "octo/workflow/scim/v1",
    "octo/workflow/composite/v1",
    // Audit (3) — RFC-0967-A1 §2.6
    "octo/audit/testnet/v1",
    "octo/audit/mainnet/v1",
    "octo/audit/ab/v1",
    // Selector (3) — RFC-0967-A1 §2.6
    "octo/selector/bychain/v1",
    "octo/selector/byasset/v1",
    "octo/selector/byamountthreshold/v1",
];

/// Derive a per-policy-kind `kind_uuid` from a namespace string
/// (RFC-0967-A1 §2.6).
///
/// The derivation uses [`CIPHEROCTO_KIND_ROOT`] as the UUIDv5 namespace
/// (NOT the public RFC-4122 `NAMESPACE_OID`) so the kind UUIDs are
/// project-private and not reproducible by anyone outside the project
/// who knows the public `NAMESPACE_OID` + the public
/// `KIND_NAMESPACE_STRINGS`.
#[must_use]
pub fn kind_uuid_from_namespace(ns: &str) -> u128 {
    uuid::Uuid::new_v5(&CIPHEROCTO_KIND_ROOT, ns.as_bytes()).as_u128()
}

/// Policy category indices (per registry_kind table in migration v017).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PolicyKindCategory {
    /// AuthorityPolicy (6 entries).
    Authority = 1,
    /// MembershipPolicy (7 entries).
    Membership = 2,
    /// InteropPolicy (4 entries).
    Interop = 3,
    /// BurnPolicy (3 entries).
    Burn = 4,
    /// WorkflowKind (4 entries).
    Workflow = 5,
    /// AuditPolicy (3 entries).
    Audit = 6,
    /// InteropSelector (3 entries).
    Selector = 7,
}

/// Index range per category (start, count) — used to slice KIND_NAMESPACE_STRINGS.
pub const fn category_range(cat: PolicyKindCategory) -> (usize, usize) {
    match cat {
        PolicyKindCategory::Authority => (0, 6),
        PolicyKindCategory::Membership => (6, 7),
        PolicyKindCategory::Interop => (13, 4),
        PolicyKindCategory::Burn => (17, 3),
        PolicyKindCategory::Workflow => (20, 4),
        PolicyKindCategory::Audit => (24, 3),
        PolicyKindCategory::Selector => (27, 3),
    }
}

/// Namespaces within a category.
pub fn namespaces_for(cat: PolicyKindCategory) -> &'static [&'static str] {
    let (start, count) = category_range(cat);
    &KIND_NAMESPACE_STRINGS[start..start + count]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn kind_count_is_30() {
        assert_eq!(KIND_NAMESPACE_STRINGS.len(), KIND_COUNT);
        assert_eq!(KIND_COUNT, 30);
    }

    #[test]
    fn all_namespaces_use_octo_prefix() {
        for ns in KIND_NAMESPACE_STRINGS.iter() {
            assert!(
                ns.starts_with("octo/"),
                "namespace {ns} must use octo/ prefix"
            );
            assert!(ns.ends_with("/v1"), "namespace {ns} must end with /v1");
        }
    }

    #[test]
    fn category_ranges_partition_30_entries() {
        let mut total = 0;
        for cat in [
            PolicyKindCategory::Authority,
            PolicyKindCategory::Membership,
            PolicyKindCategory::Interop,
            PolicyKindCategory::Burn,
            PolicyKindCategory::Workflow,
            PolicyKindCategory::Audit,
            PolicyKindCategory::Selector,
        ] {
            let (_, count) = category_range(cat);
            total += count;
            assert_eq!(namespaces_for(cat).len(), count);
        }
        assert_eq!(total, 30);
    }

    #[test]
    fn authority_kinds_are_6() {
        assert_eq!(namespaces_for(PolicyKindCategory::Authority).len(), 6);
    }

    #[test]
    fn membership_kinds_are_7() {
        assert_eq!(namespaces_for(PolicyKindCategory::Membership).len(), 7);
    }

    #[test]
    fn interop_kinds_are_4() {
        assert_eq!(namespaces_for(PolicyKindCategory::Interop).len(), 4);
    }

    #[test]
    fn burn_kinds_are_3() {
        assert_eq!(namespaces_for(PolicyKindCategory::Burn).len(), 3);
    }

    #[test]
    fn workflow_kinds_are_4() {
        assert_eq!(namespaces_for(PolicyKindCategory::Workflow).len(), 4);
    }

    #[test]
    fn audit_kinds_are_3() {
        assert_eq!(namespaces_for(PolicyKindCategory::Audit).len(), 3);
    }

    #[test]
    fn selector_kinds_are_3() {
        assert_eq!(namespaces_for(PolicyKindCategory::Selector).len(), 3);
    }

    #[test]
    fn all_30_namespaces_derive_distinct_uuids() {
        // Per F9 + RFC-0967-A1 §2.6: all 30 namespace strings must yield
        // pairwise distinct UUIDv5 derivations under CIPHEROCTO_KIND_ROOT.
        let uuids: Vec<u128> = KIND_NAMESPACE_STRINGS
            .iter()
            .map(|ns| kind_uuid_from_namespace(ns))
            .collect();
        let mut sorted = uuids.clone();
        sorted.sort_unstable();
        sorted.dedup();
        assert_eq!(
            sorted.len(),
            30,
            "expected 30 distinct kind UUIDs, got {} (collision in KIND_NAMESPACE_STRINGS)",
            sorted.len()
        );
    }

    #[test]
    fn cipherocto_kind_root_matches_derivation_recipe() {
        // Guard against future edits that re-derive the root from the
        // recipe `Uuid::new_v5(NAMESPACE_OID, b"cipherocto-octopolicy-root-v1")`
        // and break all 30 downstream kind UUIDs.
        let derived =
            uuid::Uuid::new_v5(&uuid::Uuid::NAMESPACE_OID, b"cipherocto-octopolicy-root-v1");
        assert_eq!(
            derived.as_bytes(),
            &CIPHEROCTO_KIND_ROOT_BYTES,
            "CIPHEROCTO_KIND_ROOT_BYTES drift: re-derive and freeze again"
        );
    }
}
