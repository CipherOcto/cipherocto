//! 30 per-policy-kind UUIDv5 namespace string registry (RFC-0967-A1 v1.9.2 §2.6).
//!
//! Layer A substrate — frozen; semver-major only.
//!
//! Each entry is the canonical `octo/<category>/<kind>/v1/` namespace
//! string used to derive a deterministic UUIDv5 via
//! `uuid::Uuid::new_v5(&NAMESPACE_OID, ns_string.as_bytes())`.
//!
//! Authority kinds 1-6 (6 entries per RFC-0967-A1 §2.6):
//! - octo/auth/singlekey/v1
//! - octo/auth/multisig/v1
//! - octo/auth/capability/v1
//! - octo/auth/macaroon/v1
//! - octo/auth/zkholding/v1
//! - octo/auth/hopdelegation/v1
//!
//! Membership kinds 2-7 (7 entries):
//! - octo/membership/did-attestation/v1
//! - octo/membership/sybil-resistant/v1
//! - octo/membership/permissioned/v1
//! - octo/membership/public/v1
//! - octo/membership/k-of-n/v1
//! - octo/membership/threshold/v1
//! - octo/membership/delegate/v1
//!
//! Interop kinds 3-4 (4 entries):
//! - octo/interop/primary-or-secondary/v1
//! - octo/interop/atomic-swap/v1
//! - octo/interop/burn-mint/v1
//! - octo/interop/lock-unlock/v1
//!
//! Burn kinds 4-3 (3 entries):
//! - octo/burn/timed-unlock/v1
//! - octo/burn/governance-approval/v1
//! - octo/burn/capability-bounded/v1
//!
//! Workflow kinds 5-4 (4 entries):
//! - octo/workflow/vault-creation/v1
//! - octo/workflow/subject-provision/v1
//! - octo/workflow/user-info-read/v1
//! - octo/workflow/user-update/v1
//!
//! Audit kinds 6-3 (3 entries):
//! - octo/audit/full-emit/v1
//! - octo/audit/minimal-emit/v1
//! - octo/audit/aggregated-emit/v1
//!
//! InteropSelector kinds 7-3 (3 entries):
//! - octo/selector/byamount/v1
//! - octo/selector/byassetnamespace/v1
//! - octo/selector/byamountthreshold/v1

/// Total count of per-policy-kind namespace strings (per RFC-0967-A1 §2.6).
pub const KIND_COUNT: usize = 30;

/// Canonical per-policy-kind UUIDv5 namespace strings (30 entries).
pub const KIND_NAMESPACE_STRINGS: [&str; KIND_COUNT] = [
    // Authority (6)
    "octo/auth/singlekey/v1",
    "octo/auth/multisig/v1",
    "octo/auth/capability/v1",
    "octo/auth/macaroon/v1",
    "octo/auth/zkholding/v1",
    "octo/auth/hopdelegation/v1",
    // Membership (7)
    "octo/membership/did-attestation/v1",
    "octo/membership/sybil-resistant/v1",
    "octo/membership/permissioned/v1",
    "octo/membership/public/v1",
    "octo/membership/k-of-n/v1",
    "octo/membership/threshold/v1",
    "octo/membership/delegate/v1",
    // Interop (4)
    "octo/interop/primary-or-secondary/v1",
    "octo/interop/atomic-swap/v1",
    "octo/interop/burn-mint/v1",
    "octo/interop/lock-unlock/v1",
    // Burn (3)
    "octo/burn/timed-unlock/v1",
    "octo/burn/governance-approval/v1",
    "octo/burn/capability-bounded/v1",
    // Workflow (4)
    "octo/workflow/vault-creation/v1",
    "octo/workflow/subject-provision/v1",
    "octo/workflow/user-info-read/v1",
    "octo/workflow/user-update/v1",
    // Audit (3)
    "octo/audit/full-emit/v1",
    "octo/audit/minimal-emit/v1",
    "octo/audit/aggregated-emit/v1",
    // InteropSelector (3)
    "octo/selector/byamount/v1",
    "octo/selector/byassetnamespace/v1",
    "octo/selector/byamountthreshold/v1",
];

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
}
