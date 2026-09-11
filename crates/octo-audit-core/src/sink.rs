//! `AppendOnlyAuditSink` trait per RFC-0012 §Trait G3.
//!
//! Type-level append-only enforcement: the trait takes `&mut self` and
//! exposes only the `append` method. No `delete` / `update` / `clear`
//! methods exist; this prevents accidental mutation paths at the type
//! level.

use crate::error::AuditError;
use crate::event::AuditEvent;

/// Append-only audit sink trait. Concrete impls (e.g. `StoolapAuditSink`
/// in `crates/octo-audit/src/storage/stoolap.rs`) MUST preserve the
/// `&mut self` requirement to keep type-level append-only enforcement.
///
/// # Layer discipline
///
/// The trait lives in the substrate (`octo-audit-core`, Layer A frozen).
/// Concrete storage impls live in DOMAIN crates (e.g. `octo-audit/src/
/// storage/stoolap.rs`, Layer B façade / Layer D adapter); per the
/// pattern established by `StoolapStore` for `SettlementStore` (RFC-0014
/// §Rationale) and `octo-storage-core` / `octo-storage` (RFC-0206
/// §Substrate Newtype Refactor).
pub trait AppendOnlyAuditSink {
    /// Append an event to the sink. Validates `event_id` monotonicity
    /// against the last-persisted event, computes `chain_hash` via
    /// `compute_chain_hash`, and persists atomically (Stoolap
    /// `Transaction` wrapper per RFC-0012 §Trait G3 mitigation).
    ///
    /// Returns `AuditError::SequenceGap` on non-monotonic append and
    /// `AuditError::AlreadyExists` on duplicate `event_id`.
    fn append(&mut self, event: &AuditEvent) -> Result<(), AuditError>;

    /// Read accessor for the last persisted `event_id`. Does NOT violate
    /// the append-only invariant (reads do not mutate).
    fn last_event_id(&self) -> Result<Option<u64>, AuditError>;
}
