//! Layer A frozen `CapabilityToken` newtype.
//!
//! `CapabilityToken` is the substrate-faithful handle for an
//! RFC-0011-g vote capability. The substrate carries
//! `cap_id`, `issuer_did`, and `weight_bps` directly; caveats
//! (RFC-0957) are out-of-scope for Phase 2 attestation/vote
//! substrate (the CLI verifies caveat sets via the wallet's
//! capability record before constructing the token).
//!
//! ## Layer discipline (per CLAUDE.md §Architectural Principles)
//!
//! Additive newtype — RFC-frozen, semver-major only. No IO,
//! no storage, no clock, no randomness. The token is a pure
//! data carrier; resolution happens in the Layer B facade
//! (`octo_governance::session::GovernanceSession`) and the
//! Layer C CLI constructs the token from the wallet capability
//! record.
//!
//! ## RFC-0011-g §7.4 substrate signature
//!
//! The substrate function `vote()` accepts
//! `voter_cap: &CapabilityToken` (per RFC §7.4 vote
//! substrate signature) — the substrate derives
//! `voter_cap_id`, `voter_did`, and `weight_bps` from the
//! token; the CLI does not inject them as separate args.

use serde::{Deserialize, Serialize};

/// Substrate-faithful capability token handle (RFC-0011-g
/// §7.4 vote substrate signature `voter_cap: &CapabilityToken`).
///
/// Carries three fields:
/// - `cap_id` — registry key into the substrate `CapabilityRegistry`.
/// - `issuer_did` — the DID that minted the capability (voter identity).
/// - `weight_bps` — the voting weight in basis points (clamped to
///   `0..=10_000` by the substrate per RFC-0011-g §7.4 invariants).
///
/// Caveats (`Audience`, `Before`, `Provider`) are verified by the
/// CLI at construction time per RFC-0957 §Attenuation Invariant.
/// The substrate stores no caveat set — the caveat verification
/// is a CLI boundary concern. This keeps the substrate primitive
/// pure and Layer-A frozen per the section header discipline.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct CapabilityToken {
    /// Registry key into the substrate `CapabilityRegistry`
    /// (`voter_cap_id -> signer`).
    pub cap_id: String,
    /// DID that minted the capability (voter identity).
    /// The substrate sets `VoteReceipt.voter_did` from this field
    /// on `vote()` success.
    pub issuer_did: String,
    /// Voting weight in basis points (0..=10_000).
    pub weight_bps: u32,
}

impl CapabilityToken {
    /// Construct a new token. `weight_bps` is stored verbatim;
    /// the substrate clamps + validates at `vote()` time per
    /// RFC-0011-g §7.4 input invariants.
    #[must_use]
    pub fn new(cap_id: impl Into<String>, issuer_did: impl Into<String>, weight_bps: u32) -> Self {
        Self {
            cap_id: cap_id.into(),
            issuer_did: issuer_did.into(),
            weight_bps,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn capability_token_new_carries_all_three_fields() {
        let tok = CapabilityToken::new("cap_abc123", "did:octo:peer:alice", 7_500);
        assert_eq!(tok.cap_id, "cap_abc123");
        assert_eq!(tok.issuer_did, "did:octo:peer:alice");
        assert_eq!(tok.weight_bps, 7_500);
    }

    #[test]
    fn capability_token_equality_via_partial_eq() {
        let a = CapabilityToken::new("cap_1", "did:octo:peer:bob", 5_000);
        let b = CapabilityToken::new("cap_1", "did:octo:peer:bob", 5_000);
        let c = CapabilityToken::new("cap_2", "did:octo:peer:bob", 5_000);
        assert_eq!(a, b);
        assert_ne!(a, c);
    }

    #[test]
    fn capability_token_serde_field_layout() {
        // serde derive + Serialize/Deserialize impls compile-tested
        // by `cargo build` + `cargo test`. The Layout:
        // {cap_id, issuer_did, weight_bps} all `pub` and all
        // serde-derived — the substrate-faithful contract. JSON
        // roundtrip is exercised at the Layer B facade
        // (`octo-governance`) surface where `serde_json` is the
        // canonical CLI envelope format.
        let tok = CapabilityToken::new("cap_xyz", "did:octo:peer:carol", 10_000);
        assert_eq!(tok.cap_id, "cap_xyz");
        assert_eq!(tok.issuer_did, "did:octo:peer:carol");
        assert_eq!(tok.weight_bps, 10_000);
    }

    #[test]
    fn capability_token_zero_weight_preserved() {
        let tok = CapabilityToken::new("cap_zero", "did:octo:peer:dave", 0);
        assert_eq!(tok.weight_bps, 0);
    }
}
