//! `IDENTITY_RESOLVE_CHAIN` handler (RFC-0871 §Future Work +
//! RFC-0010 v1.3 §Storage Extension, mission
//! 0871b-cross-domain-resolution-impl).
//!
//! Receives: `<target: String, hops: Vec<ResolverHop>, ttl_remaining_ms: u64>`
//! — a canonical DID lookup target + an ordered list of intermediate
//! resolver hops + a per-chain TTL budget.
//! Returns: `<canonical_did: String, public_key: [u8; 32], hops_traversed: u8>`
//! — the canonical DID wire form + the resolved storage-pubkey form +
//! the number of hops the chain walked before reaching the terminal
//! registry.
//!
//! # Mission 0871b-cross-domain-resolution-impl scope
//!
//! This handler implements the chain-traversal LOGIC substrate:
//!
//! 1. Target DID validation via `CanonicalCodec::parse(s, false)`
//!    (rejects legacy bare form per RFC-0010 v1.2 F4).
//! 2. `ResolverChainContext` initialization: `visited` set seeded with
//!    the target DID; `ttl_remaining_ms` from the request.
//! 3. Hop iteration: each hop
//!    - checks `visited.insert(hop.hop_did)` (cycle detection; abort
//!      `ChainCycle` on collision)
//!    - decrements `ttl_remaining_ms` by `HOP_LATENCY_MS_ESTIMATE` (10 ms
//!      conservative default; abort `ChainTtlExpired` on underflow)
//!    - validates `hop.hop_did` canonical shape (defense-in-depth;
//!      reject `InvalidDid` for malformed hops)
//! 4. Terminal registry call: walks the local `DidRegistry::resolve`
//!    with `target_raw.hash`. Returns `Storage("unknown DID")` for
//!    unregistered / revoked targets (fail-closed; matches the
//!    `ResolveHandler` posture).
//!
//! # Cross-node forwarding (OUT OF SCOPE)
//!
//! The handler does NOT perform cross-node I/O. Real forwarding from
//! hop N to hop N+1 requires an envelope request/response substrate
//! that does not exist in `octo-transport` today (only `broadcast` and
//! `send_best` fire-and-forget are present). The chain-traversal logic
//! landing here is the substrate for a network-capable
//! `ResolveChainHandler` that a follow-on mission will wire once the
//! request/response substrate lands.
//!
//! # Layer discipline
//!
//! Per [[cipherocto-design-principles]]:
//! - `octo-protocol` (Layer A) — `IDENTITY_RESOLVE_CHAIN` payload kind UUID.
//! - `octo-ident` (Layer B) — `DidRegistry` trait (UNCHANGED).
//! - `octo-identity-resolver-node` (Layer C) — handler + chain types
//!   (extension-shaped; no modification to existing handlers).

use std::collections::BTreeSet;
use std::sync::Arc;

use borsh::{BorshDeserialize, BorshSerialize};
use octo_ident::{DidCodec, DidRegistry};

use super::{HandlerOutput, IdentityResolveError};

/// Per-hop latency estimate subtracted from `ttl_remaining_ms`.
///
/// Conservative default of 10 ms per hop — keeps the chain bounded even
/// when individual hop latencies are not measured. A future mission
/// that introduces a real request/response substrate will replace this
/// with measured per-hop latency.
pub const HOP_LATENCY_MS_ESTIMATE: u64 = 10;

/// A single intermediate resolver hop in a chain resolution request.
///
/// Wire form: borsh `(hop_did, hop_transport_hint)`.
///
/// `hop_transport_hint` is opaque bytes — the future request/response
/// substrate interprets them (URL, peer ID, gossip tag, etc.). This
/// handler does NOT consume the hint; it travels untouched through the
/// chain. Per RFC-0871 §Future Work, hop authorization (RFC-0970
/// forwarding-hop signature envelope) lands in a follow-on mission.
#[derive(Clone, Debug, BorshSerialize, BorshDeserialize, PartialEq, Eq)]
pub struct ResolverHop {
    /// Canonical DID wire form (`did:octo:z<base58btc>`) of the next-hop
    /// resolver. Required for cycle detection.
    pub hop_did: String,
    /// Opaque transport hint (URL, peer ID, gossip tag) — interpreted
    /// by the future request/response substrate. Not consumed by this
    /// handler.
    pub hop_transport_hint: Vec<u8>,
}

impl ResolverHop {
    /// Construct a hop with no transport hint (terminal-hop case where
    /// the local resolver-node is the destination).
    #[must_use]
    pub fn local(hop_did: String) -> Self {
        Self {
            hop_did,
            hop_transport_hint: Vec::new(),
        }
    }

    /// Construct a hop with a transport hint.
    #[must_use]
    pub fn with_hint(hop_did: String, hop_transport_hint: Vec<u8>) -> Self {
        Self {
            hop_did,
            hop_transport_hint,
        }
    }
}

/// Chain-traversal context: visited set + TTL budget.
///
/// `visited` is a `BTreeSet<String>` (not `HashSet`) for deterministic
/// ordering — matches the `check_wrapped_chain` cycle-detection pattern
/// in `crates/octo-cap-macaroon/src/macaroon.rs`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ResolverChainContext {
    /// Canonical DID strings already visited in this chain (target +
    /// each successfully-validated hop).
    pub visited: BTreeSet<String>,
    /// Remaining TTL budget in milliseconds.
    pub ttl_remaining_ms: u64,
}

/// Request payload for `IDENTITY_RESOLVE_CHAIN`.
///
/// Wire form: borsh `(target, hops, ttl_remaining_ms)`.
#[derive(Clone, Debug, BorshSerialize, BorshDeserialize, PartialEq, Eq)]
pub struct ChainResolveRequest {
    /// Canonical DID wire form of the resolution target.
    pub target: String,
    /// Ordered list of intermediate hops. Empty list = equivalent to
    /// `IDENTITY_RESOLVE` against the local registry.
    pub hops: Vec<ResolverHop>,
    /// TTL budget for the chain in milliseconds.
    pub ttl_remaining_ms: u64,
}

impl ChainResolveRequest {
    /// Decode from borsh wire form.
    /// # Errors
    /// Returns `IdentityResolveError::Serialization` if borsh decode fails.
    pub fn from_borsh(bytes: &[u8]) -> Result<Self, IdentityResolveError> {
        borsh::from_slice(bytes).map_err(|e| IdentityResolveError::Serialization(e.to_string()))
    }

    /// Encode to borsh wire form.
    /// # Errors
    /// Returns `IdentityResolveError::Serialization` if borsh encode fails.
    pub fn to_borsh(&self) -> Result<Vec<u8>, IdentityResolveError> {
        borsh::to_vec(self).map_err(|e| IdentityResolveError::Serialization(e.to_string()))
    }
}

/// Response payload for `IDENTITY_RESOLVE_CHAIN`.
///
/// Wire form: borsh `(canonical_did, public_key, hops_traversed)`.
#[derive(Clone, Debug, BorshSerialize, BorshDeserialize, PartialEq, Eq)]
pub struct ChainResolveResponse {
    /// Canonical DID wire form (post-parse).
    pub canonical_did: String,
    /// 32-byte storage-pubkey form (from `DidDocument.public_key`).
    pub public_key: [u8; 32],
    /// Number of hops traversed. Capped at `u8::MAX` (`hops.len()` is a
    /// `Vec` length so the realistic ceiling is well below that).
    pub hops_traversed: u8,
}

/// `IDENTITY_RESOLVE_CHAIN` handler.
///
/// Same DI shape as `ResolveHandler`: `Arc<dyn DidRegistry>` injected at
/// construction time. The handler does not depend on the
/// `DidWriteCoordinator` (chain resolution is read-only).
pub struct ResolveChainHandler {
    registry: Arc<dyn DidRegistry>,
}

impl ResolveChainHandler {
    /// Construct a new `ResolveChainHandler` bound to the supplied registry.
    #[must_use]
    pub fn new(registry: Arc<dyn DidRegistry>) -> Self {
        Self { registry }
    }

    /// Walk the resolver chain.
    ///
    /// # Errors
    /// - `IdentityResolveError::InvalidDid` if `target` or any hop is
    ///   not a canonical DID shape (legacy bare form rejected).
    /// - `IdentityResolveError::ChainCycle` if any hop revisits a
    ///   previously-visited canonical DID.
    /// - `IdentityResolveError::ChainTtlExpired` if the TTL budget
    ///   reaches zero before the chain completes.
    /// - `IdentityResolveError::Storage` if the local registry call fails.
    pub fn handle(&self, req: &ChainResolveRequest) -> Result<HandlerOutput, IdentityResolveError> {
        // 1. Validate target canonical DID shape; reject legacy bare form.
        let target_wire = octo_ident::CanonicalCodec::parse(&req.target, false)
            .map_err(|e| IdentityResolveError::InvalidDid(e.to_string()))?;
        let target_raw = octo_ident::CanonicalCodec::wire_to_raw(&target_wire)
            .map_err(|e| IdentityResolveError::InvalidDid(e.to_string()))?;

        // 2. Initialize chain context. Seed `visited` with the target
        //    so a hop that re-uses the target DID triggers cycle detection.
        let mut ctx = ResolverChainContext {
            visited: BTreeSet::from([req.target.clone()]),
            ttl_remaining_ms: req.ttl_remaining_ms,
        };

        // 3. Walk the hop chain. Each hop:
        //    - check visited.insert(hop.hop_did) → ChainCycle on collision
        //    - decrement ttl_remaining_ms → ChainTtlExpired on underflow
        //    - validate hop canonical DID shape → InvalidDid on bad shape
        for hop in &req.hops {
            if !ctx.visited.insert(hop.hop_did.clone()) {
                return Err(IdentityResolveError::ChainCycle);
            }
            ctx.ttl_remaining_ms = ctx.ttl_remaining_ms.saturating_sub(HOP_LATENCY_MS_ESTIMATE);
            if ctx.ttl_remaining_ms == 0 {
                return Err(IdentityResolveError::ChainTtlExpired);
            }
            // Defense-in-depth: reject malformed hops before consuming
            // the registry. The canonical-DID validation is the same
            // call shape used for `target` validation above.
            octo_ident::CanonicalCodec::parse(&hop.hop_did, false)
                .map_err(|e| IdentityResolveError::InvalidDid(e.to_string()))?;
        }

        // 4. After walking all hops, attempt local registry resolve on
        //    the target. The "terminal" hop is the local resolver-node
        //    itself — real cross-node forwarding lands in a follow-on
        //    mission when the request/response substrate is available.
        let doc = self
            .registry
            .resolve(&target_raw.hash)?
            .ok_or_else(|| IdentityResolveError::Storage("unknown DID".to_owned()))?;

        let resp = ChainResolveResponse {
            canonical_did: target_wire.as_str().to_owned(),
            public_key: doc.public_key,
            hops_traversed: u8::try_from(req.hops.len()).unwrap_or(u8::MAX),
        };
        let payload =
            borsh::to_vec(&resp).map_err(|e| IdentityResolveError::Serialization(e.to_string()))?;
        Ok(HandlerOutput::response(
            payload,
            octo_protocol::payload_kind::IDENTITY_RESOLVE_CHAIN,
        ))
    }

    /// Borrow the chain context for inspection (test surface).
    #[cfg(test)]
    #[must_use]
    pub fn context_after_walk(&self, req: &ChainResolveRequest) -> Option<ResolverChainContext> {
        // Test helper: returns the chain context after a successful walk.
        // Caller is expected to invoke `handle` first; this helper is
        // only meaningful in tests where the handler has already
        // validated the request. Returns `None` on validation failure.
        let _ = octo_ident::CanonicalCodec::parse(&req.target, false).ok()?;
        let mut ctx = ResolverChainContext {
            visited: BTreeSet::from([req.target.clone()]),
            ttl_remaining_ms: req.ttl_remaining_ms,
        };
        for hop in &req.hops {
            if !ctx.visited.insert(hop.hop_did.clone()) {
                return None;
            }
            ctx.ttl_remaining_ms = ctx.ttl_remaining_ms.saturating_sub(HOP_LATENCY_MS_ESTIMATE);
            if ctx.ttl_remaining_ms == 0 {
                return None;
            }
            if octo_ident::CanonicalCodec::parse(&hop.hop_did, false).is_err() {
                return None;
            }
        }
        Some(ctx)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use octo_ident::{DidDocument, InMemoryDidRegistry};

    fn sample_did_bytes(seed: u8) -> [u8; 32] {
        let mut k = [0u8; 32];
        for (i, b) in k.iter_mut().enumerate() {
            *b = (i as u8).wrapping_add(seed);
        }
        k
    }

    fn sample_did_str(seed: u8) -> String {
        let pk = sample_did_bytes(seed);
        let raw = octo_ident::CanonicalCodec::mint(&pk);
        let wire = octo_ident::CanonicalCodec::raw_to_wire(&raw).unwrap();
        wire.as_str().to_owned()
    }

    #[test]
    fn resolver_hop_local_helper() {
        let hop = ResolverHop::local("did:octo:z".to_owned());
        assert_eq!(hop.hop_did, "did:octo:z");
        assert!(hop.hop_transport_hint.is_empty());
    }

    #[test]
    fn resolver_hop_with_hint_helper() {
        let hop = ResolverHop::with_hint("did:octo:z".to_owned(), vec![0x01, 0x02, 0x03]);
        assert_eq!(hop.hop_did, "did:octo:z");
        assert_eq!(hop.hop_transport_hint, vec![0x01, 0x02, 0x03]);
    }

    #[test]
    fn chain_resolve_request_borsh_round_trip() {
        let req = ChainResolveRequest {
            target: sample_did_str(1),
            hops: vec![ResolverHop::local(sample_did_str(2))],
            ttl_remaining_ms: 100,
        };
        let bytes = req.to_borsh().unwrap();
        let back = ChainResolveRequest::from_borsh(&bytes).unwrap();
        assert_eq!(back, req);
    }

    #[test]
    fn chain_resolve_response_borsh_round_trip() {
        let resp = ChainResolveResponse {
            canonical_did: sample_did_str(7),
            public_key: [0xAAu8; 32],
            hops_traversed: 3,
        };
        let bytes = borsh::to_vec(&resp).unwrap();
        let back: ChainResolveResponse = borsh::from_slice(&bytes).unwrap();
        assert_eq!(back, resp);
    }

    #[test]
    fn handle_empty_hops_chain_resolves_locally() {
        let registry = Arc::new(InMemoryDidRegistry::default());
        let target_pk = sample_did_bytes(11);
        let target_raw = octo_ident::CanonicalCodec::mint(&target_pk);
        let custom_pubkey = [0xCCu8; 32];
        registry
            .register(
                &target_raw.hash,
                DidDocument {
                    public_key: custom_pubkey,
                    revoked: false,
                    ..Default::default()
                },
            )
            .unwrap();
        let handler = ResolveChainHandler::new(registry);
        let req = ChainResolveRequest {
            target: sample_did_str(11),
            hops: vec![],
            ttl_remaining_ms: 100,
        };
        let out = handler.handle(&req).unwrap();
        let payload = out.response_payload.unwrap();
        let resp: ChainResolveResponse = borsh::from_slice(&payload).unwrap();
        assert_eq!(resp.canonical_did, sample_did_str(11));
        assert_eq!(resp.public_key, custom_pubkey);
        assert_eq!(resp.hops_traversed, 0);
    }

    #[test]
    fn handle_rejects_invalid_target_did() {
        let registry = Arc::new(InMemoryDidRegistry::default());
        let handler = ResolveChainHandler::new(registry);
        let req = ChainResolveRequest {
            target: "did:octo:bad".into(),
            hops: vec![],
            ttl_remaining_ms: 100,
        };
        let err = handler.handle(&req).unwrap_err();
        assert!(matches!(err, IdentityResolveError::InvalidDid(_)));
    }

    #[test]
    fn handle_rejects_legacy_bare_target() {
        let registry = Arc::new(InMemoryDidRegistry::default());
        let handler = ResolveChainHandler::new(registry);
        let legacy = "did:octo:babcdefghijklmnopqrstuvwxyz234567abcdefghijklmnopqrstuvwxyz2345";
        let req = ChainResolveRequest {
            target: legacy.into(),
            hops: vec![],
            ttl_remaining_ms: 100,
        };
        let err = handler.handle(&req).unwrap_err();
        assert!(matches!(err, IdentityResolveError::InvalidDid(_)));
    }
}
