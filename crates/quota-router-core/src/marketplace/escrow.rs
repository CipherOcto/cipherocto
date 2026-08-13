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
//! ```
//!
//! The state machine is purely operational — it does not move funds
//! itself. Callers (the routing layer / settle orchestrator) interpret
//! the new state and run the corresponding balance / stake transitions.
//!
//! **No `cancel()` method** (Round 1 review fix): prior `cancel()`
//! was a no-op (`Pending → Pending`) which made buyer abandonment
//! indistinguishable from a never-cancelled escrow, and the
//! documented `Locked → cancel() → Pending` transition was
//! economically nonsensical (funds already locked; cancel should
//! refund via the dispute path, not silently rollback). Buyers who
//! wish to abandon a Pending escrow should simply not call `lock()`
//! — `Escrow::drop` will leave it in `Pending` state and no funds
//! are at risk.

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

/// Party identity for escrow state-machine authorization
/// (mission `marketplace-escrow-caller-authorization`).
///
/// Each transition takes a `&Party` and verifies that the claimed
/// identity matches the escrow's stored counterparty. This makes the
/// state machine fail-closed: a non-buyer holding `&mut Escrow`
/// cannot drive `Locked → Disputed → Slashed` without authority.
///
/// The string inside the variant is the participant's identity
/// (typically a DID per RFC-0010). Comparison is exact, case-sensitive.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Party {
    /// Buyer-side caller (transitions: lock, dispute).
    Buyer(String),
    /// Seller-side caller (transition: settle).
    Seller(String),
    /// Arbitrator caller (transitions: resolve_valid, resolve_invalid).
    /// The escrow's `arbitrator` field must be set at construction for
    /// arbitrator transitions to succeed.
    Arbitrator(String),
}

impl Party {
    /// String identity of the caller.
    #[must_use]
    pub fn identity(&self) -> &str {
        match self {
            Self::Buyer(s) | Self::Seller(s) | Self::Arbitrator(s) => s,
        }
    }

    /// String tag for error reporting ("buyer" / "seller" / "arbitrator").
    #[must_use]
    pub fn role(&self) -> &'static str {
        match self {
            Self::Buyer(_) => "buyer",
            Self::Seller(_) => "seller",
            Self::Arbitrator(_) => "arbitrator",
        }
    }
}

/// Required role for an authorization check (the role the escrow
/// expects; compared against the caller's `Party::role()`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum RequiredRole {
    Buyer,
    Seller,
    Arbitrator,
}

impl RequiredRole {
    #[must_use]
    fn as_str(self) -> &'static str {
        match self {
            Self::Buyer => "buyer",
            Self::Seller => "seller",
            Self::Arbitrator => "arbitrator",
        }
    }
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
    #[error("cannot resolve dispute in state {0:?} (only Disputed can resolve)")]
    ResolveFromInvalid(EscrowState),
    /// Caller identity does not match the role required by the transition.
    /// Returned when a `Party::Buyer` passes an identity that does not
    /// equal the escrow's `buyer` field, or analogous mismatches for
    /// `Seller` / `Arbitrator`. Always fail-closed: unknown / unset
    /// arbitrators reject all `resolve_*` calls.
    #[error("unauthorized escrow caller: required role={required}, caller_role={caller_role}, caller_identity={caller_identity}")]
    UnauthorizedCaller {
        required: &'static str,
        caller_role: &'static str,
        caller_identity: String,
    },
}

/// A single escrow record. `id` is unique within the marketplace.
///
/// `amount_micro_octo_w` is the locked amount. `buyer` / `seller` /
/// `arbitrator` are the participant identities (DIDs or addresses).
/// `arbitrator` may be empty if the marketplace is not yet wired to
/// a dispute arbiter; in that case `resolve_*` calls always fail with
/// `UnauthorizedCaller` (fail-closed).
///
/// **Not `Clone`** (Round 1 review fix): prior `#[derive(Clone)]`
/// enabled a double-settle vector where two independently-cloned
/// Escrows could both transition Pending → Locked → Settled without
/// coordinating, allowing duplicate fund release. Callers that need
/// a snapshot should use `EscrowSnapshot` (state + immutable fields)
/// or `&Escrow` / `&mut Escrow` references.
#[derive(Debug, PartialEq, Eq)]
pub struct Escrow {
    pub id: [u8; 32],
    pub buyer: String,
    pub seller: String,
    /// Arbitrator identity for `resolve_valid` / `resolve_invalid`.
    /// Empty when the marketplace has no arbiter wired; in that
    /// state, all `resolve_*` calls fail with `UnauthorizedCaller`.
    pub arbitrator: String,
    pub amount_micro_octo_w: u128,
    pub state: EscrowState,
}

/// Immutable snapshot of an Escrow — safe to clone, cannot mutate.
/// Use this when callers need to capture the escrow state at a point
/// in time without holding the original (e.g., logging, audit trail).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EscrowSnapshot {
    pub id: [u8; 32],
    pub buyer: String,
    pub seller: String,
    pub arbitrator: String,
    pub amount_micro_octo_w: u128,
    pub state: EscrowState,
}

impl From<&Escrow> for EscrowSnapshot {
    fn from(e: &Escrow) -> Self {
        Self {
            id: e.id,
            buyer: e.buyer.clone(),
            seller: e.seller.clone(),
            arbitrator: e.arbitrator.clone(),
            amount_micro_octo_w: e.amount_micro_octo_w,
            state: e.state,
        }
    }
}

impl Escrow {
    /// Construct a new escrow in `Pending` state with no arbitrator.
    /// Use [`with_arbitrator`](Self::with_arbitrator) to set one.
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
            arbitrator: String::new(),
            amount_micro_octo_w,
            state: EscrowState::Pending,
        }
    }

    /// Construct a new escrow with an explicit arbitrator identity.
    /// Required for `resolve_valid` / `resolve_invalid` to succeed.
    #[must_use]
    pub fn with_arbitrator(
        id: [u8; 32],
        buyer: impl Into<String>,
        seller: impl Into<String>,
        arbitrator: impl Into<String>,
        amount_micro_octo_w: u128,
    ) -> Self {
        Self {
            id,
            buyer: buyer.into(),
            seller: seller.into(),
            arbitrator: arbitrator.into(),
            amount_micro_octo_w,
            state: EscrowState::Pending,
        }
    }

    /// Internal authorization check. Returns `UnauthorizedCaller` if
    /// the caller's role does not match `required`, or if the caller's
    /// identity does not equal the escrow's stored counterparty.
    fn check_caller(&self, caller: &Party, required: RequiredRole) -> Result<(), EscrowError> {
        // Caller must claim the right role.
        let caller_role_matches = matches!(
            (required, caller),
            (RequiredRole::Buyer, Party::Buyer(_))
                | (RequiredRole::Seller, Party::Seller(_))
                | (RequiredRole::Arbitrator, Party::Arbitrator(_))
        );
        if !caller_role_matches {
            return Err(EscrowError::UnauthorizedCaller {
                required: required.as_str(),
                caller_role: caller.role(),
                caller_identity: caller.identity().to_string(),
            });
        }
        // Caller identity must equal the stored counterparty.
        let stored = match required {
            RequiredRole::Buyer => &self.buyer,
            RequiredRole::Seller => &self.seller,
            RequiredRole::Arbitrator => &self.arbitrator,
        };
        if stored.is_empty() || caller.identity() != stored {
            return Err(EscrowError::UnauthorizedCaller {
                required: required.as_str(),
                caller_role: caller.role(),
                caller_identity: caller.identity().to_string(),
            });
        }
        Ok(())
    }

    /// Lock the escrow (Pending → Locked).
    /// # Errors
    /// Returns `EscrowError::LockFromInvalid` if not currently Pending,
    /// or `EscrowError::UnauthorizedCaller` if `caller` is not the
    /// buyer (or does not match `self.buyer`).
    pub fn lock(&mut self, caller: &Party) -> Result<EscrowState, EscrowError> {
        self.check_caller(caller, RequiredRole::Buyer)?;
        if self.state != EscrowState::Pending {
            return Err(EscrowError::LockFromInvalid(self.state));
        }
        self.state = EscrowState::Locked;
        Ok(self.state)
    }

    /// Settle the escrow on success (Locked → Settled).
    /// # Errors
    /// Returns `EscrowError::SettleFromInvalid` if not currently Locked,
    /// or `EscrowError::UnauthorizedCaller` if `caller` is not the
    /// seller (or does not match `self.seller`).
    pub fn settle(&mut self, caller: &Party) -> Result<EscrowState, EscrowError> {
        self.check_caller(caller, RequiredRole::Seller)?;
        if self.state != EscrowState::Locked {
            return Err(EscrowError::SettleFromInvalid(self.state));
        }
        self.state = EscrowState::Settled;
        Ok(self.state)
    }

    /// Buyer raises a dispute (Locked → Disputed).
    /// # Errors
    /// Returns `EscrowError::DisputeFromInvalid` if not currently Locked,
    /// or `EscrowError::UnauthorizedCaller` if `caller` is not the
    /// buyer (or does not match `self.buyer`).
    pub fn dispute(&mut self, caller: &Party) -> Result<EscrowState, EscrowError> {
        self.check_caller(caller, RequiredRole::Buyer)?;
        if self.state != EscrowState::Locked {
            return Err(EscrowError::DisputeFromInvalid(self.state));
        }
        self.state = EscrowState::Disputed;
        Ok(self.state)
    }

    /// Dispute outcome: valid → slash the seller (Disputed → Slashed).
    /// # Errors
    /// Returns `EscrowError::ResolveFromInvalid` if not currently
    /// Disputed, or `EscrowError::UnauthorizedCaller` if `caller` is
    /// not the arbitrator (or no arbitrator is set).
    pub fn resolve_valid(&mut self, caller: &Party) -> Result<EscrowState, EscrowError> {
        self.check_caller(caller, RequiredRole::Arbitrator)?;
        if self.state != EscrowState::Disputed {
            return Err(EscrowError::ResolveFromInvalid(self.state));
        }
        self.state = EscrowState::Slashed;
        Ok(self.state)
    }

    /// Dispute outcome: invalid → keep payment (Disputed → Settled).
    /// # Errors
    /// Returns `EscrowError::ResolveFromInvalid` if not currently
    /// Disputed, or `EscrowError::UnauthorizedCaller` if `caller` is
    /// not the arbitrator (or no arbitrator is set).
    pub fn resolve_invalid(&mut self, caller: &Party) -> Result<EscrowState, EscrowError> {
        self.check_caller(caller, RequiredRole::Arbitrator)?;
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
        Escrow::with_arbitrator(
            [0xaa; 32],
            octo_ident::test_helpers::sample_did(130),
            octo_ident::test_helpers::sample_did(95),
            octo_ident::test_helpers::sample_did(50),
            100_000,
        )
    }

    fn buyer() -> Party {
        Party::Buyer(octo_ident::test_helpers::sample_did(130))
    }
    fn seller() -> Party {
        Party::Seller(octo_ident::test_helpers::sample_did(95))
    }
    fn arbiter() -> Party {
        Party::Arbitrator(octo_ident::test_helpers::sample_did(50))
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
        e.lock(&buyer()).unwrap();
        assert_eq!(e.state, EscrowState::Locked);
    }

    #[test]
    fn settle_transitions_locked_to_settled() {
        let mut e = sample();
        e.lock(&buyer()).unwrap();
        e.settle(&seller()).unwrap();
        assert_eq!(e.state, EscrowState::Settled);
        assert!(e.is_terminal());
    }

    #[test]
    fn dispute_transitions_locked_to_disputed() {
        let mut e = sample();
        e.lock(&buyer()).unwrap();
        e.dispute(&buyer()).unwrap();
        assert_eq!(e.state, EscrowState::Disputed);
    }

    #[test]
    fn resolve_valid_transitions_disputed_to_slashed() {
        let mut e = sample();
        e.lock(&buyer()).unwrap();
        e.dispute(&buyer()).unwrap();
        e.resolve_valid(&arbiter()).unwrap();
        assert_eq!(e.state, EscrowState::Slashed);
        assert!(e.is_terminal());
    }

    #[test]
    fn resolve_invalid_transitions_disputed_to_settled() {
        let mut e = sample();
        e.lock(&buyer()).unwrap();
        e.dispute(&buyer()).unwrap();
        e.resolve_invalid(&arbiter()).unwrap();
        assert_eq!(e.state, EscrowState::Settled);
    }

    #[test]
    fn lock_rejects_non_pending() {
        let mut e = sample();
        e.lock(&buyer()).unwrap();
        assert_eq!(
            e.lock(&buyer()).unwrap_err(),
            EscrowError::LockFromInvalid(EscrowState::Locked)
        );
    }

    #[test]
    fn settle_rejects_non_locked() {
        let mut e = sample();
        assert_eq!(
            e.settle(&seller()).unwrap_err(),
            EscrowError::SettleFromInvalid(EscrowState::Pending)
        );
    }

    #[test]
    fn dispute_rejects_non_locked() {
        let mut e = sample();
        assert_eq!(
            e.dispute(&buyer()).unwrap_err(),
            EscrowError::DisputeFromInvalid(EscrowState::Pending)
        );
    }

    #[test]
    fn resolve_rejects_non_disputed() {
        let mut e = sample();
        assert_eq!(
            e.resolve_valid(&arbiter()).unwrap_err(),
            EscrowError::ResolveFromInvalid(EscrowState::Pending)
        );
        assert_eq!(
            e.resolve_invalid(&arbiter()).unwrap_err(),
            EscrowError::ResolveFromInvalid(EscrowState::Pending)
        );
    }

    // ========================================================================
    // Authorization tests (mission: marketplace-escrow-caller-authorization)
    //
    // Each transition must reject callers that do not match the
    // required role. The state-machine check is independent of the
    // authorization check; an UnauthorizedCaller error must win over
    // a FromInvalid error when both apply (test below: caller wrong
    // AND state invalid).
    // ========================================================================

    #[test]
    fn lock_rejects_seller_caller() {
        let mut e = sample();
        assert!(matches!(
            e.lock(&seller()).unwrap_err(),
            EscrowError::UnauthorizedCaller {
                required: "buyer",
                ..
            }
        ));
    }

    #[test]
    fn lock_rejects_arbitrator_caller() {
        let mut e = sample();
        assert!(matches!(
            e.lock(&arbiter()).unwrap_err(),
            EscrowError::UnauthorizedCaller {
                required: "buyer",
                ..
            }
        ));
    }

    #[test]
    fn lock_rejects_wrong_buyer_identity() {
        let mut e = sample();
        let wrong = Party::Buyer(octo_ident::test_helpers::sample_did(99));
        assert!(matches!(
            e.lock(&wrong).unwrap_err(),
            EscrowError::UnauthorizedCaller {
                required: "buyer",
                caller_identity: _,
                ..
            }
        ));
    }

    #[test]
    fn settle_rejects_buyer_caller() {
        let mut e = sample();
        e.lock(&buyer()).unwrap();
        assert!(matches!(
            e.settle(&buyer()).unwrap_err(),
            EscrowError::UnauthorizedCaller {
                required: "seller",
                ..
            }
        ));
    }

    #[test]
    fn settle_rejects_wrong_seller_identity() {
        let mut e = sample();
        e.lock(&buyer()).unwrap();
        let wrong = Party::Seller(octo_ident::test_helpers::sample_did(99));
        assert!(matches!(
            e.settle(&wrong).unwrap_err(),
            EscrowError::UnauthorizedCaller {
                required: "seller",
                ..
            }
        ));
    }

    #[test]
    fn dispute_rejects_seller_caller() {
        let mut e = sample();
        e.lock(&buyer()).unwrap();
        assert!(matches!(
            e.dispute(&seller()).unwrap_err(),
            EscrowError::UnauthorizedCaller {
                required: "buyer",
                ..
            }
        ));
    }

    #[test]
    fn resolve_valid_rejects_buyer_caller() {
        let mut e = sample();
        e.lock(&buyer()).unwrap();
        e.dispute(&buyer()).unwrap();
        assert!(matches!(
            e.resolve_valid(&buyer()).unwrap_err(),
            EscrowError::UnauthorizedCaller {
                required: "arbitrator",
                ..
            }
        ));
    }

    #[test]
    fn resolve_invalid_rejects_seller_caller() {
        let mut e = sample();
        e.lock(&buyer()).unwrap();
        e.dispute(&buyer()).unwrap();
        assert!(matches!(
            e.resolve_invalid(&seller()).unwrap_err(),
            EscrowError::UnauthorizedCaller {
                required: "arbitrator",
                ..
            }
        ));
    }

    #[test]
    fn resolve_rejects_when_no_arbitrator_set() {
        // No arbitrator configured → all resolve_* calls must fail-closed.
        let mut e = Escrow::new(
            [0xaa; 32],
            octo_ident::test_helpers::sample_did(130),
            octo_ident::test_helpers::sample_did(95),
            100_000,
        );
        e.lock(&buyer()).unwrap();
        e.dispute(&buyer()).unwrap();
        // Caller claims arbitrator role with the right identity, but
        // the escrow has no arbitrator wired → must reject.
        let arb = Party::Arbitrator(octo_ident::test_helpers::sample_did(50));
        assert!(matches!(
            e.resolve_valid(&arb).unwrap_err(),
            EscrowError::UnauthorizedCaller {
                required: "arbitrator",
                ..
            }
        ));
        assert!(matches!(
            e.resolve_invalid(&arb).unwrap_err(),
            EscrowError::UnauthorizedCaller {
                required: "arbitrator",
                ..
            }
        ));
    }

    #[test]
    fn unauthorized_wins_over_state_invalid() {
        // Caller is wrong AND state is invalid → must return
        // UnauthorizedCaller (not FromInvalid). Fail-closed on identity
        // before fail-closed on state.
        let mut e = sample();
        // State is Pending; lock() with seller must reject on auth.
        assert!(matches!(
            e.lock(&seller()).unwrap_err(),
            EscrowError::UnauthorizedCaller {
                required: "buyer",
                ..
            }
        ));
    }

    #[test]
    fn unauthorized_caller_error_carries_required_role() {
        let mut e = sample();
        let err = e.lock(&seller()).unwrap_err();
        match err {
            EscrowError::UnauthorizedCaller {
                required,
                caller_role,
                caller_identity,
            } => {
                assert_eq!(required, "buyer");
                assert_eq!(caller_role, "seller");
                assert_eq!(caller_identity, octo_ident::test_helpers::sample_did(95));
            }
            other => panic!("expected UnauthorizedCaller, got {other:?}"),
        }
    }
}
