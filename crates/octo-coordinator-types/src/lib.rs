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
//! typed wrapper (UUID-tag-style). RFC-0855p-b §Appendix B canonical
//! entries `0x0001..=0x0012` (except reserved `0x000C..=0x000D` per
//! RFC-0855p-d) + `0x0013..=0x0016` (RFC-0855p-e extension) are named
//! enum variants; anything outside stays in the extension namespace. Old
//! code fails-closed on unknown reserved values via `try_from_reason_id`.

#![allow(missing_docs)]
#![allow(clippy::missing_docs_in_private_items)]

use thiserror::Error;

// -----------------------------------------------------------------------------
// Module surface — RFC-0855p-b §Phase 1 state-machine substrate.
// -----------------------------------------------------------------------------

/// Mission Coordinator state-machine substrate (RFC-0855p-b §Implementation
/// Phase 1): 8 base types + v1.1 `GenesisState` + 12-transition
/// validity table.
pub mod state;

/// Mission Coordinator election algorithm (RFC-0855p-b §Implementation
/// Phase 2): 5 governance-model branches + eligibility filter + lex
/// tie-break + `ELECTION_TIMEOUT` closed-epoch rule.
pub mod election;

/// Mission Coordinator liveness substrate (RFC-0855p-b §Implementation
/// Phase 3): `CoordinatorHeartbeat` envelope + `LivenessTracker` +
/// `evaluate_liveness` (Active → Suspect / Suspect → Active /
/// Suspect → Handover).
pub mod liveness;

/// Mission Coordinator slashing substrate (RFC-0855p-b §Implementation
/// Phase 5): `verify_slash_proof` + `apply_slash` + `CoolDownTracker`.
/// Composes with `SlashReasonCode` + `SlashTallyUpdate` from this crate
/// and `SlashProof` + `validate_transition` from the `state` module.
pub mod slashing;

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
/// Per RFC-0855p-b §Appendix B L945+ (extending RFC-0855 §17 "Token
/// Economics Integration"), the canonical slash offense code set covers
/// `0x0001..=0x0012` (skipping reserved `0x000C..=0x000D` per RFC-0855p-d
/// §Sub-DC delegation protocol) + `0x0013..=0x0016` (RFC-0855p-e Future
/// Work F-7 extension: `FalseAttestation` / `QuorumTimeout` /
/// `TallyTamper` / `LateDelivery`). Outside that range (user-extension
/// registry `0x0100..=0xFFFF`), the [`SlashReasonCode::Extension`] catch-all
/// variant absorbs unknown codes; values in `0x0017..=0x00FF` are reserved
/// and `try_from_reason_id` rejects them.
///
/// ## Discriminant mapping
///
/// | u16 | Variant | Source |
/// |-----|---------|--------|
/// | `0x0001` | `DoubleSign` | RFC-0855p-b §Appendix B |
/// | `0x0002` | `LivenessFailure` | RFC-0855p-b §Appendix B |
/// | `0x0003` | `FounderSquat` | RFC-0855p-b §Appendix B |
/// | `0x0004` | `Censorship` | RFC-0855p-b §Appendix B |
/// | `0x0005` | `CoordinatorMisbehavior` | RFC-0855p-b §Appendix B |
/// | `0x0006` | `KeyCompromise` | RFC-0855p-b §Appendix B |
/// | `0x0007` | `BanningLegitimateMember` | RFC-0855p-b §Appendix B |
/// | `0x0008` | `VoteBuying` | RFC-0855p-b §Appendix B |
/// | `0x0009` | `GenesisCompromise` | RFC-0855p-b §Appendix B (v1.1 R1-CL-1) |
/// | `0x000A` | `PlatformMigration` | RFC-0850p-c §6a |
/// | `0x000B` | `IsReconnectLie` | RFC-0850p-c §8 |
/// | `0x000C..=0x000D` | RESERVED (per RFC-0855p-d) |
/// | `0x000E` | `CreateGroupFailed` | RFC-0855p-b §Appendix B |
/// | `0x000F` | `CgGroupSpam` | RFC-0855p-b §Appendix B |
/// | `0x0010` | `FalseWitness` | RFC-0855p-b §Appendix B |
/// | `0x0011` | `SelfKicked` | RFC-0855p-b §Appendix B |
/// | `0x0012` | `CrossPlatformWitnessCollusion` | RFC-0855p-c §9b |
/// | `0x0013` | `FalseAttestation` | RFC-0855p-e Future Work F-7 |
/// | `0x0014` | `QuorumTimeout` | RFC-0855p-e Future Work F-7 |
/// | `0x0015` | `TallyTamper` | RFC-0855p-e Future Work F-7 |
/// | `0x0016` | `LateDelivery` | RFC-0855p-e Future Work F-7 |
/// | `0x0100..=0xFFFF` | `Extension(id)` | user-extension registry |
/// | `0x0100` | `DomainCoordinatorMisbehavior` (sub-codes .01-.04) | RFC-0855p-c §9c (user-extension namespace; first allocated slot) |
///
/// ## Extension safety
///
/// The `Extension(u16)` variant catches the user-extension registry
/// `0x0100..=0xFFFF`. Match sites MUST include a wildcard arm or explicit
/// `Self::Extension(_) =>` handling. The `reason_id()` getter returns the
/// raw u16 for wire-format mapping.
///
/// First allocated user-extension slot: `0x0100 = DomainCoordinatorMisbehavior`
/// (RFC-0855p-c §9c "Cross-Domain Slash") with 4 sub-codes stored in
/// `slash_reason_data` low 16 bits (`.01 invalid_bind_envelope`,
/// `.02 failed_attest`, `.03 censored_legit_member`,
/// `.04 signed_malicious_envelope`). Wired into
/// `octo-network::dc::slash::{DcMisbehavior, DcSlashEnvelope}` (mission
/// `0855p-c-cross-domain-slash`); the canonical enum exposes this slot
/// through `Extension(0x0100)` rather than a typed variant, preserving
/// `Extension`-over-`enum` design per CLAUDE.md §Extension over
/// enumeration. Callers that need typed semantics for `0x0100` should
/// match `Self::Extension(0x0100)` and route to `dc::slash::DcMisbehavior`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum SlashReasonCode {
    /// RFC-0855p-b §Appendix B 0x0001 — same-block double-sign.
    DoubleSign,
    /// RFC-0855p-b §Appendix B 0x0002 — coordinator liveness failure.
    LivenessFailure,
    /// RFC-0855p-b §Appendix B 0x0003 — founder squat on mission slot.
    FounderSquat,
    /// RFC-0855p-b §Appendix B 0x0004 — selective censorship of
    /// admissible operations.
    Censorship,
    /// RFC-0855p-b §Appendix B 0x0005 — generic coordinator misbehavior
    /// (catch-all for RFC-discrete-defined coordinator wrongdoing).
    CoordinatorMisbehavior,
    /// RFC-0855p-b §Appendix B 0x0006 — coordinator key compromise.
    KeyCompromise,
    /// RFC-0855p-b §Appendix B 0x0007 — banning a legitimate mission
    /// member.
    BanningLegitimateMember,
    /// RFC-0855p-b §Appendix B 0x0008 — vote-buying proof.
    VoteBuying,
    /// RFC-0855p-b §Appendix B 0x0009 — creator-key compromise at
    /// mission genesis (v1.1 R1-CL-1).
    GenesisCompromise,
    /// RFC-0850p-c §6a — falsified platform-migration envelope.
    PlatformMigration,
    /// RFC-0850p-c §8 — reconnect-lie assertion.
    IsReconnectLie,
    /// RFC-0855p-b §Appendix B 0x000E — `CreateGroup` envelope failed
    /// protocol rules.
    CreateGroupFailed,
    /// RFC-0855p-b §Appendix B 0x000F — `CG` group spam.
    CgGroupSpam,
    /// RFC-0855p-b §Appendix B 0x0010 — false witness attestation.
    FalseWitness,
    /// RFC-0855p-b §Appendix B 0x0011 — coordinator self-kick.
    SelfKicked,
    /// RFC-0855p-c §9b — cross-platform witness collusion.
    CrossPlatformWitnessCollusion,
    /// RFC-0855p-e Future Work F-7 0x0013 — attested to invalid
    /// predecessor state.
    FalseAttestation,
    /// RFC-0855p-e Future Work F-7 0x0014 — failed to ack `HORQ` within
    /// witness window.
    QuorumTimeout,
    /// RFC-0855p-e Future Work F-7 0x0015 — tampered with slash tally
    /// evidence.
    TallyTamper,
    /// RFC-0855p-e Future Work F-7 0x0016 — delivered slash tally update
    /// after grace window.
    LateDelivery,
    /// User-extension variant (RFC-allocated namespace 0x0100-0xFFFF).
    Extension(u16),
}

impl SlashReasonCode {
    /// Wire-format reason code (u16 per RFC-0008 §B).
    pub fn reason_id(&self) -> u16 {
        match self {
            Self::DoubleSign => 0x0001,
            Self::LivenessFailure => 0x0002,
            Self::FounderSquat => 0x0003,
            Self::Censorship => 0x0004,
            Self::CoordinatorMisbehavior => 0x0005,
            Self::KeyCompromise => 0x0006,
            Self::BanningLegitimateMember => 0x0007,
            Self::VoteBuying => 0x0008,
            Self::GenesisCompromise => 0x0009,
            Self::PlatformMigration => 0x000A,
            Self::IsReconnectLie => 0x000B,
            Self::CreateGroupFailed => 0x000E,
            Self::CgGroupSpam => 0x000F,
            Self::FalseWitness => 0x0010,
            Self::SelfKicked => 0x0011,
            Self::CrossPlatformWitnessCollusion => 0x0012,
            Self::FalseAttestation => 0x0013,
            Self::QuorumTimeout => 0x0014,
            Self::TallyTamper => 0x0015,
            Self::LateDelivery => 0x0016,
            Self::Extension(id) => *id,
        }
    }

    /// Parse from wire-format u16. Per RFC-0855p-b §Appendix B, the
    /// canonical set `0x0001..=0x0012` (skipping reserved `0x000C..=0x000D`)
    /// + `0x0013..=0x0016` round-trip as typed variants; everything else
    /// (including the `0x0100..=0xFFFF` user-extension range) returns
    /// `Self::Extension(id)`. Note: passing a reserved `0x000C` or `0x000D`
    /// value silently promotes to `Extension(0x000C)` — use
    /// [`try_from_reason_id`] when fail-closed is required.
    pub fn from_reason_id(id: u16) -> Self {
        match id {
            0x0001 => Self::DoubleSign,
            0x0002 => Self::LivenessFailure,
            0x0003 => Self::FounderSquat,
            0x0004 => Self::Censorship,
            0x0005 => Self::CoordinatorMisbehavior,
            0x0006 => Self::KeyCompromise,
            0x0007 => Self::BanningLegitimateMember,
            0x0008 => Self::VoteBuying,
            0x0009 => Self::GenesisCompromise,
            0x000A => Self::PlatformMigration,
            0x000B => Self::IsReconnectLie,
            0x000E => Self::CreateGroupFailed,
            0x000F => Self::CgGroupSpam,
            0x0010 => Self::FalseWitness,
            0x0011 => Self::SelfKicked,
            0x0012 => Self::CrossPlatformWitnessCollusion,
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
/// Reserved range `0x000C..=0x000D` (per RFC-0855p-d §Sub-DC delegation
/// protocol — NOT slash reasons) + `0x0017..=0x00FF` is NOT valid as
/// either a typed variant or a user-extension. Callers using those values
/// must upgrade.
#[derive(Debug, Error, Clone, PartialEq, Eq)]
#[error("invalid SlashReasonCode 0x{got:04x}: reserved range 0x000C-0x000D / 0x0017-0x00FF")]
pub struct SlashReasonCodeError {
    /// Rejected reserved-range u16 value.
    pub got: u16,
}

impl SlashReasonCode {
    /// Strict constructor — rejects reserved-range u16 values.
    ///
    /// Use `from_reason_id` when parsing untrusted input that may include
    /// reserved values (which become `Extension` variants without error).
    /// Use `try_from_reason_id` when you want to fail-closed on reserved
    /// values rather than silently promote them to the extension namespace.
    ///
    /// Reserved range: `0x000C..=0x000D` + `0x0017..=0x00FF`. After
    /// RFC-0855p-b §Appendix B canonical-set extension, the previous
    /// `0x0002..=0x0012` "reserved" carve-out is gone (those codes are now
    /// named variants in `SlashReasonCode`).
    pub fn try_from_reason_id(id: u16) -> Result<Self, SlashReasonCodeError> {
        match id {
            0x000C..=0x000D | 0x0017..=0x00FF => Err(SlashReasonCodeError { got: id }),
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

    // TV-CT-2: SlashReasonCode canonical-set entries; reserved-range rejection.
    #[test]
    fn tv_ct_2_slash_reason_code_entries_and_reserved_range() {
        // Allocated entries map round-trip (RFC-0855p-b §Appendix B
        // canonical + RFC-0855p-e 0x0013-0x0016 extension).
        for (id, expected) in [
            (0x0001u16, SlashReasonCode::DoubleSign),
            (0x0002, SlashReasonCode::LivenessFailure),
            (0x0003, SlashReasonCode::FounderSquat),
            (0x0009, SlashReasonCode::GenesisCompromise),
            (0x000A, SlashReasonCode::PlatformMigration),
            (0x000B, SlashReasonCode::IsReconnectLie),
            (0x000E, SlashReasonCode::CreateGroupFailed),
            (0x0010, SlashReasonCode::FalseWitness),
            (0x0011, SlashReasonCode::SelfKicked),
            (0x0012, SlashReasonCode::CrossPlatformWitnessCollusion),
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

        // Strict constructor accepts canonical + extension; rejects reserved.
        assert!(SlashReasonCode::try_from_reason_id(0x0001).is_ok());
        assert!(SlashReasonCode::try_from_reason_id(0x0009).is_ok());
        assert!(SlashReasonCode::try_from_reason_id(0x0012).is_ok());
        assert!(SlashReasonCode::try_from_reason_id(0x0013).is_ok());
        assert!(SlashReasonCode::try_from_reason_id(0x0100).is_ok());

        // Reserved: 0x000C..=0x000D (RFC-0855p-d) + 0x0017..=0x00FF.
        for reserved in [0x000Cu16, 0x000D, 0x0017, 0x00FF, 0x0080] {
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
