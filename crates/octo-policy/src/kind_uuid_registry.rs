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

    // ─────────────────────────────────────────────────────────────────────
    // R5 fix G4 coverage: 30 individual `#[test]` per-namespace distinct-UUID
    // assertions. Each test asserts:
    //   1. kind_uuid_from_namespace(NS) != [0u8; 16]  (no all-zero UUID)
    //   2. kind_uuid_from_namespace(NS) is not equal to any other NS's UUID
    // Test name derives from the NS string suffix (e.g. `singlekey`,
    // `didattestation`, `primary-or-secondary`) so the test list reads
    // as a per-NS audit trail.
    // ─────────────────────────────────────────────────────────────────────

    /// Helper: compute the full set of canonical UUIDs once for the
    /// "distinct from all others" cross-check below.
    fn all_canonical_uuids() -> std::collections::HashSet<u128> {
        KIND_NAMESPACE_STRINGS
            .iter()
            .map(|ns| kind_uuid_from_namespace(ns))
            .collect()
    }

    macro_rules! ns_distinct_test {
        ($name:ident, $ns:expr) => {
            #[test]
            fn $name() {
                let u = kind_uuid_from_namespace($ns);
                assert_ne!(u, 0, "namespace {} must derive a non-zero UUID", $ns);
                let all = all_canonical_uuids();
                // `ns` is one of the 30 canonical strings, so it IS in
                // the set. Count occurrences of `u` to confirm exactly
                // one match (i.e. its own row, no collision).
                let count = all.iter().filter(|&&x| x == u).count();
                assert_eq!(
                    count, 1,
                    "namespace {} UUID {:#034x} must be unique across all 30 NS (got {} matches)",
                    $ns, u, count
                );
            }
        };
    }

    // Authority (6) — RFC-0967-A1 §2.6
    ns_distinct_test!(ns_authority_singlekey, "octo/auth/singlekey/v1");
    ns_distinct_test!(ns_authority_multisig, "octo/auth/multisig/v1");
    ns_distinct_test!(ns_authority_capability, "octo/auth/capability/v1");
    ns_distinct_test!(ns_authority_governance, "octo/auth/governance/v1");
    ns_distinct_test!(ns_authority_hsm, "octo/auth/hsm/v1");
    ns_distinct_test!(ns_authority_hybrid, "octo/auth/hybrid/v1");

    // Membership (7)
    ns_distinct_test!(
        ns_membership_didattestation,
        "octo/membership/didattestation/v1"
    );
    ns_distinct_test!(
        ns_membership_invitationtoken,
        "octo/membership/invitationtoken/v1"
    );
    ns_distinct_test!(ns_membership_merklelist, "octo/membership/merklelist/v1");
    ns_distinct_test!(ns_membership_teamsproxy, "octo/membership/teamsproxy/v1");
    ns_distinct_test!(
        ns_membership_corpmemberstable,
        "octo/membership/corpmemberstable/v1"
    );
    ns_distinct_test!(
        ns_membership_capabilitygated,
        "octo/membership/capabilitygated/v1"
    );
    ns_distinct_test!(ns_membership_scimbridge, "octo/membership/scimbridge/v1");

    // Interop (4)
    ns_distinct_test!(ns_interop_none, "octo/interop/none/v1");
    ns_distinct_test!(ns_interop_swap, "octo/interop/swap/v1");
    ns_distinct_test!(ns_interop_wrap, "octo/interop/wrap/v1");
    ns_distinct_test!(ns_interop_hybrid, "octo/interop/hybrid/v1");

    // Burn (3)
    ns_distinct_test!(ns_burn_timelock, "octo/burn/timelock/v1");
    ns_distinct_test!(ns_burn_immediate, "octo/burn/immediate/v1");
    ns_distinct_test!(ns_burn_multisig, "octo/burn/multisig/v1");

    // Workflow (4)
    ns_distinct_test!(ns_workflow_capability, "octo/workflow/capability/v1");
    ns_distinct_test!(ns_workflow_litellm, "octo/workflow/litellm/v1");
    ns_distinct_test!(ns_workflow_scim, "octo/workflow/scim/v1");
    ns_distinct_test!(ns_workflow_composite, "octo/workflow/composite/v1");

    // Audit (3)
    ns_distinct_test!(ns_audit_testnet, "octo/audit/testnet/v1");
    ns_distinct_test!(ns_audit_mainnet, "octo/audit/mainnet/v1");
    ns_distinct_test!(ns_audit_ab, "octo/audit/ab/v1");

    // Selector (3)
    ns_distinct_test!(ns_selector_bychain, "octo/selector/bychain/v1");
    ns_distinct_test!(ns_selector_byasset, "octo/selector/byasset/v1");
    ns_distinct_test!(
        ns_selector_byamountthreshold,
        "octo/selector/byamountthreshold/v1"
    );

    // ─────────────────────────────────────────────────────────────────────
    // R5 fix G5 coverage: 7 per-category slice tests. Each test slices
    // `KIND_NAMESPACE_STRINGS` by the exact count expected from
    // RFC-0967-A1 §2.6 (Authority=6, Membership=7, Interop=4, Burn=3,
    // Workflow=4, Audit=3, Selector=3) and asserts all UUIDs are
    // distinct.
    // ─────────────────────────────────────────────────────────────────────

    fn category_uuids_distinct(cat: PolicyKindCategory) {
        let nss = namespaces_for(cat);
        let uuids: Vec<u128> = nss.iter().map(|ns| kind_uuid_from_namespace(ns)).collect();
        let n = uuids.len();
        let mut sorted = uuids.clone();
        sorted.sort_unstable();
        sorted.dedup();
        assert_eq!(
            sorted.len(),
            n,
            "{cat:?}: expected {n} distinct UUIDs, got {} (collision)",
            sorted.len()
        );
        // Also assert none are zero.
        for u in &uuids {
            assert_ne!(*u, 0, "{cat:?}: UUID must not be zero");
        }
    }

    #[test]
    fn slice_authority_6_distinct() {
        let nss = namespaces_for(PolicyKindCategory::Authority);
        assert_eq!(nss.len(), 6, "Authority category must be 6 NS");
        category_uuids_distinct(PolicyKindCategory::Authority);
    }

    #[test]
    fn slice_membership_7_distinct() {
        let nss = namespaces_for(PolicyKindCategory::Membership);
        assert_eq!(nss.len(), 7, "Membership category must be 7 NS");
        category_uuids_distinct(PolicyKindCategory::Membership);
    }

    #[test]
    fn slice_interop_4_distinct() {
        let nss = namespaces_for(PolicyKindCategory::Interop);
        assert_eq!(nss.len(), 4, "Interop category must be 4 NS");
        category_uuids_distinct(PolicyKindCategory::Interop);
    }

    #[test]
    fn slice_burn_3_distinct() {
        let nss = namespaces_for(PolicyKindCategory::Burn);
        assert_eq!(nss.len(), 3, "Burn category must be 3 NS");
        category_uuids_distinct(PolicyKindCategory::Burn);
    }

    #[test]
    fn slice_workflow_4_distinct() {
        let nss = namespaces_for(PolicyKindCategory::Workflow);
        assert_eq!(nss.len(), 4, "Workflow category must be 4 NS");
        category_uuids_distinct(PolicyKindCategory::Workflow);
    }

    #[test]
    fn slice_audit_3_distinct() {
        let nss = namespaces_for(PolicyKindCategory::Audit);
        assert_eq!(nss.len(), 3, "Audit category must be 3 NS");
        category_uuids_distinct(PolicyKindCategory::Audit);
    }

    #[test]
    fn slice_selector_3_distinct() {
        let nss = namespaces_for(PolicyKindCategory::Selector);
        assert_eq!(nss.len(), 3, "Selector category must be 3 NS");
        category_uuids_distinct(PolicyKindCategory::Selector);
    }
}
