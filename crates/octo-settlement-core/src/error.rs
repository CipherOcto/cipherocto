//! Cross-trait `SettlementError` envelope.

use thiserror::Error;

/// Cross-trait settlement error envelope. Concrete stores / sinks MAY
/// return `SettlementError::SinkSpecific` for adapter-specific failures.
#[derive(Debug, Error)]
pub enum SettlementError {
    /// The requested ask was not found in the store.
    #[error("ask not found: {0:?}")]
    AskNotFound([u8; 32]),

    /// The requested ask is already in `Consumed` state and cannot be
    /// settled again.
    #[error("ask {0:?} already consumed")]
    AlreadyConsumed([u8; 32]),

    /// Attempted invalid state transition on an `AskState` or
    /// `ReservationState`.
    #[error("invalid state transition from {from:?} to {to:?}")]
    InvalidTransition {
        /// Source state.
        from: String,
        /// Target state.
        to: String,
    },

    /// `receipt_id` is not the successor of the last persisted receipt.
    #[error("receipt_id sequence gap: {receipt_id} after {prev}")]
    SequenceGap {
        /// The non-monotonic receipt_id that triggered the gap.
        receipt_id: u64,
        /// The last persisted receipt_id before the gap.
        prev: u64,
    },

    /// A receipt's `settlement_hash` field does not match
    /// `receipt_id_for(receipt)`.
    #[error("chain integrity violation at receipt_id {receipt_id}")]
    ChainIntegrity {
        /// The receipt_id whose settlement_hash failed verification.
        receipt_id: u64,
    },

    /// Attempted to append a duplicate `receipt_id`.
    #[error("receipt_id {0} already persisted")]
    AlreadyExists(u64),

    /// Sink-specific failure (e.g. Stoolap transaction aborted).
    #[error("sink-specific error: {0}")]
    SinkSpecific(String),
}

/// Opaque 32-byte settlement-hash newtype whose `Display` impl emits
/// `<redacted-hash>` (RFC-0014-v3 §S5.2 — paired substrate amendment;
/// defect 2 oracle; Layer A frozen substrate primitive).
///
/// The raw bytes are preserved at `Debug` + `source()` so programmatic
/// callers (chain-integrity verification, log post-processing) can
/// inspect them; human-facing `Display` + `to_string()` never leak.
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub struct SettlementHashOpaque([u8; 32]);

impl SettlementHashOpaque {
    /// Wrap a 32-byte hash for redacted Display.
    #[must_use]
    pub const fn new(bytes: [u8; 32]) -> Self {
        Self(bytes)
    }

    /// Access the raw bytes (programmatic callers only; never log).
    #[must_use]
    pub const fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }
}

impl std::fmt::Debug for SettlementHashOpaque {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // Debug intentionally also redacts — symmetric with Display so
        // `dbg!()` / `unwrap_or_else(|e| panic!("{:?}", e))` paths
        // cannot leak via accidental Debug formatting either.
        f.write_str("SettlementHashOpaque(<redacted-hash>)")
    }
}

impl std::fmt::Display for SettlementHashOpaque {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("<redacted-hash>")
    }
}

impl std::error::Error for SettlementHashOpaque {}
