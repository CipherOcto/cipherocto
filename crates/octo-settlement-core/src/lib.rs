//! Layer A frozen settlement substrate (RFC-0014 §Specification).
//!
//! This crate owns the canonical settlement primitives:
//!
//! - [`Receipt`] — canonical receipt struct (6 fields per RFC-0959
//!   §Data Structures).
//! - [`Ask`] + [`AskState`] — canonical ask struct + 3-variant state
//!   machine (Minted, Settled, Consumed) per RFC-0959 §State Machine.
//! - [`Reservation`] + [`ReservationState`] — canonical reservation
//!   struct + 8-variant state machine per RFC-0960 §2.3.
//! - [`SettlementStore`] trait — CRUD-style canonical interface (`&self`,
//!   interior mutability pattern; mirrors `StoolapStore`).
//! - [`AppendOnlyReceiptSink`] trait — type-level append-only
//!   enforcement via `&mut self` (raw ledger path).
//! - [`verify_receipt_chain`] + [`receipt_id_for`] — chain-integrity
//!   helpers (BLAKE3-256 keyed with `cipherocto/reservation/v1/`).
//! - [`SettlementError`] — cross-trait error envelope.
//!
//! ## Layer discipline (per CLAUDE.md §Architectural Principles)
//!
//! Per RFC-0014 §Security Considerations, this crate is **RFC-frozen +
//! semver-major only**. Deps are restricted to Layer A primitives
//! (`blake3` + `serde` + `thiserror`); no IO, no storage. The two-trait
//! split (`SettlementStore` + `AppendOnlyReceiptSink`) preserves
//! append-only enforcement on the raw ledger path while keeping the
//! canonical CRUD interface ergonomic for domain crates.
//!
//! ## Module layout (RFC-0014 §Module Layout)
//!
//! - [`receipt`] — `Receipt` struct
//! - [`ask`] — `Ask` struct + `AskState` enum + `as_sql` / `from_sql` helpers
//! - [`reservation`] — `Reservation` struct + `ReservationState` enum
//! - [`store`] — `SettlementStore` trait (`&self` CRUD interface)
//! - [`sink`] — `AppendOnlyReceiptSink` trait (`&mut self` raw ledger)
//! - [`chain`] — `verify_receipt_chain` + `receipt_id_for` helpers
//! - [`error`] — `SettlementError` cross-trait envelope

#![forbid(unsafe_code)]
#![warn(missing_docs)]

pub mod ask;
pub mod chain;
pub mod error;
pub mod receipt;
pub mod reservation;
pub mod sink;
pub mod store;

pub use ask::Ask;
pub use ask::AskState;
pub use chain::{receipt_id_for, verify_receipt_chain, CHAIN_DOMAIN_SEPARATOR};
pub use error::SettlementError;
pub use receipt::Receipt;
pub use reservation::{Reservation, ReservationState};
pub use sink::AppendOnlyReceiptSink;
pub use store::SettlementStore;
