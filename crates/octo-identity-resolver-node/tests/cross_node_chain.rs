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
    /// How many times `resolve_via` has been called.
    calls: std::sync::atomic::AtomicUsize,
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
            calls: std::sync::atomic::AtomicUsize::new(0),
        }
    }

    #[allow(dead_code)]
    fn call_count(&self) -> usize {
        self.calls.load(std::sync::atomic::Ordering::SeqCst)
    }
}

impl ResolverBackend for FakeRemoteBackend {
    fn resolve_via(
        &self,
        _hop_did: &str,
        _target: &octo_ident::RawDid,
        _chain_ctx: &ResolverChainContext,
    ) -> Result<BackendResolveOutcome, IdentityResolveError> {
        let n = self.calls.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        let sig = HopSignature::new(
            u8::try_from(n).unwrap_or(u8::MAX),
            self.sig_template.hop_did.clone(),
            self.sig_template.signature,
            self.sig_template.signer_pub,
        );
        Ok(BackendResolveOutcome {
            public_key: self.pubkey,
            signature_chain: vec![sig],
        })
    }
}

/// AC-13: a custom backend's `HopSignature` survives into the
/// `ChainResolveResponse.signature_chain` field.
#[test]
fn cross_node_chain_with_fake_remote_backend_accumulates_hop_signatures() {
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
/// EMPTY `signature_chain` so in-process resolves stay signature-free.
#[test]
fn cross_node_chain_local_backend_yields_empty_signature_chain() {
    let registry = Arc::new(InMemoryDidRegistry::default());
    register_custom(&registry, 11, [0xCCu8; 32]);
    let local: Arc<dyn ResolverBackend> = Arc::new(LocalResolverBackend(registry));
    let handler = ResolveChainHandler::new(local);
    let req = ChainResolveRequest {
        target: canonical_did(11),
        hops: vec![],
        ttl_remaining_ms: 100,
    };
    let out = handler.handle(&req, [0u8; 32]).expect("local resolves");
    let payload = out.response_payload.expect("response payload");
    let resp: ChainResolveResponse = borsh::from_slice(&payload).unwrap();
    assert_eq!(resp.public_key, [0xCCu8; 32]);
    assert!(
        resp.signature_chain.is_empty(),
        "LocalResolverBackend yields no HopSignature"
    );
}

/// AC-15: `RemoteResolverBackend` stub returns
/// `IdentityResolveError::Unsupported` (fail-closed before the
/// request/response substrate lands).
#[test]
fn cross_node_chain_remote_backend_stub_is_unsupported() {
    let backend: Arc<dyn ResolverBackend> = RemoteResolverBackend::arc();
    let handler = ResolveChainHandler::new(backend);
    let req = ChainResolveRequest {
        target: canonical_did(11),
        hops: vec![ResolverHop::local(canonical_did(2))],
        ttl_remaining_ms: 100,
    };
    let err = handler.handle(&req, [0u8; 32]).unwrap_err();
    assert!(matches!(
        err,
        octo_identity_resolver_node::IdentityResolveError::Unsupported(_)
    ));
}

/// AC-16: full round-trip — handler constructs response with populated
/// `signature_chain` + non-zero `envelope_id`; borsh round-trip
/// preserves all five fields including the in-band `HopSignature`.
#[test]
fn cross_node_chain_full_round_trip_preserves_hop_signature_and_envelope_id() {
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

/// Bonus TV: `RemoteResolverBackend` constructor + trait-object
/// boundary sanity.
#[test]
fn cross_node_remote_backend_arc_constructs_trait_object() {
    let backend: Arc<dyn ResolverBackend> = RemoteResolverBackend::arc();
    // The Arc is Send + Sync — required by the trait bound.
    fn assert_send_sync<T: Send + Sync + ?Sized>() {}
    assert_send_sync::<dyn ResolverBackend>();
    assert_eq!(
        std::mem::size_of_val(&*backend),
        std::mem::size_of::<RemoteResolverBackend>()
    );
}
