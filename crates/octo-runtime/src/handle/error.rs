//! `octo-runtime::handle::error` — error envelope for `AttachHandle`
//! token operations (RFC-0011-c §F.4).
//!
//! Each variant maps 1:1 to an `OctoCliError` slot per RFC-0011-c
//! §9.8 (extension 39-61). The substrate owns the canonical error
//! distinction; the CLI mirrors via per-variant `From<AttachError>`
//! arms so an additive substrate variant lands a corresponding CLI
//! slot without central-enum edits.

use thiserror::Error;

use crate::handle::SessionId;

/// Substrate error envelope for `AttachHandle` operations
/// (RFC-0011-c §F.4).
///
/// Mirrors 1:1 to the `OctoCliError` slots 53-61 per RFC-0011-c
/// §9.8 extension (9 variants, 8 slots; `InvalidSinceCursor`
/// shares slot 53 with `Expired` per amendment-chain shared-slot
/// pattern; `ReplayDetected` occupies its own slot 61 per the §9.7
/// follow-on amendment). The CLI boundary translates per-variant;
/// the substrate owns the canonical distinction.
#[derive(Error, Debug)]
#[non_exhaustive]
pub enum AttachError {
    /// Token TTL has elapsed (`now_unix > token.ttl_unix`).
    /// Surfaces verbatim from `attach_with_token` step (c) per
    /// RFC-0011-c §F.2 validation chain. CLI exit 53.
    #[error(
        "attach handle expired for session {session_id:?}: now {now_unix} > ttl {expired_at_unix} (mint {mint_unix})"
    )]
    Expired {
        /// Session id from the token.
        session_id: SessionId,
        /// Token mint timestamp (the lower-bound replay horizon).
        /// Surfaced to the CLI so the operator can read the full
        /// `{mint, ttl, now}` triplet without re-deriving it from
        /// the (typed-discriminator-bearing) substrate signature
        /// envelope.
        mint_unix: u64,
        /// TTL boundary from the token (`mint_unix + ttl_duration`).
        expired_at_unix: u64,
        /// Observed wall-clock when the check fired.
        now_unix: u64,
    },

    /// Signature verification failed at validation step (a) of the
    /// `attach_with_token` chain (or on `decode_token`). CLI exit 54.
    #[error("attach handle signature invalid: {reason}")]
    BadSignature {
        /// Diagnostic reason (substrate-internal; CLI sanitizes).
        reason: String,
    },

    /// Session id in the token doesn't match the running session
    /// registry (validation step (e) of `attach_with_token`).
    /// CLI exit 55.
    #[error("session mismatch (declared {declared:?}, actual {actual:?})")]
    SessionMismatch {
        /// Session id declared in the token.
        declared: SessionId,
        /// Session id observed in the running registry.
        actual: SessionId,
    },

    /// Session id unknown to the running session registry. CLI exit 56.
    #[error("session {session_id:?} not found")]
    UnknownSession {
        /// Session id from the token.
        session_id: SessionId,
    },

    /// `since_unix < token.mint_timestamp_unix` — replay cursor
    /// below token mint. CLI exit 53 (shared slot with `Expired`;
    /// see enum-level rustdoc for shared-slot rationale).
    #[error("since cursor {requested} is below token mint {mint_unix}")]
    InvalidSinceCursor {
        /// Token mint timestamp (the lower-bound replay horizon).
        mint_unix: u64,
        /// `since_unix` supplied by the caller (below mint).
        requested: u64,
    },

    /// Persistence layer failure (Stoolap cursor store; gated on the
    /// `octo-runtime-persistence` feature). CLI exit 57.
    #[error("persistence error: {0}")]
    PersistenceError(String),

    /// Revocation-set failure (in-memory `RwLock<HashSet<SessionId>>`
    /// revocation set). CLI exit 58.
    #[error("revocation error: {0}")]
    RevocationError(String),

    /// Step (e) — no `Handler` registered for the token's
    /// `TransportKind`. Extension transports (UnixSocket, Raw
    /// scheme UUIDs) register via follow-on Layer D crates (see
    /// enum-level rustdoc). CLI exit 59.
    #[error("transport handler not registered for kind `{kind_label}` (register a Handler via octo_runtime::handle::transport::HANDLE_TRANSPORT_REGISTRY in a follow-on Layer D crate)")]
    TransportHandlerNotRegistered {
        /// Transport-kind discriminator label for the operator
        /// ("InProcess", "UnixSocket", or "Raw(<uuid>)").
        kind_label: String,
    },

    /// Replay detected — `since_unix` cursor at or behind the recorded
    /// session cursor. Surfaces from `attach_with_token` step (e)
    /// when the consumption guarantee is violated: the same token
    /// has been bound once and is being replayed, or a different
    /// token on the same session is being bound with a stale cursor.
    /// CLI exit 61 (additive typed-discriminator variant per the §9.7
    /// follow-on amendment).
    #[error(
        "replay detected: since cursor {since_unix} is at or behind the recorded cursor {recorded_cursor}"
    )]
    ReplayDetected {
        /// `since_unix` from the rejected attach invocation
        /// (at or behind the recorded session cursor).
        since_unix: u64,
        /// Highest `since_unix` previously accepted for this
        /// session (the substrate-faithful replay-detection
        /// reference).
        recorded_cursor: u64,
    },

    /// Substrate-internal failure surfaced as a substrate-visible
    /// `AttachError` rather than a typed-discriminator. Used by
    /// Layer D extension `Handler` impls (`octo-runtime-transport-unix`,
    /// `octo-runtime-transport-hybrid`, …) for protocol-level
    /// failures that don't fit the typed-discriminator pattern
    /// (handshake failures, multiplex aggregation, connection
    /// failures, configuration errors). CLI exit 64 (additive
    /// variant routed via the existing `From<AttachError>`
    /// wildcard arm in `crates/octo-cli/src/error.rs`).
    #[error("internal handler error: {0}")]
    Internal(String),
}

/// Persistence-layer error envelope (Stoolap cursor store).
///
/// Gated on the `octo-runtime-persistence` feature flag (Cargo.toml
/// `[features]`); when the feature is OFF, `persist_event_cursor` /
/// `load_event_cursor` return `PersistenceError::FeatureNotEnabled`
/// instead of constructing a Stoolap-backed connection.
#[derive(Error, Debug)]
#[non_exhaustive]
pub enum PersistenceError {
    /// `octo-runtime-persistence` feature is OFF (default build).
    #[error("persistence feature not enabled: enable feature `octo-runtime-persistence`")]
    FeatureNotEnabled,

    /// Stoolap cursor write failed.
    #[error("stoolap cursor write failed: {0}")]
    StoolapWriteFailed(String),

    /// Stoolap cursor read failed.
    #[error("stoolap cursor read failed: {0}")]
    StoolapReadFailed(String),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn expired_display_includes_session_id_and_times() {
        let e = AttachError::Expired {
            session_id: [0xab; 32],
            mint_unix: 1_000,
            expired_at_unix: 2_000,
            now_unix: 3_000,
        };
        let s = e.to_string();
        assert!(s.contains("expired"), "{s}");
        assert!(s.contains("2000"), "{s}");
        assert!(s.contains("3000"), "{s}");
        assert!(s.contains("1000"), "{s}");
    }

    #[test]
    fn bad_signature_display_includes_reason() {
        let e = AttachError::BadSignature {
            reason: "ed25519 verify failed".to_string(),
        };
        let s = e.to_string();
        assert!(s.contains("ed25519"), "{s}");
    }

    #[test]
    fn session_mismatch_display_includes_both_ids() {
        let e = AttachError::SessionMismatch {
            declared: [0x01; 32],
            actual: [0x02; 32],
        };
        let s = e.to_string();
        assert!(s.contains("mismatch"), "{s}");
    }

    #[test]
    fn unknown_session_display_includes_session_id() {
        let e = AttachError::UnknownSession {
            session_id: [0xee; 32],
        };
        let s = e.to_string();
        assert!(s.contains("not found"), "{s}");
    }

    #[test]
    fn persistence_error_display_carries_payload() {
        let e = AttachError::PersistenceError("stoolap down".to_string());
        let s = e.to_string();
        assert!(s.contains("stoolap down"), "{s}");
    }

    #[test]
    fn revocation_error_display_carries_payload() {
        let e = AttachError::RevocationError("poisoned lock".to_string());
        let s = e.to_string();
        assert!(s.contains("poisoned lock"), "{s}");
    }

    #[test]
    fn persistence_error_variants_display() {
        let e = PersistenceError::FeatureNotEnabled;
        assert!(e.to_string().contains("not enabled"));
        let e = PersistenceError::StoolapWriteFailed("disk full".into());
        assert!(e.to_string().contains("disk full"));
        let e = PersistenceError::StoolapReadFailed("connection lost".into());
        assert!(e.to_string().contains("connection lost"));
    }

    #[test]
    fn invalid_since_cursor_display_includes_mint_and_requested() {
        let e = AttachError::InvalidSinceCursor {
            mint_unix: 1_000,
            requested: 500,
        };
        let s = e.to_string();
        assert!(s.contains("since cursor"), "{s}");
        assert!(s.contains("1000"), "{s}");
        assert!(s.contains("500"), "{s}");
    }

    #[test]
    fn transport_handler_not_registered_display_includes_kind_label() {
        let e = AttachError::TransportHandlerNotRegistered {
            kind_label: "UnixSocket".to_string(),
        };
        let s = e.to_string();
        assert!(s.contains("UnixSocket"), "{s}");
        assert!(s.contains("not registered"), "{s}");
    }

    #[test]
    fn replay_detected_display_includes_cursors() {
        let e = AttachError::ReplayDetected {
            since_unix: 1_000,
            recorded_cursor: 2_000,
        };
        let s = e.to_string();
        assert!(s.contains("replay"), "{s}");
        assert!(s.contains("1000"), "{s}");
        assert!(s.contains("2000"), "{s}");
    }

    #[test]
    fn internal_display_carries_reason() {
        let e = AttachError::Internal("handshake failed: bad reply".to_string());
        let s = e.to_string();
        assert!(s.contains("handshake failed: bad reply"), "{s}");
    }
}
