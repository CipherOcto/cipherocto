//! Settlement state machine: Mint → Settled → Consumed.
//!
//! Pure validation functions that wrap `SettlementStore` calls with
//! state-transition guards.

use crate::{AskState, SettlementError, SettlementStore};

/// State transition errors (distinct from `SettlementError` for tests).
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum StateTransitionError {
    #[error("invalid transition: {from:?} → {to:?}")]
    Invalid { from: AskState, to: AskState },
}

/// Validate a transition.
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
}
