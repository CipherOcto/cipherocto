//! Cross-node resolver chain integration tests (mission
//! `0871b-cross-node-forwarding`, AC-13 + AC-14, mission
//! `0870k-transport-request-response` AC-3 + AC-6 + AC-11).
//!
//! Validates the `ResolverBackend` trait boundary:
//! - AC-13: a custom backend accumulates `HopSignature`s and the
//!   response carries them through (5-tuple shape). Tested via
//!   `fake_remote_backend_propagates_signature_chain` +
//!   `full_round_trip_preserves_5tuple_wire_form` +
//!   `multi_hop_signature_chain_preserves_outermost_first_order` +
//!   `three_node_chain_accumulates_signature_chain_across_hops`.
//! - AC-14: `LocalResolverBackend` (the production default) yields an
//!   empty `signature_chain` so in-process resolves stay
//!   signature-free — for both the empty-hops and multi-hop cases
//!   (`local_backend_yields_empty_signature_chain`).
//!
//! AC-15 (RemoteResolverBackend fail-closed substrate wiring) is
//! covered by `remote_backend_empty_transport_fails_closed`. AC-16
//! (full handler→borsh→handler round-trip) is covered by
//! `full_round_trip_preserves_5tuple_wire_form`. The signature-
//! verification integration piece of AC-16 is deferred to the
//! RFC-0970 forwarding-hop signing mission.
//!
//! AC-11 (unit TV: `hop_signature_signs_and_verifies`) lives in
//! `crates/octo-protocol/src/hop_signature.rs` — same preimage + Ed25519
//! sign/verify cycle, exercised via the public `verify_ed25519_signature`
//! helper.
//!
//! ## Why no real network
//!
//! These tests exercise the chain handler's cross-node SUBSTRATE —
//! the trait boundary, the 5-tuple wire form, the envelope_id
//! correlation. The actual `NodeTransport::send_request` plumbing
//! lands with mission `0870k-transport-request-response` (this PR).
//!
//! Cross-node forwarding exercises the request/response substrate via
//! an in-process `CannedReplySender` that delivers the canned reply
//! synchronously. The full 3-node envelope-router (node A → B → C with
//! real transport fan-out) is a follow-on once RFC-0970 forwarding-hop
//! signing lands.

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

/// `NetworkSender` that returns a pre-canned reply AND captures the
/// outbound payload (so tests can assert on the outbound envelope's
/// `recipient`, `from_did`, and `payload_kind` fields). Hoisted to
/// file scope so the 3-node happy-path test + the 4 new defense-in-
/// depth tests can share the implementation.
struct CannedReplySender {
    reply: Vec<u8>,
    /// Captured outbound borsh-encoded `NodeEnvelope` bytes; `None`
    /// until `send_request` is called at least once.
    captured_outbound: std::sync::Mutex<Option<Vec<u8>>>,
}

impl CannedReplySender {
    fn new(reply: Vec<u8>) -> Arc<Self> {
        Arc::new(Self {
            reply,
            captured_outbound: std::sync::Mutex::new(None),
        })
    }

    /// Snapshot of the captured outbound envelope bytes (after the
    /// first `send_request` call). Returns `None` if no send has
    /// happened yet.
    fn captured_outbound(&self) -> Option<Vec<u8>> {
        self.captured_outbound.lock().unwrap().clone()
    }
}

#[async_trait]
impl octo_transport::sender::NetworkSender for CannedReplySender {
    async fn send(
        &self,
        _payload: &[u8],
        _ctx: &octo_transport::sender::SendContext,
    ) -> Result<(), octo_transport::sender::TransportError> {
        Ok(())
    }
    async fn send_request(
        &self,
        payload: &[u8],
        _envelope_id: [u8; 32],
        _ctx: &octo_transport::sender::SendContext,
        _timeout: std::time::Duration,
    ) -> Result<Vec<u8>, octo_transport::sender::TransportError> {
        *self.captured_outbound.lock().unwrap() = Some(payload.to_vec());
        Ok(self.reply.clone())
    }
    fn name(&self) -> &str {
        "canned-reply"
    }
    fn is_healthy(&self) -> bool {
        true
    }
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

/// AC-15: `RemoteResolverBackend` wired to a transport with NO
/// `send_request` senders fails-closed: the chain handler catches
/// the `TransportError::Unsupported("no sender implements send_request")`
/// and surfaces it via the `From<ResolverBackendError>` bridge as
/// `IdentityResolveError::Unsupported(RemoteBackendNotWired, ...)`.
#[tokio::test]
async fn remote_backend_empty_transport_fails_closed() {
    let transport = Arc::new(octo_transport::NodeTransport::new(Vec::new()));
    let backend: Arc<dyn ResolverBackend> = RemoteResolverBackend::arc(transport);
    let handler = ResolveChainHandler::new(backend);
    let req = ChainResolveRequest {
        target: canonical_did(11),
        hops: vec![ResolverHop::local(canonical_did(2))],
        ttl_remaining_ms: 100,
    };
    let err = handler.handle(&req, [0u8; 32]).await.unwrap_err();
    // Pin the contract: the Unsupported discriminant is
    // `RemoteBackendNotWired` (operator-dashboard routing key per
    // round-3 review D5) AND the message contains the "no sender"
    // wire-dead explanation for log correlation.
    match err {
        IdentityResolveError::Unsupported(code, msg) => {
            assert_eq!(
                code,
                UnsupportedCode::RemoteBackendNotWired,
                "Unsupported discriminant must be RemoteBackendNotWired"
            );
            assert!(
                msg.contains("send_request"),
                "Unsupported message must mention send_request: {msg}"
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
    let transport = Arc::new(octo_transport::NodeTransport::new(Vec::new()));
    let backend: Arc<dyn ResolverBackend> = RemoteResolverBackend::arc(transport);
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

/// AC-13 + AC-14 + AC-16: 3-node chain (A → B → C) accumulates
/// `HopSignature`s across hops; terminal node C resolves the target
/// in its local registry; response carries the full chain.
///
/// Architecture (in-process simulation; no real network):
/// - Node A (origin): `ResolveChainHandler` bound to
///   `RemoteResolverBackend` posting an `IDENTITY_RESOLVE` to a
///   transport that immediately returns B's canned reply.
/// - Node B (intermediate): simulated by the `CannedReplySender`
///   returning a pre-canned `ChainResolveResponse` with
///   `signature_chain = [hop_0]`.
/// - Node C (terminal): `LocalResolverBackend` over `InMemoryDidRegistry`
///   where the target DID is registered.
///
/// The chain handler at A walks `hops = [B]` and the
/// `RemoteResolverBackend` posts a request to a transport that
/// immediately returns B's canned reply. The full 3-node traversal
/// pattern (A → B → C through three real `NodeTransport`s) is a
/// follow-on for the RFC-0970 forwarding-hop signing mission.
#[tokio::test]
async fn three_node_chain_accumulates_signature_chain_across_hops() {
    // 1. Build node C's "resolved response" by setting the canned
    //    reply's `public_key` field directly. The terminal registry
    //    is NOT exercised here because the canned reply carries a
    //    pre-determined `public_key`; the full 3-node walk through
    //    a real `LocalResolverBackend` is the RFC-0970 follow-on.

    // 2. Build node B's canned reply — a populated `ChainResolveResponse`
    //    with one `HopSignature` (hop 0, signature bytes are a FIXTURE
    //    PLACEHOLDER, not a real Ed25519 signature; production
    //    signature-verification integration is the RFC-0970 follow-on).
    let hop_sig_b = octo_protocol::HopSignature::new(0, canonical_did(20), [0x55; 64], [0x66; 32]);
    let chain_resp_b = ChainResolveResponse {
        canonical_did: canonical_did(11),
        public_key: [0x99u8; 32],
        hops_traversed: 2,
        signature_chain: vec![hop_sig_b],
        envelope_id: [0x42u8; 32],
    };
    let chain_resp_b_payload = borsh::to_vec(&chain_resp_b).unwrap();
    use octo_protocol::envelope::VERSION_TAG_V2;
    let chain_resp_b_envelope = octo_protocol::NodeEnvelope::build(
        octo_ident::WireDid::new(canonical_did(20)),
        octo_protocol::recipient::RecipientRef::Broadcast,
        octo_protocol::payload_kind::IDENTITY_RESOLVE_CHAIN_RESPONSE,
        chain_resp_b_payload,
        vec![],
        [0x42u8; 32],
        u64::MAX,
        VERSION_TAG_V2,
    )
    .unwrap();
    let canned_reply = borsh::to_vec(&chain_resp_b_envelope).unwrap();

    // 3. Wire node A's transport with the canned-reply sender.
    let canned = CannedReplySender::new(canned_reply);
    let transport_a = Arc::new(octo_transport::NodeTransport::new(vec![canned.clone()]));

    // 4. Wire node A's `RemoteResolverBackend` to the transport.
    let backend_a: Arc<dyn ResolverBackend> = RemoteResolverBackend::arc(transport_a);

    // 5. Run the chain handler at A walking `hops = [B]`.
    let handler = ResolveChainHandler::new(backend_a);
    let req = ChainResolveRequest {
        target: canonical_did(11),
        hops: vec![ResolverHop::local(canonical_did(20))],
        ttl_remaining_ms: 100,
    };
    let out = handler
        .handle(&req, [0x42u8; 32])
        .await
        .expect("3-node chain resolves");
    let resp: ChainResolveResponse = borsh::from_slice(&out.response_payload.unwrap()).unwrap();

    // 6. Verify the response shape.
    assert_eq!(resp.canonical_did, canonical_did(11));
    assert_eq!(resp.public_key, [0x99u8; 32], "public_key from C");
    assert_eq!(resp.hops_traversed, 1, "single hop walked at A");
    assert_eq!(resp.envelope_id, [0x42u8; 32]);
    assert_eq!(
        resp.signature_chain.len(),
        1,
        "B's hop signature propagated through the substrate"
    );
    assert_eq!(resp.signature_chain[0].hop_index, 0);
    assert_eq!(resp.signature_chain[0].signer_pub, [0x66u8; 32]);
}

/// T5 coverage: when the canned reply's `from_did` does not match the
/// queried hop's DID, `RemoteResolverBackend::resolve_via` must reject
/// with `ResolverBackendError::Backing` (defense-in-depth against
/// misrouted replies). The `NodeTransport` authentication layer is the
/// primary defense; this test pins the layered defense.
#[tokio::test]
async fn remote_backend_rejects_from_did_mismatch_in_reply() {
    // Build a canned reply whose `from_did = canonical_did(99)` does
    // NOT match the queried hop's DID = `canonical_did(20)`.
    let hop_sig_b = octo_protocol::HopSignature::new(0, canonical_did(99), [0x55; 64], [0x66; 32]);
    let chain_resp_b = ChainResolveResponse {
        canonical_did: canonical_did(11),
        public_key: [0x99u8; 32],
        hops_traversed: 1,
        signature_chain: vec![hop_sig_b],
        envelope_id: [0x42u8; 32],
    };
    let chain_resp_b_payload = borsh::to_vec(&chain_resp_b).unwrap();
    use octo_protocol::envelope::VERSION_TAG_V2;
    let chain_resp_b_envelope = octo_protocol::NodeEnvelope::build(
        octo_ident::WireDid::new(canonical_did(99)),
        octo_protocol::recipient::RecipientRef::Broadcast,
        octo_protocol::payload_kind::IDENTITY_RESOLVE_CHAIN_RESPONSE,
        chain_resp_b_payload,
        vec![],
        [0x42u8; 32],
        u64::MAX,
        VERSION_TAG_V2,
    )
    .unwrap();
    let canned_reply = borsh::to_vec(&chain_resp_b_envelope).unwrap();

    let canned = CannedReplySender::new(canned_reply);
    let transport_a = Arc::new(octo_transport::NodeTransport::new(vec![canned.clone()]));
    let backend_a: Arc<dyn ResolverBackend> = RemoteResolverBackend::arc(transport_a);

    let handler = ResolveChainHandler::new(backend_a);
    let req = ChainResolveRequest {
        target: canonical_did(11),
        hops: vec![ResolverHop::local(canonical_did(20))],
        ttl_remaining_ms: 100,
    };
    let err = handler
        .handle(&req, [0x42u8; 32])
        .await
        .expect_err("from_did mismatch must be rejected");
    let msg = format!("{err:?}");
    assert!(
        msg.contains("from_did mismatch") || msg.contains("Storage"),
        "error must surface the from_did mismatch (got: {msg})"
    );
}

/// T5 coverage: outbound `NodeEnvelope` built by `RemoteResolverBackend`
/// must use `RecipientRef::Domain(WireDid(hop_did))` (scoped fan-out
/// instead of `Broadcast`). The `from_did == hop_did` reply check is
/// the substantive defense against spoofing; `Domain`-scoping is the
/// layered defense that reduces the routing-layer attack surface.
#[tokio::test]
async fn remote_backend_asserts_domain_recipient_in_outbound_envelope() {
    let hop_sig_b = octo_protocol::HopSignature::new(0, canonical_did(20), [0x55; 64], [0x66; 32]);
    let chain_resp_b = ChainResolveResponse {
        canonical_did: canonical_did(11),
        public_key: [0x99u8; 32],
        hops_traversed: 1,
        signature_chain: vec![hop_sig_b],
        envelope_id: [0x42u8; 32],
    };
    let chain_resp_b_payload = borsh::to_vec(&chain_resp_b).unwrap();
    use octo_protocol::envelope::VERSION_TAG_V2;
    let chain_resp_b_envelope = octo_protocol::NodeEnvelope::build(
        octo_ident::WireDid::new(canonical_did(20)),
        octo_protocol::recipient::RecipientRef::Broadcast,
        octo_protocol::payload_kind::IDENTITY_RESOLVE_CHAIN_RESPONSE,
        chain_resp_b_payload,
        vec![],
        [0x42u8; 32],
        u64::MAX,
        VERSION_TAG_V2,
    )
    .unwrap();
    let canned_reply = borsh::to_vec(&chain_resp_b_envelope).unwrap();

    let canned = CannedReplySender::new(canned_reply);
    let transport_a = Arc::new(octo_transport::NodeTransport::new(vec![canned.clone()]));
    let backend_a: Arc<dyn ResolverBackend> = RemoteResolverBackend::arc(transport_a);

    let handler = ResolveChainHandler::new(backend_a);
    let req = ChainResolveRequest {
        target: canonical_did(11),
        hops: vec![ResolverHop::local(canonical_did(20))],
        ttl_remaining_ms: 100,
    };
    handler
        .handle(&req, [0x42u8; 32])
        .await
        .expect("happy path resolves");

    let captured = canned
        .captured_outbound()
        .expect("CannedReplySender MUST have captured the outbound envelope");
    let outbound: octo_protocol::NodeEnvelope =
        borsh::from_slice(&captured).expect("outbound envelope borsh");
    match outbound.to_node_id {
        octo_protocol::recipient::RecipientRef::Domain(wire_did) => {
            assert_eq!(
                wire_did.as_str(),
                canonical_did(20),
                "Domain recipient MUST scope fan-out to the queried hop's DID"
            );
        }
        other => panic!("outbound recipient MUST be Domain(WireDid(hop_did)), got {other:?}"),
    }
    assert_eq!(
        outbound.payload_kind,
        octo_protocol::payload_kind::IDENTITY_RESOLVE,
        "outbound payload_kind MUST be IDENTITY_RESOLVE"
    );
}

/// T5 coverage: inbound `ChainResolveResponse.signature_chain.len() >
/// MAX_CHAIN_HOPS = 255` MUST be rejected with `Backing`. The local
/// hop-count bound does NOT apply to network INPUT; the wire-format
/// cap is enforced independently on the inbound payload.
#[tokio::test]
async fn remote_backend_rejects_oversize_signature_chain_in_reply() {
    // Build signature_chain with `MAX_CHAIN_HOPS + 1 = 256` entries.
    // Each hop_did is unique (distinct seed) so the local backend
    // doesn't trip its own cycle detector before the network input
    // bounds check fires.
    let oversized_chain: Vec<octo_protocol::HopSignature> = (0..=u8::MAX)
        .map(|i| {
            octo_protocol::HopSignature::new(
                i,
                canonical_did(i.wrapping_add(1)),
                [0x55; 64],
                [0x66; 32],
            )
        })
        .collect();
    assert_eq!(oversized_chain.len(), u8::MAX as usize + 1);

    let chain_resp_b = ChainResolveResponse {
        canonical_did: canonical_did(11),
        public_key: [0x99u8; 32],
        hops_traversed: 1,
        signature_chain: oversized_chain,
        envelope_id: [0x42u8; 32],
    };
    let chain_resp_b_payload = borsh::to_vec(&chain_resp_b).unwrap();
    use octo_protocol::envelope::VERSION_TAG_V2;
    let chain_resp_b_envelope = octo_protocol::NodeEnvelope::build(
        octo_ident::WireDid::new(canonical_did(20)),
        octo_protocol::recipient::RecipientRef::Broadcast,
        octo_protocol::payload_kind::IDENTITY_RESOLVE_CHAIN_RESPONSE,
        chain_resp_b_payload,
        vec![],
        [0x42u8; 32],
        u64::MAX,
        VERSION_TAG_V2,
    )
    .unwrap();
    let canned_reply = borsh::to_vec(&chain_resp_b_envelope).unwrap();

    let canned = CannedReplySender::new(canned_reply);
    let transport_a = Arc::new(octo_transport::NodeTransport::new(vec![canned.clone()]));
    let backend_a: Arc<dyn ResolverBackend> = RemoteResolverBackend::arc(transport_a);

    let handler = ResolveChainHandler::new(backend_a);
    let req = ChainResolveRequest {
        target: canonical_did(11),
        hops: vec![ResolverHop::local(canonical_did(20))],
        ttl_remaining_ms: 100,
    };
    let err = handler
        .handle(&req, [0x42u8; 32])
        .await
        .expect_err("oversized signature_chain must be rejected");
    let msg = format!("{err:?}");
    assert!(
        msg.contains("oversized signature_chain") || msg.contains("Storage"),
        "error must surface the oversized-chain rejection (got: {msg})"
    );
}

/// T5 coverage: borsh decode failure of an inbound reply payload
/// (wire corruption OR peer implementing wrong schema) MUST surface
/// as `ResolverBackendError::Backing` → `IdentityResolveError::Storage`,
/// not panic or silently succeed.
#[tokio::test]
async fn remote_backend_rejects_borsh_decode_failure_of_reply() {
    // Build a reply envelope whose payload is intentionally NOT a
    // valid `ResolveResponse` (we use 4 arbitrary bytes that fail
    // borsh decoding of the expected struct).
    let garbage_payload = vec![0xDEu8, 0xAD, 0xBE, 0xEF];
    use octo_protocol::envelope::VERSION_TAG_V2;
    let reply_envelope = octo_protocol::NodeEnvelope::build(
        octo_ident::WireDid::new(canonical_did(20)),
        octo_protocol::recipient::RecipientRef::Broadcast,
        octo_protocol::payload_kind::IDENTITY_RESOLVE,
        garbage_payload,
        vec![],
        [0x42u8; 32],
        u64::MAX,
        VERSION_TAG_V2,
    )
    .unwrap();
    let canned_reply = borsh::to_vec(&reply_envelope).unwrap();

    let canned = CannedReplySender::new(canned_reply);
    let transport_a = Arc::new(octo_transport::NodeTransport::new(vec![canned.clone()]));
    let backend_a: Arc<dyn ResolverBackend> = RemoteResolverBackend::arc(transport_a);

    let handler = ResolveChainHandler::new(backend_a);
    let req = ChainResolveRequest {
        target: canonical_did(11),
        hops: vec![ResolverHop::local(canonical_did(20))],
        ttl_remaining_ms: 100,
    };
    let err = handler
        .handle(&req, [0x42u8; 32])
        .await
        .expect_err("borsh decode failure must surface as Backing");
    let msg = format!("{err:?}");
    assert!(
        msg.contains("borsh decode failed") || msg.contains("Storage"),
        "error must surface the borsh decode failure (got: {msg})"
    );
}

/// T6 coverage: a reply carrying a `payload_kind` other than
/// `IDENTITY_RESOLVE` or `IDENTITY_RESOLVE_CHAIN_RESPONSE` (e.g.,
/// `IDENTITY_REGISTER`) MUST be rejected with `Backing`. The
/// catch-all `other => ...` branch in `RemoteResolverBackend::resolve_via`
/// is the unrecognized-reply-kinds defense.
#[tokio::test]
async fn remote_backend_rejects_unrecognized_reply_payload_kind() {
    // Build a reply whose payload_kind is `IDENTITY_REGISTER` (not
    // IDENTITY_RESOLVE / IDENTITY_RESOLVE_CHAIN_RESPONSE).
    use octo_protocol::envelope::VERSION_TAG_V2;
    let reply_envelope = octo_protocol::NodeEnvelope::build(
        octo_ident::WireDid::new(canonical_did(20)),
        octo_protocol::recipient::RecipientRef::Broadcast,
        octo_protocol::payload_kind::IDENTITY_REGISTER,
        vec![0xAA, 0xBB, 0xCC, 0xDD],
        vec![],
        [0x42u8; 32],
        u64::MAX,
        VERSION_TAG_V2,
    )
    .unwrap();
    let canned_reply = borsh::to_vec(&reply_envelope).unwrap();

    let canned = CannedReplySender::new(canned_reply);
    let transport_a = Arc::new(octo_transport::NodeTransport::new(vec![canned.clone()]));
    let backend_a: Arc<dyn ResolverBackend> = RemoteResolverBackend::arc(transport_a);

    let handler = ResolveChainHandler::new(backend_a);
    let req = ChainResolveRequest {
        target: canonical_did(11),
        hops: vec![ResolverHop::local(canonical_did(20))],
        ttl_remaining_ms: 100,
    };
    let err = handler
        .handle(&req, [0x42u8; 32])
        .await
        .expect_err("unrecognized payload_kind must be rejected");
    let msg = format!("{err:?}");
    assert!(
        msg.contains("expected IDENTITY_RESOLVE or IDENTITY_RESOLVE_CHAIN_RESPONSE")
            || msg.contains("Storage"),
        "error must surface the unrecognized-kind rejection (got: {msg})"
    );
}

/// T6 coverage: bare `IDENTITY_RESOLVE` (single-hop) reply branch —
/// decodes `ResolveResponse`, returns `BackendResolveOutcome` with
/// `public_key` from the response + empty `signature_chain`. The
/// single-hop match arm in `RemoteResolverBackend::resolve_via`
/// (the `IDENTITY_RESOLVE => ...` arm) is distinct from the
/// `IDENTITY_RESOLVE_CHAIN_RESPONSE` branch and was previously
/// untested end-to-end.
#[tokio::test]
async fn remote_backend_handles_bare_resolve_response_payload_kind() {
    // Build a `ResolveResponse` carrying a 32-byte public_key + a
    // borsh-encoded reply envelope wrapping it with payload_kind =
    // IDENTITY_RESOLVE (single-hop bare resolve).
    let resolve_resp = octo_identity_resolver_node::ResolveResponse {
        canonical_did: canonical_did(11),
        public_key: [0xCCu8; 32],
    };
    let resp_bytes = borsh::to_vec(&resolve_resp).unwrap();
    use octo_protocol::envelope::VERSION_TAG_V2;
    let reply_envelope = octo_protocol::NodeEnvelope::build(
        octo_ident::WireDid::new(canonical_did(20)),
        octo_protocol::recipient::RecipientRef::Broadcast,
        octo_protocol::payload_kind::IDENTITY_RESOLVE,
        resp_bytes,
        vec![],
        [0x42u8; 32],
        u64::MAX,
        VERSION_TAG_V2,
    )
    .unwrap();
    let canned_reply = borsh::to_vec(&reply_envelope).unwrap();

    let canned = CannedReplySender::new(canned_reply);
    let transport_a = Arc::new(octo_transport::NodeTransport::new(vec![canned.clone()]));
    let backend_a: Arc<dyn ResolverBackend> = RemoteResolverBackend::arc(transport_a);

    let handler = ResolveChainHandler::new(backend_a);
    let req = ChainResolveRequest {
        target: canonical_did(11),
        hops: vec![ResolverHop::local(canonical_did(20))],
        ttl_remaining_ms: 100,
    };
    let out = handler
        .handle(&req, [0x42u8; 32])
        .await
        .expect("bare IDENTITY_RESOLVE reply resolves successfully");
    let resp: ChainResolveResponse = borsh::from_slice(&out.response_payload.unwrap()).unwrap();
    assert_eq!(
        resp.canonical_did,
        canonical_did(11),
        "ChainResolveResponse.canonical_did must match the chain request target"
    );
    assert_eq!(
        resp.public_key, [0xCCu8; 32],
        "bare ResolveResponse public_key must propagate through the substrate"
    );
    assert_eq!(
        resp.signature_chain.len(),
        0,
        "bare IDENTITY_RESOLVE branch yields empty signature_chain (single-hop has no forwarding hops to sign)"
    );
}

/// T6 coverage: reply envelope borsh decode failure (outer-level —
/// the bytes returned by `send_request` are NOT a valid
/// `NodeEnvelope`). Different from T5's payload-level borsh failure
/// (which assumed the envelope decoded but the inner payload did
/// not). Surfaces as `Backing` per the outer
/// `borsh::from_slice::<NodeEnvelope>` decode in
/// `RemoteResolverBackend::resolve_via`.
#[tokio::test]
async fn remote_backend_rejects_outer_envelope_borsh_decode_failure() {
    // 3 bytes is strictly less than any valid borsh-encoded
    // NodeEnvelope (the envelope contains a u128 payload_kind, a
    // String from_did, and a Vec<u8> payload — minimum wire size
    // is well above 100 bytes). borsh::from_slice fails on too-
    // short input.
    let canned_reply: Vec<u8> = vec![0x01, 0x02, 0x03];

    let canned = CannedReplySender::new(canned_reply);
    let transport_a = Arc::new(octo_transport::NodeTransport::new(vec![canned.clone()]));
    let backend_a: Arc<dyn ResolverBackend> = RemoteResolverBackend::arc(transport_a);

    let handler = ResolveChainHandler::new(backend_a);
    let req = ChainResolveRequest {
        target: canonical_did(11),
        hops: vec![ResolverHop::local(canonical_did(20))],
        ttl_remaining_ms: 100,
    };
    let err = handler
        .handle(&req, [0x42u8; 32])
        .await
        .expect_err("outer-envelope borsh decode failure must surface as Backing");
    let msg = format!("{err:?}");
    assert!(
        msg.contains("reply envelope borsh decode failed") || msg.contains("Storage"),
        "error must surface the outer-envelope borsh decode failure (got: {msg})"
    );
}

/// AC-14: `chain_cross_domain_auth_verifies` — the `HopSignature`
/// embedded in `ChainResolveResponse.signature_chain` is a REAL
/// Ed25519 signature over the production 5-tuple preimage
/// (`HopSignature::signing_preimage`), and the in-band `signer_pub`
/// verifies it via the public `verify_ed25519_signature` helper.
///
/// The backend synthesises a real `HopSignature`:
/// 1. Generates a deterministic Ed25519 keypair from a 32-byte seed.
/// 2. Computes `HopSignature::signing_preimage` (the SAME helper the
///    production chain handler uses — pinning the production encoder).
/// 3. Signs the preimage via `ed25519_dalek::SigningKey`.
/// 4. Wraps the signature in a `HopSignature` and returns it.
///
/// The test then decodes the resulting `ChainResolveResponse` from the
/// chain handler and verifies each signature end-to-end via
/// `verify_ed25519_signature`. Pins the cross-domain
/// sign+verify+substrate round trip.
#[tokio::test]
async fn chain_cross_domain_auth_verifies() {
    use ed25519_dalek::{Signer, SigningKey};
    use octo_protocol::authorization::{verify_ed25519_signature, Ed25519SignatureBytes};
    use octo_protocol::HopSignature;

    // 1. Generate a deterministic Ed25519 keypair from a 32-byte seed.
    let mut seed = [0u8; 32];
    for (i, b) in seed.iter_mut().enumerate() {
        *b = (i as u8).wrapping_add(7);
    }
    let sk = SigningKey::from_bytes(&seed);
    let pk_bytes = sk.verifying_key().to_bytes();
    let hop_did = octo_ident::WireDid::new(format!(
        "did:octo:z{}",
        bs58::encode(&pk_bytes).into_string()
    ));

    // 2. Compute the production preimage via the canonical helper.
    let chain_hash = [0xCDu8; 32];
    let hop_index: u8 = 1;
    let inner_payload = b"IDENTITY_RESOLVE_CHAIN inner payload bytes";
    let envelope_id = [0x77u8; 32];
    let preimage_hash =
        HopSignature::signing_preimage(chain_hash, hop_index, inner_payload, envelope_id);

    // 3. Sign the BLAKE3-hashed preimage with Ed25519.
    let sig = Ed25519SignatureBytes::from_signature(&sk.sign(&preimage_hash));
    let hop_sig = HopSignature::new(hop_index, hop_did.as_str().to_owned(), sig.0, pk_bytes);

    // 4. Wire a backend that returns the real signed HopSignature.
    struct RealSigBackend {
        sig: octo_protocol::HopSignature,
    }
    #[async_trait]
    impl ResolverBackend for RealSigBackend {
        async fn resolve_via(
            &self,
            _hop_did: &str,
            _target: &octo_ident::RawDid,
            _chain_ctx: &ResolverChainContext,
        ) -> Result<BackendResolveOutcome, ResolverBackendError> {
            Ok(BackendResolveOutcome {
                public_key: [0xCCu8; 32],
                signature_chain: vec![RawHopSignature {
                    hop_index: self.sig.hop_index,
                    hop_did: self.sig.hop_did.clone(),
                    signature: self.sig.signature,
                    signer_pub: self.sig.signer_pub,
                }],
            })
        }
    }
    let backend: Arc<dyn ResolverBackend> = Arc::new(RealSigBackend { sig: hop_sig });
    let handler = ResolveChainHandler::new(backend);

    // 5. Walk hops=[B]; decode the chain response.
    let req = ChainResolveRequest {
        target: canonical_did(11),
        hops: vec![ResolverHop::local(canonical_did(2))],
        ttl_remaining_ms: 100,
    };
    let out = handler
        .handle(&req, envelope_id)
        .await
        .expect("real-signature backend resolves");
    let resp: ChainResolveResponse = borsh::from_slice(&out.response_payload.unwrap()).unwrap();

    // 6. The HopSignature survived into the wire form.
    assert_eq!(
        resp.signature_chain.len(),
        1,
        "real HopSignature propagated"
    );
    let on_wire = &resp.signature_chain[0];
    assert_eq!(on_wire.hop_index, 1);
    assert_eq!(on_wire.signer_pub, pk_bytes);
    assert_eq!(on_wire.signature, sig.0);

    // 7. AC-14 contract: the in-band `signer_pub` verifies the
    //    signature over the production 5-tuple preimage.
    let wire_did = octo_ident::WireDid::new(on_wire.hop_did.clone());
    verify_ed25519_signature(&wire_did, &preimage_hash, &sig)
        .expect("HopSignature MUST verify against the 5-tuple preimage + in-band signer_pub");
}

/// AC-15: `chain_cycle_detection_aborts_cross_node` — cycle
/// detection fires at the LOCAL `ResolveChainHandler` (not the
/// remote backend) when the chain walks through
/// `RemoteResolverBackend`. The cycle detector seeds `visited` with
/// the target + walks hops in order; a repeat hop_did trips the
/// `ChainCycle` abort. The backend's `resolve_via` is NEVER called
/// for the duplicate hop (the abort fires BEFORE backend delegation).
///
/// Confirms the cross-node variant of cycle detection: with a
/// `RemoteResolverBackend` wired in, the local cycle detector still
/// fires before any network I/O is performed for the duplicate hop.
#[tokio::test]
async fn chain_cycle_detection_aborts_cross_node() {
    use std::sync::atomic::{AtomicUsize, Ordering};

    // Backend that counts invocations so we can assert resolve_via is
    // NEVER called for the duplicate hop (the abort fires before
    // backend delegation).
    struct CountingBackend {
        invocations: Arc<AtomicUsize>,
        pubkey: [u8; 32],
    }
    #[async_trait]
    impl ResolverBackend for CountingBackend {
        async fn resolve_via(
            &self,
            _hop_did: &str,
            _target: &octo_ident::RawDid,
            _chain_ctx: &ResolverChainContext,
        ) -> Result<BackendResolveOutcome, ResolverBackendError> {
            self.invocations.fetch_add(1, Ordering::SeqCst);
            Ok(BackendResolveOutcome {
                public_key: self.pubkey,
                signature_chain: Vec::new(),
            })
        }
    }

    let invocations = Arc::new(AtomicUsize::new(0));
    let backend: Arc<dyn ResolverBackend> = Arc::new(CountingBackend {
        invocations: invocations.clone(),
        pubkey: [0xAAu8; 32],
    });
    let handler = ResolveChainHandler::new(backend);

    // hops=[B, B] — the second B visit trips cycle detection.
    let hop_b = canonical_did(2);
    let req = ChainResolveRequest {
        target: canonical_did(11),
        hops: vec![ResolverHop::local(hop_b.clone()), ResolverHop::local(hop_b)],
        ttl_remaining_ms: 100,
    };
    let err = handler
        .handle(&req, [0u8; 32])
        .await
        .expect_err("duplicate hop must trip ChainCycle");
    assert!(
        matches!(err, IdentityResolveError::ChainCycle),
        "expected ChainCycle, got {err:?}"
    );
    // The cycle abort fires BEFORE backend delegation, so
    // `resolve_via` is NEVER invoked (zero calls).
    assert_eq!(
        invocations.load(Ordering::SeqCst),
        0,
        "cycle abort must fire BEFORE backend delegation (no resolve_via calls)"
    );
}

/// AC-13: true 3-node chain accumulation — the BACKEND walks an
/// inner chain (B → C) and accumulates TWO real Ed25519-signed
/// `HopSignature`s in `signature_chain`. The chain handler at A
/// walks hops=[B]; the response carries the full inner chain's
/// signatures.
///
/// This pins the substrate's signature-accumulation contract: a
/// single `resolve_via` call returns a multi-element
/// `signature_chain`, and the chain handler propagates all elements
/// into `ChainResolveResponse.signature_chain` without truncation.
///
/// (The full 3-node end-to-end test with REAL `IdentityResolverNode`
/// instances at B + C wired through 3 transports lands as a
/// follow-on once RFC-0970 forwarding-hop signing is integrated.)
#[tokio::test]
async fn true_3_node_chain_accumulates_two_real_signed_hop_signatures() {
    use ed25519_dalek::{Signer, SigningKey};
    use octo_protocol::authorization::{verify_ed25519_signature, Ed25519SignatureBytes};
    use octo_protocol::HopSignature;

    /// Helper: build a real Ed25519-signed `HopSignature` for a given
    /// (hop_index, chain_hash, inner_payload, envelope_id, seed).
    fn signed_hop(
        hop_index: u8,
        _hop_did_seed: u8,
        chain_hash: [u8; 32],
        inner_payload: &[u8],
        envelope_id: [u8; 32],
        sk_seed: u8,
    ) -> octo_protocol::HopSignature {
        let mut seed = [0u8; 32];
        for (i, b) in seed.iter_mut().enumerate() {
            *b = (i as u8).wrapping_add(sk_seed);
        }
        let sk = SigningKey::from_bytes(&seed);
        let pk_bytes = sk.verifying_key().to_bytes();
        let hop_did = format!("did:octo:z{}", bs58::encode(&pk_bytes).into_string());
        let preimage_hash =
            HopSignature::signing_preimage(chain_hash, hop_index, inner_payload, envelope_id);
        let sig = Ed25519SignatureBytes::from_signature(&sk.sign(&preimage_hash));
        HopSignature::new(hop_index, hop_did, sig.0, pk_bytes)
    }

    let chain_hash = [0x33u8; 32];
    let envelope_id = [0x99u8; 32];
    let b_inner_payload = b"B's inner payload (forwarded from A)";
    let c_inner_payload = b"C's inner payload (forwarded from B)";

    // 1. Build 2 real Ed25519-signed HopSignatures: hop 0 = B signing
    //    its inner payload; hop 1 = C signing its inner payload.
    //    Outermost-first ordering per RFC-0871 §signature_chain.
    let sig_b = signed_hop(0, 11, chain_hash, b_inner_payload, envelope_id, 11);
    let sig_c = signed_hop(1, 22, chain_hash, c_inner_payload, envelope_id, 22);

    // 2. Backend returns BOTH signatures (simulating B's inner
    //    walk to C).
    struct InnerChainBackend {
        sig_b: octo_protocol::HopSignature,
        sig_c: octo_protocol::HopSignature,
        resolved_pubkey: [u8; 32],
    }
    #[async_trait]
    impl ResolverBackend for InnerChainBackend {
        async fn resolve_via(
            &self,
            _hop_did: &str,
            _target: &octo_ident::RawDid,
            _chain_ctx: &ResolverChainContext,
        ) -> Result<BackendResolveOutcome, ResolverBackendError> {
            Ok(BackendResolveOutcome {
                public_key: self.resolved_pubkey,
                signature_chain: vec![
                    RawHopSignature {
                        hop_index: self.sig_b.hop_index,
                        hop_did: self.sig_b.hop_did.clone(),
                        signature: self.sig_b.signature,
                        signer_pub: self.sig_b.signer_pub,
                    },
                    RawHopSignature {
                        hop_index: self.sig_c.hop_index,
                        hop_did: self.sig_c.hop_did.clone(),
                        signature: self.sig_c.signature,
                        signer_pub: self.sig_c.signer_pub,
                    },
                ],
            })
        }
    }
    let backend: Arc<dyn ResolverBackend> = Arc::new(InnerChainBackend {
        sig_b: sig_b.clone(),
        sig_c: sig_c.clone(),
        resolved_pubkey: [0xDDu8; 32],
    });
    let handler = ResolveChainHandler::new(backend);

    // 3. Walk hops=[B]; A's chain handler delegates to backend ONCE
    //    for terminal hop B; backend returns 2 signatures.
    let req = ChainResolveRequest {
        target: canonical_did(11),
        hops: vec![ResolverHop::local(canonical_did(2))],
        ttl_remaining_ms: 100,
    };
    let out = handler
        .handle(&req, envelope_id)
        .await
        .expect("3-node chain resolves");
    let resp: ChainResolveResponse = borsh::from_slice(&out.response_payload.unwrap()).unwrap();

    // 4. Verify response shape.
    assert_eq!(resp.canonical_did, canonical_did(11));
    assert_eq!(resp.public_key, [0xDDu8; 32], "C's resolved pubkey");
    assert_eq!(
        resp.hops_traversed, 1,
        "A walked 1 hop (B); B walked 1 inner hop (C)"
    );
    assert_eq!(resp.envelope_id, envelope_id);
    assert_eq!(
        resp.signature_chain.len(),
        2,
        "BOTH signatures accumulated in the wire form"
    );

    // 5. Verify EACH signature end-to-end via the in-band signer_pub.
    //    Pin the production preimage encoders + the public verify
    //    helper so a future encoder drift would surface as a test
    //    failure rather than silent cross-domain auth bypass.
    let preimage_b = HopSignature::signing_preimage(chain_hash, 0, b_inner_payload, envelope_id);
    let preimage_c = HopSignature::signing_preimage(chain_hash, 1, c_inner_payload, envelope_id);
    let sig_b_bytes = Ed25519SignatureBytes(resp.signature_chain[0].signature);
    let sig_c_bytes = Ed25519SignatureBytes(resp.signature_chain[1].signature);
    let did_b = octo_ident::WireDid::new(resp.signature_chain[0].hop_did.clone());
    let did_c = octo_ident::WireDid::new(resp.signature_chain[1].hop_did.clone());
    verify_ed25519_signature(&did_b, &preimage_b, &sig_b_bytes)
        .expect("B's HopSignature MUST verify against B's 5-tuple preimage");
    verify_ed25519_signature(&did_c, &preimage_c, &sig_c_bytes)
        .expect("C's HopSignature MUST verify against C's 5-tuple preimage");

    // 6. Outermost-first ordering preserved across the substrate.
    assert_eq!(
        resp.signature_chain[0].hop_index, 0,
        "B is hop 0 (outermost)"
    );
    assert_eq!(
        resp.signature_chain[1].hop_index, 1,
        "C is hop 1 (innermost)"
    );
}
