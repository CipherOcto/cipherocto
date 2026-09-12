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
    ///
    /// RFC-0012-v3 §S6.2 (paired substrate amendment — defect 4 oracle):
    /// `prev` + `current` numeric timestamps are RETAINED at the source
    /// (programmatic chain-integrity verification needs them) but the
    /// `Display` impl emits `<redacted-timestamp>` only — `event_id`
    /// remains in Display because it is NOT a chronological side-channel
    /// (event_id is monotonic and recoverable from chain-hash lookup).
    #[error("timestamp regression at event_id {event_id} (<redacted-timestamp>)")]
    TimestampRegression {
        /// The event_id whose timestamp regressed.
        event_id: u64,
        /// The previous (larger) timestamp. Retained for programmatic
        /// inspection; Display redacts as `<redacted-timestamp>`.
        prev: TimestampOpaque,
        /// The regressed (smaller) timestamp. Same source-retain /
        /// Display-redact contract as `prev`.
        current: TimestampOpaque,
    },
}

/// Opaque unix-millis timestamp newtype whose `Display` impl emits
/// `<redacted-timestamp>` (RFC-0012-v3 §S6.2 — paired substrate
/// amendment; defect 4 oracle).
///
/// The raw u64 is preserved at `Debug` + `source()` so programmatic
/// callers (chain-integrity verification, replay protection) can
/// inspect it; human-facing `Display` + `to_string()` never leak the
/// chronological value (which would be a side-channel: callers could
/// reconstruct timing of an event from the regression error).
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub struct TimestampOpaque(u64);

impl TimestampOpaque {
    /// Wrap a unix-millis timestamp for redacted Display.
    #[must_use]
    pub const fn new(millis_unix: u64) -> Self {
        Self(millis_unix)
    }

    /// Access the raw millis-unix (programmatic callers only; never log).
    #[must_use]
    pub const fn as_millis_unix(&self) -> u64 {
        self.0
    }
}

impl std::fmt::Debug for TimestampOpaque {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // Debug intentionally also redacts — symmetric with Display so
        // `dbg!()` / `unwrap_or_else(|e| panic!("{:?}", e))` paths
        // cannot leak via accidental Debug formatting either. Mirror of
        // `SettlementHashOpaque` symmetry-rationale comment in
        // RFC-0014-v3 §S5.2.
        f.write_str("TimestampOpaque(<redacted-timestamp>)")
    }
}

impl std::fmt::Display for TimestampOpaque {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("<redacted-timestamp>")
    }
}

impl std::error::Error for TimestampOpaque {}
