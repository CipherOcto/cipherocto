//! Identity-resolver payload handlers (RFC-0871 §Roles and Authorities).
//!
//! Each handler maps one `IDENTITY_*` payload kind to its business logic.
//! All handlers route through `EnvelopeDispatcher` for envelope_id dedup
//! + expiry + signature verification (no handler shortcuts, no per-handler HMAC bypass).
//!
//! Layer B boundary: handlers consume `octo-ident` types and never reach
//! into raw crypto primitives. Validation always flows through
//! `octo_ident::CanonicalCodec::parse(s, false)`.

use octo_protocol::ProtocolError;

pub mod resolve;

pub use resolve::{ResolveHandler, ResolveRequest, ResolveResponse};

/// Output of an identity-resolver handler invocation.
///
/// Either returns a response envelope that the caller (`IdentityResolverNode`)
/// transmits back to the originating peer.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct HandlerOutput {
    /// Optional response envelope to send back to the requester.
    pub response_payload: Option<Vec<u8>>,
    /// Optional response payload kind (RFC-0871 §Response convention).
    pub response_payload_kind: Option<octo_protocol::PayloadKindId>,
    /// Optional human-readable note (for logs; never on wire).
    pub note: Option<String>,
}

impl HandlerOutput {
    /// Empty output (no response, no local effect).
    #[must_use]
    pub fn empty() -> Self {
        Self::default()
    }

    /// Build a response envelope payload + payload kind.
    #[must_use]
    pub fn response(payload: Vec<u8>, payload_kind: octo_protocol::PayloadKindId) -> Self {
        Self {
            response_payload: Some(payload),
            response_payload_kind: Some(payload_kind),
            note: None,
        }
    }

    /// Attach a note (for logs).
    #[must_use]
    pub fn with_note(mut self, note: impl Into<String>) -> Self {
        self.note = Some(note.into());
        self
    }
}

/// Identity-resolver specific errors.
///
/// Phase 1 MVP: surfaces `ProtocolError::InvalidDid` for malformed DID
/// inputs and `ProtocolError::AuthorizationFailed` for borsh / encoding
/// failures. Real backend errors (storage layer, registry not found)
/// land in a follow-on mission once the `DidRegistry` trait is wired.
#[derive(Debug, thiserror::Error)]
pub enum IdentityResolveError {
    /// Canonical DID validation failed.
    #[error("invalid DID: {0}")]
    InvalidDid(String),

    /// Borsh (de)serialization failure.
    #[error("serialization error: {0}")]
    Serialization(String),
}

impl From<IdentityResolveError> for ProtocolError {
    fn from(e: IdentityResolveError) -> Self {
        match e {
            IdentityResolveError::InvalidDid(msg) => ProtocolError::InvalidDid(msg),
            IdentityResolveError::Serialization(msg) => ProtocolError::SerializationError(msg),
        }
    }
}

/// Map `IdentityResolveError` into `ProtocolError` at the dispatch
/// boundary (`IdentityResolverNode::handle_envelope`). Equivalent to the
/// `From` impl above, but named for clarity at call sites.
pub fn resolver_error_to_protocol(e: IdentityResolveError) -> ProtocolError {
    e.into()
}
