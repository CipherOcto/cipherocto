//! Cross-trait `AuditError` envelope + `AuditChainError` chain-specific
//! variants.

use thiserror::Error;

/// Cross-trait audit error envelope. Concrete sinks MAY return
/// `AuditError::SinkSpecific` for adapter-specific failures
/// (e.g. `StoolapAuditSink` Stoolap transaction errors).
#[derive(Debug, Error)]
pub enum AuditError {
    /// Event ID is not the successor of the last persisted event.
    #[error("sequence gap: event_id {event_id} after {prev}")]
    SequenceGap {
        /// The non-monotonic event_id that triggered the gap.
        event_id: u64,
        /// The last persisted event_id before the gap.
        prev: u64,
    },

    /// Attempted to append a duplicate event_id (idempotent re-append).
    #[error("event_id {0} already persisted")]
    AlreadyExists(u64),

    /// Sink-specific failure (e.g. Stoolap Transaction aborted).
    #[error("sink-specific error: {0}")]
    SinkSpecific(String),
}

/// Chain-integrity error variants returned by `verify_chain`.
#[derive(Debug, Error, PartialEq, Eq)]
pub enum AuditChainError {
    /// `event_id` is not the successor of the previous event's `event_id`.
    #[error("sequence gap at event_id {event_id} (previous was {prev})")]
    SequenceGap {
        /// The event_id that triggered the gap.
        event_id: u64,
        /// The previous event_id (the last valid one).
        prev: u64,
    },

    /// `chain_hash` field does not match `compute_chain_hash(event)`.
    #[error("chain_hash mismatch at event_id {event_id}")]
    HashMismatch {
        /// The event_id whose chain_hash failed verification.
        event_id: u64,
    },

    /// `at_millis_unix` decreased between consecutive events.
    #[error("timestamp regression at event_id {event_id} (was {prev}, now {current})")]
    TimestampRegression {
        /// The event_id whose timestamp regressed.
        event_id: u64,
        /// The previous (larger) timestamp.
        prev: u64,
        /// The regressed (smaller) timestamp.
        current: u64,
    },
}
