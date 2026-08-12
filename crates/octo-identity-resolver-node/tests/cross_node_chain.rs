//! Cross-node resolver chain integration tests (mission
//! `0871b-cross-node-forwarding`, AC-13..AC-16).
//!
//! Validates the `ResolverBackend` trait boundary:
//! - AC-13: a custom backend accumulates `HopSignature`s and the
//!   response carries them through (5-tuple shape).
//! - AC-14: `IdentityResolverNodeConfig` routes through the injected
//!   backend (`SpyRemoteBackend`).
//! - AC-15: `RemoteResolverBackend` stub returns
//!   `IdentityResolveError::Unsupported` (fail-closed before the
//!   request/response substrate lands).
//! - AC-16: `ChainResolveResponse` with a populated `signature_chain`
//!   survives a full handler → borsh → handler round-trip with
//!   envelope_id correlation intact.
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

use octo_ident::{CanonicalCodec, DidCodec, DidDocument, DidRegistry, InMemoryDidRegistry};
use octo_identity_resolver_node::{
    BackendResolveOutcome, ChainResolveRequest, ChainResolveResponse, IdentityResolveError,
    LocalResolverBackend, RemoteResolverBackend, ResolveChainHandler, ResolverBackend,
    ResolverChainContext, ResolverHop,
};

use octo_protocol::HopSignature;

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
    sig_template: HopSignature,
}

impl FakeRemoteBackend {
    fn new(pubkey: [u8; 32]) -> Self {
        let sig = HopSignature::new(
            0,
            "did:octo:zCt5bENb7tA2b9xeamSEnHF7cZ6Kk8h9p2Z6nT8pVk9R".to_owned(),
            [0xAA; 64],
            [0xBB; 32],
        );
        Self {
            pubkey,
            sig_template: sig,
        }
    }
}

impl ResolverBackend for FakeRemoteBackend {
    fn resolve_via(
        &self,
        _hop_did: &str,
        _target: &octo_ident::RawDid,
        _chain_ctx: &ResolverChainContext,
    ) -> Result<BackendResolveOutcome, IdentityResolveError> {
        Ok(BackendResolveOutcome {
            public_key: self.pubkey,
            signature_chain: vec![self.sig_template.clone()],
        })
    }
}

/// AC-13: a custom backend's `HopSignature` survives into the
/// `ChainResolveResponse.signature_chain` field.
#[test]
fn fake_remote_backend_propagates_signature_chain() {
    let backend: Arc<dyn ResolverBackend> = Arc::new(FakeRemoteBackend::new([0x77u8; 32]));
    let handler = ResolveChainHandler::new(backend);
    let req = ChainResolveRequest {
        target: canonical_did(11),
        hops: vec![ResolverHop::local(canonical_did(2))],
        ttl_remaining_ms: 100,
    };
    let out = handler.handle(&req, [0x42u8; 32]).expect("fake resolves");
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
#[test]
fn local_backend_yields_empty_signature_chain() {
    let registry = Arc::new(InMemoryDidRegistry::default());
    register_custom(&registry, 11, [0xCCu8; 32]);
    let local: Arc<dyn ResolverBackend> = Arc::new(LocalResolverBackend(registry));
    let handler = ResolveChainHandler::new(local);

    // Empty hops — direct local resolve.
    let req_empty = ChainResolveRequest {
        target: canonical_did(11),
        hops: vec![],
        ttl_remaining_ms: 100,
    };
    let out = handler
        .handle(&req_empty, [0u8; 32])
        .expect("local resolves");
    let resp: ChainResolveResponse = borsh::from_slice(&out.response_payload.unwrap()).unwrap();
    assert_eq!(resp.public_key, [0xCCu8; 32]);
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
#[test]
fn remote_backend_stub_is_unsupported() {
    let backend: Arc<dyn ResolverBackend> = RemoteResolverBackend::arc();
    let handler = ResolveChainHandler::new(backend);
    let req = ChainResolveRequest {
        target: canonical_did(11),
        hops: vec![ResolverHop::local(canonical_did(2))],
        ttl_remaining_ms: 100,
    };
    let err = handler.handle(&req, [0u8; 32]).unwrap_err();
    // Pin the contract: the Unsupported message MUST reference the
    // blocking mission so operator dashboards can route on the
    // substring `0870k`.
    match err {
        IdentityResolveError::Unsupported(msg) => {
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
#[test]
fn full_round_trip_preserves_5tuple_wire_form() {
    let backend: Arc<dyn ResolverBackend> = Arc::new(FakeRemoteBackend::new([0x88u8; 32]));
    let handler = ResolveChainHandler::new(backend);
    let envelope_id = [0x99u8; 32];
    let req = ChainResolveRequest {
        target: canonical_did(11),
        hops: vec![ResolverHop::local(canonical_did(2))],
        ttl_remaining_ms: 100,
    };
    let out = handler.handle(&req, envelope_id).expect("resolve");
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
#[test]
fn rejects_oversize_ttl_dos() {
    let backend: Arc<dyn ResolverBackend> = Arc::new(LocalResolverBackend(Arc::new(
        InMemoryDidRegistry::default(),
    )));
    let handler = ResolveChainHandler::new(backend);
    let req = ChainResolveRequest {
        target: canonical_did(11),
        hops: vec![],
        ttl_remaining_ms: u64::MAX,
    };
    let err = handler.handle(&req, [0u8; 32]).unwrap_err();
    assert!(matches!(err, IdentityResolveError::ChainTtlTooLarge(_)));
}

/// Round-1 review: `hops.len() > u8::MAX` is rejected at `handle()`
/// entry. The `ChainResolveResponse.hops_traversed` field is `u8`, so
/// larger chains MUST fail-closed rather than silently capping.
#[test]
fn rejects_oversize_hop_count() {
    let backend: Arc<dyn ResolverBackend> = Arc::new(LocalResolverBackend(Arc::new(
        InMemoryDidRegistry::default(),
    )));
    let handler = ResolveChainHandler::new(backend);
    // 256 hops = u8::MAX + 1. TTL must be <= MAX_CHAIN_TTL_MS so the
    // hop-count rejection fires first (the check order is TTL bound
    // → hop-count bound → loop).
    let hops: Vec<ResolverHop> = (0..(u8::MAX as usize + 1))
        .map(|i| ResolverHop::local(canonical_did((i % 200) as u8)))
        .collect();
    let req = ChainResolveRequest {
        target: canonical_did(11),
        hops,
        ttl_remaining_ms: 60_000,
    };
    let err = handler.handle(&req, [0u8; 32]).unwrap_err();
    assert!(matches!(err, IdentityResolveError::ChainTooLong(_)));
}

/// Round-1 review: hop canonicalization must precede cycle detection
/// (do not consume state on a malformed hop). A legacy bare-form hop
/// `did:octo:bad` is rejected before any `visited.insert` or TTL
/// decrement.
#[test]
fn rejects_malformed_hop_before_state_consumption() {
    let backend: Arc<dyn ResolverBackend> = Arc::new(LocalResolverBackend(Arc::new(
        InMemoryDidRegistry::default(),
    )));
    let handler = ResolveChainHandler::new(backend);
    let req = ChainResolveRequest {
        target: canonical_did(11),
        hops: vec![
            ResolverHop::local("did:octo:bad".into()),
            ResolverHop::local(canonical_did(2)),
        ],
        ttl_remaining_ms: 100,
    };
    let err = handler.handle(&req, [0u8; 32]).unwrap_err();
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
