//! `IDENTITY_RESOLVE` handler (RFC-0871 §Roles and Authorities).
//!
//! Receives: `<query: String>` — a canonical DID lookup target (the
//! `did:octo:z<base58btc>` wire form).
//! Returns: `<canonical_did: String, public_key: [u8; 32]>` — the canonical
//! DID wire form + the resolved storage-pubkey form.
//!
//! Mission 0871b-storage-backend: the placeholder `RawDid::hash` derivation
//! is replaced with a `DidRegistry::resolve(canonical_hash)` call. The
//! `registry` is injected at handler construction time (DI from
//! `IdentityResolverNodeConfig.registry`), enabling production storage
//! wiring via `StoolapDidRegistry` without coupling this Layer C crate
//! to `quota-router-storage`.

use std::sync::Arc;

use borsh::{BorshDeserialize, BorshSerialize};
use octo_ident::DidCodec;
use octo_ident::DidRegistry;

use super::{HandlerOutput, IdentityResolveError};

/// Request payload for `IDENTITY_RESOLVE`.
///
/// Wire form: borsh (`query`).
#[derive(Clone, Debug, BorshSerialize, BorshDeserialize, PartialEq, Eq)]
pub struct ResolveRequest(pub String);

impl ResolveRequest {
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
        borsh::to_vec(&self.0).map_err(|e| IdentityResolveError::Serialization(e.to_string()))
    }
}

/// Response payload for `IDENTITY_RESOLVE`.
///
/// Wire form: borsh (`canonical_did`, `public_key`).
#[derive(Clone, Debug, BorshSerialize, BorshDeserialize, PartialEq, Eq)]
pub struct ResolveResponse {
    /// Canonical DID wire form (post-parse).
    pub canonical_did: String,
    /// 32-byte storage-pubkey form (from `DidDocument.public_key`).
    pub public_key: [u8; 32],
}

/// `IDENTITY_RESOLVE` handler.
///
/// Mission 0871b-storage-backend: holds an `Arc<dyn DidRegistry>` for
/// production storage wiring. Replaces the placeholder
/// `RawDid::hash`-as-`public_key` derivation.
pub struct ResolveHandler {
    registry: Arc<dyn DidRegistry>,
}

impl ResolveHandler {
    /// Construct a new `ResolveHandler` bound to the supplied registry.
    ///
    /// The registry is injected (DI) — production deployments pass a
    /// `StoolapDidRegistry`; tests pass an `InMemoryDidRegistry`. The
    /// resolver-node consumer (`IdentityResolverNode::handle_envelope`)
    /// passes the registry cloned from `IdentityResolverNodeConfig.registry`.
    #[must_use]
    pub fn new(registry: Arc<dyn DidRegistry>) -> Self {
        Self { registry }
    }

    /// Resolve the DID query.
    ///
    /// Mission 0871b-storage-backend:
    /// 1. Validates `query` is a canonical DID shape via
    ///    `octo_ident::CanonicalCodec::parse(s, false)`.
    /// 2. Decodes the canonical wire form back to a `RawDid`; uses the
    ///    leading 32-byte `hash` as the registry lookup key.
    /// 3. Calls `registry.resolve(&raw.hash)`. Returns `Ok(None)` for
    ///    unknown AND revoked DIDs (fail-closed; the response surfaces
    ///    `Storage("unknown DID")` to the caller via `IdentityResolveError::Storage`).
    /// 4. Returns the resolved `public_key` in `ResolveResponse`.
    ///
    /// # Errors
    /// Returns `IdentityResolveError::InvalidDid` if `query` is not a
    /// canonical DID shape (legacy `did:octo:b<base32>` rejected per
    /// RFC-0010 F4). Returns `IdentityResolveError::Storage` if the
    /// underlying registry call fails.
    pub fn handle(&self, req: &ResolveRequest) -> Result<HandlerOutput, IdentityResolveError> {
        // 1. Validate canonical DID shape; reject legacy bare form.
        let wire = octo_ident::CanonicalCodec::parse(&req.0, false)
            .map_err(|e| IdentityResolveError::InvalidDid(e.to_string()))?;

        // 2. Decode canonical wire form → RawDid for lookup key.
        let raw = octo_ident::CanonicalCodec::wire_to_raw(&wire)
            .map_err(|e| IdentityResolveError::InvalidDid(e.to_string()))?;

        // 3. Look up via registry.
        let doc = self
            .registry
            .resolve(&raw.hash)?
            .ok_or_else(|| IdentityResolveError::Storage("unknown DID".to_owned()))?;

        let response = ResolveResponse {
            canonical_did: wire.as_str().to_owned(),
            public_key: doc.public_key,
        };
        let payload = borsh::to_vec(&response)
            .map_err(|e| IdentityResolveError::Serialization(e.to_string()))?;
        Ok(
            HandlerOutput::response(payload, octo_protocol::payload_kind::IDENTITY_RESOLVE)
                .with_note(format!("resolved {}", wire.as_str())),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use octo_ident::InMemoryDidRegistry;

    fn sample_did_bytes() -> [u8; 32] {
        let mut pk = [0u8; 32];
        for (i, b) in pk.iter_mut().enumerate() {
            *b = i as u8;
        }
        pk
    }

    fn sample_did() -> String {
        let pk = sample_did_bytes();
        let raw = octo_ident::CanonicalCodec::mint(&pk);
        let wire = octo_ident::CanonicalCodec::raw_to_wire(&raw).unwrap();
        wire.as_str().to_owned()
    }

    #[test]
    fn resolve_request_borsh_round_trip() {
        let req = ResolveRequest(sample_did());
        let bytes = req.to_borsh().unwrap();
        let back = ResolveRequest::from_borsh(&bytes).unwrap();
        assert_eq!(back, req);
    }

    #[test]
    fn handle_rejects_invalid_did() {
        let registry = Arc::new(InMemoryDidRegistry::default());
        let handler = ResolveHandler::new(registry);
        let req = ResolveRequest("did:octo:bad".into());
        let err = handler.handle(&req).unwrap_err();
        assert!(matches!(err, IdentityResolveError::InvalidDid(_)));
    }

    #[test]
    fn handle_rejects_legacy_bare_form() {
        // `allow_legacy_bare=false` must reject legacy `did:octo:b<base32>` form.
        let registry = Arc::new(InMemoryDidRegistry::default());
        let handler = ResolveHandler::new(registry);
        let legacy = "did:octo:babcdefghijklmnopqrstuvwxyz234567abcdefghijklmnopqrstuvwxyz2345";
        let req = ResolveRequest(legacy.into());
        let err = handler.handle(&req).unwrap_err();
        assert!(matches!(err, IdentityResolveError::InvalidDid(_)));
    }

    #[test]
    fn handle_returns_storage_error_for_unregistered_did() {
        // No register() call — registry returns None → handler surfaces
        // `IdentityResolveError::Storage("unknown DID")` (fail-closed).
        let registry = Arc::new(InMemoryDidRegistry::default());
        let handler = ResolveHandler::new(registry);
        let req = ResolveRequest(sample_did());
        let err = handler.handle(&req).unwrap_err();
        assert!(matches!(err, IdentityResolveError::Storage(_)));
    }

    #[test]
    fn handle_returns_canonical_did_and_registry_pubkey() {
        // Register a custom public_key; handler returns THAT key, not
        // the placeholder hash derived from the DID itself.
        let registry = Arc::new(InMemoryDidRegistry::default());
        let did_str = sample_did();
        let raw = octo_ident::CanonicalCodec::mint(&sample_did_bytes());
        let custom_pubkey = [0xCCu8; 32];
        registry
            .register(
                &raw.hash,
                octo_ident::DidDocument {
                    public_key: custom_pubkey,
                    revoked: false,
                    ..Default::default()
                },
            )
            .unwrap();
        let handler = ResolveHandler::new(registry);
        let req = ResolveRequest(did_str.clone());
        let out = handler.handle(&req).unwrap();
        let payload = out.response_payload.unwrap();
        let resp: ResolveResponse = borsh::from_slice(&payload).unwrap();
        assert_eq!(resp.canonical_did, did_str);
        assert_eq!(
            resp.public_key, custom_pubkey,
            "resolve_handler_uses_registry TV: public_key must come from registry, not placeholder hash"
        );
    }

    #[test]
    fn handle_byte_exact_cutover_placeholder_vs_registry() {
        // TV: wire_shape_byte_exact_across_cutover — when the registry's
        // DidDocument.public_key IS the placeholder hash, the response
        // is byte-equal to the pre-cutover placeholder output.
        let registry = Arc::new(InMemoryDidRegistry::default());
        let did_str = sample_did();
        let raw = octo_ident::CanonicalCodec::mint(&sample_did_bytes());
        // Register with public_key = raw.hash (the OLD placeholder).
        registry
            .register(
                &raw.hash,
                octo_ident::DidDocument {
                    public_key: raw.hash,
                    revoked: false,
                    ..Default::default()
                },
            )
            .unwrap();
        let handler = ResolveHandler::new(registry);
        let req = ResolveRequest(did_str.clone());
        let out = handler.handle(&req).unwrap();
        let payload = out.response_payload.unwrap();
        let resp: ResolveResponse = borsh::from_slice(&payload).unwrap();
        assert_eq!(resp.canonical_did, did_str);
        assert_eq!(resp.public_key, raw.hash);
    }
}
