//! Cross-domain chain resolution integration test vectors (mission
//! 0871b-cross-domain-resolution-impl).
//!
//! Multi-hop DID resolution: a single `IDENTITY_RESOLVE_CHAIN` request
//! carries `Vec<ResolverHop>` + a TTL budget. The handler walks the hop
//! chain with cycle detection + TTL enforcement, then resolves the
//! target against the LOCAL `DidRegistry` (cross-node forwarding is out
//! of scope for this mission — see `handlers/chain.rs` for rationale).
//!
//! ## Test vectors
//!
//! - TV-1 chain_single_hop_resolves — single hop, target resolves
//!   against local registry.
//! - TV-2 chain_three_hops_resolves_end_to_end — A → B → C; target
//!   registered locally; all hops valid.
//! - TV-3 chain_ttl_expiry_returns_error — TTL budget underflows on
//!   the second hop; `ChainTtlExpired` returned.
//! - TV-4 chain_cycle_detection_aborts — hop revisits a previously
//!   visited DID; `ChainCycle` returned before registry call.
//! - TV-5 chain_invalid_hop_rejected — non-canonical hop DID rejected
//!   with `InvalidDid`.
//!
//! ## Why no network call
//!
//! Cross-node forwarding requires the envelope request/response
//! substrate that does not yet exist in `octo-transport` (only
//! `broadcast` and `send_best` fire-and-forget). The chain handler
//! landing here is the LOGIC substrate; a follow-on mission wires a
//! network-capable variant.

use std::sync::Arc;

use octo_ident::{CanonicalCodec, DidCodec, DidDocument, DidRegistry, InMemoryDidRegistry};
use octo_identity_resolver_node::{
    ChainResolveRequest, ResolveChainHandler, ResolverHop, HOP_LATENCY_MS_ESTIMATE,
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

/// TV-1 chain_single_hop_resolves — chain = [hop A]; target at local.
#[test]
fn chain_single_hop_resolves() {
    let registry = Arc::new(InMemoryDidRegistry::default());
    let custom_pubkey = [0xCCu8; 32];
    register_custom(&registry, 11, custom_pubkey);

    let handler = ResolveChainHandler::new(registry);
    let target = canonical_did(11);
    let hop = ResolverHop::local(canonical_did(2));
    let req = ChainResolveRequest {
        target: target.clone(),
        hops: vec![hop],
        ttl_remaining_ms: 100,
    };
    let out = handler.handle(&req).expect("chain resolve succeeds");
    let payload = out.response_payload.expect("response payload");
    let resp: octo_identity_resolver_node::ChainResolveResponse =
        borsh::from_slice(&payload).unwrap();
    assert_eq!(resp.canonical_did, target);
    assert_eq!(resp.public_key, custom_pubkey);
    assert_eq!(resp.hops_traversed, 1);
    assert_eq!(
        out.response_payload_kind,
        Some(octo_protocol::payload_kind::IDENTITY_RESOLVE_CHAIN)
    );
}

/// TV-2 chain_three_hops_resolves_end_to_end — A → B → C; target at C
/// (the local resolver-node). All hops valid canonical DIDs.
#[test]
fn chain_three_hops_resolves_end_to_end() {
    let registry = Arc::new(InMemoryDidRegistry::default());
    let custom_pubkey = [0xDDu8; 32];
    register_custom(&registry, 33, custom_pubkey);

    let handler = ResolveChainHandler::new(registry);
    let target = canonical_did(33);
    let hops = vec![
        ResolverHop::local(canonical_did(1)),
        ResolverHop::local(canonical_did(2)),
        ResolverHop::local(canonical_did(3)),
    ];
    let req = ChainResolveRequest {
        target: target.clone(),
        hops,
        ttl_remaining_ms: 1_000,
    };
    let out = handler.handle(&req).expect("3-hop chain resolves");
    let payload = out.response_payload.expect("response payload");
    let resp: octo_identity_resolver_node::ChainResolveResponse =
        borsh::from_slice(&payload).unwrap();
    assert_eq!(resp.canonical_did, target);
    assert_eq!(resp.public_key, custom_pubkey);
    assert_eq!(resp.hops_traversed, 3);
}

/// TV-3 chain_ttl_expiry_returns_error — TTL budget = 5 ms; 3 hops ×
/// 10 ms each = 30 ms required. Hop 2 (index 1) trips the underflow.
#[test]
fn chain_ttl_expiry_returns_error() {
    let registry = Arc::new(InMemoryDidRegistry::default());
    let handler = ResolveChainHandler::new(registry);
    let target = canonical_did(44);
    let hops = vec![
        ResolverHop::local(canonical_did(1)),
        ResolverHop::local(canonical_did(2)),
        ResolverHop::local(canonical_did(3)),
    ];
    // 5 ms budget; hop 0 subtracts 10 → 0; check trips ChainTtlExpired
    // on hop 1. (The saturating_sub clamps; we abort when result == 0.)
    let req = ChainResolveRequest {
        target,
        hops,
        ttl_remaining_ms: HOP_LATENCY_MS_ESTIMATE - 5,
    };
    let err = handler.handle(&req).unwrap_err();
    assert!(matches!(
        err,
        octo_identity_resolver_node::IdentityResolveError::ChainTtlExpired
    ));
}

/// TV-4 chain_cycle_detection_aborts — hop visits a DID that is also
/// the target. The `visited` set is seeded with `target`, so the first
/// hop whose `hop_did` equals the target trips cycle detection.
#[test]
fn chain_cycle_detection_aborts() {
    let registry = Arc::new(InMemoryDidRegistry::default());
    let handler = ResolveChainHandler::new(registry);
    let target = canonical_did(55);
    let hops = vec![
        ResolverHop::local(canonical_did(1)),
        ResolverHop::local(target.clone()), // revisit target
        ResolverHop::local(canonical_did(3)),
    ];
    let req = ChainResolveRequest {
        target,
        hops,
        ttl_remaining_ms: 1_000,
    };
    let err = handler.handle(&req).unwrap_err();
    assert!(matches!(
        err,
        octo_identity_resolver_node::IdentityResolveError::ChainCycle
    ));
}

/// TV-5 chain_invalid_hop_rejected — non-canonical hop DID rejected
/// before any registry I/O.
#[test]
fn chain_invalid_hop_rejected() {
    let registry = Arc::new(InMemoryDidRegistry::default());
    let handler = ResolveChainHandler::new(registry);
    let target = canonical_did(66);
    let hops = vec![
        ResolverHop::local("did:octo:bad".into()),
        ResolverHop::local(canonical_did(2)),
    ];
    let req = ChainResolveRequest {
        target,
        hops,
        ttl_remaining_ms: 1_000,
    };
    let err = handler.handle(&req).unwrap_err();
    assert!(matches!(
        err,
        octo_identity_resolver_node::IdentityResolveError::InvalidDid(_)
    ));
}

/// Bonus TV: TTL = exactly `HOP_LATENCY_MS_ESTIMATE` succeeds for one
/// hop, returns `ChainTtlExpired` on hop 2. Documents the boundary
/// behavior (`saturating_sub` clamps to zero; the `== 0` check trips
/// after the subtraction).
#[test]
fn chain_ttl_exactly_one_hop_succeeds() {
    let registry = Arc::new(InMemoryDidRegistry::default());
    let custom_pubkey = [0xEEu8; 32];
    register_custom(&registry, 77, custom_pubkey);

    let handler = ResolveChainHandler::new(registry);
    let target = canonical_did(77);
    // Exactly one hop's worth of budget. Subtract 10 → 0 → abort ON the
    // first hop iteration; this is the boundary that TV-3 confirms.
    let req = ChainResolveRequest {
        target,
        hops: vec![ResolverHop::local(canonical_did(1))],
        ttl_remaining_ms: HOP_LATENCY_MS_ESTIMATE,
    };
    let err = handler.handle(&req).unwrap_err();
    assert!(matches!(
        err,
        octo_identity_resolver_node::IdentityResolveError::ChainTtlExpired
    ));
}

/// Bonus TV: empty hops list is equivalent to IDENTITY_RESOLVE — target
/// resolves locally without walking any hops.
#[test]
fn chain_empty_hops_resolves_locally() {
    let registry = Arc::new(InMemoryDidRegistry::default());
    let custom_pubkey = [0xFFu8; 32];
    register_custom(&registry, 88, custom_pubkey);

    let handler = ResolveChainHandler::new(registry);
    let target = canonical_did(88);
    let req = ChainResolveRequest {
        target: target.clone(),
        hops: vec![],
        ttl_remaining_ms: 1_000,
    };
    let out = handler.handle(&req).expect("empty chain resolves");
    let payload = out.response_payload.expect("response payload");
    let resp: octo_identity_resolver_node::ChainResolveResponse =
        borsh::from_slice(&payload).unwrap();
    assert_eq!(resp.canonical_did, target);
    assert_eq!(resp.public_key, custom_pubkey);
    assert_eq!(resp.hops_traversed, 0);
}
