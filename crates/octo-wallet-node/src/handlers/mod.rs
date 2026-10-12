//! Wallet payload handlers (RFC-0871 §Wallet Node Lifecycle).
//!
//! Each handler maps one `WALLET_*` payload kind to its business logic.
//! All handlers route through `EnvelopeDispatcher` for envelope_id dedup
//! + expiry + signature verification (no handler shortcuts, no per-handler HMAC bypass).
//!
//! Layer B boundary: handlers consume `octo-wallet` + `octo-ident` types
//! and never reach into `ed25519_dalek` directly. Signing always flows
//! through `Arc<dyn HsmAdapter>`.

use octo_protocol::ProtocolError;

pub mod attenuate;
pub mod event_log_producer;
pub mod mint;
pub mod paid_query;
pub mod resolve;
pub mod sign;

/// The one `HandlerOutput`, defined once in `octo-protocol`.
///
/// This module used to declare its own copy — four identical ones across
/// the node crates, plus a placeholder in `octo-protocol` that the
/// reference dispatcher echoed into. The placeholder is now the real
/// shape, so the node handlers and the dispatcher agree by construction
/// instead of by four copies having stayed in step by luck.
pub use octo_protocol::HandlerOutput;

pub use attenuate::{AttenuateHandler, AttenuateRequest};
pub use mint::{MintHandler, MintRequest};
pub use paid_query::{PaidQueryVerifyHandler, PaidQueryVerifyRequest};
pub use resolve::{ResolveDIDHandler, ResolveDIDRequest};
pub use sign::{SignHandler, SignRequest};

/// Convert a `Wallet` error into a `ProtocolError` for handler failures.
///
/// Mapping is shallow: `wallet_error_to_protocol(error)` carries the
/// display string through. Handlers SHOULD preserve enough context
/// (e.g. error in `note`) for callers to diagnose.
pub fn wallet_error_to_protocol(e: impl std::fmt::Display) -> ProtocolError {
    ProtocolError::AuthorizationFailed(e.to_string())
}

/// Map a `octo_ident::DidError` to a `ProtocolError` for invalid DID inputs.
pub fn did_error_to_protocol(e: impl std::fmt::Display) -> ProtocolError {
    ProtocolError::InvalidDid(e.to_string())
}

/// Map a `octo_cap_macaroon::wire::WireError` to a `ProtocolError`
/// for the macaroon wire-form substrate (RFC-0957 §3.7, mission
/// 0957-phase2a). The wire form is byte-string; serialization
/// failures surface as `AuthorizationFailed` so callers don't
/// mistake them for runtime state (the wire form is pure crypto,
/// not envelope state).
pub fn wire_error_to_protocol(e: octo_cap_macaroon::wire::WireError) -> ProtocolError {
    ProtocolError::AuthorizationFailed(format!("capability wire form: {e}"))
}
