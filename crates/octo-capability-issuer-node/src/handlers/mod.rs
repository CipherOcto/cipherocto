//! Capability-issuer payload handlers (RFC-0871 §Roles and Authorities,
//! mission 0871d-capability-issuer-node).
//!
//! Each handler maps one `CAPABILITY_*` payload kind to its business logic.
//! All handlers route through `EnvelopeDispatcher` for envelope_id dedup
//! and expiry and signature verification (no handler shortcuts, no
//! per-handler HMAC bypass).
//!
//! Layer B boundary: handlers consume `octo-wallet` + `octo-ident` +
//! `octo-cap-macaroon` types and never reach into storage substrate
//! directly. The real macaroon mint + `HolderRegistry` registration
//! substrate (RFC-0957 §Algorithms and RFC-0957-A1 §Data Structures)
//! lands in mission 0957 Phase 2 follow-on.

use octo_protocol::ProtocolError;

pub mod issue;
pub mod lookup;
pub mod revoke;

/// The one `HandlerOutput`, defined once in `octo-protocol`.
///
/// This module used to declare its own copy — four identical ones across
/// the node crates, plus a placeholder in `octo-protocol` that the
/// reference dispatcher echoed into. The placeholder is now the real
/// shape, so the node handlers and the dispatcher agree by construction
/// instead of by four copies having stayed in step by luck.
pub use octo_protocol::HandlerOutput;

pub use issue::{IssueHandler, IssueRequest};
pub use lookup::{CapabilityLookupHandler, CapabilityLookupRequest, CapabilityLookupResponse};
pub use revoke::{RevokeHandler, RevokeRequest, RevokeResponse};

/// Map a `octo_ident::DidError` to a `ProtocolError` for invalid DID inputs.
///
/// Mirrors the wallet-node mapping pattern (RFC-0010 F4).
pub fn did_error_to_protocol(e: impl std::fmt::Display) -> ProtocolError {
    ProtocolError::InvalidDid(e.to_string())
}

/// Map a wallet-side error (e.g. `CapabilityToken::mint` failure in
/// mission 0957 Phase 2) to a `ProtocolError`.
///
/// Phase 3 MVP: reserved for the macaroon substrate landing in
/// mission 0957 Phase 2; the stub handlers don't reach into wallet
/// substrate (they only validate DID shape).
#[allow(dead_code)]
pub fn wallet_error_to_protocol(e: impl std::fmt::Display) -> ProtocolError {
    ProtocolError::AuthorizationFailed(e.to_string())
}
