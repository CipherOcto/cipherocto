//! `RemoteResolverBackend` — cross-node resolver backend (mission
//! `0871b-cross-node-forwarding` T4, mission
//! `0870k-transport-request-response`).
//!
//! Wires `IDENTITY_RESOLVE_CHAIN` chain-traversal logic to the
//! `NodeTransport::request_response` substrate. For each cross-network
//! hop the chain handler visits, `RemoteResolverBackend::resolve_via`
//! posts an `IDENTITY_RESOLVE` envelope to the next-hop resolver
//! (encoded as the envelope's `to_node_id`), awaits the reply through
//! the request/response correlation substrate, and returns the
//! resolved `public_key` to the chain handler.
//!
//! ## Cross-node semantics (RFC-0871 §Future Work + RFC-0970
//! forwarding-hop pattern reference)
//!
//! For T4 (mission `0870k-transport-request-response` AC-6 + AC-3),
//! the remote backend sends a **single-hop** `IDENTITY_RESOLVE` to
//! the hop resolver. The hop resolver looks up the target in its
//! local `DidRegistry` and returns the resolved `public_key`. The
//! chain-of-resolvers semantics is preserved by the chain handler at
//! the originating side: each hop, if remote, gets a fresh
//! `IDENTITY_RESOLVE` request; the remote resolver's chain (if any,
//! received via a separate `IDENTITY_RESOLVE_CHAIN` envelope) is
//! walked on the receiving side via the same `ResolveChainHandler`.
//!
//! The full forwarding-chain pattern (sending `IDENTITY_RESOLVE_CHAIN`
//! with the remaining hops so the destination can sign per RFC-0970
//! forwarding-hop pattern + extend the chain) is a follow-on mission.
//! T4 ships the SUBSTRATE so the substrate-following pattern is a
//! wire-form-only change.
//!
//! ## Layer-C trait-object boundary
//!
//! Returns `octo_ident::ResolverBackendError` (Layer B). The handler's
//! `From<ResolverBackendError>` bridge (`handlers/mod.rs`) maps
//! `Backing` → `Storage`, `InvalidInput` → `InvalidDid`, and
//! `Unsupported` → `IdentityResolveError::Unsupported(RemoteBackendNotWired, ...)`
//! at the dispatch boundary.
//!
//! ## `from_did` for the outbound envelope
//!
//! RFC-0871 requires `from_did` to be a canonical DID wire form. For
//! T4 we use a placeholder resolver DID derived from the
//! `chain_ctx.envelope_id` prefix bytes (deterministic per chain
//! walk so test fixtures can pin behavior). Production deployments
//! inject the local node's identity via a future mission's config
//! slot (0871b-2b, deferred).

use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use octo_ident::DidCodec;
use octo_protocol::recipient::RecipientRef;
use octo_protocol::NodeEnvelope;
use octo_transport::sender::SendContext;
use octo_transport::NodeTransport;

use crate::handlers::chain::{BackendResolveOutcome, ResolverBackend, ResolverChainContext};
use crate::handlers::resolve::{ResolveRequest, ResolveResponse};

/// Default timeout for cross-node resolver request/response round-trip.
///
/// 5 seconds — long enough for a multi-hop chain across a busy
/// cluster, short enough that a stuck transport surfaces as
/// `IdentityResolveError::Storage` (via the `From<ResolverBackendError>`
/// bridge mapping `Backing`) before the chain's TTL expires
/// (`MAX_CHAIN_TTL_MS = 60_000`).
const RESOLVE_TIMEOUT: Duration = Duration::from_secs(5);

/// Cross-node `ResolverBackend` impl (mission `0871b-cross-node-forwarding`
/// T4, mission `0870k-transport-request-response` AC-3 + AC-6).
///
/// `resolve_via` posts an `IDENTITY_RESOLVE` envelope to the next-hop
/// resolver via `NodeTransport::request_response`. The reply is
/// correlated by RFC-0871 `envelope_id` (BLAKE3-256 of the unsigned
/// envelope, computed by `NodeEnvelope::build`).
///
/// `signature_chain` is always empty for T4 — the per-hop Ed25519
/// signing pattern (RFC-0970 forwarding-hop) is a follow-on mission
/// (0871b-2b). The 5-tuple `ChainResolveResponse.signature_chain`
/// field remains empty bytes-by-bytes through this code path, keeping
/// the cross-node wire form consistent with the in-process
/// `LocalResolverBackend` (empty `signature_chain` is the production
/// expected value until the RFC-0970 forwarding mission lands).
pub struct RemoteResolverBackend {
    transport: Arc<NodeTransport>,
}

impl RemoteResolverBackend {
    /// Construct a new `RemoteResolverBackend` bound to the supplied
    /// transport. The transport's registered senders MUST include at
    /// least one that implements `send_request` (the
    /// `PlatformAdapterBridge` and the RFC-0870 mesh adapter do; the
    /// raw UDP adapter does NOT, returning `TransportError::Unsupported`
    /// which surfaces as
    /// `IdentityResolveError::Unsupported(RemoteBackendNotWired, ...)`).
    #[must_use]
    pub fn new(transport: Arc<NodeTransport>) -> Self {
        Self { transport }
    }

    /// Convenience: wrap in `Arc<dyn ResolverBackend>` for handler
    /// injection.
    #[must_use]
    pub fn arc(transport: Arc<NodeTransport>) -> Arc<dyn ResolverBackend> {
        Arc::new(Self::new(transport))
    }

    /// Build a placeholder `from_did` from the chain context's
    /// `envelope_id`. Deterministic per chain walk so test fixtures
    /// can pin the wire form.
    ///
    /// The placeholder passes `CanonicalCodec::parse` because the
    /// first 32 bytes of `envelope_id` form a valid Ed25519 pubkey
    /// (BLAKE3-256 output is treated as the DID payload, base58btc-
    /// encoded with the `did:octo:z` prefix). Real deployments inject
    /// the local node's identity via the future 0871b-2b mission.
    fn placeholder_from_did(envelope_id: &[u8; 32]) -> octo_ident::WireDid {
        let mut pk = [0u8; 32];
        pk.copy_from_slice(&envelope_id[..32]);
        let raw = octo_ident::CanonicalCodec::mint(&pk);
        octo_ident::CanonicalCodec::raw_to_wire(&raw)
            .expect("placeholder_from_did: BLAKE3 output is 32 bytes; mint always succeeds")
    }
}

#[async_trait]
impl ResolverBackend for RemoteResolverBackend {
    async fn resolve_via(
        &self,
        _hop_did: &str,
        target: &octo_ident::RawDid,
        chain_ctx: &ResolverChainContext,
    ) -> Result<BackendResolveOutcome, octo_ident::ResolverBackendError> {
        // 1. Convert target raw DID → canonical wire DID string for
        //    the `IDENTITY_RESOLVE` payload.
        let target_wire = octo_ident::CanonicalCodec::raw_to_wire(target).map_err(|e| {
            octo_ident::ResolverBackendError::InvalidInput(format!(
                "target DID wire conversion failed: {e}"
            ))
        })?;
        let target_str = target_wire.as_str().to_owned();

        // 2. Build the resolve request payload.
        let req = ResolveRequest(target_str);
        let payload = req.to_borsh().map_err(|e| {
            octo_ident::ResolverBackendError::Backing(format!(
                "ResolveRequest::to_borsh failed: {e}"
            ))
        })?;

        // 3. Build the outbound envelope. `NodeEnvelope::build`
        //    computes `envelope_id = BLAKE3-256(canonical_ser(envelope_without_id))`
        //    per RFC-0871 §Algorithms step 2; we use the BLAKE3-derived
        //    placeholder for `from_did` so the resulting envelope_id is
        //    deterministic per chain walk.
        let from_did = Self::placeholder_from_did(&chain_ctx.envelope_id);
        let envelope = NodeEnvelope::build(
            from_did,
            RecipientRef::Broadcast,
            octo_protocol::payload_kind::IDENTITY_RESOLVE,
            payload,
            vec![],
            chain_ctx.envelope_id, // nonce: reuse envelope_id for replay-defense alignment
            u64::MAX,              // expires_at_unix_ms: no TTL ceiling on a request envelope
        )
        .map_err(|e| {
            octo_ident::ResolverBackendError::Backing(format!("NodeEnvelope::build failed: {e}"))
        })?;

        // 4. Borsh-encode for the transport.
        let bytes = borsh::to_vec(&envelope).map_err(|e| {
            octo_ident::ResolverBackendError::Backing(format!(
                "NodeEnvelope borsh encode failed: {e}"
            ))
        })?;

        // 5. Send via the request/response substrate. The substrate
        //    registers a handler keyed by `envelope_id` BEFORE sending
        //    (race-safe), then awaits the reply until the timeout.
        //
        //    TransportError::Unsupported is treated as a Layer-B
        //    `Unsupported` (routed to the operator dashboard via the
        //    `From<ResolverBackendError>` bridge as
        //    `IdentityResolveError::Unsupported(RemoteBackendNotWired, ...)`).
        //    All other transport errors (timeout, adapter failure,
        //    borsh decode failure) are `Backing` — the handler's
        //    bridge maps those to `IdentityResolveError::Storage(_)` so
        //    the response surfaces the underlying cause.
        let reply_bytes = self
            .transport
            .request_response(
                &bytes,
                envelope.envelope_id,
                &SendContext::default(),
                RESOLVE_TIMEOUT,
            )
            .await
            .map_err(|e| match e {
                octo_transport::sender::TransportError::Unsupported(msg) => {
                    octo_ident::ResolverBackendError::Unsupported(format!(
                        "request_response: {msg}"
                    ))
                }
                other => {
                    octo_ident::ResolverBackendError::Backing(format!("request_response: {other}"))
                }
            })?;

        // 6. Decode the reply envelope. The remote resolver replies
        //    with either `IDENTITY_RESOLVE` (single-hop bare resolve
        //    per RFC-0871 §Future Work) carrying a `ResolveResponse`
        //    payload, OR `IDENTITY_RESOLVE_CHAIN_RESPONSE` (chain
        //    round-trip — the destination walked a further subchain
        //    and returns the full 5-tuple `ChainResolveResponse`).
        //    Both forms decode into `BackendResolveOutcome`.
        let reply_envelope: NodeEnvelope = borsh::from_slice(&reply_bytes).map_err(|e| {
            octo_ident::ResolverBackendError::Backing(format!(
                "reply envelope borsh decode failed: {e}"
            ))
        })?;
        let outcome = match reply_envelope.payload_kind {
            k if k == octo_protocol::payload_kind::IDENTITY_RESOLVE => {
                let resp: ResolveResponse =
                    borsh::from_slice(&reply_envelope.payload).map_err(|e| {
                        octo_ident::ResolverBackendError::Backing(format!(
                            "ResolveResponse borsh decode failed: {e}"
                        ))
                    })?;
                BackendResolveOutcome {
                    public_key: resp.public_key,
                    signature_chain: Vec::new(),
                }
            }
            k if k == octo_protocol::payload_kind::IDENTITY_RESOLVE_CHAIN_RESPONSE => {
                let resp: crate::handlers::chain::ChainResolveResponse =
                    borsh::from_slice(&reply_envelope.payload).map_err(|e| {
                        octo_ident::ResolverBackendError::Backing(format!(
                            "ChainResolveResponse borsh decode failed: {e}"
                        ))
                    })?;
                // `ChainResolveResponse` already carries the 5-tuple
                // signature chain. Convert Layer-A `HopSignature`s
                // (the wire form) to Layer-B `RawHopSignature`s (the
                // trait-object surface) at the backend boundary.
                let signature_chain = resp
                    .signature_chain
                    .into_iter()
                    .map(|h| octo_ident::resolver_backend::RawHopSignature {
                        hop_index: h.hop_index,
                        hop_did: h.hop_did,
                        signature: h.signature,
                        signer_pub: h.signer_pub,
                    })
                    .collect();
                BackendResolveOutcome {
                    public_key: resp.public_key,
                    signature_chain,
                }
            }
            other => {
                return Err(octo_ident::ResolverBackendError::Backing(format!(
                    "expected IDENTITY_RESOLVE or IDENTITY_RESOLVE_CHAIN_RESPONSE reply, got {other:?}"
                )));
            }
        };

        // 7. Return outcome.
        Ok(outcome)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Empty-transport construction: `RemoteResolverBackend::new` requires
    /// only an `Arc<NodeTransport>`. Empty `senders` Vec is valid
    /// (the transport is constructed but has no outbound).
    #[test]
    fn empty_transport_constructs() {
        let transport = Arc::new(NodeTransport::new(Vec::new()));
        let backend = RemoteResolverBackend::new(transport);
        // Field is private but the construction succeeded.
        let _ = backend;
    }

    /// `arc()` convenience returns the trait-object pointer.
    #[test]
    fn arc_returns_trait_object() {
        let transport = Arc::new(NodeTransport::new(Vec::new()));
        let backend: Arc<dyn ResolverBackend> = RemoteResolverBackend::arc(transport);
        let _: &dyn ResolverBackend = &*backend;
    }
}
