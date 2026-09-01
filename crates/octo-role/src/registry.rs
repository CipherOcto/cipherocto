//! Built-in role registry — canonical 7 base roles per RFC-0011-d §7.5.
//!
//! In-memory registry used by `octo_role::list` / `show` until the
//! substrate-backed registry (slash ledger per RFC-0900) lands. This
//! module is the **registry of record for Phase 1**; production
//! deployments replace it with the slash-ledger-backed table.

use crate::types::{RoleKindUuid, RoleRecord, RoleSummary, SlashingRule};

/// UUIDv5 of `urn:octo:role:0855:<slug>` (RFC-0855 namespace pattern).
///
/// Uses the RFC-4122 §4.3 SHA-1-based UUIDv5 derivation. The
/// namespace anchor is fixed (RFC-0855 §Namespace) so two independent
/// processes compute the same 16-byte UUID for the same slug.
fn role_uuid(slug: &str) -> RoleKindUuid {
    // RFC-4122 §4.3 UUIDv5 namespace (RFC-0855 anchor).
    let namespace: [u8; 16] = [
        0x6b, 0xa7, 0xb8, 0x10, 0x9d, 0xad, 0x11, 0xd1, 0x80, 0xb4, 0x00, 0xc0, 0x4f, 0xd4, 0x30,
        0xc8,
    ];
    let name = format!("urn:octo:role:0855:{slug}");
    let hasher = uuid::Uuid::new_v5(&uuid::Uuid::from_bytes(namespace), name.as_bytes());
    let bytes = hasher.as_bytes().to_vec();
    let mut out = [0u8; 16];
    out.copy_from_slice(&bytes[..16]);
    out
}

fn role_summary(
    slug: &str,
    class: &str,
    role_token_ticker: Option<&str>,
    requires_octo_min: Option<u64>,
) -> RoleSummary {
    RoleSummary {
        role_kind_uuid: role_uuid(slug),
        name: slug.to_string(),
        role_token_ticker: role_token_ticker.map(str::to_string),
        requires_octo_min,
        class: class.to_string(),
    }
}

/// Canonical 7 base roles per RFC-0011-d §7.5 Role Summary.
fn base_records() -> Vec<RoleRecord> {
    let base = vec![
        role_summary(
            "builder",
            "infrastructure",
            Some("OCTO-A"),
            Some(1_000_000_000), // 1000 OCTO
        ),
        role_summary(
            "provider",
            "infrastructure",
            Some("OCTO-A"),
            Some(1_000_000_000),
        ),
        role_summary(
            "storage",
            "infrastructure",
            Some("OCTO-B"),
            Some(500_000_000),
        ),
        role_summary(
            "bandwidth",
            "infrastructure",
            Some("OCTO-B"),
            Some(500_000_000),
        ),
        role_summary(
            "orchestrator",
            "economic",
            Some("OCTO-O"),
            Some(5_000_000_000),
        ),
        role_summary(
            "recorder",
            "governance",
            None, // OCTO-only role
            Some(100_000_000),
        ),
        role_summary(
            "wallet",
            "governance",
            None, // OCTO-only role
            None, // No minimum OCTO stake
        ),
    ];

    base.into_iter()
        .map(|summary| RoleRecord {
            slashing_rules: default_slashing_rules(&summary.name),
            allowed_actions: default_allowed_actions(&summary.name),
            registry_ref: Some(format!("urn:octo:role:0855:{}", summary.name)),
            summary,
        })
        .collect()
}

fn default_slashing_rules(slug: &str) -> Vec<SlashingRule> {
    // Per RFC-0900 §Slashing Model. Slugs map to specific rule sets.
    match slug {
        "builder" | "provider" | "storage" | "bandwidth" => vec![
            SlashingRule {
                reason_code: "liveness_fault".into(),
                description: "Failed to produce expected output within SLA window.".into(),
                penalty_pct_micro: 50_000,              // 5%
                escalation_multiplier_micro: 1_500_000, // 1.5x per offense
            },
            SlashingRule {
                reason_code: "double_sign".into(),
                description: "Signed two conflicting envelopes.".into(),
                penalty_pct_micro: 1_000_000, // 100%
                escalation_multiplier_micro: 1_000_000,
            },
        ],
        "orchestrator" => vec![SlashingRule {
            reason_code: "handover_timeout".into(),
            description: "Failed to complete coordinator handover within RFC-0855p-e SLA.".into(),
            penalty_pct_micro: 100_000, // 10%
            escalation_multiplier_micro: 2_000_000,
        }],
        "recorder" | "wallet" => vec![SlashingRule {
            reason_code: "tamper_evidence".into(),
            description: "Produced audit/wallet state inconsistent with substrate truth.".into(),
            penalty_pct_micro: 1_000_000,
            escalation_multiplier_micro: 1_000_000,
        }],
        _ => Vec::new(),
    }
}

fn default_allowed_actions(slug: &str) -> Vec<String> {
    match slug {
        "builder" => vec!["attest_build".into(), "publish_artifact".into()],
        "provider" => vec!["run_inference".into(), "serve_model".into()],
        "storage" => vec!["store_blob".into(), "retrieve_blob".into()],
        "bandwidth" => vec!["relay_mesh".into(), "forward_packet".into()],
        "orchestrator" => vec!["coordinate_mission".into(), "hand_over".into()],
        "recorder" => vec!["emit_audit".into(), "witness_event".into()],
        "wallet" => vec!["manage_identity".into(), "sign_envelope".into()],
        _ => Vec::new(),
    }
}

/// Return all canonical base roles.
pub fn all_roles() -> Vec<RoleRecord> {
    base_records()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn base_registry_has_7_roles() {
        let roles = all_roles();
        assert_eq!(roles.len(), 7);
        let names: Vec<_> = roles.iter().map(|r| r.summary.name.as_str()).collect();
        assert!(names.contains(&"builder"));
        assert!(names.contains(&"wallet"));
        assert!(names.contains(&"recorder"));
    }

    #[test]
    fn octo_only_roles_have_no_role_token() {
        let roles = all_roles();
        for name in ["recorder", "wallet"] {
            let r = roles.iter().find(|r| r.summary.name == name).unwrap();
            assert!(
                r.summary.role_token_ticker.is_none(),
                "{name} should have no role token"
            );
        }
    }

    #[test]
    fn role_uuids_are_distinct() {
        let roles = all_roles();
        let mut uuids: Vec<_> = roles.iter().map(|r| r.summary.role_kind_uuid).collect();
        uuids.sort();
        uuids.dedup();
        assert_eq!(
            uuids.len(),
            7,
            "all 7 role slugs must produce distinct UUIDs"
        );
    }
}
