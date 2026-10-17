//! `ResolverBackend` trait (Layer B; RFC-0871 §Future Work + mission
//! `0871b-cross-node-forwarding` AC-1).
//!
//! Abstracts the resolution mechanism for one hop in a resolver chain.
//! `ResolveChainHandler` (Layer C, `octo-identity-resolver-node`) consumes
//! `Arc<dyn ResolverBackend>` so cross-node hops can be intercepted
//! without leaking Layer C types into Layer B.
//!
//! ## Layer discipline
//!
//! - `octo-ident` (Layer B) — owns the trait + `LocalResolverBackend`
//!   impl + `ResolverBackendError` enum + the supporting context /
//!   outcome / per-hop signature structs.
//! - `octo-identity-resolver-node` (Layer C) — owns `RemoteResolverBackend`
//!   impl (network substrate) + the handler that dispatches through
//!   `Arc<dyn ResolverBackend>` and bridges `ResolverBackendError`
//!   → `IdentityResolveError` via `From`.
//!
//! ## Why no `octo-protocol` dep?
//!
//! The per-hop signature (`RawHopSignature`) mirrors `octo_protocol::HopSignature`
//! wire form BUT lives locally in octo-ident (Layer B). Reason:
//! `octo-protocol → octo-ident` already exists (Layer 1 envelope codec depends
//! on the canonical DID forms), so a reverse `octo-ident → octo-protocol`
//! dep would close a Cargo cycle. The Layer B `RawHopSignature` is the
//! canonical form; the handler converts to `HopSignature` at the wire
//! boundary (Layer C → A is allowed). See `From<RawHopSignature>` impl in
//! `octo-identity-resolver-node` for the bridge.
//!
//! ## F6 (RFC-0871 §Future Work) — LANDED
//!
//! Per AC-1 of mission `0871b-cross-node-forwarding`. The placeholder
//! comment at `crate::registry:49` (F6) is removed in the same commit.
//! Mirror of the `DidWriteCoordinator` topology (Layer B trait, Layer C
//! consumer) — see `crate::write_coordinator` for precedent.

#![forbid(unsafe_code)]

use std::sync::Arc;

use async_trait::async_trait;
use thiserror::Error;

use crate::{DidRegistry, DidRegistryError, RawDid};

/// Per-hop signature carried in a `BackendResolveOutcome.signature_chain`.
///
/// This is the Layer-B canonical form; the Layer-A wire form lives at
/// `octo_protocol::HopSignature`. They are byte-identical on the wire (same
/// fields, same Borsh encoding) so a Layer-C handler can convert with no
/// crypto / encoding roundtrip — see `From<RawHopSignature>` impl in
/// `octo-identity-resolver-node`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RawHopSignature {
    /// Hop index (0 = outermost / requester; increases with each hop).
    pub hop_index: u8,
    /// Canonical DID wire form of the hop that produced this signature.
    pub hop_did: String,
    /// 64-byte Ed25519 signature over the chain_hash preimage.
    pub signature: [u8; 64],
    /// 32-byte Ed25519 verifying key of the signer (RFC-0970 forwarding
    /// pattern — recipient verifies against the resolver-node's published
    /// pubkey).
    pub signer_pub: [u8; 32],
}

/// Per-hop outcome returned by `ResolverBackend::resolve_via`.
///
/// The chain-handler fills in the remaining response fields
/// (`canonical_did`, `hops_traversed`, `envelope_id`); the backend
/// supplies the pubkey + signature chain only.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BackendResolveOutcome {
    /// 32-byte storage-pubkey form (from `DidDocument.public_key`).
    pub public_key: [u8; 32],
    /// Per-hop Ed25519 signatures accumulated by the backend
    /// (outermost-first). Empty for `LocalResolverBackend` (in-process
    /// resolve never signs).
    pub signature_chain: Vec<RawHopSignature>,
}

/// Read-only chain context the backend may inspect.
///
/// The handler has already enforced cycle + TTL bounds before calling
/// `resolve_via`; a remote backend's `chain_hash` / signature preimage
/// construction (RFC-0970 §Data Structures) reads from this context
/// but does not extend it. The struct is therefore `Clone + Debug`
/// only (no mutation API).
#[derive(Clone, Debug)]
pub struct ResolverChainContext {
    /// DIDs already visited in this chain walk (canonical wire form).
    /// Cycle defense lives in the handler; the backend uses this for
    /// observability + chain_hash construction only.
    pub visited: Vec<String>,
    /// Remaining TTL in milliseconds (entry-point capped at
    /// `MAX_CHAIN_TTL_MS`).
    pub ttl_remaining_ms: u64,
    /// BLAKE3-256 of the originating envelope (RFC-0871 §Algorithms
    /// step 2). Required for `chain_hash` preimage construction.
    pub envelope_id: [u8; 32],
    /// Per-hop accumulator (constructed by the handler before
    /// `resolve_via`; read-only for the backend).
    pub chain_hash: [u8; 32],
    /// Current hop index (0 = requester).
    pub hop_index: u8,
}

/// Errors a `ResolverBackend` can surface. Layer C handler converts
/// each variant to `IdentityResolveError` at the boundary.
///
/// Layer B lifetime: this enum is years-stable per
/// CLAUDE.md §crate-level stability (Layer B substrate). Variant
/// additions land via additive RFC.
#[derive(Clone, Debug, Error, PartialEq, Eq)]
pub enum ResolverBackendError {
    /// Feature not yet implemented (e.g. transport request/response
    /// substrate pending — mission `0870k-transport-request-response`).
    /// The string carries the pending mission slug for log correlation.
    /// Operator dashboards route on the `UnsupportedCode` discriminant
    /// after conversion at the Layer C boundary.
    #[error("unsupported: {0}")]
    Unsupported(String),

    /// Backing store / registry error during resolution. Used by
    /// `LocalResolverBackend` when wrapping `DidRegistryError`.
    #[error("backing: {0}")]
    Backing(String),

    /// Malformed input (e.g. invalid `hop_did` shape, invariant on
    /// `chain_ctx` violated). The handler has already validated
    /// canonical form, so this is a defensive / signature-path error.
    #[error("invalid input: {0}")]
    InvalidInput(String),
}

impl From<DidRegistryError> for ResolverBackendError {
    fn from(e: DidRegistryError) -> Self {
        match e {
            // `AlreadyRevoked` and `UnknownDid` are unexpected at
            // resolve-time (the registry returns `Ok(None)`); tunnel
            // them through `Backing` so they surface as a registry
            // failure rather than an input-validation failure.
            DidRegistryError::AlreadyRevoked => Self::Backing("registry: AlreadyRevoked".into()),
            DidRegistryError::UnknownDid => Self::Backing("registry: UnknownDid".into()),
            DidRegistryError::Storage(msg) => Self::Backing(msg),
        }
    }
}

/// Layer B trait: abstracts the resolution mechanism for one hop in a
/// resolver chain. `ResolveChainHandler` (Layer C) consumes
/// `Arc<dyn ResolverBackend>` so cross-node hops can be intercepted.
///
/// Async trait (`#[async_trait]`): the trait signature is `async` so
/// the wire stays stable when mission `0870k-transport-request-response`
/// lands the real `RemoteResolverBackend` that performs network I/O.
/// Adopting `async` from day-1 avoids a breaking trait signature change
/// mid-substrate (Open/Closed: the trait is closed for modification
/// once it ships).
#[async_trait]
pub trait ResolverBackend: Send + Sync {
    /// Resolve `target` at the hop identified by `hop_did`.
    ///
    /// `chain_ctx` is borrowed IMMUTABLY — the backend may inspect
    /// the visited set + remaining TTL (e.g. for `chain_hash`
    /// preimage construction) but MUST NOT mutate them. The handler
    /// has already enforced cycle detection + TTL bounds before this
    /// call.
    async fn resolve_via(
        &self,
        hop_did: &str,
        target: &RawDid,
        chain_ctx: &ResolverChainContext,
    ) -> Result<BackendResolveOutcome, ResolverBackendError>;
}

/// In-process `ResolverBackend` that delegates to the local
/// `DidRegistry`. Mirrors mission `0871b-cross-domain-resolution-impl`
/// behavior (no cross-network I/O, no signing) but exposes the
/// Layer-B-shaped trait object so the chain handler can dispatch
/// through `Arc<dyn ResolverBackend>` without leaking Layer C types.
pub struct LocalResolverBackend(Arc<dyn DidRegistry>);

impl LocalResolverBackend {
    /// Construct a `LocalResolverBackend` over the supplied registry.
    /// Returns `Arc<dyn ResolverBackend>` directly so callers don't
    /// need explicit coercion at the call site.
    #[must_use]
    #[allow(clippy::new_ret_no_self)] // intentional: Arc<dyn Trait> return skips caller coercion
    pub fn new(registry: Arc<dyn DidRegistry>) -> Arc<dyn ResolverBackend> {
        Arc::new(Self(registry))
    }
}

#[async_trait]
impl ResolverBackend for LocalResolverBackend {
    async fn resolve_via(
        &self,
        _hop_did: &str,
        target: &RawDid,
        _chain_ctx: &ResolverChainContext,
    ) -> Result<BackendResolveOutcome, ResolverBackendError> {
        let doc = self
            .0
            .resolve(&target.hash)?
            .ok_or_else(|| ResolverBackendError::Backing("unknown DID".to_owned()))?;
        Ok(BackendResolveOutcome {
            public_key: doc.public_key,
            signature_chain: Vec::new(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{DidCodec, DidDocument, InMemoryDidRegistry};

    fn sample_pubkey(seed: u8) -> [u8; 32] {
        let mut k = [0u8; 32];
        for (i, b) in k.iter_mut().enumerate() {
            *b = (i as u8).wrapping_add(seed);
        }
        k
    }

    fn sample_raw(seed: u8) -> RawDid {
        crate::CanonicalCodec::mint(&sample_pubkey(seed))
    }

    #[tokio::test]
    async fn local_backend_returns_empty_signature_chain() {
        let registry = Arc::new(InMemoryDidRegistry::default());
        let raw = sample_raw(11);
        registry
            .register(
                &raw.hash,
                DidDocument {
                    public_key: sample_pubkey(11),
                    revoked: false,
                    ..Default::default()
                },
            )
            .unwrap();

        let backend = LocalResolverBackend::new(registry);
        let ctx = ResolverChainContext {
            visited: vec!["did:octo:z".into()],
            ttl_remaining_ms: 100,
            envelope_id: [0u8; 32],
            chain_hash: [0u8; 32],
            hop_index: 0,
        };
        let out = backend
            .resolve_via("did:octo:any", &raw, &ctx)
            .await
            .expect("local resolves");
        assert_eq!(out.public_key, sample_pubkey(11));
        assert!(
            out.signature_chain.is_empty(),
            "LocalResolverBackend yields no HopSignature"
        );
    }

    #[tokio::test]
    async fn local_backend_propagates_unknown_did_as_backing_error() {
        let registry = Arc::new(InMemoryDidRegistry::default());
        let backend = LocalResolverBackend::new(registry);
        let ctx = ResolverChainContext {
            visited: vec![],
            ttl_remaining_ms: 100,
            envelope_id: [0u8; 32],
            chain_hash: [0u8; 32],
            hop_index: 0,
        };
        let raw = sample_raw(99);
        let err = backend
            .resolve_via("did:octo:any", &raw, &ctx)
            .await
            .expect_err("unknown DID must surface as Backing");
        assert!(
            matches!(err, ResolverBackendError::Backing(_)),
            "expected Backing, got {err:?}"
        );
    }

    #[test]
    fn local_backend_is_send_sync_static() {
        fn assert_send_sync<T: Send + Sync + ?Sized>() {}
        assert_send_sync::<dyn ResolverBackend>();
        assert_send_sync::<LocalResolverBackend>();
    }

    #[test]
    fn resolver_backend_error_display_covers_all_variants() {
        let _ = ResolverBackendError::Unsupported("x".into()).to_string();
        let _ = ResolverBackendError::Backing("y".into()).to_string();
        let _ = ResolverBackendError::InvalidInput("z".into()).to_string();
    }
}
