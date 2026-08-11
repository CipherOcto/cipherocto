//! Identity-resolver payload handlers (RFC-0871 §Roles and Authorities).
//!
//! Each handler maps one `IDENTITY_*` payload kind to its business logic.
//! All handlers route through `EnvelopeDispatcher` for envelope_id dedup
//! + expiry + signature verification (no handler shortcuts, no per-handler HMAC bypass).
//!
//! Layer B boundary: handlers consume `octo-ident` types and never reach
//! into raw crypto primitives. Validation always flows through
//! `octo_ident::CanonicalCodec::parse(s, false)`.

use octo_ident::DidRegistryError;
use octo_protocol::ProtocolError;

pub mod registration;
pub mod resolve;

pub use registration::{
    RegisterHandler, RegisterRequest, RegisterResponse, RevokeHandler, RevokeRequest,
    RevokeResponse,
};
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
/// Mission 0871b-storage-backend: storage-layer errors tunnel through
/// `Storage(String)` after conversion from `DidRegistryError`. Storage
/// errors are reported as `ProtocolError::AuthorizationFailed` at the
/// dispatch boundary — this matches the fail-closed posture of the rest
/// of the resolver-node (a registry backend error is treated as
/// "cannot authenticate the request", which is the same security class
/// as a signature verification failure).
///
/// Mission 0871e-f7-impl-resolver-mediation: adds `Coordinator` +
/// `CoordinatorUnavailable` for the `DidWriteCoordinator` mediation
/// path. Coordinator errors are reported as `ProtocolError::InvalidDid`
/// at the dispatch boundary — a coordinator failure is treated as
/// "cannot authenticate the request to write", mirroring the storage
/// security class.
#[derive(Debug, thiserror::Error)]
pub enum IdentityResolveError {
    /// Canonical DID validation failed.
    #[error("invalid DID: {0}")]
    InvalidDid(String),

    /// Borsh (de)serialization failure.
    #[error("serialization error: {0}")]
    Serialization(String),

    /// Underlying registry storage failure (tunneled from
    /// `DidRegistryError::Storage`). Treated as authorization-class
    /// error at the dispatch boundary.
    #[error("registry storage error: {0}")]
    Storage(String),

    /// No `DidWriteCoordinator` was configured for the resolver-node
    /// (fail-closed per RFC-0862 v1.3 R12). Operator must inject a
    /// concrete coordinator for writes to succeed.
    #[error("coordinator unavailable: {0}")]
    CoordinatorUnavailable(String),

    /// Coordinator returned a `DidWriteCoordinatorError` (Debug-formatted
    /// for operator observability). Treated as authorization-class
    /// error at the dispatch boundary — the write was not applied to
    /// the local registry.
    #[error("coordinator error: {0}")]
    Coordinator(String),
}

impl From<IdentityResolveError> for ProtocolError {
    fn from(e: IdentityResolveError) -> Self {
        match e {
            IdentityResolveError::InvalidDid(msg) => ProtocolError::InvalidDid(msg),
            IdentityResolveError::Serialization(msg) => ProtocolError::SerializationError(msg),
            IdentityResolveError::Storage(msg) => ProtocolError::AuthorizationFailed(msg),
            // Coordinator failures share the "cannot authenticate the
            // request to write" security class with storage failures.
            // The write was NOT applied; the caller MUST NOT retry
            // without resolving the underlying coordinator error.
            IdentityResolveError::CoordinatorUnavailable(msg) => {
                ProtocolError::AuthorizationFailed(format!("coordinator unavailable: {msg}"))
            }
            IdentityResolveError::Coordinator(msg) => {
                ProtocolError::AuthorizationFailed(format!("coordinator error: {msg}"))
            }
        }
    }
}

impl From<DidRegistryError> for IdentityResolveError {
    fn from(e: DidRegistryError) -> Self {
        match e {
            // `AlreadyRevoked` and `UnknownDid` are unexpected at
            // resolve-time (the registry returns `Ok(None)` for both);
            // tunnel them through `Storage` so they surface as
            // authorization-class failures.
            DidRegistryError::AlreadyRevoked => {
                IdentityResolveError::Storage("registry: AlreadyRevoked".to_owned())
            }
            DidRegistryError::UnknownDid => {
                IdentityResolveError::Storage("registry: UnknownDid".to_owned())
            }
            DidRegistryError::Storage(msg) => IdentityResolveError::Storage(msg),
        }
    }
}

/// Map `IdentityResolveError` into `ProtocolError` at the dispatch
/// boundary (`IdentityResolverNode::handle_envelope`). Equivalent to the
/// `From` impl above, but named for clarity at call sites.
pub fn resolver_error_to_protocol(e: IdentityResolveError) -> ProtocolError {
    e.into()
}
