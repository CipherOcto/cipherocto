//! Shared coordinator substrate types (Layer B per CLAUDE.md §Architectural Principles)
//!
//! Canonical home for the coordinator slash + handover type surface that both
//! RFC-0855p-b (slash tally) and RFC-0855p-e (handover) consume. Both RFCs
//! re-import from this crate (no upward dependency; no duplicate definitions
//! across consumer crates).
//!
//! Per RFC-0855p-e §Layer-C Substrate Types follow-on note + RFC-0855p-b
//! §Slash tally references, the previous local Layer-C definitions in
//! `crates/octo-network/src/dot/handover.rs` lift to this Layer-B shared
//! crate. Consumer crates (`octo-network::dot::handover` + future
//! `octo-network::dot::slash`) re-import rather than re-declare.
//!
//! ## Layer discipline
//!
//! - **Layer B (this crate)** — pure types + canonical encoding; no I/O, no
//!   storage, no business logic. Depends only on Layer A primitives
//!   (`ed25519-dalek`, `borsh`, `thiserror`, `serde`).
//! - **Layer C consumers** — `octo-network` (handover + slash tally
//!   substrates) re-imports. Future `octo-coordinator` (if/when it lands)
//!   re-imports identically.
//!
//! ## Extension over enumeration
//!
//! Per CLAUDE.md §Architectural Principles, types with infinite extension
//! surface use **typed-discriminator + Raw escape hatch**, not central enums.
//! `SlashReasonCode` uses an `Extension(u16)` variant that catches the
//! user-extension registry 0x0100-0xFFFF; `HandoverReasonTypeId` is a 128-bit
//! typed wrapper (UUID-tag-style). Old code fails-closed on unknown values.

#![allow(missing_docs)]
#![allow(clippy::missing_docs_in_private_items)]

use thiserror::Error;

// -----------------------------------------------------------------------------
// SlashTallyUpdate (Layer B shared — slash tally update event)
// -----------------------------------------------------------------------------

/// Slash tally update event — published when a coordinator slash decision
/// reaches threshold per RFC-0855p-b §Slash tally references + RFC-0855p-e
/// §Slash tally substrate. Carries the canonical reason code, slashed peer
/// identity, witness count, and the epoch the slash was applied.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct SlashTallyUpdate {
    /// Slash reason code (RFC-0008 §B code space 0x0001-0xFFFF).
    pub slash_reason_code: u16,
    /// Public key of the slashed peer.
    pub slashed_peer_id: [u8; 32],
    /// Number of witness signatures collected.
    pub witness_count: u16,
    /// Epoch when the slash was applied.
    pub epoch: u64,
}

impl SlashTallyUpdate {
    /// Constructor for ergonomic call-site use.
    pub fn new(
        slash_reason_code: u16,
        slashed_peer_id: [u8; 32],
        witness_count: u16,
        epoch: u64,
    ) -> Self {
        Self {
            slash_reason_code,
            slashed_peer_id,
            witness_count,
            epoch,
        }
    }
}

// -----------------------------------------------------------------------------
// SlashReasonCode (Layer B shared — typed discriminator with extension variant)
// -----------------------------------------------------------------------------

/// `SlashReasonCode` typed discriminator for slash tally updates.
///
/// RFC-0855p-e §Layer placement constants block specifies 5 entries (one
/// generic + four handover-flavoured); entries 0x0013-0x0016 (`FalseAttestation`
/// / `QuorumTimeout` / `TallyTamper` / `LateDelivery`) per RFC-0855p-e §Future
/// Work F-7.
///
/// ## Extension safety
///
/// The `Extension(u16)` variant catches the user-extension registry
/// 0x0100-0xFFFF. Match sites MUST include a wildcard arm or explicit
/// `Self::Extension(_) =>` handling. The `reason_id()` getter returns the
/// raw u16 for wire-format mapping.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum SlashReasonCode {
    /// Generic slash (baseline).
    Generic,
    /// Attested to invalid predecessor state.
    FalseAttestation,
    /// Failed to ack HORQ within witness window.
    QuorumTimeout,
    /// Tampered with slash tally evidence.
    TallyTamper,
    /// Delivered slash tally update after grace window.
    LateDelivery,
    /// User-extension variant (RFC-allocated namespace 0x0100-0xFFFF).
    Extension(u16),
}

impl SlashReasonCode {
    /// Wire-format reason code (u16 per RFC-0008 §B).
    pub fn reason_id(&self) -> u16 {
        match self {
            Self::Generic => 0x0001,
            Self::FalseAttestation => 0x0013,
            Self::QuorumTimeout => 0x0014,
            Self::TallyTamper => 0x0015,
            Self::LateDelivery => 0x0016,
            Self::Extension(id) => *id,
        }
    }

    /// Parse from wire-format u16. RFC-allocated entries 0x0001 + 0x0013-0x0016
    /// return their typed variants; everything else (including the
    /// 0x0100-0xFFFF user-extension range) returns `Self::Extension(id)`.
    pub fn from_reason_id(id: u16) -> Self {
        match id {
            0x0001 => Self::Generic,
            0x0013 => Self::FalseAttestation,
            0x0014 => Self::QuorumTimeout,
            0x0015 => Self::TallyTamper,
            0x0016 => Self::LateDelivery,
            other => Self::Extension(other),
        }
    }
}

/// Error type for `SlashReasonCode` reserved-range violations.
///
/// Reserved range 0x0002-0x0012 + 0x0017-0x00FF is NOT valid as either
/// a typed variant or a user-extension. Callers using those values must
/// upgrade.
#[derive(Debug, Error, Clone, PartialEq, Eq)]
#[error("invalid SlashReasonCode 0x{got:04x}: reserved range 0x0002-0x0012 / 0x0017-0x00FF")]
pub struct SlashReasonCodeError {
    pub got: u16,
}

impl SlashReasonCode {
    /// Strict constructor — rejects reserved-range u16 values.
    ///
    /// Use `from_reason_id` when parsing untrusted input that may include
    /// reserved values (which become `Extension` variants without error).
    /// Use `try_from_reason_id` when you want to fail-closed on reserved
    /// values rather than silently promote them to the extension namespace.
    pub fn try_from_reason_id(id: u16) -> Result<Self, SlashReasonCodeError> {
        match id {
            0x0002..=0x0012 | 0x0017..=0x00FF => Err(SlashReasonCodeError { got: id }),
            other => Ok(Self::from_reason_id(other)),
        }
    }
}

// -----------------------------------------------------------------------------
// HandoverReasonTypeId (Layer B shared — typed 128-bit handover reason tag)
// -----------------------------------------------------------------------------

/// Typed handover reason discriminator (128-bit UUID-tag-style value).
///
/// Used by slash tally to map handover triggers (`SubDCVoluntaryResignation`
/// etc.) to canonical handover reason tags. The 128-bit tag gives a
/// flat extension surface — no central enum; user extensions mint their
/// own UUID-style tag and consumers match on the typed wrapper.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub struct HandoverReasonTypeId(pub [u8; 16]);

impl HandoverReasonTypeId {
    /// RFC-0855p-e §Layer placement: `SubDCVoluntaryResignation` slash
    /// reason triggers this handover reason tag.
    pub const SUBDC_VOLUNTARY_RESIGNATION: HandoverReasonTypeId = HandoverReasonTypeId([
        0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
        0x01,
    ]);

    /// RFC-0855p-e §Layer placement: `SubDCMisconduct` slash reason.
    pub const SUBDC_MISCONDUCT: HandoverReasonTypeId = HandoverReasonTypeId([
        0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
        0x02,
    ]);

    /// RFC-0855p-e §Layer placement: `TermExpired` slash reason.
    pub const TERM_EXPIRED: HandoverReasonTypeId = HandoverReasonTypeId([
        0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
        0x03,
    ]);

    /// Construct from a raw 16-byte tag.
    pub const fn new(bytes: [u8; 16]) -> Self {
        Self(bytes)
    }

    /// Borrow the underlying 16-byte tag.
    pub fn as_bytes(&self) -> &[u8; 16] {
        &self.0
    }
}

// -----------------------------------------------------------------------------
// Tests
// -----------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    // TV-CT-1: SlashTallyUpdate round-trip + structural equality.
    #[test]
    fn tv_ct_1_slash_tally_update_round_trip() {
        let update = SlashTallyUpdate::new(0x0013, [0xAAu8; 32], 3, 100);
        assert_eq!(update.slash_reason_code, 0x0013);
        assert_eq!(update.slashed_peer_id, [0xAAu8; 32]);
        assert_eq!(update.witness_count, 3);
        assert_eq!(update.epoch, 100);

        let copy = update.clone();
        assert_eq!(update, copy);
    }

    // TV-CT-2: SlashReasonCode 0x0013-0x0016 entries; reserved-range rejection.
    #[test]
    fn tv_ct_2_slash_reason_code_entries_and_reserved_range() {
        // Allocated entries map round-trip.
        for (id, expected) in [
            (0x0001u16, SlashReasonCode::Generic),
            (0x0013, SlashReasonCode::FalseAttestation),
            (0x0014, SlashReasonCode::QuorumTimeout),
            (0x0015, SlashReasonCode::TallyTamper),
            (0x0016, SlashReasonCode::LateDelivery),
        ] {
            assert_eq!(SlashReasonCode::from_reason_id(id).reason_id(), id);
            assert_eq!(SlashReasonCode::from_reason_id(id), expected);
        }

        // User-extension range 0x0100-0xFFFF round-trips via Extension variant.
        for id in [0x0100u16, 0x0200, 0xABCD, 0xFFFF] {
            assert_eq!(
                SlashReasonCode::from_reason_id(id),
                SlashReasonCode::Extension(id)
            );
        }

        // Strict constructor accepts allocated + extension; rejects reserved.
        assert!(SlashReasonCode::try_from_reason_id(0x0001).is_ok());
        assert!(SlashReasonCode::try_from_reason_id(0x0013).is_ok());
        assert!(SlashReasonCode::try_from_reason_id(0x0100).is_ok());
        for reserved in [0x0002u16, 0x0007, 0x0012, 0x0017, 0x00FF] {
            assert_eq!(
                SlashReasonCode::try_from_reason_id(reserved),
                Err(SlashReasonCodeError { got: reserved })
            );
        }
    }

    // TV-CT-3: HandoverReasonTypeId typed-discriminator pattern.
    #[test]
    fn tv_ct_3_handover_reason_type_id_typed_discriminator() {
        let tag = HandoverReasonTypeId::SUBDC_VOLUNTARY_RESIGNATION;
        assert_eq!(
            tag.as_bytes(),
            &[0u8, 0, 0, 0, 0, 0, 0, 1, 0, 0, 0, 0, 0, 0, 0, 1]
        );
        let misconduct = HandoverReasonTypeId::SUBDC_MISCONDUCT;
        assert_ne!(tag, misconduct);
        assert_eq!(
            misconduct,
            HandoverReasonTypeId::new([0, 0, 0, 0, 0, 0, 0, 1, 0, 0, 0, 0, 0, 0, 0, 2])
        );
    }

    // TV-CT-4: SlashReasonCode extension variant equality + reason_id getter.
    #[test]
    fn tv_ct_4_extension_variant_equality() {
        let a = SlashReasonCode::Extension(0xABCD);
        let b = SlashReasonCode::from_reason_id(0xABCD);
        assert_eq!(a, b);
        assert_eq!(a.reason_id(), 0xABCD);
    }
}
