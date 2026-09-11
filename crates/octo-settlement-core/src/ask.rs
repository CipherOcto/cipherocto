//! Canonical `Ask` struct + `AskState` enum per RFC-0014 §Module Layout
//! `ask` + RFC-0959 §State Machine.

use serde::{Deserialize, Serialize};

/// Canonical ask struct.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Ask {
    /// Ask ID (32-byte BLAKE3-256 digest).
    pub ask_id: [u8; 32],
    /// Issuer node DID (raw canonical wire form).
    pub issuer: String,
    /// Counterparty node DID (raw canonical wire form; counterparty is
    /// the party that may consume the ask).
    pub counterparty: String,
    /// Settlement amount (basis points of the unit; e.g. 1_000_000 = 1
    /// full unit).
    pub amount_bps: u64,
    /// Current state in the ask lifecycle state machine.
    pub state: AskState,
    /// Wall-clock timestamp in seconds since UNIX epoch (ask minted).
    pub minted_at_unix: u64,
}

/// Canonical ask state machine. `#[non_exhaustive]` per CLAUDE.md
/// §Extension over enumeration.
///
/// # State transitions
///
/// - `Minted → Settled` (router signs settlement receipt)
/// - `Settled → Consumed` (counterparty claims the settled value)
/// - `Minted → Consumed` (counterparty burns ask without router
///   settlement — non-default path; subject to policy gates)
///
/// # Discriminants
///
/// `#[repr(u8)]` for byte-identical SQL persistence per RFC-0959
/// §State Machine: Minted=0, Settled=1, Consumed=2.
#[non_exhaustive]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[repr(u8)]
pub enum AskState {
    /// Ask created, not yet settled by router.
    Minted = 0,
    /// Router has signed a settlement receipt referencing this ask.
    Settled = 1,
    /// Counterparty has claimed the settled value (terminal state).
    Consumed = 2,
}

impl AskState {
    /// SQL literal form for the state column. Stored as INTEGER per
    /// RFC-0959 §State Machine.
    pub fn as_sql(self) -> i64 {
        self as i64
    }

    /// Inverse of [`as_sql`]. Returns `None` on unknown discriminant.
    pub fn from_sql(value: i64) -> Option<Self> {
        match value {
            0 => Some(AskState::Minted),
            1 => Some(AskState::Settled),
            2 => Some(AskState::Consumed),
            _ => None,
        }
    }
}
