//! Internal [`SyncError`] enum and wire-level [`WireError`] enum.
//!
//! Per RFC-0862 v1.1.0 §DatabaseSyncAdapter Trait §Error Model. The internal
//! `SyncError` variants collapse into a subset of the wire-level codes
//! (RFC-0862 §Error Handling) because the wire codes also cover errors that
//! originate outside the database adapter (envelope validation, DDL, schema
//! drift, heartbeat timeout, role checks).
//!
//! # Mapping table
//!
//! | `SyncError` variant | Wire code |
//! |---|---|
//! | `LsnRegression { expected, actual }` | `E_SYNC_LSN_REGRESSION` |
//! | `InvalidLsnRange { from, to }` | `E_SYNC_LSN_REGRESSION` |
//! | `UnknownPeer` | `E_SYNC_AUTH_FAIL` |
//! | `AllCarriersFailed` | `E_SYNC_RATE_LIMIT` |
//! | `UnknownEnvelopeSubtype` | `E_SYNC_AUTH_FAIL` |
//! | `DecryptionFailed` | `E_SYNC_AUTH_FAIL` |
//! | `SegmentNotFound` | `E_SYNC_SEGMENT_NOT_FOUND` |
//! | `UnknownCarrier` | `E_SYNC_AUTH_FAIL` |
//! | `BackendNotReady` | `E_SYNC_RATE_LIMIT` |

use crate::state::{SyncLifecycle, TransitionTrigger};
use crate::types::Lsn;
use thiserror::Error;

/// Internal error enum returned by [`DatabaseSyncAdapter`](crate::DatabaseSyncAdapter)
/// methods.
///
/// 11 variants. The cipherocto sync engine maps these to wire-level error codes
/// via [`From<SyncError> for WireError`].
#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum SyncError {
    /// LSN regression: the adapter received a request with an LSN less than
    /// the previously applied LSN + 1. Maps to `E_SYNC_LSN_REGRESSION`.
    #[error("LSN regression: expected {expected}, got {actual}")]
    LsnRegression {
        /// The LSN the adapter expected (i.e., the previous LSN + 1).
        expected: u64,
        /// The LSN the adapter actually received.
        actual: u64,
    },

    /// Invalid LSN range: the adapter received a range where `from > to`, or
    /// the range is empty. Maps to `E_SYNC_LSN_REGRESSION` (with extended
    /// detail; the wire protocol surfaces both as a regression).
    #[error("invalid LSN range: from {from} > to {to}")]
    InvalidLsnRange {
        /// The lower bound of the range (which is greater than `to`).
        from: Lsn,
        /// The upper bound of the range (which is less than `from`).
        to: Lsn,
    },

    /// Unknown peer: the adapter has no record of the given `SyncPeerId`.
    /// Maps to `E_SYNC_AUTH_FAIL` (no such peer = auth fail).
    #[error("unknown peer: {0:?}")]
    UnknownPeer([u8; 32]),

    /// All transport carriers failed (the cipherocto sync engine broadcasts
    /// the same envelope over multiple carriers; if all fail, the adapter
    /// surfaces this error). Maps to `E_SYNC_RATE_LIMIT` (all carriers failed
    /// = rate-limited from the perspective of the wire).
    #[error("all carriers failed")]
    AllCarriersFailed,

    /// Unknown envelope subtype: the adapter received an envelope with a
    /// payload discriminator that is not in the 0xA0–0xC2 Sync range.
    /// Maps to `E_SYNC_AUTH_FAIL` (unknown subtype = corrupt/forged envelope).
    #[error("unknown envelope subtype: 0x{0:02X}")]
    UnknownEnvelopeSubtype(u8),

    /// AEAD decryption failure: the adapter's `apply_wal_entry` could not
    /// verify the ciphertext (wrong key, tampered bytes, or AAD mismatch).
    /// Maps to `E_SYNC_AUTH_FAIL`.
    #[error("decryption failed")]
    DecryptionFailed,

    /// Snapshot segment not found at the requested ordinal position, or the
    /// file at that position has a different root. The `regenerated` flag
    /// indicates whether the adapter has already triggered a regeneration
    /// (in which case the reader should re-fetch the summary and re-descend).
    /// Maps to `E_SYNC_SEGMENT_NOT_FOUND`.
    #[error("segment not found: table_id={table_id}, segment_index={segment_index}, regenerated={regenerated}")]
    SegmentNotFound {
        /// The table id.
        table_id: u32,
        /// The segment index.
        segment_index: u32,
        /// Whether the adapter has already triggered a regeneration.
        regenerated: bool,
    },

    /// Unknown transport carrier: the operator's config references a carrier
    /// name that the adapter does not know. Maps to `E_SYNC_AUTH_FAIL`.
    #[error("unknown carrier: {0}")]
    UnknownCarrier(String),

    /// Backend not ready: the database is in a state that cannot service the
    /// request (e.g., the DB is shutting down, or the apply queue is full).
    /// The cipherocto sync engine treats this as a transient error and retries
    /// with backoff. Maps to `E_SYNC_RATE_LIMIT` (backpressure signal).
    #[error("backend not ready: {0}")]
    BackendNotReady(String),

    /// Invalid state transition: the per-peer state machine (RFC-0862
    /// §Lifecycle Requirements) does not allow this transition. Indicates
    /// a bug in the cipherocto sync engine — the caller should log and
    /// transition the peer to `Terminated`. Maps to `E_SYNC_AUTH_FAIL`
    /// (defensive: the engine is sending an out-of-sequence state update).
    #[error("invalid state transition: {from:?} → {to:?} via {trigger:?}")]
    InvalidStateTransition {
        /// The state being transitioned from.
        from: SyncLifecycle,
        /// The state being transitioned to.
        to: SyncLifecycle,
        /// The trigger that caused the invalid transition.
        trigger: TransitionTrigger,
    },

    // ── Slashing detection (RFC-0862 Phase 4, mission 0862m) ──────────
    /// Corrupted WAL entry: CRC32 verification failed. The entry payload
    /// does not match its CRC32 checksum, indicating data corruption or
    /// tampering. Maps to slash code `SyncCorruptedWalEntry` (0x0020).
    #[error("corrupted WAL entry: CRC32 mismatch")]
    CorruptedWalEntry,

    /// Fake summary: HMAC verification failed. The summary's HMAC does not
    /// match the expected value computed from the transport key, indicating
    /// the summary was forged or tampered with. Maps to slash code
    /// `SyncFakeSummary` (0x0021).
    #[error("fake summary: HMAC mismatch")]
    FakeSummary,
}

/// Wire-level error code (the codes defined in RFC-0862 §Error Handling).
///
/// These are the bytes-on-the-wire error codes; the cipherocto sync engine
/// emits one of these for every error. Implementers of
/// [`DatabaseSyncAdapter`](crate::DatabaseSyncAdapter) do NOT emit these directly —
/// they return a [`SyncError`] and the engine maps via
/// [`From<SyncError> for WireError`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum WireError {
    /// `E_SYNC_AUTH_FAIL` — authentication failure.
    AuthFailure,
    /// `E_SYNC_LSN_REGRESSION` — LSN regression.
    LsnRegression,
    /// `E_SYNC_SEGMENT_CORRUPTION` — segment corruption (BLAKE3/CRC32 mismatch).
    /// Fired by the envelope validator, NOT by the adapter.
    SegmentCorruption,
    /// `E_SYNC_SEGMENT_NOT_FOUND` — segment not found.
    SegmentNotFound,
    /// `E_SYNC_RATE_LIMIT` — rate limit exceeded / backpressure.
    RateLimit,
    /// `E_SYNC_WAL_APPEND_FAIL` — WAL append failed (schema mismatch).
    /// Fired by the engine, NOT by the adapter.
    WalAppendFail,
    /// `E_SYNC_SCHEMA_DRIFT` — schema drift (DDL out-of-order).
    /// Fired by the envelope handler, NOT by the adapter.
    SchemaDrift,
    /// `E_SYNC_HEARTBEAT_TIMEOUT` — heartbeat timeout.
    /// Fired by the heartbeat scheduler, NOT by the adapter.
    HeartbeatTimeout,
    /// `E_SYNC_ROLE_NOT_SYNC_CAPABLE` — role check failure.
    /// Fired before the adapter is even called.
    RoleNotSyncCapable,
    /// `E_SYNC_CORRUPTED_WAL` — WAL entry CRC32 mismatch (slash code 0x0020).
    /// Fired by the sync engine when a received WAL entry fails CRC32 validation.
    CorruptedWalEntry,
    /// `E_SYNC_FAKE_SUMMARY` — Summary HMAC mismatch (slash code 0x0021).
    /// Fired by the sync engine when a received summary fails HMAC verification.
    FakeSummary,
}

impl WireError {
    /// Return the canonical 8-bit wire code for this error.
    /// Per RFC-0862 §Error Handling.
    pub fn code(self) -> u8 {
        match self {
            WireError::AuthFailure => 0x01,
            WireError::LsnRegression => 0x02,
            WireError::SegmentCorruption => 0x03,
            WireError::SegmentNotFound => 0x04,
            WireError::RateLimit => 0x05,
            WireError::WalAppendFail => 0x06,
            WireError::SchemaDrift => 0x07,
            WireError::HeartbeatTimeout => 0x08,
            WireError::RoleNotSyncCapable => 0x09,
            WireError::CorruptedWalEntry => 0x0A,
            WireError::FakeSummary => 0x0B,
        }
    }

    /// Return the human-readable name for this error.
    pub fn name(self) -> &'static str {
        match self {
            WireError::AuthFailure => "E_SYNC_AUTH_FAIL",
            WireError::LsnRegression => "E_SYNC_LSN_REGRESSION",
            WireError::SegmentCorruption => "E_SYNC_SEGMENT_CORRUPTION",
            WireError::SegmentNotFound => "E_SYNC_SEGMENT_NOT_FOUND",
            WireError::RateLimit => "E_SYNC_RATE_LIMIT",
            WireError::WalAppendFail => "E_SYNC_WAL_APPEND_FAIL",
            WireError::SchemaDrift => "E_SYNC_SCHEMA_DRIFT",
            WireError::HeartbeatTimeout => "E_SYNC_HEARTBEAT_TIMEOUT",
            WireError::RoleNotSyncCapable => "E_SYNC_ROLE_NOT_SYNC_CAPABLE",
            WireError::CorruptedWalEntry => "E_SYNC_CORRUPTED_WAL",
            WireError::FakeSummary => "E_SYNC_FAKE_SUMMARY",
        }
    }
}

/// Mapping from internal [`SyncError`] to wire-level [`WireError`] codes.
///
/// Many-to-one: the internal variants collapse to distinct wire codes.
/// Some wire codes (`SegmentCorruption`, `WalAppendFail`,
/// `SchemaDrift`, `HeartbeatTimeout`, `RoleNotSyncCapable`) originate
/// outside the adapter and have no `SyncError` variant.
impl From<SyncError> for WireError {
    fn from(err: SyncError) -> Self {
        match err {
            SyncError::LsnRegression { .. } | SyncError::InvalidLsnRange { .. } => {
                WireError::LsnRegression
            }
            SyncError::UnknownPeer(_)
            | SyncError::UnknownEnvelopeSubtype(_)
            | SyncError::DecryptionFailed
            | SyncError::UnknownCarrier(_)
            | SyncError::InvalidStateTransition { .. } => WireError::AuthFailure,
            SyncError::AllCarriersFailed | SyncError::BackendNotReady(_) => WireError::RateLimit,
            SyncError::SegmentNotFound { .. } => WireError::SegmentNotFound,
            SyncError::CorruptedWalEntry => WireError::CorruptedWalEntry,
            SyncError::FakeSummary => WireError::FakeSummary,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wire_codes_are_stable() {
        // Per RFC-0862 §Error Handling, the 9 wire codes are stable.
        // If any of these change, downstream parsers break.
        assert_eq!(WireError::AuthFailure.code(), 0x01);
        assert_eq!(WireError::LsnRegression.code(), 0x02);
        assert_eq!(WireError::SegmentCorruption.code(), 0x03);
        assert_eq!(WireError::SegmentNotFound.code(), 0x04);
        assert_eq!(WireError::RateLimit.code(), 0x05);
        assert_eq!(WireError::WalAppendFail.code(), 0x06);
        assert_eq!(WireError::SchemaDrift.code(), 0x07);
        assert_eq!(WireError::HeartbeatTimeout.code(), 0x08);
        assert_eq!(WireError::RoleNotSyncCapable.code(), 0x09);
    }

    #[test]
    fn from_lsn_regression() {
        let e = SyncError::LsnRegression {
            expected: 100,
            actual: 99,
        };
        assert_eq!(WireError::from(e), WireError::LsnRegression);
    }

    #[test]
    fn from_invalid_lsn_range() {
        let e = SyncError::InvalidLsnRange { from: 200, to: 100 };
        assert_eq!(WireError::from(e), WireError::LsnRegression);
    }

    #[test]
    fn from_unknown_peer_is_auth_failure() {
        let e = SyncError::UnknownPeer([0u8; 32]);
        assert_eq!(WireError::from(e), WireError::AuthFailure);
    }

    #[test]
    fn from_all_carriers_failed_is_rate_limit() {
        let e = SyncError::AllCarriersFailed;
        assert_eq!(WireError::from(e), WireError::RateLimit);
    }

    #[test]
    fn from_unknown_envelope_subtype_is_auth_failure() {
        let e = SyncError::UnknownEnvelopeSubtype(0x99);
        assert_eq!(WireError::from(e), WireError::AuthFailure);
    }

    #[test]
    fn from_decryption_failed_is_auth_failure() {
        let e = SyncError::DecryptionFailed;
        assert_eq!(WireError::from(e), WireError::AuthFailure);
    }

    #[test]
    fn from_segment_not_found() {
        let e = SyncError::SegmentNotFound {
            table_id: 42,
            segment_index: 7,
            regenerated: false,
        };
        assert_eq!(WireError::from(e), WireError::SegmentNotFound);
    }

    #[test]
    fn from_unknown_carrier_is_auth_failure() {
        let e = SyncError::UnknownCarrier("telegram".to_string());
        assert_eq!(WireError::from(e), WireError::AuthFailure);
    }

    #[test]
    fn from_backend_not_ready_is_rate_limit() {
        let e = SyncError::BackendNotReady("shutting down".to_string());
        assert_eq!(WireError::from(e), WireError::RateLimit);
    }

    #[test]
    fn names_match_rfc() {
        assert_eq!(WireError::AuthFailure.name(), "E_SYNC_AUTH_FAIL");
        assert_eq!(WireError::LsnRegression.name(), "E_SYNC_LSN_REGRESSION");
        assert_eq!(
            WireError::SegmentCorruption.name(),
            "E_SYNC_SEGMENT_CORRUPTION"
        );
        assert_eq!(
            WireError::SegmentNotFound.name(),
            "E_SYNC_SEGMENT_NOT_FOUND"
        );
        assert_eq!(WireError::RateLimit.name(), "E_SYNC_RATE_LIMIT");
        assert_eq!(WireError::WalAppendFail.name(), "E_SYNC_WAL_APPEND_FAIL");
        assert_eq!(WireError::SchemaDrift.name(), "E_SYNC_SCHEMA_DRIFT");
        assert_eq!(
            WireError::HeartbeatTimeout.name(),
            "E_SYNC_HEARTBEAT_TIMEOUT"
        );
        assert_eq!(
            WireError::RoleNotSyncCapable.name(),
            "E_SYNC_ROLE_NOT_SYNC_CAPABLE"
        );
    }
}
