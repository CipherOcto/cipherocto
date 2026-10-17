//! SQL string byte-stability tests (RFC-0014 §Test Vectors).
//!
//! All 3 vectors exercise canonical RFC-0014 §Test Vectors IDs:
//!
//! - `ask-state-sql-roundtrip` (canonical)
//! - `ask-state-unknown-sql` (canonical; "unknown" maps to the
//!   substrate fail-closed `from_sql` `None` return)
//! - `ask-state-discriminant-byte-pinning` (mission-defined; supports
//!   the canonical roundtrip vector with explicit integer
//!   discriminant pinning)
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

// RFC-0014 §Test Vectors `ask-state-unknown-sql`: a SQL value outside
// the canonical discriminant set returns `None` (fail-closed;
// substrate MUST NOT silently default to a state). The RFC's
// scenario text "from_sql('Unknown')" maps to the substrate
// integer-form `from_sql` which returns `None` for any
// non-canonical discriminant.
#[test]
fn vector_02_ask_state_unknown_sql_is_none() {
    assert_eq!(AskState::from_sql(-1), None);
    assert_eq!(AskState::from_sql(3), None);
    assert_eq!(AskState::from_sql(99), None);
    assert_eq!(AskState::from_sql(i64::MIN), None);
    assert_eq!(AskState::from_sql(i64::MAX), None);
}

// mission-defined: explicit integer-discriminant byte-pinning
// supporting `ask-state-sql-roundtrip`. Asserts Minted=0, Settled=1,
// Consumed=2. `#[repr(u8)]` plus `as i64` cast yields byte-stable
// SQL integer representation.
#[test]
fn mission_01_ask_state_discriminant_byte_pinning() {
    assert_eq!(AskState::Minted.as_sql(), 0);
    assert_eq!(AskState::Settled.as_sql(), 1);
    assert_eq!(AskState::Consumed.as_sql(), 2);
    assert_eq!(AskState::Minted as i64, 0);
    assert_eq!(AskState::Settled as i64, 1);
    assert_eq!(AskState::Consumed as i64, 2);
}
