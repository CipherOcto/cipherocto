//! Identity Lifecycle State Machine (RFC-0009 §Lifecycle Requirements).
//!
//! Lifecycle states + transitions per RFC-0009 §Identity Lifecycle State Machine:
//!
//! ```text
//! Designated ──[activate]──→ Active ──[revoke]──→ Revoked (terminal)
//!                                ↑
//!                                └─── (l2) Rotating ──[complete|abort]──→ Active/Revoked
//! ```
//!
//! ## Layer discipline
//!
//! This module is Layer B (identity substrate). It does NOT depend on
//! `octo-transport` (Layer D). Revocation event fan-out ships in
//! `0009-l2-rotation-successor-linkage` via `octo_transport::NodeTransport`.
//!
//! ## Discriminant representation
//!
//! `#[repr(u8)]` values match RFC-0009 Appendix A:
//! - `Designated = 0x00`
//! - `Active     = 0x01`
//! - `Rotating   = 0x02` — `Active ↔ Rotating` transitions live in
//!   `IdentityKey::begin_rotation` / `complete_rotation` / `abort_rotation`.
//!   Both `Rotating → Revoked` and `Rotating → Active` are valid edges
//!   per RFC-0009 §Lifecycle table.
//! - `Revoked    = 0x03`
//!
//! ## Forward-compat surface
//!
//! `from_u8`, `is_active`, `is_revoked`, `is_rotating`, and
//! `can_transition_to` are public API for the planned `IdentityKey` wire
//! format (RFC-0009 v2) + state-machine introspection. Not called
//! internally today; reserved for future cross-node state
//! synchronization.

use std::fmt;

/// Identity lifecycle state (RFC-0009 §Identity Lifecycle State Machine).
///
/// `repr(u8)` discriminant values match RFC-0009 Appendix A exactly
/// (cross-implementation determinism invariant).
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum LifecycleState {
    /// Named at init, not yet active. Holder may NOT sign.
    Designated = 0x00,
    /// Identity in use; signing operations live.
    Active = 0x01,
    /// Successor link established; old key valid during grace.
    /// Transitions: `Active → Rotating` via `begin_rotation`,
    /// `Rotating → Active` via `complete_rotation` or `abort_rotation`.
    /// Old key may sign during the grace window per RFC-0009 §Lifecycle row 3.
    Rotating = 0x02,
    /// Identity retired; signature verification rejected. Terminal.
    Revoked = 0x03,
}

impl LifecycleState {
    /// True iff this state permits signing.
    ///
    /// `Active` and `Rotating` (during grace) permit signing per RFC-0009
    /// §Lifecycle row 3 + row 4. `Designated` rejects (not yet active);
    /// `Revoked` rejects (terminal).
    #[must_use]
    pub const fn can_sign(self) -> bool {
        matches!(self, Self::Active | Self::Rotating)
    }

    /// True iff this state is the terminal `Revoked` state.
    #[must_use]
    pub const fn is_revoked(self) -> bool {
        matches!(self, Self::Revoked)
    }

    /// True iff this state is the `Rotating` state.
    #[must_use]
    pub const fn is_rotating(self) -> bool {
        matches!(self, Self::Rotating)
    }

    /// True iff this state is the `Active` state.
    #[must_use]
    pub const fn is_active(self) -> bool {
        matches!(self, Self::Active)
    }

    /// Valid state machine edges per RFC-0009 §Identity Lifecycle State Machine.
    ///
    /// l1 owns: `Designated → Active`, `Active → Revoked`.
    /// l2 owns: `Active ↔ Rotating`, `Rotating → Revoked`.
    /// `Revoked` is terminal — no outbound edges.
    #[must_use]
    pub const fn can_transition_to(self, target: Self) -> bool {
        matches!(
            (self, target),
            (Self::Designated | Self::Rotating, Self::Active)
                | (Self::Designated | Self::Active, Self::Rotating)
                | (Self::Active | Self::Rotating, Self::Revoked)
        )
    }
}

/// Manual `Debug` impl: unit variants display as their name (no credential
/// material — `LifecycleState` carries no PII).
impl fmt::Debug for LifecycleState {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let s = match self {
            Self::Designated => "Designated",
            Self::Active => "Active",
            Self::Rotating => "Rotating",
            Self::Revoked => "Revoked",
        };
        f.write_str(s)
    }
}

/// Reconstruct `LifecycleState` from its `#[repr(u8)]` discriminant byte.
///
/// # Errors
/// Returns `None` if the byte does not match a defined variant (forward
/// compat: future RFC-0009 amendments may add states; old impls fail
/// closed on unknown discriminators).
#[must_use]
pub fn from_u8(byte: u8) -> Option<LifecycleState> {
    match byte {
        0x00 => Some(LifecycleState::Designated),
        0x01 => Some(LifecycleState::Active),
        0x02 => Some(LifecycleState::Rotating),
        0x03 => Some(LifecycleState::Revoked),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lifecycle_repr_u8_matches_appendix_a() {
        assert_eq!(LifecycleState::Designated as u8, 0x00);
        assert_eq!(LifecycleState::Active as u8, 0x01);
        assert_eq!(LifecycleState::Rotating as u8, 0x02);
        assert_eq!(LifecycleState::Revoked as u8, 0x03);
    }

    #[test]
    fn from_u8_roundtrips_all_variants() {
        for variant in [
            LifecycleState::Designated,
            LifecycleState::Active,
            LifecycleState::Rotating,
            LifecycleState::Revoked,
        ] {
            let byte = variant as u8;
            assert_eq!(from_u8(byte), Some(variant));
        }
    }

    #[test]
    fn from_u8_rejects_unknown_discriminant() {
        assert_eq!(from_u8(0x04), None);
        assert_eq!(from_u8(0xFF), None);
        assert_eq!(from_u8(0x80), None);
    }

    #[test]
    fn lifecycle_debug_displays_unit_variant_name() {
        assert_eq!(format!("{:?}", LifecycleState::Designated), "Designated");
        assert_eq!(format!("{:?}", LifecycleState::Active), "Active");
        assert_eq!(format!("{:?}", LifecycleState::Rotating), "Rotating");
        assert_eq!(format!("{:?}", LifecycleState::Revoked), "Revoked");
    }

    #[test]
    fn can_sign_only_for_active_and_rotating() {
        assert!(!LifecycleState::Designated.can_sign());
        assert!(LifecycleState::Active.can_sign());
        assert!(LifecycleState::Rotating.can_sign()); // within grace per RFC-0009 row 3
        assert!(!LifecycleState::Revoked.can_sign());
    }

    #[test]
    fn can_transition_to_validates_all_edges() {
        // l1 edges
        assert!(LifecycleState::Designated.can_transition_to(LifecycleState::Active));
        assert!(LifecycleState::Active.can_transition_to(LifecycleState::Revoked));
        // l2 edges (declared here for totality; actual transitions land in l2)
        assert!(LifecycleState::Active.can_transition_to(LifecycleState::Rotating));
        assert!(LifecycleState::Rotating.can_transition_to(LifecycleState::Active));
        assert!(LifecycleState::Rotating.can_transition_to(LifecycleState::Revoked));
        // invalid edges
        assert!(!LifecycleState::Designated.can_transition_to(LifecycleState::Revoked));
        assert!(!LifecycleState::Active.can_transition_to(LifecycleState::Active));
        assert!(!LifecycleState::Revoked.can_transition_to(LifecycleState::Active));
        assert!(!LifecycleState::Revoked.can_transition_to(LifecycleState::Designated));
    }

    #[test]
    fn revoked_is_terminal() {
        assert!(!LifecycleState::Revoked.can_transition_to(LifecycleState::Active));
        assert!(!LifecycleState::Revoked.can_transition_to(LifecycleState::Rotating));
        assert!(!LifecycleState::Revoked.can_transition_to(LifecycleState::Designated));
    }
}
