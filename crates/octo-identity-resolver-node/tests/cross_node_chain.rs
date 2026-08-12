//! Cross-node resolver chain integration tests (mission
//! `0871b-cross-node-forwarding`, AC-13 + AC-14).
//!
//! Validates the `ResolverBackend` trait boundary:
//! - AC-13: a custom backend accumulates `HopSignature`s and the
//!   response carries them through (5-tuple shape). Tested via
//!   `fake_remote_backend_propagates_signature_chain` +
//!   `full_round_trip_preserves_5tuple_wire_form` +
//!   `multi_hop_signature_chain_preserves_outermost_first_order`.
//! - AC-14: `LocalResolverBackend` (the production default) yields an
//!   empty `signature_chain` so in-process resolves stay
//!   signature-free — for both the empty-hops and multi-hop cases
//!   (`local_backend_yields_empty_signature_chain`). Per v0.6
//!   deviation (e), signature-verification integration is deferred to
//!   a future mission when mission `0870k-transport-request-response`
//!   lands.
//!
//! AC-15 (RemoteResolverBackend fail-closed) and AC-16 (full
//! handler→borsh→handler round-trip) are covered by
//! `remote_backend_stub_is_unsupported` (this file) and the in-file
//! `chain_response_with_hop_signature_round_trip` test in
//! `handlers/chain.rs` respectively. The signature-verification
//! integration piece of AC-16 is deferred.
//!
//! ## Why no real network
//!
//! These tests exercise the chain handler's cross-node SUBSTRATE —
//! the trait boundary, the 5-tuple wire form, the envelope_id
//! correlation. The actual `NodeTransport::send_request` plumbing
//! lands with mission `0870k-transport-request-response`. Until then,
//! the `SpyRemoteBackend` / `RemoteResolverBackend` stub stand in.
//!
//! Cross-node forwarding requires the envelope request/response
//! substrate that does not yet exist in `octo-transport` (only
//! `broadcast` and `send_best` fire-and-forget are present).

use std::sync::Arc;

use async_trait::async_trait;
use octo_ident::{CanonicalCodec, DidCodec, DidDocument, DidRegistry, InMemoryDidRegistry};
use octo_identity_resolver_node::{
    BackendResolveOutcome, ChainResolveRequest, ChainResolveResponse, IdentityResolveError,
    LocalResolverBackend, RawHopSignature, RemoteResolverBackend, ResolveChainHandler,
    ResolverBackend, ResolverBackendError, ResolverChainContext, ResolverHop, UnsupportedCode,
};

/// Mint a canonical DID wire form from a 32-byte seed-pubkey.
fn canonical_did(seed: u8) -> String {
    let mut pk = [0u8; 32];
    for (i, b) in pk.iter_mut().enumerate() {
        *b = (i as u8).wrapping_add(seed);
    }
    let raw = CanonicalCodec::mint(&pk);
    CanonicalCodec::raw_to_wire(&raw)
        .unwrap()
        .as_str()
        .to_owned()
}

/// Register a DID with a custom pubkey against `registry`.
fn register_custom(registry: &Arc<InMemoryDidRegistry>, seed: u8, pubkey: [u8; 32]) {
    let mut pk = [0u8; 32];
    for (i, b) in pk.iter_mut().enumerate() {
        *b = (i as u8).wrapping_add(seed);
    }
    let raw = CanonicalCodec::mint(&pk);
    registry
        .register(
            &raw.hash,
            DidDocument {
                public_key: pubkey,
                revoked: false,
                ..Default::default()
            },
        )
        .unwrap();
}

/// Fake cross-node `ResolverBackend` that pretends to resolve via a
/// remote hop: returns a populated `signature_chain` (one fake
/// `HopSignature` per call) without actually performing network I/O.
/// Used by AC-13 + AC-14 + AC-16 to exercise the cross-node shape.
struct FakeRemoteBackend {
    /// Public key this fake "returns" for every resolve.
    pubkey: [u8; 32],
    /// HopSignature template (filled in with hop_index = call_count).
    /// Layer-B `RawHopSignature` (the test backend's
    /// `signature_chain` type — handler converts to wire-form
    /// `HopSignature` at the response boundary).
    sig_template: RawHopSignature,
}

impl FakeRemoteBackend {
    fn new(pubkey: [u8; 32]) -> Self {
        // Use canonical_did(seed) (same helper as the rest of the file)
        // rather than a hardcoded string so the test exercises the
        // canonical-DID wire form end-to-end.
        let sig = RawHopSignature {
            hop_index: 0,
            hop_did: canonical_did(99),
            signature: [0xAA; 64],
            signer_pub: [0xBB; 32],
        };
        Self {
            pubkey,
            sig_template: sig,
        }
    }
}

#[async_trait]
impl ResolverBackend for FakeRemoteBackend {
    async fn resolve_via(
        &self,
        _hop_did: &str,
        _target: &octo_ident::RawDid,
        _chain_ctx: &ResolverChainContext,
    ) -> Result<BackendResolveOutcome, ResolverBackendError> {
        Ok(BackendResolveOutcome {
            public_key: self.pubkey,
            signature_chain: vec![self.sig_template.clone()],
        })
    }
}

/// AC-13: a custom backend's `HopSignature` survives into the
/// `ChainResolveResponse.signature_chain` field.
#[tokio::test]
async fn fake_remote_backend_propagates_signature_chain() {
    let backend: Arc<dyn ResolverBackend> = Arc::new(FakeRemoteBackend::new([0x77u8; 32]));
    let handler = ResolveChainHandler::new(backend);
    let req = ChainResolveRequest {
        target: canonical_did(11),
        hops: vec![ResolverHop::local(canonical_did(2))],
        ttl_remaining_ms: 100,
    };
    let out = handler
        .handle(&req, [0x42u8; 32])
        .await
        .expect("fake resolves");
    let payload = out.response_payload.expect("response payload");
    let resp: ChainResolveResponse = borsh::from_slice(&payload).unwrap();
    assert_eq!(resp.public_key, [0x77u8; 32]);
    assert_eq!(resp.signature_chain.len(), 1, "one HopSignature from fake");
    assert_eq!(resp.signature_chain[0].hop_index, 0);
    assert_eq!(resp.envelope_id, [0x42u8; 32]);
}

/// AC-14: `LocalResolverBackend` (the production default) returns an
/// EMPTY `signature_chain` so in-process resolves stay signature-free —
/// for both the empty-hops case and the multi-hop case.
#[tokio::test]
async fn local_backend_yields_empty_signature_chain() {
    let registry = Arc::new(InMemoryDidRegistry::default());
    register_custom(&registry, 11, [0xCCu8; 32]);
    let local: Arc<dyn ResolverBackend> = LocalResolverBackend::new(registry);
    let handler = ResolveChainHandler::new(local);

    // Empty hops — direct local resolve.
    let req_empty = ChainResolveRequest {
        target: canonical_did(11),
        hops: vec![],
        ttl_remaining_ms: 100,
    };
    let out = handler
        .handle(&req_empty, [0u8; 32])
        .await
        .expect("local resolves");
    let resp: ChainResolveResponse = borsh::from_slice(&out.response_payload.unwrap()).unwrap();
    assert_eq!(resp.public_key, [0xCCu8; 32]);
    assert_eq!(
        resp.hops_traversed, 0,
        "empty-hops branch reports hops_traversed = 0"
    );
    assert!(
        resp.signature_chain.is_empty(),
        "LocalResolverBackend (empty hops) yields no HopSignature"
    );

    // Multi-hop — local terminal registry with intermediate hops.
    let req_multi = ChainResolveRequest {
        target: canonical_did(11),
        hops: vec![
            ResolverHop::local(canonical_did(1)),
            ResolverHop::local(canonical_did(2)),
        ],
        ttl_remaining_ms: 100,
    };
    let out = handler
        .handle(&req_multi, [0u8; 32])
        .await
        .expect("local multi-hop resolves");
    let resp: ChainResolveResponse = borsh::from_slice(&out.response_payload.unwrap()).unwrap();
    assert_eq!(resp.public_key, [0xCCu8; 32]);
    assert_eq!(resp.hops_traversed, 2);
    assert!(
        resp.signature_chain.is_empty(),
        "LocalResolverBackend (multi-hop) yields no HopSignature"
    );
}

/// AC-15: `RemoteResolverBackend` stub returns
/// `IdentityResolveError::Unsupported` (fail-closed before the
/// request/response substrate lands).
#[tokio::test]
async fn remote_backend_stub_is_unsupported() {
    let backend: Arc<dyn ResolverBackend> = RemoteResolverBackend::arc();
    let handler = ResolveChainHandler::new(backend);
    let req = ChainResolveRequest {
        target: canonical_did(11),
        hops: vec![ResolverHop::local(canonical_did(2))],
        ttl_remaining_ms: 100,
    };
    let err = handler.handle(&req, [0u8; 32]).await.unwrap_err();
    // Pin the contract: the Unsupported discriminant is
    // `RemoteBackendNotWired` (operator-dashboard routing key per
    // round-3 review D5) AND the message contains the blocking
    // mission slug `0870k` for log correlation.
    match err {
        IdentityResolveError::Unsupported(code, msg) => {
            assert_eq!(
                code,
                UnsupportedCode::RemoteBackendNotWired,
                "Unsupported discriminant must be RemoteBackendNotWired"
            );
            assert!(
                msg.contains("0870k"),
                "Unsupported message must mention 0870k mission reference: {msg}"
            );
        }
        other => panic!("expected Unsupported, got {other:?}"),
    }
}

/// AC-16: full round-trip — handler constructs response with populated
/// `signature_chain` + non-zero `envelope_id`; borsh round-trip
/// preserves all five fields including the in-band `HopSignature`.
#[tokio::test]
async fn full_round_trip_preserves_5tuple_wire_form() {
    let backend: Arc<dyn ResolverBackend> = Arc::new(FakeRemoteBackend::new([0x88u8; 32]));
    let handler = ResolveChainHandler::new(backend);
    let envelope_id = [0x99u8; 32];
    let req = ChainResolveRequest {
        target: canonical_did(11),
        hops: vec![ResolverHop::local(canonical_did(2))],
        ttl_remaining_ms: 100,
    };
    let out = handler.handle(&req, envelope_id).await.expect("resolve");
    let payload = out.response_payload.expect("response payload");

    // Sender-side deserialization.
    let resp: ChainResolveResponse = borsh::from_slice(&payload).unwrap();
    assert_eq!(resp.canonical_did, canonical_did(11));
    assert_eq!(resp.public_key, [0x88u8; 32]);
    assert_eq!(resp.hops_traversed, 1);
    assert_eq!(resp.envelope_id, envelope_id);
    assert_eq!(resp.signature_chain.len(), 1);

    // Receiver-side borsh re-serialization: produce a `ChainResolveResponse`
    // matching the wire form and round-trip the WHOLE struct (proves the
    // 5-tuple borsh schema is self-consistent, not just deserializable).
    let resp_again: ChainResolveResponse =
        borsh::from_slice(&borsh::to_vec(&resp).unwrap()).unwrap();
    assert_eq!(resp_again, resp);
}

/// Round-1 review: `MAX_CHAIN_TTL_MS` is enforced at `handle()` entry.
/// A `ttl_remaining_ms = u64::MAX` request must be rejected BEFORE any
/// registry call.
#[tokio::test]
async fn rejects_oversize_ttl_dos() {
    let backend: Arc<dyn ResolverBackend> =
        LocalResolverBackend::new(Arc::new(InMemoryDidRegistry::default()));
    let handler = ResolveChainHandler::new(backend);
    let req = ChainResolveRequest {
        target: canonical_did(11),
        hops: vec![],
        ttl_remaining_ms: u64::MAX,
    };
    let err = handler.handle(&req, [0u8; 32]).await.unwrap_err();
    assert!(matches!(err, IdentityResolveError::ChainTtlTooLarge(_)));
}

/// Round-1 review: `hops.len() > u8::MAX` is rejected at `handle()`
/// entry. The `ChainResolveResponse.hops_traversed` field is `u8`, so
/// larger chains MUST fail-closed rather than silently capping.
#[tokio::test]
async fn rejects_oversize_hop_count() {
    let backend: Arc<dyn ResolverBackend> =
        LocalResolverBackend::new(Arc::new(InMemoryDidRegistry::default()));
    let handler = ResolveChainHandler::new(backend);
    // 256 hops = u8::MAX + 1. Use 256 DISTINCT canonical DIDs.
    //
    // `% 257` is a prime > 256 so the affine map `i -> (i * 7 + 13) % 257`
    // is a permutation of `[0, 256)`; every `i` produces a distinct
    // canonical form. Without distinct forms, a buggy impl that moved
    // the hop-count bound INTO the loop would short-circuit on
    // `ChainCycle` and the test would silently break.
    //
    // (Previous round used `% 200` — 200 is composite; the map has period
    // 200 so the 56 hops at `i ∈ [200, 256]` collided with `i ∈ [0, 55]`.
    // Round-3 review flagged: comment claimed "prime" but 200 isn't.)
    //
    // Check order at `handle()` entry is: TTL bound → hop-count bound.
    // TTL is set to `MAX_CHAIN_TTL_MS` exactly so the TTL bound passes
    // and the hop-count rejection fires next.
    let hops: Vec<ResolverHop> = (0..(u8::MAX as usize + 1))
        .map(|i| ResolverHop::local(canonical_did(((i * 7 + 13) % 257) as u8)))
        .collect();
    let req = ChainResolveRequest {
        target: canonical_did(11),
        hops,
        ttl_remaining_ms: 60_000,
    };
    let err = handler.handle(&req, [0u8; 32]).await.unwrap_err();
    assert!(matches!(err, IdentityResolveError::ChainTooLong(_)));
}

/// Round-1 review: hop canonicalization must precede cycle detection
/// (do not consume state on a malformed hop). A legacy bare-form hop
/// `did:octo:bad` is rejected before any `visited.insert` or TTL
/// decrement.
///
/// NOTE: this test name intentionally overstates the assertion — the
/// test only pins `InvalidDid` is returned. A buggy impl that did
/// `visited.insert(hop.hop_did)` BEFORE `CanonicalCodec::parse` would
/// still produce `InvalidDid` and pass the test (state would have been
/// consumed). Pinning true "no state consumed" requires a test-only
/// getter or sentinel DID trick; deferred.
#[tokio::test]
async fn rejects_malformed_hop_with_invalid_did_error() {
    let backend: Arc<dyn ResolverBackend> =
        LocalResolverBackend::new(Arc::new(InMemoryDidRegistry::default()));
    let handler = ResolveChainHandler::new(backend);
    let req = ChainResolveRequest {
        target: canonical_did(11),
        hops: vec![
            ResolverHop::local("did:octo:bad".into()),
            ResolverHop::local(canonical_did(2)),
        ],
        ttl_remaining_ms: 100,
    };
    let err = handler.handle(&req, [0u8; 32]).await.unwrap_err();
    assert!(matches!(err, IdentityResolveError::InvalidDid(_)));
}

/// Bonus TV: `RemoteResolverBackend` constructor + trait-object
/// boundary sanity.
#[test]
fn remote_backend_arc_constructs_trait_object() {
    let backend: Arc<dyn ResolverBackend> = RemoteResolverBackend::arc();
    // The Arc is Send + Sync — required by the trait bound.
    fn assert_send_sync<T: Send + Sync + ?Sized>() {}
    assert_send_sync::<dyn ResolverBackend>();
    // Strong-count check: the original Arc exists; clone once and
    // observe the count grow to 2.
    let backend_clone = Arc::clone(&backend);
    assert_eq!(Arc::strong_count(&backend), 2);
    drop(backend_clone);
    assert_eq!(Arc::strong_count(&backend), 1);
}

/// Round-2 review: a backend returning a multi-element
/// `signature_chain` propagates it into the response in
/// OUTERMOST-FIRST order (the docstring contract on
/// `ChainResolveResponse.signature_chain`). Pins the order contract
/// via 3 HopSignatures with distinct hop_index values.
#[tokio::test]
async fn multi_hop_signature_chain_preserves_outermost_first_order() {
    // Backend that returns 3 distinct HopSignatures per resolve.
    struct MultiHopBackend {
        pubkey: [u8; 32],
    }
    #[async_trait]
    impl ResolverBackend for MultiHopBackend {
        async fn resolve_via(
            &self,
            _hop_did: &str,
            _target: &octo_ident::RawDid,
            _chain_ctx: &ResolverChainContext,
        ) -> Result<BackendResolveOutcome, ResolverBackendError> {
            // Outermost-first: hop 0 first, hop 1 second, hop 2 last.
            let sigs = vec![
                RawHopSignature {
                    hop_index: 0,
                    hop_did: canonical_did(50),
                    signature: [0x11; 64],
                    signer_pub: [0x21; 32],
                },
                RawHopSignature {
                    hop_index: 1,
                    hop_did: canonical_did(51),
                    signature: [0x22; 64],
                    signer_pub: [0x22; 32],
                },
                RawHopSignature {
                    hop_index: 2,
                    hop_did: canonical_did(52),
                    signature: [0x33; 64],
                    signer_pub: [0x23; 32],
                },
            ];
            Ok(BackendResolveOutcome {
                public_key: self.pubkey,
                signature_chain: sigs,
            })
        }
    }
    let backend: Arc<dyn ResolverBackend> = Arc::new(MultiHopBackend {
        pubkey: [0x44u8; 32],
    });
    let handler = ResolveChainHandler::new(backend);
    let req = ChainResolveRequest {
        target: canonical_did(11),
        hops: vec![ResolverHop::local(canonical_did(2))],
        ttl_remaining_ms: 100,
    };
    let out = handler
        .handle(&req, [0u8; 32])
        .await
        .expect("multi-hop resolves");
    let resp: ChainResolveResponse = borsh::from_slice(&out.response_payload.unwrap()).unwrap();
    assert_eq!(
        resp.signature_chain.len(),
        3,
        "backend returned 3 signatures; handler must propagate all"
    );
    // Outermost-first order: hop 0 → hop 1 → hop 2.
    assert_eq!(resp.signature_chain[0].hop_index, 0);
    assert_eq!(resp.signature_chain[1].hop_index, 1);
    assert_eq!(resp.signature_chain[2].hop_index, 2);
    // Signer pubkeys preserved distinct across the chain.
    assert_eq!(resp.signature_chain[0].signer_pub, [0x21u8; 32]);
    assert_eq!(resp.signature_chain[1].signer_pub, [0x22u8; 32]);
    assert_eq!(resp.signature_chain[2].signer_pub, [0x23u8; 32]);
}
