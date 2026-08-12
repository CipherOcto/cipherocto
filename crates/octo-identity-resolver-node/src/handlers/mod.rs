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

pub mod chain;
pub mod registration;
pub mod resolve;
pub mod resolve_with_chain;

pub use chain::{
    BackendResolveOutcome, ChainResolveRequest, ChainResolveResponse, LocalResolverBackend,
    ResolveChainHandler, ResolverBackend, ResolverChainContext, ResolverHop,
    HOP_LATENCY_MS_ESTIMATE,
};
pub use registration::{
    RegisterHandler, RegisterRequest, RegisterResponse, RevokeHandler, RevokeRequest,
    RevokeResponse,
};
pub use resolve::{ResolveHandler, ResolveRequest, ResolveResponse};
pub use resolve_with_chain::{
    ResolveWithChainHandler, ResolveWithChainRequest, ResolveWithChainResponse,
};

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

    /// Mission 0871b-cross-domain-resolution-impl: a `ResolverHop`
    /// re-visited a canonical DID already present in the chain's
    /// `visited` set. The handler aborts BEFORE consuming the registry
    /// (no registry I/O was performed for this request).
    #[error("resolver chain cycle detected")]
    ChainCycle,

    /// Mission 0871b-cross-domain-resolution-impl: per-hop TTL budget
    /// (`ttl_remaining_ms`) reached zero before the chain completed.
    /// The handler aborts with no registry call.
    #[error("resolver chain TTL expired")]
    ChainTtlExpired,

    /// Mission 0010-f2-multi-chain-routing: the supplied `ChainId`
    /// literal failed RFC-0010 v1.4 validation (empty, > 64 chars,
    /// or contained a control char). The handler aborts BEFORE any
    /// registry call (fail-closed; no implicit default to mainnet).
    #[error("invalid chain id: {0}")]
    InvalidChainId(String),

    /// Mission `0871b-cross-node-forwarding`: a `RemoteResolverBackend`
    /// was injected but the request/response substrate is not yet
    /// available (mission `0870k-transport-request-response` pending).
    /// The handler aborts with no registry call so cross-network
    /// resolution is never silently downgraded to local-only.
    ///
    /// Mapped to `ProtocolError::AuthorizationFailed` at the dispatch
    /// boundary — same fail-closed security class as the other chain
    /// errors (`ChainCycle`, `ChainTtlExpired`). The chain never reached
    /// the terminal registry, so no partial resolution was committed.
    #[error("unsupported: {0}")]
    Unsupported(String),

    /// Round-1 review (mission `0871b-cross-node-forwarding`): the
    /// request's `ttl_remaining_ms` exceeds `MAX_CHAIN_TTL_MS`. Rejected
    /// at `handle()` entry to prevent denial-of-service via `u64::MAX`
    /// TTL that would bypass per-hop TTL depletion.
    #[error("chain TTL too large: {0} ms exceeds MAX_CHAIN_TTL_MS")]
    ChainTtlTooLarge(u64),

    /// Round-1 review (mission `0871b-cross-node-forwarding`): the
    /// request's `hops.len()` exceeds `u8::MAX`. Rejected at `handle()`
    /// entry because `ChainResolveResponse.hops_traversed: u8` cannot
    /// represent larger chains. The ceiling is `u8::MAX = 255`; chains
    /// that large are a misconfiguration — bound the request instead.
    #[error("chain too long: {0} hops exceeds u8::MAX")]
    ChainTooLong(usize),
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
            // Chain traversal failures are a routing-class error from
            // the caller's perspective — same authorization-class
            // treatment as `Storage`. The chain never reached the
            // terminal registry, so no partial state was committed.
            IdentityResolveError::ChainCycle => {
                ProtocolError::AuthorizationFailed("resolver chain cycle detected".to_owned())
            }
            IdentityResolveError::ChainTtlExpired => {
                ProtocolError::AuthorizationFailed("resolver chain TTL expired".to_owned())
            }
            // Mission 0010-f2-multi-chain-routing: malformed chain_id
            // literal — same authorization-class treatment as invalid
            // DID. No registry call was made.
            IdentityResolveError::InvalidChainId(msg) => {
                ProtocolError::AuthorizationFailed(format!("invalid chain id: {msg}"))
            }
            // Mission 0871b-cross-node-forwarding: cross-network hop
            // requested before the request/response substrate exists.
            // Same authorization-class treatment as the other failure
            // modes — no partial resolution was committed. The
            // upstream `#[error("unsupported: {0}")]` already prefixes
            // with `unsupported: `; do NOT add a second prefix here.
            IdentityResolveError::Unsupported(msg) => ProtocolError::AuthorizationFailed(msg),
            // Round-1 review: TTL too large / chain too long failed
            // validation BEFORE any registry call. No state was committed.
            IdentityResolveError::ChainTtlTooLarge(ms) => {
                ProtocolError::AuthorizationFailed(format!("chain TTL too large: {ms} ms"))
            }
            IdentityResolveError::ChainTooLong(n) => {
                ProtocolError::AuthorizationFailed(format!("chain too long: {n} hops"))
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
