//! `AppendOnlyReceiptSink` trait per RFC-0014 §Module Layout `sink`.
//!
//! Type-level append-only enforcement: the trait takes `&mut self` and
//! exposes only the `append` method. No `delete` / `update` / `clear`
//! methods exist; this prevents accidental mutation paths at the type
//! level (raw ledger path).

use crate::error::SettlementError;
use crate::receipt::Receipt;

/// Append-only receipt sink trait. Concrete impls (e.g.
/// `StoolapReceiptSink` in the `quota-router-sm-engine` DOMAIN crate)
/// MUST preserve the `&mut self` requirement to keep type-level
/// append-only enforcement.
///
/// # Layer discipline
///
/// The trait lives in the substrate (`octo-settlement-core`, Layer A
/// frozen). Concrete storage impls live in DOMAIN crates (e.g.
/// `quota-router-sm-engine`); per the pattern established by
/// `StoolapAuditSink` for `AppendOnlyAuditSink` (RFC-0012 §Trait G3).
pub trait AppendOnlyReceiptSink {
    /// Append a receipt to the sink. Validates `receipt_id`
    /// monotonicity against the last-persisted receipt, computes the
    /// chain hash via `receipt_id_for`, and persists atomically.
    ///
    /// Returns `SettlementError::SequenceGap` on non-monotonic append
    /// and `SettlementError::AlreadyExists` on duplicate `receipt_id`.
    fn append(&mut self, receipt: &Receipt) -> Result<(), SettlementError>;

    /// Read accessor for the last persisted `receipt_id`. Does NOT
    /// violate the append-only invariant (reads do not mutate).
    fn last_receipt_id(&self) -> Result<Option<u64>, SettlementError>;
}
