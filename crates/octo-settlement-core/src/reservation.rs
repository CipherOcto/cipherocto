//! Canonical `Reservation` struct + `ReservationState` enum per RFC-0014
//! §Module Layout `reservation` + RFC-0960 §2.3.

use serde::{Deserialize, Serialize};

/// Canonical reservation struct. A reservation locks quota for a future
/// ask; the ask must be settled (or the reservation must be released)
/// before the lock expires.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Reservation {
    /// Reservation ID (32-byte BLAKE3-256 digest).
    pub reservation_id: [u8; 32],
    /// Reserver node DID (raw canonical wire form).
    pub reserver: String,
    /// Reserved amount (basis points).
    pub amount_bps: u64,
    /// Current state in the reservation lifecycle state machine.
    pub state: ReservationState,
    /// Wall-clock timestamp in seconds since UNIX epoch (reservation
    /// minted).
    pub minted_at_unix: u64,
    /// Wall-clock timestamp in seconds since UNIX epoch (reservation
    /// expires; lock is auto-released if not consumed by this time).
    pub expires_at_unix: u64,
}

/// Canonical reservation state machine. `#[non_exhaustive]` per CLAUDE.md
/// §Extension over enumeration.
///
/// # State transitions
///
/// Per RFC-0960 §2.3:
/// - `Pending → Active` (admin approves reservation)
/// - `Active → Redeemed` (ask is settled against reservation)
/// - `Active → Released` (reserver voluntarily releases the lock)
/// - `Active → Expired` (lock expires without redemption)
/// - `Pending → Cancelled` (admin or reserver cancels before activation)
/// - `Pending → Denied` (admin denies reservation)
/// - `Pending → Expired` (pending lock expires without activation)
/// - `Settling → Redeemed | Failed` (settlement attempt completes)
///
/// # Discriminants
///
/// `#[repr(u8)]` for byte-identical SQL persistence per RFC-0960 §2.3:
/// Pending=0, Active=1, Redeemed=2, Released=3, Expired=4, Cancelled=5,
/// Denied=6, Settling=7.
#[non_exhaustive]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[repr(u8)]
pub enum ReservationState {
    /// Reservation created, not yet active.
    Pending = 0,
    /// Reservation active, lock in effect.
    Active = 1,
    /// Reservation consumed by ask settlement.
    Redeemed = 2,
    /// Reserver voluntarily released the lock.
    Released = 3,
    /// Lock expired without redemption.
    Expired = 4,
    /// Admin or reserver cancelled before activation.
    Cancelled = 5,
    /// Admin denied the reservation.
    Denied = 6,
    /// Settlement attempt in flight (transient state).
    Settling = 7,
}

impl ReservationState {
    /// Whether `self` is a terminal state (no further transitions
    /// possible). `Redeemed`, `Released`, `Expired`, `Cancelled`,
    /// `Denied` are terminal. `Pending`, `Active`, `Settling` are
    /// non-terminal.
    pub fn is_terminal(self) -> bool {
        matches!(
            self,
            ReservationState::Redeemed
                | ReservationState::Released
                | ReservationState::Expired
                | ReservationState::Cancelled
                | ReservationState::Denied
        )
    }

    /// Validate a state transition per RFC-0960 §2.3 table.
    pub fn can_transition_to(self, to: ReservationState) -> bool {
        use ReservationState::*;
        matches!(
            (self, to),
            (Pending, Active)
                | (Pending, Cancelled)
                | (Pending, Denied)
                | (Pending, Expired)
                | (Active, Redeemed)
                | (Active, Released)
                | (Active, Expired)
                | (Settling, Redeemed)
        )
    }
}
