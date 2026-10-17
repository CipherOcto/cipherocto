//! Sub-Group Teardown substrate — RFC-0855p-d3 §Sub-Group Decommission
//!
//! Implements the `TeardownProofEnvelope` (subtype `b"SGTP"`) plus the
//! `TeardownProof` + `TeardownReasonCode` types and the grace-elapsed check
//! (`local_epoch - dissolving_epoch >= TEARDOWN_GRACE_EPOCHS`) that triggers
//! the `Dissolving → Dissolved` state transition per RFC-0855p-d3
//! §Sub-Group Decommission + Appendix A.
//!
//! See RFC-0855p-d3 and `missions/claimed/0855p-d3-subgroup-routing-aggregation-teardown.md`.
//!
//! ## Re-export
//!
//! Constants `MAX_AGGREGATE_ATTESTATIONS` + `TEARDOWN_GRACE_EPOCHS` +
//! `SUBGROUP_TEARDOWN_CONTEXT` + the `SUBGROUP_TEARDOWN` subtype tag are
//! defined in `subgroup_routing.rs` (canonical home per §Layer placement
//! table). This module re-uses them and adds teardown-specific types.

use super::subgroup_routing::{
    SUBGROUP_TEARDOWN, SUBGROUP_TEARDOWN_CONTEXT, TEARDOWN_GRACE_EPOCHS,
};

// -----------------------------------------------------------------------------
// TeardownReasonCode (Layer C — typed discriminator with extension variant)
// -----------------------------------------------------------------------------

/// `TeardownReasonCode` typed discriminator for sub-group teardown triggers
/// per RFC-0855p-d3 §Data Structure.
///
/// ## Extension safety
///
/// The `Extension([u8; 16])` variant catches user-extension registry. Match
/// sites MUST include a wildcard arm or explicit `Self::Extension(_) =>`
/// handling. `reason_tag()` returns the raw 16-byte tag for wire mapping.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum TeardownReasonCode {
    /// Sub-DC voluntary teardown (child decided to dissolve).
    ChildVoluntary,
    /// Parent cascade (parent UNBIND initiated cascade).
    ParentCascade,
    /// Coordinator rotation triggered teardown.
    CoordinatorRotation,
    /// Sub-DC revoked (RFC-0855p-d2 SDRV).
    SubDCRevoked,
    /// Mission concluded (RFC-0855p-b bridge).
    MissionConcluded,
    /// User-extension variant (128-bit tag).
    Extension([u8; 16]),
}

impl TeardownReasonCode {
    /// Wire-format 16-byte tag (RFC-0855p-d3 §Data Structure canonical form).
    /// Typed variants map to a 16-byte tag with a unique low byte (0x01..0x05);
    /// user-extension variants carry their own tag verbatim.
    pub fn reason_tag(&self) -> [u8; 16] {
        match self {
            Self::ChildVoluntary => {
                let mut t = [0u8; 16];
                t[15] = 0x01;
                t
            }
            Self::ParentCascade => {
                let mut t = [0u8; 16];
                t[15] = 0x02;
                t
            }
            Self::CoordinatorRotation => {
                let mut t = [0u8; 16];
                t[15] = 0x03;
                t
            }
            Self::SubDCRevoked => {
                let mut t = [0u8; 16];
                t[15] = 0x04;
                t
            }
            Self::MissionConcluded => {
                let mut t = [0u8; 16];
                t[15] = 0x05;
                t
            }
            Self::Extension(tag) => *tag,
        }
    }
}

// -----------------------------------------------------------------------------
// TeardownProof (Layer B — wire-format semantics; canonical form per RFC-0126)
// -----------------------------------------------------------------------------

/// `TeardownProof` — inner DCS structure carried by `TeardownProofEnvelope`.
/// Per RFC-0855p-d3 §Data Structure.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TeardownProof {
    pub parent_domain_id: [u8; 32],
    pub sub_domain_id: [u8; 32],
    pub tearing_down_dc_id: [u8; 32],
    pub dissolving_epoch: u64,
    /// Informational/audit-only per RFC-0855p-d3 Appendix A — grace check
    /// uses `local_epoch - dissolving_epoch`, NOT this field.
    pub teardown_epoch: u64,
    pub reason: TeardownReasonCode,
    pub dc_signature: [u8; 64],
}

// -----------------------------------------------------------------------------
// TeardownProofEnvelope (Layer B — outer wire format)
// -----------------------------------------------------------------------------

/// SGTP — sub-group teardown proof envelope (final attestation that releases
/// resources + cascades dissolution).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TeardownProofEnvelope {
    pub envelope_type: [u8; 4],
    pub envelope_subtype: [u8; 4],
    pub version: u16,
    pub teardown_proof: TeardownProof,
}

impl TeardownProofEnvelope {
    /// Construct an SGTP envelope with the canonical 10-byte header.
    pub fn new(teardown_proof: TeardownProof) -> Self {
        Self {
            envelope_type: *b"DOT1",
            envelope_subtype: SUBGROUP_TEARDOWN,
            version: 0x0001,
            teardown_proof,
        }
    }
}

// -----------------------------------------------------------------------------
// Teardown grace check (Layer C — Appendix A enforcement)
// -----------------------------------------------------------------------------

/// Outcome of the teardown grace-elapsed check.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TeardownGraceOutcome {
    /// Grace elapsed; SGTP valid; transition `Dissolving → Dissolved`.
    GraceElapsed,
    /// Grace NOT elapsed; reject `TeardownTooEarly`.
    TeardownTooEarly,
    /// Local clock < dissolving_epoch (clock anomaly; reject).
    LocalClockBehind,
}

/// Verify the teardown grace check per RFC-0855p-d3 Appendix A:
/// `local_epoch - dissolving_epoch >= TEARDOWN_GRACE_EPOCHS`.
///
/// The recipient's `local_epoch` is the source of truth; the envelope's
/// `teardown_epoch` field is informational/audit-only (NOT used in the
/// grace arithmetic). This prevents late-delivery bypass: an attacker
/// cannot forge a `teardown_epoch` to make the grace check pass early.
pub fn check_teardown_grace(local_epoch: u64, dissolving_epoch: u64) -> TeardownGraceOutcome {
    if local_epoch < dissolving_epoch {
        return TeardownGraceOutcome::LocalClockBehind;
    }
    if local_epoch - dissolving_epoch >= TEARDOWN_GRACE_EPOCHS {
        TeardownGraceOutcome::GraceElapsed
    } else {
        TeardownGraceOutcome::TeardownTooEarly
    }
}

// -----------------------------------------------------------------------------
// BLAKE3 signature domain helper (Layer A — pure derivation)
// -----------------------------------------------------------------------------

/// Derive the BLAKE3 keyed-hash key for `SUBGROUP_TEARDOWN_CONTEXT`. Used by
/// SGTP issuers when signing the canonical DCS encoding.
pub fn teardown_signature_key() -> [u8; 32] {
    blake3::derive_key(SUBGROUP_TEARDOWN_CONTEXT, b"")
}

// -----------------------------------------------------------------------------
// Tests
// -----------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    // TV-SG-9: teardown grace enforcement (early + elapsed paths).
    #[test]
    fn tv_sg_9_teardown_grace_enforcement() {
        // Early teardown: current_epoch - dissolving_epoch = 20 < 50
        let early = check_teardown_grace(120, 100);
        assert_eq!(early, TeardownGraceOutcome::TeardownTooEarly);

        // Boundary: exactly TEARDOWN_GRACE_EPOCHS elapsed
        let boundary = check_teardown_grace(150, 100);
        assert_eq!(boundary, TeardownGraceOutcome::GraceElapsed);

        // Past boundary: 100 epochs elapsed
        let elapsed = check_teardown_grace(200, 100);
        assert_eq!(elapsed, TeardownGraceOutcome::GraceElapsed);

        // Clock anomaly: local behind dissolving
        let behind = check_teardown_grace(50, 100);
        assert_eq!(behind, TeardownGraceOutcome::LocalClockBehind);
    }

    // TV-SG-9 (continued): teardown envelope canonical header.
    #[test]
    fn sgtp_envelope_canonical_header() {
        let proof = TeardownProof {
            parent_domain_id: [0x01; 32],
            sub_domain_id: [0x02; 32],
            tearing_down_dc_id: [0x03; 32],
            dissolving_epoch: 100,
            teardown_epoch: 150,
            reason: TeardownReasonCode::ChildVoluntary,
            dc_signature: [0u8; 64],
        };
        let env = TeardownProofEnvelope::new(proof);
        assert_eq!(env.envelope_type, *b"DOT1");
        assert_eq!(env.envelope_subtype, SUBGROUP_TEARDOWN);
        assert_eq!(env.envelope_subtype, *b"SGTP");
        assert_eq!(env.version, 0x0001);
    }

    // TeardownReasonCode reason_tag round-trip for typed variants.
    #[test]
    fn teardown_reason_code_reason_tag_round_trip() {
        let cases = [
            (TeardownReasonCode::ChildVoluntary, 0x01u8),
            (TeardownReasonCode::ParentCascade, 0x02),
            (TeardownReasonCode::CoordinatorRotation, 0x03),
            (TeardownReasonCode::SubDCRevoked, 0x04),
            (TeardownReasonCode::MissionConcluded, 0x05),
        ];
        for (variant, expected_last_byte) in cases {
            let tag = variant.reason_tag();
            assert_eq!(tag[15], expected_last_byte);
            assert_eq!(&tag[..15], &[0u8; 15]);
        }

        // Extension variant returns its own tag verbatim.
        let ext_tag = [1u8; 16];
        let ext = TeardownReasonCode::Extension(ext_tag);
        assert_eq!(ext.reason_tag(), ext_tag);
    }

    // TEARDOWN_GRACE_EPOCHS re-exported value matches RFC-0855p-d3 §Data Structure.
    #[test]
    fn teardown_grace_epochs_constant() {
        assert_eq!(TEARDOWN_GRACE_EPOCHS, 50);
    }
}
