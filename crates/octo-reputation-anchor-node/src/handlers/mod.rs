//! Reputation-anchor payload handlers (RFC-0871 §Roles and Authorities,
//! mission 0871c-reputation-anchor-node).
//!
//! Phase 3 MVP exposes only `REPUTATION_ANCHOR_QUERY` — a typed hand-off
//! point that validates a canonical DID and returns a stub
//! `(anchor_score, attestation_count)` response. All handlers route through
//! `EnvelopeDispatcher` for envelope_id dedup + expiry + signature
//! verification (no handler shortcuts, no per-handler HMAC bypass).
//!
//! Layer B boundary: handlers consume `octo-protocol` + `octo-ident` types
//! and never reach into storage or registry substrate directly. The real
//! reputation registry lookup (RFC-0968 `ReputationRegistry` +
//! RFC-0955-R1 anchoring substrate) lands in mission
//! 0968a-reputation-anchoring follow-on.

use octo_protocol::ProtocolError;

pub mod query;

/// The one `HandlerOutput`, defined once in `octo-protocol`.
///
/// This module used to declare its own copy — four identical ones across
/// the node crates, plus a placeholder in `octo-protocol` that the
/// reference dispatcher echoed into. The placeholder is now the real
/// shape, so the node handlers and the dispatcher agree by construction
/// instead of by four copies having stayed in step by luck.
pub use octo_protocol::HandlerOutput;

pub use query::{QueryAnchorHandler, QueryAnchorRequest, QueryAnchorResponse};

/// Map a `octo_ident::DidError` to a `ProtocolError` for invalid DID inputs.
///
/// Mirrors the wallet-node mapping pattern (RFC-0010 F4).
pub fn did_error_to_protocol(e: impl std::fmt::Display) -> ProtocolError {
    ProtocolError::InvalidDid(e.to_string())
}
