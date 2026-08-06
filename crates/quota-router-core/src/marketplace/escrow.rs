//! Escrow — state machine for buyer → seller settlement (RFC-0900 §Escrow Flow).
//!
//! Escrow lifecycle per RFC-0900:
//!
//! ```text
//!   Pending ──lock()──▶ Locked ──settle()──▶ Settled
//!                          │
//!                          ├─dispute()──▶ Disputed ──resolve_valid()──▶ Slashed
//!                          │                 │
//!                          │                 └─resolve_invalid()──▶ Settled
//!                          │
//!                          └─cancel()──▶ Pending
//! ```
//!
//! The state machine is purely operational — it does not move funds
//! itself. Callers (the routing layer / settle orchestrator) interpret
//! the new state and run the corresponding balance / stake transitions.

use serde::{Deserialize, Serialize};

/// Escrow state per RFC-0900 §Escrow Flow.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EscrowState {
    /// Buyer initiated; funds not yet held.
    Pending,
    /// Funds locked in escrow; settlement in progress.
    Locked,
    /// Funds released to seller (success path).
    Settled,
    /// Buyer raised a dispute (RFC-0900 §Dispute Resolution).
    Disputed,
    /// Dispute resolved against seller (slashed).
    Slashed,
}

/// Escrow transition errors.
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum EscrowError {
    #[error("cannot lock escrow in state {0:?} (only Pending can lock)")]
    LockFromInvalid(EscrowState),
    #[error("cannot settle escrow in state {0:?} (only Locked can settle)")]
    SettleFromInvalid(EscrowState),
    #[error("cannot dispute escrow in state {0:?} (only Locked can be disputed)")]
    DisputeFromInvalid(EscrowState),
    #[error("cannot cancel escrow in state {0:?} (only Pending can cancel)")]
    CancelFromInvalid(EscrowState),
    #[error("cannot resolve dispute in state {0:?} (only Disputed can resolve)")]
    ResolveFromInvalid(EscrowState),
}

/// A single escrow record. `id` is unique within the marketplace.
///
/// `amount_micro_octo_w` is the locked amount. `buyer` / `seller` are
/// the participant identities (DIDs or addresses).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Escrow {
    pub id: [u8; 32],
    pub buyer: String,
    pub seller: String,
    pub amount_micro_octo_w: u128,
    pub state: EscrowState,
}

impl Escrow {
    /// Construct a new escrow in `Pending` state.
    #[must_use]
    pub fn new(
        id: [u8; 32],
        buyer: impl Into<String>,
        seller: impl Into<String>,
        amount_micro_octo_w: u128,
    ) -> Self {
        Self {
            id,
            buyer: buyer.into(),
            seller: seller.into(),
            amount_micro_octo_w,
            state: EscrowState::Pending,
        }
    }

    /// Lock the escrow (Pending → Locked).
    /// # Errors
    /// Returns `EscrowError::LockFromInvalid` if not currently Pending.
    pub fn lock(&mut self) -> Result<EscrowState, EscrowError> {
        if self.state != EscrowState::Pending {
            return Err(EscrowError::LockFromInvalid(self.state));
        }
        self.state = EscrowState::Locked;
        Ok(self.state)
    }

    /// Settle the escrow on success (Locked → Settled).
    /// # Errors
    /// Returns `EscrowError::SettleFromInvalid` if not currently Locked.
    pub fn settle(&mut self) -> Result<EscrowState, EscrowError> {
        if self.state != EscrowState::Locked {
            return Err(EscrowError::SettleFromInvalid(self.state));
        }
        self.state = EscrowState::Settled;
        Ok(self.state)
    }

    /// Buyer raises a dispute (Locked → Disputed).
    /// # Errors
    /// Returns `EscrowError::DisputeFromInvalid` if not currently Locked.
    pub fn dispute(&mut self) -> Result<EscrowState, EscrowError> {
        if self.state != EscrowState::Locked {
            return Err(EscrowError::DisputeFromInvalid(self.state));
        }
        self.state = EscrowState::Disputed;
        Ok(self.state)
    }

    /// Cancel a Pending escrow (buyer abandons the purchase).
    /// # Errors
    /// Returns `EscrowError::CancelFromInvalid` if not currently Pending.
    pub fn cancel(&mut self) -> Result<EscrowState, EscrowError> {
        if self.state != EscrowState::Pending {
            return Err(EscrowError::CancelFromInvalid(self.state));
        }
        self.state = EscrowState::Pending; // no-op terminal marker
        Ok(self.state)
    }

    /// Dispute outcome: valid → slash the seller (Disputed → Slashed).
    /// # Errors
    /// Returns `EscrowError::ResolveFromInvalid` if not currently Disputed.
    pub fn resolve_valid(&mut self) -> Result<EscrowState, EscrowError> {
        if self.state != EscrowState::Disputed {
            return Err(EscrowError::ResolveFromInvalid(self.state));
        }
        self.state = EscrowState::Slashed;
        Ok(self.state)
    }

    /// Dispute outcome: invalid → keep payment (Disputed → Settled).
    /// # Errors
    /// Returns `EscrowError::ResolveFromInvalid` if not currently Disputed.
    pub fn resolve_invalid(&mut self) -> Result<EscrowState, EscrowError> {
        if self.state != EscrowState::Disputed {
            return Err(EscrowError::ResolveFromInvalid(self.state));
        }
        self.state = EscrowState::Settled;
        Ok(self.state)
    }

    /// True if escrow reached a terminal state (Settled or Slashed).
    #[must_use]
    pub fn is_terminal(&self) -> bool {
        matches!(self.state, EscrowState::Settled | EscrowState::Slashed)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> Escrow {
        Escrow::new(
            [0xaa; 32],
            octo_ident::test_helpers::sample_did(130),
            octo_ident::test_helpers::sample_did(95),
            100_000,
        )
    }

    #[test]
    fn new_starts_pending() {
        let e = sample();
        assert_eq!(e.state, EscrowState::Pending);
        assert!(!e.is_terminal());
    }

    #[test]
    fn lock_transitions_pending_to_locked() {
        let mut e = sample();
        e.lock().unwrap();
        assert_eq!(e.state, EscrowState::Locked);
    }

    #[test]
    fn settle_transitions_locked_to_settled() {
        let mut e = sample();
        e.lock().unwrap();
        e.settle().unwrap();
        assert_eq!(e.state, EscrowState::Settled);
        assert!(e.is_terminal());
    }

    #[test]
    fn dispute_transitions_locked_to_disputed() {
        let mut e = sample();
        e.lock().unwrap();
        e.dispute().unwrap();
        assert_eq!(e.state, EscrowState::Disputed);
    }

    #[test]
    fn resolve_valid_transitions_disputed_to_slashed() {
        let mut e = sample();
        e.lock().unwrap();
        e.dispute().unwrap();
        e.resolve_valid().unwrap();
        assert_eq!(e.state, EscrowState::Slashed);
        assert!(e.is_terminal());
    }

    #[test]
    fn resolve_invalid_transitions_disputed_to_settled() {
        let mut e = sample();
        e.lock().unwrap();
        e.dispute().unwrap();
        e.resolve_invalid().unwrap();
        assert_eq!(e.state, EscrowState::Settled);
    }

    #[test]
    fn lock_rejects_non_pending() {
        let mut e = sample();
        e.lock().unwrap();
        assert_eq!(
            e.lock().unwrap_err(),
            EscrowError::LockFromInvalid(EscrowState::Locked)
        );
    }

    #[test]
    fn settle_rejects_non_locked() {
        let mut e = sample();
        assert_eq!(
            e.settle().unwrap_err(),
            EscrowError::SettleFromInvalid(EscrowState::Pending)
        );
    }

    #[test]
    fn dispute_rejects_non_locked() {
        let mut e = sample();
        assert_eq!(
            e.dispute().unwrap_err(),
            EscrowError::DisputeFromInvalid(EscrowState::Pending)
        );
    }

    #[test]
    fn cancel_only_from_pending() {
        let mut e = sample();
        e.cancel().unwrap(); // idempotent no-op
        e.lock().unwrap();
        assert_eq!(
            e.cancel().unwrap_err(),
            EscrowError::CancelFromInvalid(EscrowState::Locked)
        );
    }

    #[test]
    fn resolve_rejects_non_disputed() {
        let mut e = sample();
        assert_eq!(
            e.resolve_valid().unwrap_err(),
            EscrowError::ResolveFromInvalid(EscrowState::Pending)
        );
        assert_eq!(
            e.resolve_invalid().unwrap_err(),
            EscrowError::ResolveFromInvalid(EscrowState::Pending)
        );
    }
}
