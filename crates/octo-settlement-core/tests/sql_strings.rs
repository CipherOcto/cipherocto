//! SQL string byte-stability tests (RFC-0014 §Test Vectors
//! `ask-state-sql-*` + mission 0014-settlement-verify-chain-tests).
//!
//! 3 vectors exercising the substrate `AskState::as_sql` +
//! `AskState::from_sql` round-trip invariants. SQL strings / integer
//! discriminants MUST be byte-stable per RFC-0959 §State Machine —
//! any drift would require an SQL migration to redefine the column
//! type and update every persisted row.
//!
//! Run with:
//!   cargo test -p octo-settlement-core --test sql_strings
#![allow(clippy::doc_markdown)]

use octo_settlement_core::AskState;

// RFC-0014 §Test Vectors `ask-state-sql-roundtrip`: every canonical
// AskState variant MUST round-trip through `as_sql` + `from_sql`
// without loss.
#[test]
fn vector_01_ask_state_sql_roundtrip() {
    for state in [AskState::Minted, AskState::Settled, AskState::Consumed] {
        let sql_value = state.as_sql();
        let recovered = AskState::from_sql(sql_value);
        assert_eq!(recovered, Some(state), "roundtrip failed for {state:?}");
    }
}

// RFC-0014 §Test Vectors `ask-state-unknown-sql-is-none`: a SQL value
// outside the canonical discriminant set returns `None` (fail-closed;
// substrate MUST NOT silently default to a state).
#[test]
fn vector_02_ask_state_unknown_sql_is_none() {
    assert_eq!(AskState::from_sql(-1), None);
    assert_eq!(AskState::from_sql(3), None);
    assert_eq!(AskState::from_sql(99), None);
    assert_eq!(AskState::from_sql(i64::MIN), None);
    assert_eq!(AskState::from_sql(i64::MAX), None);
}

// RFC-0014 §Test Vectors `ask-state-discriminant-byte-identical`: the
// SQL integer discriminants MUST be byte-pinned: Minted=0,
// Settled=1, Consumed=2. Any drift would break byte-level parity
// with existing persisted rows.
#[test]
fn vector_03_ask_state_discriminant_byte_identical() {
    assert_eq!(AskState::Minted.as_sql(), 0);
    assert_eq!(AskState::Settled.as_sql(), 1);
    assert_eq!(AskState::Consumed.as_sql(), 2);
    // `#[repr(u8)]` plus `as i64` cast -> matches byte-stable SQL
    // integer representation.
    assert_eq!(AskState::Minted as i64, 0);
    assert_eq!(AskState::Settled as i64, 1);
    assert_eq!(AskState::Consumed as i64, 2);
}
