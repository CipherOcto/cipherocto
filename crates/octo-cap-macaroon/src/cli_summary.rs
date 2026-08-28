//! Layer B [ADD] summary types per RFC-0011 §Subcommand Taxonomy.
//!
//! CLI consumers (`octo capability list` etc.) consume these via
//! `octo_cap_macaroon::CapabilitySummary`. The types intentionally
//! carry only CLI-visible fields — no holder signatures, no HMAC
//! chain, no discharge bytes (those stay on the full
//! [`crate::token::CapabilityToken`] and are rendered by higher layers
//! via the `RedactedHex` envelope from mission
//! `0011-core-output-envelope-redaction`).
//!
//! ## Layer discipline
//!
//! Per [[cipherocto-design-principles]]: this is **Layer B additive
//! surface** — the CLI (`octo-cli`, Layer C/D) depends downward into
//! `octo-cap-macaroon` (Layer B); never the reverse. Adding new fields
//! to these structs is fine; renaming or removing fields is breaking
//! and requires a Layer B RFC amendment.
//!
//! ## Attenuation invariants
//!
//! `remaining_budget` and `expires_at_unix` are derived from the caveat
//! chain via [`crate::caveat::Caveat`]; the values shown are the
//! tightest applicable constraint (highest-budget-preserving when no
//! budget caveat exists returns `None`; earliest deadline when no time
//! caveat exists returns `None`).
//!
//! Reference: RFC-0011 §Subcommand Taxonomy entries #8 (`CapabilitySummary`)
//! and #9 (`CaveatSummary`).

use serde::{Deserialize, Serialize};

/// Summary of a capability token — what `octo capability list` shows.
///
/// `cap_id` and `root_id` are truncated to the first 16 hex chars for
/// the list view; the CLI's `-detail` flag (when implemented) renders
/// the full hex via [`crate::token::CapabilityToken::holder_pub`] and
/// [`crate::macaroon::compute_capability_id`].
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CapabilitySummary {
    /// First 16 hex chars of the `capability_id` (truncated for list view).
    pub cap_id: String,
    /// First 16 hex chars of the macaroon `root_id` (truncated for list view).
    pub root_id: String,
    /// Caveat chain — what the holder is constrained to do.
    pub caveats: Vec<CaveatSummary>,
    /// Remaining budget (None if no `Caveat::AmountMax` / `PaymentCaveat`).
    pub remaining_budget: Option<u64>,
    /// Expiry unix timestamp (None if no `Caveat::Before` caveat).
    pub expires_at_unix: Option<i64>,
}

/// Caveat summary (CLI-visible form).
///
/// `kind` is the `CaveatName` discriminant as a stable string
/// (e.g., `"before"`, `"model"`, `"amount_max"`); `body` is the
/// caveat-specific payload as a JSON value (canonical serde form per
/// RFC-0126).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CaveatSummary {
    pub kind: String,
    pub body: serde_json::Value,
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Roundtrip guard: serde_json on the summary types must be
    /// lossless (CLI marshalling contract per RFC-0011 §Output Envelope).
    #[test]
    fn capability_summary_serde_json_roundtrip() {
        let summary = CapabilitySummary {
            cap_id: "0123456789abcdef".to_owned(),
            root_id: "fedcba9876543210".to_owned(),
            caveats: vec![CaveatSummary {
                kind: "model".to_owned(),
                body: serde_json::json!({"model": "gpt-4"}),
            }],
            remaining_budget: Some(1_000),
            expires_at_unix: Some(2_000_000_000),
        };
        let json = serde_json::to_string(&summary).expect("serialize");
        let restored: CapabilitySummary = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(restored.cap_id, summary.cap_id);
        assert_eq!(restored.root_id, summary.root_id);
        assert_eq!(restored.caveats.len(), 1);
        assert_eq!(restored.caveats[0].kind, "model");
        assert_eq!(restored.remaining_budget, Some(1_000));
        assert_eq!(restored.expires_at_unix, Some(2_000_000_000));
    }

    #[test]
    fn caveat_summary_serde_json_roundtrip() {
        let summary = CaveatSummary {
            kind: "before".to_owned(),
            body: serde_json::json!({"deadline_unix": 1_700_000_000}),
        };
        let json = serde_json::to_string(&summary).expect("serialize");
        let restored: CaveatSummary = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(restored.kind, summary.kind);
        assert_eq!(restored.body, summary.body);
    }

    /// `None` fields must serialize as JSON `null` and deserialize
    /// back to `None` — pins the CLI envelope contract for caps with
    /// no budget / no expiry.
    #[test]
    fn capability_summary_optional_fields_null_roundtrip() {
        let summary = CapabilitySummary {
            cap_id: "aabb".to_owned(),
            root_id: "ccdd".to_owned(),
            caveats: vec![],
            remaining_budget: None,
            expires_at_unix: None,
        };
        let json = serde_json::to_string(&summary).expect("serialize");
        assert!(json.contains("\"remaining_budget\":null"));
        assert!(json.contains("\"expires_at_unix\":null"));
        let restored: CapabilitySummary = serde_json::from_str(&json).expect("deserialize");
        assert!(restored.remaining_budget.is_none());
        assert!(restored.expires_at_unix.is_none());
    }
}
