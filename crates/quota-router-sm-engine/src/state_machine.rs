//! Settlement state machines: AskState (Minted → Settled → Consumed) +
//! ReservationState (audit-window state machine per RFC-0960 §4).
//!
//! Pure validation functions that wrap `SettlementStore` calls with
//! state-transition guards.

use crate::{AskState, ReservationState, SettlementError, SettlementStore};

/// State transition errors (distinct from `SettlementError` for tests).
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum StateTransitionError {
    #[error("invalid transition: {from:?} → {to:?}")]
    Invalid { from: AskState, to: AskState },
}

/// Reservation audit-window state machine transition errors (RFC-0960 §4).
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum ReservationTransitionError {
    #[error("invalid reservation transition: {from:?} → {to:?}")]
    Invalid {
        from: ReservationState,
        to: ReservationState,
    },
}

/// Validate an AskState transition.
pub fn transition(from: AskState, to: AskState) -> Result<(), StateTransitionError> {
    let valid = matches!(
        (from, to),
        (AskState::Minted, AskState::Settled) | (AskState::Settled, AskState::Consumed)
    );
    if valid {
        Ok(())
    } else {
        Err(StateTransitionError::Invalid { from, to })
    }
}

/// Validate a ReservationState transition (RFC-0960 §4 audit-window state machine).
///
/// Valid transitions:
/// - `Reserved` → `Executing` (provider starts)
/// - `Reserved` → `Cancelled` (holder cancels)
/// - `Reserved` → `Expired` (deadline passes without settlement)
/// - `Executing` → `Settled` (receipt attached; settlement_ref set)
/// - `Settled` → `Auditable` (audit window opens)
/// - `Settled` → `Frozen` (dispute filed during settlement)
/// - `Auditable` → `Released` (audit window closed; transfers applied)
/// - `Auditable` → `Frozen` (dispute filed during audit window)
/// - `Frozen` → `Released` (dispute upheld)
/// - `Frozen` → `Settled` (dispute rolled back; restart audit)
pub fn transition_reservation(
    from: ReservationState,
    to: ReservationState,
) -> Result<(), ReservationTransitionError> {
    let valid = matches!(
        (from, to),
        (ReservationState::Reserved, ReservationState::Executing)
            | (ReservationState::Reserved, ReservationState::Cancelled)
            | (ReservationState::Reserved, ReservationState::Expired)
            | (ReservationState::Executing, ReservationState::Settled)
            | (ReservationState::Settled, ReservationState::Auditable)
            | (ReservationState::Settled, ReservationState::Frozen)
            | (ReservationState::Auditable, ReservationState::Released)
            | (ReservationState::Auditable, ReservationState::Frozen)
            | (ReservationState::Frozen, ReservationState::Released)
            | (ReservationState::Frozen, ReservationState::Settled)
    );
    if valid {
        Ok(())
    } else {
        Err(ReservationTransitionError::Invalid { from, to })
    }
}

/// High-level: settle an ask (Mint → Settled transition).
pub fn settle_ask(
    store: &impl SettlementStore,
    ask_id: &[u8; 32],
    receipt: &crate::Receipt,
) -> Result<[u8; 32], SettlementError> {
    let (_, state) = store.get(ask_id)?;
    transition(state, AskState::Settled).map_err(|_| SettlementError::InvalidTransition {
        from: state,
        to: AskState::Settled,
    })?;
    store.settle(ask_id, receipt)
}

/// High-level: consume a receipt (Settled → Consumed transition).
pub fn consume_receipt(
    store: &impl SettlementStore,
    receipt_id: &[u8; 32],
) -> Result<(), SettlementError> {
    // store.consume() performs the INSERT OR IGNORE + state update atomically.
    // We just delegate + map error.
    store.consume(receipt_id)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn minted_to_settled_valid() {
        assert!(transition(AskState::Minted, AskState::Settled).is_ok());
    }

    #[test]
    fn settled_to_consumed_valid() {
        assert!(transition(AskState::Settled, AskState::Consumed).is_ok());
    }

    #[test]
    fn minted_to_consumed_invalid() {
        assert!(transition(AskState::Minted, AskState::Consumed).is_err());
    }

    #[test]
    fn settled_to_minted_invalid() {
        assert!(transition(AskState::Settled, AskState::Minted).is_err());
    }

    #[test]
    fn consumed_is_terminal() {
        for to in [AskState::Minted, AskState::Settled, AskState::Consumed] {
            assert!(transition(AskState::Consumed, to).is_err());
        }
    }

    // Reservation audit-window state machine (RFC-0960 §4).

    #[test]
    fn reservation_reserved_to_executing_valid() {
        assert!(transition_reservation(ReservationState::Reserved, ReservationState::Executing).is_ok());
    }

    #[test]
    fn reservation_reserved_to_cancelled_valid() {
        assert!(transition_reservation(ReservationState::Reserved, ReservationState::Cancelled).is_ok());
    }

    #[test]
    fn reservation_reserved_to_expired_valid() {
        assert!(transition_reservation(ReservationState::Reserved, ReservationState::Expired).is_ok());
    }

    #[test]
    fn reservation_executing_to_settled_valid() {
        assert!(transition_reservation(ReservationState::Executing, ReservationState::Settled).is_ok());
    }

    #[test]
    fn reservation_settled_to_auditable_valid() {
        assert!(transition_reservation(ReservationState::Settled, ReservationState::Auditable).is_ok());
    }

    #[test]
    fn reservation_settled_to_frozen_valid() {
        assert!(transition_reservation(ReservationState::Settled, ReservationState::Frozen).is_ok());
    }

    #[test]
    fn reservation_auditable_to_released_valid() {
        assert!(transition_reservation(ReservationState::Auditable, ReservationState::Released).is_ok());
    }

    #[test]
    fn reservation_auditable_to_frozen_valid() {
        assert!(transition_reservation(ReservationState::Auditable, ReservationState::Frozen).is_ok());
    }

    #[test]
    fn reservation_frozen_to_released_valid() {
        assert!(transition_reservation(ReservationState::Frozen, ReservationState::Released).is_ok());
    }

    #[test]
    fn reservation_frozen_to_settled_valid() {
        assert!(transition_reservation(ReservationState::Frozen, ReservationState::Settled).is_ok());
    }

    #[test]
    fn reservation_released_is_terminal() {
        for to in [
            ReservationState::Reserved,
            ReservationState::Executing,
            ReservationState::Settled,
            ReservationState::Auditable,
            ReservationState::Frozen,
            ReservationState::Released,
            ReservationState::Expired,
            ReservationState::Cancelled,
        ] {
            assert!(transition_reservation(ReservationState::Released, to).is_err());
        }
    }

    #[test]
    fn reservation_cancelled_is_terminal() {
        for to in [
            ReservationState::Reserved,
            ReservationState::Released,
            ReservationState::Settled,
        ] {
            assert!(transition_reservation(ReservationState::Cancelled, to).is_err());
        }
    }

    #[test]
    fn reservation_expired_is_terminal() {
        for to in [
            ReservationState::Reserved,
            ReservationState::Released,
            ReservationState::Settled,
        ] {
            assert!(transition_reservation(ReservationState::Expired, to).is_err());
        }
    }

    #[test]
    fn reservation_no_backward_to_reserved() {
        // No transition may go back to Reserved from any non-Reserved state.
        for from in [
            ReservationState::Executing,
            ReservationState::Settled,
            ReservationState::Auditable,
            ReservationState::Released,
            ReservationState::Frozen,
            ReservationState::Expired,
            ReservationState::Cancelled,
        ] {
            assert!(transition_reservation(from, ReservationState::Reserved).is_err());
        }
    }

    #[test]
    fn reservation_no_skip_steps() {
        // Cannot skip from Reserved directly to Settled.
        assert!(transition_reservation(ReservationState::Reserved, ReservationState::Settled).is_err());
        // Cannot skip from Reserved to Auditable.
        assert!(transition_reservation(ReservationState::Reserved, ReservationState::Auditable).is_err());
        // Cannot skip from Executing to Released.
        assert!(transition_reservation(ReservationState::Executing, ReservationState::Released).is_err());
    }
}
