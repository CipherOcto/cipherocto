//! `IDENTITY_RESOLVE_CHAIN` handler (RFC-0871 §Future Work +
//! RFC-0010 v1.3 §Storage Extension, missions
//! `0871b-cross-domain-resolution-impl` + `0871b-cross-node-forwarding`).
//!
//! Receives: `<target: String, hops: Vec<ResolverHop>, ttl_remaining_ms: u64>`
//! — a canonical DID lookup target + an ordered list of intermediate
//! resolver hops + a per-chain TTL budget.
//! Returns: 5-tuple `(canonical_did: String, public_key: [u8; 32],
//! hops_traversed: u8, signature_chain: Vec<HopSignature>,
//! envelope_id: [u8; 32])` per mission `0871b-cross-node-forwarding`,
//! following RFC-0871 §Algorithms step 2 `envelope_id` semantics.
//!
//! # Mission 0871b-cross-domain-resolution-impl scope (LOGIC substrate)
//!
//! This handler implements the chain-traversal LOGIC substrate:
//!
//! 0. Entry-point bounds (round-1 review): reject
//!    `ttl_remaining_ms > MAX_CHAIN_TTL_MS` as `ChainTtlTooLarge` (DoS
//!    defense against `u64::MAX` TTL bypass) and
//!    `hops.len() > MAX_CHAIN_HOPS` as `ChainTooLong` (silent u8-cap
//!    smell — `hops_traversed: u8` cannot represent larger chains).
//! 1. Target DID validation via `CanonicalCodec::parse(s, false)`
//!    (rejects legacy bare form per RFC-0010 v1.2 F4).
//! 2. `ResolverChainContext` initialization: `visited` set seeded with
//!    the canonical (post-parse) target wire form; `ttl_remaining_ms`
//!    from the request.
//! 3. Hop iteration (round-1 review: canonicalize FIRST):
//!    - canonicalize `hop.hop_did` via `CanonicalCodec::parse` (reject
//!      `InvalidDid` for malformed hops — NO state consumed on failure)
//!    - `visited.insert(canonical_hop_did)` → `ChainCycle` on collision
//!    - `ttl_remaining_ms = ttl_remaining_ms.saturating_sub(HOP_LATENCY_MS_ESTIMATE)`
//!      → `ChainTtlExpired` on underflow to zero
//! 4. Backend delegation: `self.backend.resolve_via(terminal_hop_did,
//!    target_raw, &ctx).await` — `LocalResolverBackend` calls the local
//!    `DidRegistry::resolve`; `RemoteResolverBackend` will perform
//!    network I/O (once mission `0870k-transport-request-response`
//!    lands). Returns `Storage("unknown DID")` for unregistered /
//!    revoked targets (fail-closed; matches the `ResolveHandler`
//!    posture).
//!
//! # Cross-node forwarding
//!
//! Mission `0871b-cross-node-forwarding` introduces the async
//! `ResolverBackend` trait so the chain walk can intercept
//! cross-network hops. `LocalResolverBackend` wraps
//! `DidRegistry::resolve`; `RemoteResolverBackend` (in
//! `octo-identity-resolver-node/src/backend.rs`) returns
//! `IdentityResolveError::Unsupported` until the request/response
//! substrate (mission `0870k-transport-request-response`) lands.
//!
//! # Layer discipline
//!
//! Per [[cipherocto-design-principles]]:
//! - `octo-protocol` (Layer A) — `IDENTITY_RESOLVE_CHAIN` payload kind UUID + `HopSignature`.
//! - `octo-ident` (Layer B) — `DidRegistry` trait (UNCHANGED).
//! - `octo-identity-resolver-node` (Layer C) — handler + chain types
//!   (extension-shaped; no modification to existing handlers).

use std::sync::Arc;

use borsh::{BorshDeserialize, BorshSerialize};
use octo_ident::{DidCodec, DidRegistry};
use octo_protocol::HopSignature;

use super::{HandlerOutput, IdentityResolveError};

/// Per-hop latency estimate subtracted from `ttl_remaining_ms`.
///
/// Optimistic default of 10 ms per hop — keeps the chain bounded even
/// when individual hop latencies are not measured. Real cross-network
/// hops (50–200 ms RTT) will exceed this; the bound is loose enough
/// that any practical chain stays well under `MAX_CHAIN_TTL_MS`. A
/// future mission that introduces a real request/response substrate
/// will replace this with measured per-hop latency.
pub const HOP_LATENCY_MS_ESTIMATE: u64 = 10;

/// Maximum `ttl_remaining_ms` value accepted at `handle()` entry.
///
/// Defense against denial-of-service via `u64::MAX` TTL that would
/// bypass per-hop TTL depletion. Set to 60 seconds — 6_000 hops at
/// the optimistic 10 ms estimate, plus ample reserve for
/// `IDENTITY_RESOLVE_CHAIN` envelope transport + verification
/// overhead.
pub const MAX_CHAIN_TTL_MS: u64 = 60_000;

/// Maximum `hops.len()` accepted at `handle()` entry.
///
/// Wire-format invariant bound: `ChainResolveResponse.hops_traversed`
/// is `u8`, so chains above `u8::MAX` hops cannot be faithfully
/// represented in the response. Rejected as `ChainTooLong` rather
/// than silently capped.
pub const MAX_CHAIN_HOPS: usize = u8::MAX as usize;

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

// NOTE: `ResolverChainContext` re-exported from Layer B
// (`octo_ident::resolver_backend`, mission 0871b-cross-node-forwarding
// AC-1). The local struct that previously lived here has been removed
// and the handler body now uses the Layer-B shape (Vec<String> visited
// + envelope_id + chain_hash + hop_index fields). Cycle detection
// remains a `Vec::contains` check — the field-order determinism that
// `BTreeSet` provided is no longer needed after the Handler-vs-Backend
// split (handler owns the visited set; backend reads it for
// chain_hash preimage construction only).

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
/// Wire form: borsh `(canonical_did, public_key, hops_traversed,
/// signature_chain, envelope_id)`.
///
/// Mission `0871b-cross-node-forwarding` extends the original
/// 3-tuple (RFC-0871 §Future Work + mission
/// `0871b-cross-domain-resolution-impl`) with two new fields that bind
/// cross-network chain integrity:
///
/// - `signature_chain`: per-hop Ed25519 signatures, outermost-first.
///   Empty when `ResolverBackend` is `LocalResolverBackend` (single-hop
///   local resolve; no signing needed).
/// - `envelope_id`: BLAKE3-256 of the originating
///   `IDENTITY_RESOLVE_CHAIN` envelope per RFC-0871 §Algorithms step 2.
///   Replay defense: the requester binds the chain to the envelope it
///   sent.
#[derive(Clone, Debug, BorshSerialize, BorshDeserialize, PartialEq, Eq)]
pub struct ChainResolveResponse {
    /// Canonical DID wire form (post-parse).
    pub canonical_did: String,
    /// 32-byte storage-pubkey form (from `DidDocument.public_key`).
    pub public_key: [u8; 32],
    /// Number of hops traversed. Capped at `u8::MAX` (`hops.len()` is a
    /// `Vec` length so the realistic ceiling is well below that).
    pub hops_traversed: u8,
    /// Per-hop Ed25519 signature chain, outermost-first. Empty for
    /// in-process / local-registry resolves; populated by cross-network
    /// hops via `RemoteResolverBackend`.
    pub signature_chain: Vec<HopSignature>,
    /// `envelope_id` of the originating `IDENTITY_RESOLVE_CHAIN`
    /// envelope (RFC-0871 §Algorithms step 2). Replay defense + request
    /// correlation. The handler threads this through from the dispatch
    /// site but does NOT verify the value — the caller MUST supply the
    /// genuine BLAKE3-256 envelope_id; passing `[0u8; 32]` silently
    /// would defeat replay defense.
    ///
    /// Wire form: borsh `(canonical_did, public_key, hops_traversed,
    /// signature_chain, envelope_id)` (5-tuple).
    pub envelope_id: [u8; 32],
}

/// `IDENTITY_RESOLVE_CHAIN` handler.
///
/// Mission `0871b-cross-node-forwarding` migrates the DI shape from
/// `Arc<dyn DidRegistry>` to `Arc<dyn ResolverBackend>` so the chain
/// walk can intercept cross-network hops. The `LocalResolverBackend`
/// wrapper preserves the in-process behavior (a no-op shim around
/// `DidRegistry::resolve`); `RemoteResolverBackend` carries the
/// request/response substrate payload once it lands.
///
/// The handler does not depend on the `DidWriteCoordinator` (chain
/// resolution is read-only).
pub struct ResolveChainHandler {
    backend: Arc<dyn ResolverBackend>,
}

impl ResolveChainHandler {
    /// Construct a new `ResolveChainHandler` bound to the supplied
    /// `ResolverBackend`. Primary constructor (mission
    /// `0871b-cross-node-forwarding` AC-7).
    #[must_use]
    pub fn new(backend: Arc<dyn ResolverBackend>) -> Self {
        Self { backend }
    }

    /// Back-compat constructor: wrap an `Arc<dyn DidRegistry>` in a
    /// `LocalResolverBackend` so existing callers (7 in-process
    /// chain-traversal TV in `tests/cross_domain_chain.rs` + the
    /// in-file tests below) keep compiling. Mission
    /// `0871b-cross-node-forwarding` AC-7.
    #[must_use]
    pub fn new_local(registry: Arc<dyn DidRegistry>) -> Self {
        Self {
            backend: LocalResolverBackend::new(registry),
        }
    }

    /// Walk the resolver chain.
    ///
    /// # Errors
    /// - `IdentityResolveError::ChainTtlTooLarge` if `ttl_remaining_ms`
    ///   exceeds `MAX_CHAIN_TTL_MS` (entry-point DoS bound).
    /// - `IdentityResolveError::ChainTooLong` if `hops.len()` exceeds
    ///   `MAX_CHAIN_HOPS` (entry-point wire-format bound).
    /// - `IdentityResolveError::InvalidDid` if `target` or any hop is
    ///   not a canonical DID shape (legacy bare form rejected; NO state
    ///   consumed on failure — canonicalization happens BEFORE cycle
    ///   insert / TTL decrement).
    /// - `IdentityResolveError::ChainCycle` if any hop revisits a
    ///   previously-visited canonical DID.
    /// - `IdentityResolveError::ChainTtlExpired` if the TTL budget
    ///   reaches zero before the chain completes.
    /// - `IdentityResolveError::Storage` if the backend call fails.
    ///
    /// Mission `0871b-cross-node-forwarding`: `envelope_id` is the
    /// RFC-0871 correlation key (BLAKE3-256 of the unsigned envelope)
    /// threaded in from the dispatch site so the response can bind
    /// back to the originating request envelope (replay defense).
    pub async fn handle(
        &self,
        req: &ChainResolveRequest,
        envelope_id: [u8; 32],
    ) -> Result<HandlerOutput, IdentityResolveError> {
        // 0. Bound TTL at entry (DoS defense — `u64::MAX` would bypass
        //    per-hop TTL depletion).
        if req.ttl_remaining_ms > MAX_CHAIN_TTL_MS {
            return Err(IdentityResolveError::ChainTtlTooLarge(req.ttl_remaining_ms));
        }
        // 0b. Bound hop count (silent u8-cap smell — wire-format
        //     `hops_traversed: u8` cannot represent larger chains).
        if req.hops.len() > MAX_CHAIN_HOPS {
            return Err(IdentityResolveError::ChainTooLong(req.hops.len()));
        }

        // 1. Validate target canonical DID shape; reject legacy bare form.
        let target_wire = octo_ident::CanonicalCodec::parse(&req.target, false)
            .map_err(|e| IdentityResolveError::InvalidDid(e.to_string()))?;
        let target_raw = octo_ident::CanonicalCodec::wire_to_raw(&target_wire)
            .map_err(|e| IdentityResolveError::InvalidDid(e.to_string()))?;

        // 2. Initialize chain context. Seed `visited` with the
        //    post-parse canonical target wire form so a hop that re-uses
        //    the target DID triggers cycle detection (regardless of the
        //    input wire form the caller used). `chain_hash` is
        //    initialized to the BLAKE3-256 of the canonical target DID
        //    (entry-point accumulator seed — handler extends it with
        //    each hop's contribution before `resolve_via`; the
        //    `RemoteResolverBackend` then reads it for its Ed25519
        //    preimage construction per RFC-0970 §Data Structures).
        // 2. Initialize chain context. Seed `visited` with the
        //    post-parse canonical target wire form so a hop that re-uses
        //    the target DID triggers cycle detection (regardless of the
        //    input wire form the caller used). `chain_hash` is the
        //    per-hop accumulator — seeded to all-zero at entry, the
        //    handler extends it with each hop's contribution before
        //    `resolve_via`; the `RemoteResolverBackend` reads it for
        //    its Ed25519 preimage construction per RFC-0970 §Data
        //    Structures. `envelope_id` is threaded through from the
        //    dispatch site (handler parameter) so the backend can
        //    include it in the reply signature preimage (RFC-0871
        //    §Algorithms step 2).
        let mut ctx = ResolverChainContext {
            visited: vec![target_wire.as_str().to_owned()],
            ttl_remaining_ms: req.ttl_remaining_ms,
            envelope_id,
            chain_hash: [0u8; 32],
            hop_index: 0,
        };

        // 3. Walk the hop chain. Each hop:
        //    - canonicalize hop DID FIRST (the `let hop_canonical =` /
        //      `let hop_wire =` bindings are local-stack; nothing
        //      observable mutates on `InvalidDid` failure)
        //    - decrement TTL, then check (round-4 R3 MEDIUM: this is
        //      decrement-then-check, not check-then-decrement; the
        //      symmetry with `InvalidDid` holds at the OBSERVABLE level
        //      — `ctx` is `let mut ctx = ...` inside `handle()`, dropped
        //      on any `Err` return, so no observer sees the transient
        //      decrement on the failing hop). Round-3 C2 still holds
        //      by virtue of local-only `ctx` lifetime.
        //    - visited.insert(canonical_hop_did) → ChainCycle
        //      (cycle check happens AFTER canonicalize + TTL check so
        //      a malformed or TTL-depleted hop never lands in `visited`)
        //    - capture canonical form for the terminal-hop handoff
        let mut terminal_hop_canonical: String = target_wire.as_str().to_owned();
        for hop in &req.hops {
            let hop_wire = octo_ident::CanonicalCodec::parse(&hop.hop_did, false)
                .map_err(|e| IdentityResolveError::InvalidDid(e.to_string()))?;
            let hop_canonical = hop_wire.as_str().to_owned();
            ctx.ttl_remaining_ms = ctx.ttl_remaining_ms.saturating_sub(HOP_LATENCY_MS_ESTIMATE);
            if ctx.ttl_remaining_ms == 0 {
                return Err(IdentityResolveError::ChainTtlExpired);
            }
            // Layer-B `ResolverChainContext.visited` is a `Vec<String>`,
            // not a `BTreeSet` (was a `BTreeSet` in the previous Layer-C
            // local definition — re-exported from `octo-ident` for cycle
            // detection + chain_hash observability). Cycle defense is
            // `contains` (linear scan, O(n) per hop — bounded by
            // MAX_CHAIN_HOPS so amortized cost is trivial).
            if ctx.visited.contains(&hop_canonical) {
                return Err(IdentityResolveError::ChainCycle);
            }
            ctx.visited.push(hop_canonical.clone());
            ctx.hop_index = ctx.hop_index.saturating_add(1);
            terminal_hop_canonical = hop_canonical;
        }

        // 4. Delegate to the injected `ResolverBackend`. The local
        //    backend calls `DidRegistry::resolve`; a remote backend
        //    will perform network I/O (once the request/response
        //    substrate lands).
        //
        //    `terminal_hop_canonical`:
        //    - `target_wire` when `req.hops` is empty (direct local
        //      resolve — backend may ignore)
        //    - canonical form of the LAST successful hop when non-empty
        //      (next-hop envelope recipient for a remote backend)
        //
        //    `target_raw` is the canonical-form-decoded target the
        //    terminal registry resolves against.
        let backend_outcome = self
            .backend
            .resolve_via(&terminal_hop_canonical, &target_raw, &ctx)
            .await?;

        // 5. Assemble the 5-tuple response (mission
        //    `0871b-cross-node-forwarding` T5). `LocalResolverBackend`
        //    returns an empty `signature_chain`; `RemoteResolverBackend`
        //    populates it with one `HopSignature` per cross-network hop.
        //    Convert Layer-B `RawHopSignature` → Layer-A `HopSignature`
        //    (byte-identical wire form) at the handler boundary; per
        //    `resolver_backend.rs` §"Why no `octo-protocol` dep?".
        //
        //    `hops_traversed` capped at `handle()` entry (see ChainTooLong
        //    check above), so the `try_from` unwrap is safe.
        let resp = ChainResolveResponse {
            canonical_did: target_wire.as_str().to_owned(),
            public_key: backend_outcome.public_key,
            hops_traversed: u8::try_from(req.hops.len())
                .expect("hops.len() bounded <= MAX_CHAIN_HOPS at handle() entry"),
            signature_chain: backend_outcome
                .signature_chain
                .into_iter()
                .map(|raw| {
                    HopSignature::new(raw.hop_index, raw.hop_did, raw.signature, raw.signer_pub)
                })
                .collect(),
            envelope_id,
        };
        let payload =
            borsh::to_vec(&resp).map_err(|e| IdentityResolveError::Serialization(e.to_string()))?;
        Ok(HandlerOutput::response(
            payload,
            octo_protocol::payload_kind::IDENTITY_RESOLVE_CHAIN,
        ))
    }
}

// --- ResolverBackend trait re-export from Layer B ----------------
//
// Mission `0871b-cross-node-forwarding` AC-1 (Layer C → Layer B
// relocation): the trait + `LocalResolverBackend` impl were moved to
// `octo-ident/src/resolver_backend.rs` in this commit. The Layer C
// handler re-exports the trait surface so internal callers + tests
// keep their existing import paths byte-identical.
//
// `RemoteResolverBackend` (Layer C, `octo-identity-resolver-node/src/backend.rs`)
// carries the request/response substrate payload once it lands; the
// `From<octo_ident::ResolverBackendError>` impl in `handlers/mod.rs`
// bridges the new Layer-B error type into the existing
// `IdentityResolveError` taxonomy.

pub use octo_ident::resolver_backend::{
    BackendResolveOutcome, LocalResolverBackend, RawHopSignature, ResolverBackend,
    ResolverBackendError, ResolverChainContext,
};

#[cfg(test)]
mod tests {
    use super::*;
    use async_trait::async_trait;
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
            signature_chain: Vec::new(),
            envelope_id: [0x42u8; 32],
        };
        let bytes = borsh::to_vec(&resp).unwrap();
        let back: ChainResolveResponse = borsh::from_slice(&bytes).unwrap();
        assert_eq!(back, resp);
    }

    #[tokio::test]
    async fn handle_empty_hops_chain_resolves_locally() {
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
        let handler = ResolveChainHandler::new_local(registry);
        let req = ChainResolveRequest {
            target: sample_did_str(11),
            hops: vec![],
            ttl_remaining_ms: 100,
        };
        let out = handler.handle(&req, [0u8; 32]).await.unwrap();
        let payload = out.response_payload.unwrap();
        let resp: ChainResolveResponse = borsh::from_slice(&payload).unwrap();
        assert_eq!(resp.canonical_did, sample_did_str(11));
        assert_eq!(resp.public_key, custom_pubkey);
        assert_eq!(resp.hops_traversed, 0);
        assert_eq!(resp.envelope_id, [0u8; 32]);
    }

    #[tokio::test]
    async fn handle_rejects_invalid_target_did() {
        let registry = Arc::new(InMemoryDidRegistry::default());
        let handler = ResolveChainHandler::new_local(registry);
        let req = ChainResolveRequest {
            target: "did:octo:bad".into(),
            hops: vec![],
            ttl_remaining_ms: 100,
        };
        let err = handler.handle(&req, [0u8; 32]).await.unwrap_err();
        assert!(matches!(err, IdentityResolveError::InvalidDid(_)));
    }

    #[tokio::test]
    async fn handle_rejects_legacy_bare_target() {
        let registry = Arc::new(InMemoryDidRegistry::default());
        let handler = ResolveChainHandler::new_local(registry);
        let legacy = "did:octo:babcdefghijklmnopqrstuvwxyz234567abcdefghijklmnopqrstuvwxyz2345";
        let req = ChainResolveRequest {
            target: legacy.into(),
            hops: vec![],
            ttl_remaining_ms: 100,
        };
        let err = handler.handle(&req, [0u8; 32]).await.unwrap_err();
        assert!(matches!(err, IdentityResolveError::InvalidDid(_)));
    }

    // -- Mission 0871b-cross-node-forwarding AC-9..AC-12 --

    /// AC-9: `ChainResolveResponse` 5-tuple with non-empty
    /// `signature_chain` survives borsh round-trip.
    #[test]
    fn chain_response_with_hop_signature_round_trip() {
        let sig1 = octo_protocol::HopSignature::new(0, sample_did_str(99), [0xAA; 64], [0xBB; 32]);
        let sig2 = octo_protocol::HopSignature::new(1, sample_did_str(98), [0xCC; 64], [0xDD; 32]);
        let resp = ChainResolveResponse {
            canonical_did: sample_did_str(7),
            public_key: [0xAAu8; 32],
            hops_traversed: 2,
            signature_chain: vec![sig1, sig2],
            envelope_id: [0x42u8; 32],
        };
        let bytes = borsh::to_vec(&resp).expect("serialize");
        let back: ChainResolveResponse = borsh::from_slice(&bytes).expect("deserialize");
        assert_eq!(back, resp);
        assert_eq!(back.signature_chain.len(), 2);
    }

    /// AC-10: `ResolveChainHandler` consults the injected backend.
    /// Construct with `new(backend)`; a `SpyBackend` records whether
    /// `resolve_via` was called.
    #[tokio::test]
    async fn chain_handler_uses_injected_backend() {
        let registry = Arc::new(InMemoryDidRegistry::default());
        let target_pk = sample_did_bytes(11);
        let target_raw = octo_ident::CanonicalCodec::mint(&target_pk);
        registry
            .register(
                &target_raw.hash,
                DidDocument {
                    public_key: [0xCCu8; 32],
                    revoked: false,
                    ..Default::default()
                },
            )
            .unwrap();
        let local = LocalResolverBackend::new(registry.clone());
        let spy = Arc::new(SpyBackend::new(local.clone() as Arc<dyn ResolverBackend>));
        let handler = ResolveChainHandler::new(spy.clone() as Arc<dyn ResolverBackend>);
        let req = ChainResolveRequest {
            target: sample_did_str(11),
            hops: vec![],
            ttl_remaining_ms: 100,
        };
        let out = handler
            .handle(&req, [0u8; 32])
            .await
            .expect("spy delegates");
        assert_eq!(spy.calls(), 1, "backend consulted exactly once");
        let payload = out.response_payload.unwrap();
        let resp: ChainResolveResponse = borsh::from_slice(&payload).unwrap();
        assert_eq!(resp.public_key, [0xCCu8; 32]);
    }

    /// AC-11: `HopSignature` carries the four required fields and
    /// round-trips the borsh wire form. Test pins the structural fields
    /// only — no real Ed25519 sign/verify (real sign/verify lands with
    /// mission `0870k-transport-request-response`).
    #[test]
    fn hop_signature_struct_fields_and_borsh_round_trip() {
        let sig = octo_protocol::HopSignature::new(
            2,
            "did:octo:zCt5bENb7tA2b9xeamSEnHF7cZ6Kk8h9p2Z6nT8pVk9R".to_owned(),
            [0x55; 64],
            [0x66; 32],
        );
        assert_eq!(sig.hop_index, 2);
        assert_eq!(
            sig.hop_did,
            "did:octo:zCt5bENb7tA2b9xeamSEnHF7cZ6Kk8h9p2Z6nT8pVk9R"
        );
        assert_eq!(sig.signature, [0x55u8; 64]);
        assert_eq!(sig.signer_pub, [0x66u8; 32]);
        let bytes = borsh::to_vec(&sig).expect("serialize");
        let back: octo_protocol::HopSignature = borsh::from_slice(&bytes).expect("deserialize");
        assert_eq!(back, sig);
    }

    /// AC-12: handler propagates `envelope_id` into the response's
    /// 5-tuple `envelope_id` field (RFC-0871 correlation key,
    /// replay defense).
    #[tokio::test]
    async fn chain_handler_propagates_envelope_id() {
        let registry = Arc::new(InMemoryDidRegistry::default());
        let target_pk = sample_did_bytes(11);
        let target_raw = octo_ident::CanonicalCodec::mint(&target_pk);
        registry
            .register(
                &target_raw.hash,
                DidDocument {
                    public_key: [0xCCu8; 32],
                    revoked: false,
                    ..Default::default()
                },
            )
            .unwrap();
        let handler = ResolveChainHandler::new_local(registry);
        let envelope_id = [0x42u8; 32];
        let req = ChainResolveRequest {
            target: sample_did_str(11),
            hops: vec![],
            ttl_remaining_ms: 100,
        };
        let out = handler
            .handle(&req, envelope_id)
            .await
            .expect("local resolves");
        let payload = out.response_payload.unwrap();
        let resp: ChainResolveResponse = borsh::from_slice(&payload).unwrap();
        assert_eq!(resp.envelope_id, envelope_id);
    }

    /// Round-3 review (C4): Send-future inference for
    /// `pub trait ResolverBackend: Send` is a macro convention that
    /// has shifted between minor releases of `async-trait`. Pin the
    /// Send-bound by moving the trait-object future across a
    /// `tokio::spawn` boundary (which requires the future to be Send).
    /// Production cross-thread dispatch via
    /// `NodeTransport::register_receiver → on_receive` exercises this
    /// path; the multi_thread flavor + tokio::spawn is a faithful
    /// compile-time smoke test.
    #[tokio::test(flavor = "multi_thread")]
    async fn resolver_backend_send_across_thread_boundary() {
        let registry = Arc::new(InMemoryDidRegistry::default());
        let target_pk = sample_did_bytes(11);
        let target_raw = octo_ident::CanonicalCodec::mint(&target_pk);
        registry
            .register(
                &target_raw.hash,
                DidDocument {
                    public_key: [0xCCu8; 32],
                    revoked: false,
                    ..Default::default()
                },
            )
            .unwrap();
        let backend: Arc<dyn ResolverBackend> = LocalResolverBackend::new(registry);
        let handler = ResolveChainHandler::new(backend);
        let req = ChainResolveRequest {
            target: sample_did_str(11),
            hops: vec![],
            ttl_remaining_ms: 100,
        };
        // tokio::spawn requires the future to be Send — the
        // async-trait macro must add `+ Send` to the boxed future
        // returned by `resolve_via` since `ResolverBackend: Send`.
        let join = tokio::spawn(async move { handler.handle(&req, [0u8; 32]).await });
        let out = join
            .await
            .expect("task should not panic")
            .expect("resolves");
        let payload = out.response_payload.unwrap();
        let resp: ChainResolveResponse = borsh::from_slice(&payload).unwrap();
        assert_eq!(resp.public_key, [0xCCu8; 32]);
    }

    // --- SpyBackend: counts calls + delegates to wrapped backend ---

    struct SpyBackend(Arc<dyn ResolverBackend>, std::sync::atomic::AtomicUsize);

    impl SpyBackend {
        fn new(inner: Arc<dyn ResolverBackend>) -> Self {
            Self(inner, std::sync::atomic::AtomicUsize::new(0))
        }
        fn calls(&self) -> usize {
            // Relaxed is sufficient for a monotonic test counter
            // (canonical Rust idiom per [[cargo-fmt-workflow]] style).
            self.1.load(std::sync::atomic::Ordering::Relaxed)
        }
    }

    #[async_trait]
    impl ResolverBackend for SpyBackend {
        async fn resolve_via(
            &self,
            hop_did: &str,
            target: &octo_ident::RawDid,
            chain_ctx: &ResolverChainContext,
        ) -> Result<BackendResolveOutcome, ResolverBackendError> {
            self.1.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            self.0.resolve_via(hop_did, target, chain_ctx).await
        }
    }
}
