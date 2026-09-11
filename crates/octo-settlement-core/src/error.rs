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
