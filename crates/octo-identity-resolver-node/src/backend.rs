//! `RemoteResolverBackend` — cross-node resolver backend stub (mission
//! `0871b-cross-node-forwarding` T3).
//!
//! Stub-only at this mission boundary: returns
//! `IdentityResolveError::Unsupported` for every hop so the chain
//! handler fails closed if a `RemoteResolverBackend` is injected before
//! the request/response substrate (mission `0870k-transport-request-response`)
//! lands.
//!
//! When `0870k-transport-request-response` ships, this file grows:
//! - a real transport handle (likely `Arc<NodeTransport>`),
//! - an outbound `IDENTITY_RESOLVE_CHAIN` envelope construction,
//! - correlation via `envelope_id` (request-side `envelope_id` echoed
//!   back as `ChainResolveResponse.envelope_id`),
//! - one `HopSignature` per forwarded hop (RFC-0970 forwarding-hop
//!   pattern; preimage = `BLAKE3-256(canonical_ser((chain_hash,
//!   hop_index, BLAKE3(inner_payload), envelope_id)))`).
//!
//! Layer C trait-object boundary: the backend consumes
//! `Arc<dyn ResolverBackend>` from the chain handler. No coupling to
//! `octo-transport` types in the stub shape — that coupling lands with
//! the substrate.

use std::sync::Arc;

use async_trait::async_trait;

use crate::handlers::{
    chain::{BackendResolveOutcome, ResolverBackend, ResolverChainContext},
    IdentityResolveError, UnsupportedCode,
};

/// Cross-node `ResolverBackend` stub (mission
/// `0871b-cross-node-forwarding` T3 + AC-3).
///
/// `resolve_via` returns `IdentityResolveError::Unsupported` for every
/// hop. The chain handler aborts on the first cross-network hop,
/// preventing silent local-only resolution when a remote backend is
/// injected before mission `0870k-transport-request-response` is
/// implemented.
pub struct RemoteResolverBackend;

impl RemoteResolverBackend {
    /// Construct a new `RemoteResolverBackend` wrapped in an `Arc`.
    #[must_use]
    pub fn arc() -> Arc<dyn ResolverBackend> {
        Arc::new(Self)
    }
}

#[async_trait]
impl ResolverBackend for RemoteResolverBackend {
    async fn resolve_via(
        &self,
        _hop_did: &str,
        _target: &octo_ident::RawDid,
        _chain_ctx: &ResolverChainContext,
    ) -> Result<BackendResolveOutcome, IdentityResolveError> {
        // Fail closed until mission `0870k-transport-request-response`
        // lands the substrate. A real implementation will:
        // 1. Construct an `IDENTITY_RESOLVE_CHAIN` envelope to `hop_did`.
        // 2. Submit via `NodeTransport::send_request(envelope)`.
        // 3. Await the response (correlated by `envelope_id`).
        // 4. Verify the `HopSignature` chain on the response.
        // 5. Return `BackendResolveOutcome { public_key,
        //    signature_chain }`.
        Err(IdentityResolveError::Unsupported(
            UnsupportedCode::RemoteBackendNotWired,
            "RemoteResolverBackend not implemented; mission 0870k-transport-request-response pending"
                .to_owned(),
        ))
    }
}
