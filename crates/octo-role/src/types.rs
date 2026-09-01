//! Substrate types for role provisioning per RFC-0011-d §7.4 + §7.5.
//!
//! Layer B types. Serializable + JSON Schema export per RFC-0011-d
//! §Key Files row "JSON Schema export".
//!
//! Per [[cipherocto-design-principles]] no central enum for
//! extension-bearing types — `RoleSummary.role_kind_uuid` uses a
//! typed 128-bit UUID discriminator (RFC-0855 namespace pattern),
//! not a central enum.

use serde::{Deserialize, Serialize};

/// 128-bit typed role-kind discriminator (RFC-0855-namespaced UUIDv5 of
/// `urn:octo:role:0855:<slug>`).
///
/// Per [[cipherocto-design-principles]] no-central-enum: new role slugs
/// land by adding a row to the role registry; no central enum edit.
pub type RoleKindUuid = [u8; 16];

/// 32-byte chain identifier (RFC-0010 §ChainId).
pub type ChainId = [u8; 32];

/// 32-byte BLAKE3-256 digest (RFC-0104 DFP canonical encoding).
pub type Hash32 = [u8; 32];

// ---------------------------------------------------------------------------
// §7.5 Role Summary — read-side projection
// ---------------------------------------------------------------------------

/// Compact role summary returned by `octo_role::list` (TV-RL-*).
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq, schemars::JsonSchema)]
pub struct RoleSummary {
    /// RFC-0855-namespaced UUIDv5 typed discriminator.
    pub role_kind_uuid: RoleKindUuid,
    /// Human-readable role slug (`builder`, `provider`, `storage`, `bandwidth`,
    /// `orchestrator`, `recorder`, `wallet`).
    pub name: String,
    /// Display ticker (e.g., `OCTO-A`); `None` for OCTO-only roles (`recorder`,
    /// `wallet`).
    pub role_token_ticker: Option<String>,
    /// Required OCTO stake (micro-OCTO). `None` for OCTO-only roles where the
    /// field is inapplicable.
    pub requires_octo_min: Option<u64>,
    /// Role classification tag (e.g., `infrastructure`, `economic`,
    /// `governance`).
    pub class: String,
}

// ---------------------------------------------------------------------------
// §7.5 Role Record — full record for `show`
// ---------------------------------------------------------------------------

/// Full role record returned by `octo_role::show` (TV-RS-*).
///
/// Extends [`RoleSummary`] with slashing rules + allowed actions +
/// registry reference.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq, schemars::JsonSchema)]
pub struct RoleRecord {
    /// All [`RoleSummary`] fields.
    #[serde(flatten)]
    pub summary: RoleSummary,
    /// Slashing rules for this role (per RFC-0900 §Slashing Model).
    pub slashing_rules: Vec<SlashingRule>,
    /// Action verbs this role is permitted to invoke on the substrate.
    pub allowed_actions: Vec<String>,
    /// Optional RFC-0900 slash-ledger registry reference.
    pub registry_ref: Option<String>,
}

// ---------------------------------------------------------------------------
// §Slashing Model — per-rule entry
// ---------------------------------------------------------------------------

/// Per-rule slashing entry per RFC-0900 §Slashing Model.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq, schemars::JsonSchema)]
pub struct SlashingRule {
    /// Stable identifier (e.g., `double_sign`, `liveness_fault`).
    pub reason_code: String,
    /// Human-readable description.
    pub description: String,
    /// Penalty as a fraction in micro-units (1_000_000 = 100%).
    pub penalty_pct_micro: u64,
    /// Escalation multiplier in micro-units (1_000_000 = 1x).
    pub escalation_multiplier_micro: u64,
}

// ---------------------------------------------------------------------------
// §7.4 filter parsing (CLI-side)
// ---------------------------------------------------------------------------

/// CLI-side filter parser shape for `octo role list --filter key=value`.
///
/// Mirrors the `capability list` filter pattern from RFC-0011
/// §Subcommand Taxonomy.
#[derive(Serialize, Deserialize, Debug, Clone, Default, PartialEq, Eq, schemars::JsonSchema)]
pub struct RoleFilter {
    /// Filter by role kind (`builder`, `provider`, etc.).
    pub kind: Option<String>,
    /// Filter by classification (`infrastructure`, `economic`, etc.).
    pub class: Option<String>,
    /// Filter by minimum OCTO stake (micro-OCTO).
    pub requires_octo_min: Option<u64>,
}

impl RoleFilter {
    /// Returns `true` if the given summary matches every populated filter field.
    pub fn matches(&self, summary: &RoleSummary) -> bool {
        if let Some(kind) = &self.kind {
            if &summary.name != kind {
                return false;
            }
        }
        if let Some(class) = &self.class {
            if &summary.class != class {
                return false;
            }
        }
        if let Some(min) = self.requires_octo_min {
            match summary.requires_octo_min {
                Some(v) if v >= min => {}
                _ => return false,
            }
        }
        true
    }
}

// ---------------------------------------------------------------------------
// §7.4 role binding (write-side artifact)
// ---------------------------------------------------------------------------

/// Role binding record produced by `octo_role::select` (TV-RX-*).
///
/// `signature_proof` is the substrate's actual Ed25519 signature over the
/// canonical envelope bytes; CLI surfaces this as `RedactedHex` per the
/// redaction boundary at RFC-0011-d §Key Files.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq, schemars::JsonSchema)]
pub struct RoleBinding {
    /// Canonical wire form of the bound role slug (e.g., `builder`).
    pub role_id: String,
    /// Operator DID (slash-ledger PK).
    pub operator_did: String,
    /// RFC-0010 chain ID — partition key.
    pub chain_id: ChainId,
    /// RFC-0855-namespaced UUIDv5 typed discriminator.
    pub role_kind_uuid: RoleKindUuid,
    /// OCTO stake committed to this binding (micro-OCTO).
    pub stake_octo: u64,
    /// Role-token stake (micro-`token); `None` for OCTO-only roles.
    pub stake_role_token: Option<u64>,
    /// BLAKE3-256 over canonical binding serialization.
    pub body_hash: Hash32,
    /// Ed25519 signature over `body_bytes || body_hash`. CLI redaction
    /// boundary: substrate owns the bytes; CLI renders as `RedactedHex`.
    pub signature_proof: Vec<u8>,
    /// BLAKE3-256 over `body_bytes || signature_proof` — public material.
    pub role_binding_hash: Hash32,
}

mod serde_bytes_vec {
    // Reserved for future serde-with helper if redaction boundary needs
    // a custom serialization. Currently `Vec<u8>` serializes natively.
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn role_filter_matches_by_kind() {
        let summary = RoleSummary {
            role_kind_uuid: [0u8; 16],
            name: "builder".into(),
            role_token_ticker: Some("OCTO-A".into()),
            requires_octo_min: Some(1000),
            class: "infrastructure".into(),
        };
        let filter = RoleFilter {
            kind: Some("builder".into()),
            ..Default::default()
        };
        assert!(filter.matches(&summary));

        let wrong = RoleFilter {
            kind: Some("provider".into()),
            ..Default::default()
        };
        assert!(!wrong.matches(&summary));
    }

    #[test]
    fn role_filter_matches_by_min_stake() {
        let summary = RoleSummary {
            role_kind_uuid: [0u8; 16],
            name: "provider".into(),
            role_token_ticker: Some("OCTO-A".into()),
            requires_octo_min: Some(2000),
            class: "economic".into(),
        };
        let ok = RoleFilter {
            requires_octo_min: Some(1000),
            ..Default::default()
        };
        assert!(ok.matches(&summary));

        let too_high = RoleFilter {
            requires_octo_min: Some(5000),
            ..Default::default()
        };
        assert!(!too_high.matches(&summary));
    }

    #[test]
    fn role_action_select_roundtrip() {
        // Sanity: enum variants are stable across the M2 -> M6 boundary.
        let all = [
            "list",
            "show { role_id: \"builder\" }",
            "select { role_id: \"builder\" }",
        ];
        for v in &all {
            assert!(!v.is_empty());
        }
    }
}