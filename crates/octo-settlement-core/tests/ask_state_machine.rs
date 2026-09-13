//! State-machine transition tests (RFC-0014 + RFC-0959 §State Machine
//! + RFC-0960 §2.3).
//!
//! Split per substrate-faithful test policy:
//!
//! - **RFC canonical** — `ask-state-sql-roundtrip` covers the
//!   AskState roundtrip; transition IDs are mission-defined because
//!   RFC-0014 §Test Vectors has no AskState/ReservationState
//!   transition IDs (transitions are enforced at the domain store
//!   layer, not the substrate).
//!
//! - **Mission-defined supplementary** — transition tests for
//!   AskState + ReservationState. `ReservationState::can_transition_to`
//!   is the substrate-side helper per RFC-0960 §2.3; AskState has no
//!   substrate helper so we document the documented transition
//!   contract via `SettlementError::InvalidTransition` variant
//!   construction + discriminant pinning.
//!
//! Run with:
//!   cargo test -p octo-settlement-core --test ask_state_machine
#![allow(clippy::doc_markdown)]

use octo_settlement_core::{ReservationState, SettlementError};

// === Mission-defined supplementary vectors ===
// (RFC-0014 §Test Vectors has no transition IDs for AskState or
// ReservationState; transitions are enforced at the domain store
// layer per the substrate-faithful test policy.)

// mission-defined: AskState Minted -> Consumed (skipping Settled)
// is the documented non-default path requiring policy gates. The
// substrate exposes `SettlementError::InvalidTransition` so domain
// stores can emit the error on the invalid-transition path.
#[test]
fn mission_1_invalid_transition_variant_constructible() {
    let err = SettlementError::InvalidTransition {
        from: "Minted".to_owned(),
        to: "Consumed".to_owned(),
    };
    assert!(matches!(
        err,
        SettlementError::InvalidTransition { from: _, to: _ }
    ));
    let s = err.to_string();
    assert!(s.contains("invalid state transition"));
}

// mission-defined: ReservationState Pending -> Active is a valid
// transition per RFC-0960 §2.3 (admin approves reservation).
#[test]
fn mission_2_reservation_state_pending_to_active() {
    assert!(ReservationState::Pending.can_transition_to(ReservationState::Active));
}

// mission-defined: ReservationState Active -> Redeemed is a valid
// transition per RFC-0960 §2.3 (ask settled against reservation).
#[test]
fn mission_3_reservation_state_active_to_redeemed() {
    assert!(ReservationState::Active.can_transition_to(ReservationState::Redeemed));
}

// mission-defined: ReservationState Active -> Expired is a valid
// transition per RFC-0960 §2.3 (lock expires without redemption).
#[test]
fn mission_4_reservation_state_active_to_expired() {
    assert!(ReservationState::Active.can_transition_to(ReservationState::Expired));
}

// mission-defined: ReservationState Pending -> Redeemed (skipping
// Active) is NOT a valid transition per RFC-0960 §2.3 table.
#[test]
fn mission_5_reservation_state_invalid_pending_to_redeemed() {
    assert!(!ReservationState::Pending.can_transition_to(ReservationState::Redeemed));
}
