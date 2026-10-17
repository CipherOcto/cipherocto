//! Mission 0010-f2-multi-chain-routing — IDENTITY_RESOLVE_WITH_CHAIN
//! dispatch TV.
//!
//! Verifies that:
//! - `IDENTITY_RESOLVE_WITH_CHAIN` payload kind is accepted by
//!   `IdentityResolverNode::handle_envelope`.
//! - The handler calls `registry.resolve_in_chain` (additive trait
//!   method from mission `0010-f2-registry-namespacing`, commit
//!   `a7efaabb`).
//! - Distinct docs on the same `canonical_hash` registered via
//!   different chains resolve independently.
//! - Malformed `chain_id` literal fails closed (no implicit
//!   mainnet default).
//!
//! Uses a local `MultiChainMockRegistry` that overrides
//! `register_in_chain` + `resolve_in_chain` (the `InMemoryDidRegistry`
//! fixture falls back to single-chain mode — too narrow for the
//! isolation assertion).

use std::collections::HashMap;
use std::sync::Arc;

use octo_ident::{ChainId, DidCodec, DidDocument, DidRegistry, DidRegistryError};
use octo_identity_resolver_node::handlers::{ResolveWithChainHandler, ResolveWithChainRequest};
use octo_protocol::payload_kind::IDENTITY_RESOLVE_WITH_CHAIN;
use parking_lot::RwLock;

/// Multi-chain mock registry for tests.
///
/// Holds `HashMap<(chain_id_literal, canonical_hash), DidDocument>`.
/// Overrides the additive `register_in_chain` + `resolve_in_chain`
/// methods (mission `0010-f2-registry-namespacing`); the
/// single-chain `register` / `resolve` methods fall back to the
/// mainnet namespace.
#[derive(Default)]
pub struct MultiChainMockRegistry {
    inner: RwLock<HashMap<(String, [u8; 32]), DidDocument>>,
}

impl DidRegistry for MultiChainMockRegistry {
    fn register(
        &self,
        canonical_hash: &[u8; 32],
        doc: DidDocument,
    ) -> Result<(), DidRegistryError> {
        self.register_in_chain(&ChainId::default(), canonical_hash, doc)
    }

    fn resolve(&self, canonical_hash: &[u8; 32]) -> Result<Option<DidDocument>, DidRegistryError> {
        self.resolve_in_chain(&ChainId::default(), canonical_hash)
    }

    fn register_in_chain(
        &self,
        chain_id: &ChainId,
        canonical_hash: &[u8; 32],
        doc: DidDocument,
    ) -> Result<(), DidRegistryError> {
        self.inner
            .write()
            .insert((chain_id.to_string(), *canonical_hash), doc);
        Ok(())
    }

    fn resolve_in_chain(
        &self,
        chain_id: &ChainId,
        canonical_hash: &[u8; 32],
    ) -> Result<Option<DidDocument>, DidRegistryError> {
        Ok(self
            .inner
            .read()
            .get(&(chain_id.to_string(), *canonical_hash))
            .cloned())
    }

    fn revoke(&self, _: &[u8; 32]) -> Result<(), DidRegistryError> {
        Err(DidRegistryError::UnknownDid)
    }

    fn list(&self) -> Result<Vec<DidDocument>, DidRegistryError> {
        Ok(self.inner.read().values().cloned().collect())
    }
}

fn sample_hash(seed: u8) -> [u8; 32] {
    let mut h = [0u8; 32];
    for (i, b) in h.iter_mut().enumerate() {
        *b = seed.wrapping_add(i as u8);
    }
    h
}

fn sample_doc(seed: u8) -> DidDocument {
    DidDocument {
        public_key: sample_hash(seed),
        revoked: false,
        ..Default::default()
    }
}

#[test]
fn resolve_with_chain_handler_routes_to_correct_chain() {
    let reg = Arc::new(MultiChainMockRegistry::default());

    let hash = sample_hash(0x42);
    let mainnet_doc = sample_doc(0x10);
    let partner_doc = sample_doc(0x20);

    reg.register_in_chain(&ChainId::default(), &hash, mainnet_doc.clone())
        .expect("register on mainnet");
    reg.register_in_chain(
        &ChainId::new("partner-mainnet").expect("valid partner"),
        &hash,
        partner_doc.clone(),
    )
    .expect("register on partner");

    let wire = octo_ident::CanonicalCodec::raw_to_wire(&octo_ident::RawDid {
        hash,
        version_discriminator: [0u8; 20],
    })
    .expect("raw_to_wire");
    let wire_str = wire.as_str().to_owned();

    // Mainnet resolve.
    let req_mainnet = ResolveWithChainRequest {
        query: wire_str.clone(),
        chain_id: ChainId::default().to_string(),
    };
    let out_mainnet = ResolveWithChainHandler::new(reg.clone())
        .handle(&req_mainnet)
        .expect("handle mainnet");
    let resp_mainnet: octo_identity_resolver_node::handlers::ResolveWithChainResponse =
        borsh::from_slice(out_mainnet.response_payload.as_ref().expect("payload"))
            .expect("borsh decode");
    assert_eq!(resp_mainnet.public_key, mainnet_doc.public_key);

    // Partner resolve → partner doc.
    let req_partner = ResolveWithChainRequest {
        query: wire_str,
        chain_id: "partner-mainnet".to_owned(),
    };
    let out_partner = ResolveWithChainHandler::new(reg)
        .handle(&req_partner)
        .expect("handle partner");
    let resp_partner: octo_identity_resolver_node::handlers::ResolveWithChainResponse =
        borsh::from_slice(out_partner.response_payload.as_ref().expect("payload"))
            .expect("borsh decode");
    assert_eq!(resp_partner.public_key, partner_doc.public_key);
}

#[test]
fn resolve_with_chain_rejects_malformed_chain_id() {
    let reg = Arc::new(MultiChainMockRegistry::default());
    let wire = octo_ident::CanonicalCodec::raw_to_wire(&octo_ident::RawDid {
        hash: sample_hash(0x99),
        version_discriminator: [0u8; 20],
    })
    .expect("raw_to_wire");
    let req = ResolveWithChainRequest {
        query: wire.as_str().to_owned(),
        chain_id: "bad\0chain".to_owned(),
    };
    let err = ResolveWithChainHandler::new(reg)
        .handle(&req)
        .expect_err("malformed chain_id must fail");
    assert!(
        matches!(
            err,
            octo_identity_resolver_node::handlers::IdentityResolveError::InvalidChainId(_)
        ),
        "expected InvalidChainId, got {err:?}"
    );
}

#[test]
fn payload_kind_is_advertised_in_identity_resolver_kinds() {
    use octo_identity_resolver_node::IDENTITY_RESOLVER_PAYLOAD_KINDS;
    assert!(
        IDENTITY_RESOLVER_PAYLOAD_KINDS.contains(&IDENTITY_RESOLVE_WITH_CHAIN),
        "IDENTITY_RESOLVE_WITH_CHAIN must appear in IDENTITY_RESOLVER_PAYLOAD_KINDS"
    );
}

#[test]
fn resolve_with_chain_request_borsh_roundtrip() {
    let req = ResolveWithChainRequest {
        query: "did:octo:zTest".to_owned(),
        chain_id: "partner-mainnet".to_owned(),
    };
    let bytes = req.to_borsh().expect("encode");
    let decoded = ResolveWithChainRequest::from_borsh(&bytes).expect("decode");
    assert_eq!(req, decoded);
}
