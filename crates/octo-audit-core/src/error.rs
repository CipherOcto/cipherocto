//! Cross-trait `AuditError` envelope + `AuditChainError` chain-specific
//! variants.

use thiserror::Error;

/// Cross-trait audit error envelope. Concrete sinks MAY return
/// `AuditError::SinkSpecific` for adapter-specific failures
/// (e.g. `StoolapAuditSink` Stoolap transaction errors).
///
/// `#[non_exhaustive]` per Layer A frozen contract (CLAUDE.md
/// §Architectural Principles + RFC-0016-a §6.7 substrate column):
/// downstream exhaustive match arms MUST use wildcard patterns.
/// New variants land additively without breaking downstream.
#[derive(Debug, Error)]
#[non_exhaustive]
pub enum AuditError {
    /// Event ID is not the successor of the last persisted event.
    ///
    /// RFC-0016-a §6.7 paired substrate column; test vectors
    /// `TV-AUD-sequence-gap-1` / `TV-AUD-sequence-gap-2` (caller
    /// appends `event_id = N+2` after `event_id = N`).
    #[error("sequence gap: event_id {event_id} after {prev}")]
    SequenceGap {
        /// The non-monotonic event_id that triggered the gap.
        event_id: u64,
        /// The last persisted event_id before the gap.
        prev: u64,
    },

    /// Attempted to append a duplicate event_id (idempotent re-append).
    ///
    /// RFC-0016-a §6.7 paired substrate column; test vector
    /// `TV-AUD-already-exists-1` (caller re-appends the same event_id).
    #[error("event_id {0} already persisted")]
    AlreadyExists(u64),

    /// Sink-specific failure (e.g. Stoolap Transaction aborted).
    #[error("sink-specific error: {0}")]
    SinkSpecific(String),

    /// `append_audit_event` write-path failure (RFC-0016-a §6.2).
    /// Carries the substrate-side scrubbed reason string from the
    /// underlying `AppendOnlyAuditSink::append` call (the
    /// sink-faithful byte-form reason; the CLI `From<AuditError>`
    /// conversion wraps it in `OctoCliError::AuditSubstrateNotReady`
    /// per RFC-0011-a canonical `[ADD]` error envelope pattern).
    /// `#[non_exhaustive]` carries through the `AuditError` enum
    /// (already gated) so the variant lands additively without
    /// breaking existing matchers.
    #[error("audit append failed: {0}")]
    AuditAppendFailed(String),

    /// CLI-shape variant — receipt id not found in the receipt store
    /// (RFC-0016-a §6.7 paired-with-RFC-0011-a; exit code 17).
    /// Carries the canonical decimal `u64` form of the requested
    /// receipt_id (CLI-friendly; per test vector
    /// `Err(OctoCliError::ReceiptNotFound("<decimal-u64>".into()))`).
    #[error("receipt not found: {0}")]
    ReceiptNotFound(String),

    /// CLI-shape variant — operator filter expression failed substrate
    /// validation (RFC-0016-a §6.7 paired-with-RFC-0011-a; exit code
    /// 16). Carries the reason string (limit-out-of-range,
    /// since_unix > until_unix, etc.) per test vectors
    /// `TV-AUD-4` / `TV-AUD-4b` / `TV-AUD-4c`.
    #[error("invalid filter: {0}")]
    InvalidFilter(String),

    /// CLI-shape variant — per-process trust boundary violated
    /// (RFC-0016-a §6.7 paired-with-RFC-0011-a; exit code 13).
    /// Carries the scrubbed canonical path (e.g.
    /// `<OCTO_HOME>/audit/receipts`) per test vectors
    /// `TV-AUD-permission-check-1` + `TV-AUD-permission-check-2`.
    /// Substrate-side scrubber applies the 13-pattern list before
    /// the CLI envelope maps to `OctoCliError::PermissionDenied`.
    #[error("permission denied: {0}")]
    PermissionDenied(String),

    /// Substrate-canonical chain-hash mismatch (RFC-0016-a §6.10
    /// canonical-bytes-on-write invariant). Returned by the Layer B
    /// `append_audit_event` façade when the caller-supplied
    /// `event.chain_hash` does NOT match `compute_chain_hash(&event)`
    /// over canonical bytes. Sink is NOT called.
    ///
    /// Carries the failing `event_id` only — the freshly-recomputed
    /// canonical digest is recoverable via `compute_chain_hash(&event)`
    /// and the supplied digest is the raw caller input, so the digests
    /// themselves add no diagnostic value at the error-site (Layer A
    /// substrate surface; the CLI envelope maps to
    /// `OctoCliError::Internal` with a redacted reason via the CLI
    /// 13-pattern scrubber).
    #[error("chain_hash mismatch at event_id {event_id}")]
    ChainHashMismatch {
        /// The event_id whose caller-supplied `chain_hash` failed
        /// the canonical-bytes recompute.
        event_id: u64,
    },
}

/// Chain-integrity error variants returned by `verify_chain`.
#[derive(Debug, Error, PartialEq, Eq)]
#[non_exhaustive]
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
    /// RFC-0012 §S6.2 (paired substrate amendment — defect 4 oracle):
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
/// `<redacted-timestamp>` (RFC-0012 §S6.2 — paired substrate
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
        // RFC-0014 §S5.2.
        f.write_str("TimestampOpaque(<redacted-timestamp>)")
    }
}

impl std::fmt::Display for TimestampOpaque {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("<redacted-timestamp>")
    }
}

impl std::error::Error for TimestampOpaque {}
