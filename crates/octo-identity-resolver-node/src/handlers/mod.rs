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
    ///
    /// Round-3 review (D5): the discriminant is the operator-dashboard
    /// routing key. The string field carries human-readable detail
    /// (e.g. the pending mission slug) for log correlation. Operator
    /// dashboards route on the `UnsupportedCode` discriminant, NOT on
    /// substring matching the string field (which is fragile across
    /// mission renames / mission merges).
    #[error("unsupported: {1} (code: {0:?})")]
    Unsupported(UnsupportedCode, String),

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

/// Typed discriminant for `IdentityResolveError::Unsupported` (round-3
/// review D5). Operator dashboards route on the discriminant, NOT on
/// substring matching the variant's `String` payload (which is
/// fragile across mission renames / mission merges). Add a variant
/// here when a new `Unsupported`-class failure mode lands.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UnsupportedCode {
    /// Cross-network resolver backend was injected but the
    /// request/response substrate (mission
    /// `0870k-transport-request-response`) is not yet implemented.
    /// The string payload carries the pending mission slug for log
    /// correlation.
    RemoteBackendNotWired,
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
            // modes — no partial resolution was committed. Round-3
            // review (D5): the `UnsupportedCode` discriminant is
            // operator-dashboard-routing data; the `msg` String carries
            // human-readable detail. Map the String to AuthorizationFailed;
            // preserve the discriminant at the variant level (currently
            // discarded at the protocol boundary — see v0.9 follow-on).
            IdentityResolveError::Unsupported(_code, msg) => {
                ProtocolError::AuthorizationFailed(msg)
            }
            // Round-1 review: TTL too large / chain too long failed
            // validation BEFORE any registry call. No state was committed.
            // Round-3 review: preserve the "exceeds MAX_CHAIN_TTL_MS"
            // constant-bound cross-ref from the upstream `#[error(...)]`
            // template so operator dashboards retain the bound name when
            // triaging a DoS burst (asymmetric with `ChainTooLong` which
            // never had a constant-bound cross-ref).
            IdentityResolveError::ChainTtlTooLarge(ms) => {
                ProtocolError::AuthorizationFailed(format!(
                    "chain TTL too large: {ms} ms exceeds MAX_CHAIN_TTL_MS ({} ms)",
                    chain::MAX_CHAIN_TTL_MS
                ))
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

/// Layer-B → Layer-C error bridge. `ResolverBackend::resolve_via`
/// returns `Result<_, ResolverBackendError>` (octo-ident, Layer B);
/// the handler's `?` operator implicitly converts to
/// `IdentityResolveError` via this impl. Round-3 review (D5):
/// the `UnsupportedCode` discriminant is the operator-dashboard
/// routing key — centralized here so all backends route through the
/// same code as the substrate (mission `0870k-transport-request-response`)
/// lands.
impl From<octo_ident::resolver_backend::ResolverBackendError> for IdentityResolveError {
    fn from(e: octo_ident::resolver_backend::ResolverBackendError) -> Self {
        use octo_ident::resolver_backend::ResolverBackendError as R;
        match e {
            // `Unsupported` carries the pending-mission slug for log
            // correlation; the discriminant that operator dashboards
            // route on is centralized at the IdentityResolveError::Unsupported
            // variant (currently `RemoteBackendNotWired`).
            R::Unsupported(msg) => Self::Unsupported(UnsupportedCode::RemoteBackendNotWired, msg),
            // `Backing` is a registry/storage failure — same security class
            // as `DidRegistryError::Storage` (above From impl).
            R::Backing(msg) => Self::Storage(msg),
            // `InvalidInput` is a malformed-hop / context-invariant
            // failure — same security class as `InvalidDid`. The handler
            // has already validated canonical form, so this is a defensive
            // path (e.g. signature payload mismatch from a remote backend).
            R::InvalidInput(msg) => Self::InvalidDid(msg),
        }
    }
}

/// Map `IdentityResolveError` into `ProtocolError` at the dispatch
/// boundary (`IdentityResolverNode::handle_envelope`). Equivalent to the
/// `From` impl above, but named for clarity at call sites.
pub fn resolver_error_to_protocol(e: IdentityResolveError) -> ProtocolError {
    e.into()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Round-3 review (T2): direct test of the
    /// `IdentityResolveError::Unsupported → ProtocolError` mapping.
    /// `remote_backend_stub_is_unsupported` (in `tests/cross_node_chain.rs`)
    /// only pins the variant from `handler.handle()`, not the `From`
    /// impl. This test pins the mapping: the `UnsupportedCode` discriminant
    /// is discarded at the protocol boundary (operator-dashboard routing
    /// remains at the resolver-error variant level), and the human
    /// `String` payload travels through unchanged.
    #[test]
    fn unsupported_maps_to_authorization_failed_preserving_message() {
        let err = IdentityResolveError::Unsupported(
            UnsupportedCode::RemoteBackendNotWired,
            "remote backend not wired".to_owned(),
        );
        let proto: ProtocolError = err.into();
        match proto {
            ProtocolError::AuthorizationFailed(msg) => {
                assert_eq!(
                    msg, "remote backend not wired",
                    "Unsupported mapping preserves the human message"
                );
            }
            other => panic!("expected AuthorizationFailed, got {other:?}"),
        }
    }

    /// Round-3 review (C1): the `ChainTtlTooLarge` mapping re-attaches
    /// the "exceeds MAX_CHAIN_TTL_MS" constant-bound cross-ref so
    /// operator dashboards retain the bound name when triaging a DoS
    /// burst.
    #[test]
    fn chain_ttl_too_large_mapping_preserves_bound_cross_ref() {
        let err = IdentityResolveError::ChainTtlTooLarge(120_000);
        let proto: ProtocolError = err.into();
        match proto {
            ProtocolError::AuthorizationFailed(msg) => {
                assert!(
                    msg.contains("exceeds MAX_CHAIN_TTL_MS"),
                    "ChainTtlTooLarge mapping must retain the bound cross-ref: {msg}"
                );
                assert!(
                    msg.contains("60000 ms"),
                    "ChainTtlTooLarge mapping must include the bound value: {msg}"
                );
            }
            other => panic!("expected AuthorizationFailed, got {other:?}"),
        }
    }

    /// Round-4 review (R1 finding C1.2 + C1.3 + C1.4): direct tests
    /// for the remaining `From<IdentityResolveError> for ProtocolError`
    /// mappings not covered by `unsupported_maps_to_authorization_failed_preserving_message`
    /// or `chain_ttl_too_large_mapping_preserves_bound_cross_ref`. Each
    /// variant is exercised from the unit-test level so the From-impl
    /// (not just the handler-level error construction) is pinned.
    /// Variants `ChainCycle` + `ChainTtlExpired` are also tested at the
    /// handler level (`cross_node_chain.rs::rejects_oversize_*`) but
    /// here we pin the From-impl specifically. `Storage` and the two
    /// `Coordinator*` variants are NOT tested at handler level (they
    /// require coordinator injection + storage backends); the From-impl
    /// unit test is the only place they are surfaced.

    #[test]
    fn coordinator_unavailable_maps_to_authorization_failed() {
        let err = IdentityResolveError::CoordinatorUnavailable("no backend wired".to_owned());
        let proto: ProtocolError = err.into();
        match proto {
            ProtocolError::AuthorizationFailed(msg) => {
                assert!(
                    msg.contains("coordinator unavailable"),
                    "CoordinatorUnavailable mapping must prefix the kind: {msg}"
                );
                assert!(
                    msg.contains("no backend wired"),
                    "CoordinatorUnavailable mapping must preserve the inner message: {msg}"
                );
            }
            other => panic!("expected AuthorizationFailed, got {other:?}"),
        }
    }

    #[test]
    fn coordinator_error_maps_to_authorization_failed() {
        let err = IdentityResolveError::Coordinator("write contested".to_owned());
        let proto: ProtocolError = err.into();
        match proto {
            ProtocolError::AuthorizationFailed(msg) => {
                assert!(
                    msg.contains("coordinator error"),
                    "Coordinator mapping must prefix the kind: {msg}"
                );
                assert!(
                    msg.contains("write contested"),
                    "Coordinator mapping must preserve the inner message: {msg}"
                );
            }
            other => panic!("expected AuthorizationFailed, got {other:?}"),
        }
    }

    #[test]
    fn storage_maps_to_authorization_failed() {
        let err = IdentityResolveError::Storage("disk full".to_owned());
        let proto: ProtocolError = err.into();
        match proto {
            ProtocolError::AuthorizationFailed(msg) => {
                assert_eq!(
                    msg, "disk full",
                    "Storage mapping is pass-through (no prefix)"
                );
            }
            other => panic!("expected AuthorizationFailed, got {other:?}"),
        }
    }

    #[test]
    fn chain_cycle_maps_to_authorization_failed_preserving_message() {
        let err = IdentityResolveError::ChainCycle;
        let proto: ProtocolError = err.into();
        match proto {
            ProtocolError::AuthorizationFailed(msg) => {
                assert_eq!(
                    msg, "resolver chain cycle detected",
                    "ChainCycle mapping must produce the fixed message"
                );
            }
            other => panic!("expected AuthorizationFailed, got {other:?}"),
        }
    }

    #[test]
    fn chain_ttl_expired_maps_to_authorization_failed_preserving_message() {
        let err = IdentityResolveError::ChainTtlExpired;
        let proto: ProtocolError = err.into();
        match proto {
            ProtocolError::AuthorizationFailed(msg) => {
                assert_eq!(
                    msg, "resolver chain TTL expired",
                    "ChainTtlExpired mapping must produce the fixed message"
                );
            }
            other => panic!("expected AuthorizationFailed, got {other:?}"),
        }
    }
}
