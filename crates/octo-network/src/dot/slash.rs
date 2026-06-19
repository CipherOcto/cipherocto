//! Slash reason codes — RFC-0850p-d §"Slash Reason Codes Added" and
//! RFC-0850p-e §"Slash Reason Codes for KICK_DETECTED"
//!
//! Slash codes are 16-bit identifiers (`u16`) used by the DomainCoordinator
//! to record and tally misbehaviors. The canonical allocation (R1+R2
//! fix) is:
//!
//! ```text
//! 0x000C-0x000D  reserved (sub-DC delegation / governance)
//! 0x000E         CreateGroupFailed  (RFC-0850p-d)
//! 0x000F         CgGroupSpam        (RFC-0850p-d)
//! 0x0010         FalseWitness       (RFC-0850p-d; reused by 0850p-e)
//! 0x0011         SelfKicked         (RFC-0850p-e — applied ONLY on
//!                                 false SELF_KICKED)
//! 0x0012         CrossPlatformWitnessCollusion (RFC-0855p-c §9b)
//! 0x0013-0xFFFF  reserved
//! ```
//!
//! See missions:
//! - `missions/claimed/0850p-d-dc-initiated-group-creation.md` (Phase 6)
//! - `missions/claimed/0850p-e-kick-detection.md` (Phase 3)
//! - `missions/open/0850p-c-base.md` (slash 0x000B `is_reconnect_lie`)

use serde::{Deserialize, Serialize};

/// Slash reason codes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[repr(u16)]
pub enum SlashCode {
    /// (RFC-0850p-c) The BIND's `is_reconnect: true` was a lie.
    IsReconnectLie = 0x000B,
    /// (RFC-0850p-d) `Creating → UnboundQuarantined` transition;
    /// group creation permanently failed.
    CreateGroupFailed = 0x000E,
    /// (RFC-0850p-d) CGROUP rate-limit violation.
    CgGroupSpam = 0x000F,
    /// (RFC-0850p-d) False `WitnessAssertion` (reused by 0850p-e for
    /// false `KICK_DETECTED.witness_assertion`).
    FalseWitness = 0x0010,
    /// (RFC-0850p-e) A `SELF_KICKED` was later determined to be FALSE
    /// (e.g., the bot re-BINDed within `REJOIN_GRANT_TIMEOUT = 50` epochs,
    /// contradicting the claimed kick).
    SelfKicked = 0x0011,
    /// (RFC-0855p-c §9b) Cross-platform witness collusion: two witnesses
    /// on different platforms coordinated a false attestation.
    CrossPlatformWitnessCollusion = 0x0012,
}

impl SlashCode {
    /// Construct from wire `u16`.
    pub fn from_u16(code: u16) -> Option<Self> {
        match code {
            0x000B => Some(Self::IsReconnectLie),
            0x000E => Some(Self::CreateGroupFailed),
            0x000F => Some(Self::CgGroupSpam),
            0x0010 => Some(Self::FalseWitness),
            0x0011 => Some(Self::SelfKicked),
            0x0012 => Some(Self::CrossPlatformWitnessCollusion),
            _ => None,
        }
    }

    /// Returns the wire `u16`.
    pub fn as_u16(self) -> u16 {
        self as u16
    }

    /// Human-readable name.
    pub fn name(self) -> &'static str {
        match self {
            Self::IsReconnectLie => "is_reconnect_lie",
            Self::CreateGroupFailed => "CreateGroupFailed",
            Self::CgGroupSpam => "CgGroupSpam",
            Self::FalseWitness => "FalseWitness",
            Self::SelfKicked => "SelfKicked",
            Self::CrossPlatformWitnessCollusion => "CrossPlatformWitnessCollusion",
        }
    }
}

/// Reserved slash reason code ranges (for documentation and tests).
pub mod reserved {
    /// Lower reserved range (sub-DC delegation / governance).
    pub const RANGE_LOWER: core::ops::Range<u16> = 0x000C..0x000D;
    /// Upper reserved range.
    pub const RANGE_UPPER: core::ops::Range<u16> = 0x0013..0xFFFF;
}

/// Cross-platform slash format: high bit = platform tag; bits 0-14 =
/// base reason.
///
/// Per RFC-0855p-c §9b, a cross-platform slash is encoded as
/// `0x8000 | base_reason`. The platform tag is at the call site (the
/// cross-platform slash is issued by a multi-platform coordinator).
pub fn cross_platform_code(base: u16) -> u16 {
    debug_assert!(
        base & 0x8000 == 0,
        "base reason must not have the platform bit set"
    );
    0x8000 | base
}

/// Check whether a slash code is cross-platform (high bit set).
pub fn is_cross_platform(code: u16) -> bool {
    code & 0x8000 != 0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slash_code_roundtrip() {
        for code in [
            SlashCode::IsReconnectLie,
            SlashCode::CreateGroupFailed,
            SlashCode::CgGroupSpam,
            SlashCode::FalseWitness,
            SlashCode::SelfKicked,
            SlashCode::CrossPlatformWitnessCollusion,
        ] {
            assert_eq!(SlashCode::from_u16(code.as_u16()), Some(code));
        }
        // Reserved range: returns None.
        assert_eq!(SlashCode::from_u16(0x000C), None);
        assert_eq!(SlashCode::from_u16(0x000D), None);
        assert_eq!(SlashCode::from_u16(0xFFFF), None);
    }

    #[test]
    fn cross_platform_encoding() {
        assert_eq!(cross_platform_code(0x0010), 0x8010);
        assert_eq!(cross_platform_code(0x000E), 0x800E);
        assert!(is_cross_platform(0x8010));
        assert!(!is_cross_platform(0x0010));
    }

    #[test]
    fn names_are_distinct() {
        let codes = [
            SlashCode::IsReconnectLie,
            SlashCode::CreateGroupFailed,
            SlashCode::CgGroupSpam,
            SlashCode::FalseWitness,
            SlashCode::SelfKicked,
            SlashCode::CrossPlatformWitnessCollusion,
        ];
        let mut names: Vec<&str> = codes.iter().map(|c| c.name()).collect();
        names.sort();
        names.dedup();
        assert_eq!(
            names.len(),
            codes.len(),
            "slash code names must be distinct"
        );
    }

    #[test]
    fn reserved_ranges_consistent() {
        // 0x000C-0x000D and 0x0013-0xFFFF must NOT be in SlashCode.
        for code in 0x000C..=0x000D {
            assert!(SlashCode::from_u16(code).is_none());
        }
        for code in [0x0013u16, 0x8000, 0xFFFF] {
            assert!(SlashCode::from_u16(code).is_none());
        }
    }
}
