//! Layer B settlement facade for the cipherocto workspace (RFC-0014
//! §Module Layout).
//!
//! This facade re-exports the substrate surface (`octo-settlement-core`)
//! verbatim. Concrete store / sink impls (e.g. `StoolapStore`,
//! `StoolapReceiptSink`) live at the DOMAIN layer in
//! `quota-router-sm-engine::{store,sink}` per RFC-0014 §Scope.
//!
//! ## Layer model
//!
//! Per `cipherocto-design-principles` Layer B, this crate is
//! **RFC-driven, additive only**. The re-export surface mirrors the
//! substrate's public API byte-for-byte.

#![forbid(unsafe_code)]
#![warn(missing_docs)]

// Explicit curated re-export (RFC-0014 §Module Layout).
pub use octo_settlement_core::receipt_id_for;
pub use octo_settlement_core::receipt_id_for_digest;
pub use octo_settlement_core::verify_receipt_chain;
pub use octo_settlement_core::AppendOnlyReceiptSink;
pub use octo_settlement_core::Ask;
pub use octo_settlement_core::AskState;
pub use octo_settlement_core::Receipt;
pub use octo_settlement_core::ReceiptId;
pub use octo_settlement_core::ReceiptStatus;
pub use octo_settlement_core::Reservation;
pub use octo_settlement_core::ReservationState;
pub use octo_settlement_core::SettlementError;
pub use octo_settlement_core::SettlementStore;
pub use octo_settlement_core::CHAIN_DOMAIN_SEPARATOR;

// RFC-0014 substrate amendment: per-façade 10-pattern scrubber
// (defect 1b — DOMAIN adapter error-chain redaction). Per-façade
// duplication with `octo_audit::scrub` accepted per the R34.5 trade-off
// (cross-RFC shared-utility extraction deferred to §FW6).
pub mod scrub;
pub use scrub::{scrub_adapter_error, scrub_adapter_error_with};
