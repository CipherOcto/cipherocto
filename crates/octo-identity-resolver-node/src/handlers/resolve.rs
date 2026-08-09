//! `IDENTITY_RESOLVE` handler (RFC-0871 §Roles and Authorities).
//!
//! Receives: `<query: String>` — a canonical DID lookup target (the
//! `did:octo:z<base58btc>` wire form).
//! Returns: `<canonical_did: String, public_key: [u8; 32]>` — the canonical
//! DID wire form + the resolved storage-pubkey form.
//!
//! Phase 1 MVP: validates the canonical DID shape via
//! `octo_ident::CanonicalCodec::parse(s, false)`. The actual lookup
//! (storage layer, upstream identity resolver node, etc.) lands in a
//! follow-on mission that wires a `DidRegistry` backend. The placeholder
//! `public_key` field returns the canonical DID's underlying 32-byte hash
//! (deterministic, derived from `WireDid::as_str()`).

use borsh::{BorshDeserialize, BorshSerialize};
use octo_ident::DidCodec;

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
    /// 32-byte storage-pubkey form (placeholder for real lookup backend).
    pub public_key: [u8; 32],
}

/// `IDENTITY_RESOLVE` handler.
///
/// Phase 1 MVP: stateless. Validates the canonical DID shape via
/// `octo_ident::CanonicalCodec::parse(s, false)`; returns the canonical
/// wire form + a placeholder 32-byte public key derived from the DID's
/// inner hash bytes. The real lookup backend (storage layer per RFC-0010
/// dual storage/wire split) lands in a follow-on mission.
pub struct ResolveHandler;

impl ResolveHandler {
    /// Construct a new `ResolveHandler`.
    #[must_use]
    pub const fn new() -> Self {
        Self
    }

    /// Resolve the DID query.
    ///
    /// Phase 1 MVP:
    /// 1. Validates `query` is a canonical DID shape via
    ///    `octo_ident::CanonicalCodec::parse(s, false)`.
    /// 2. Decodes the canonical wire form back to a `RawDid` and uses the
    ///    leading 32-byte hash as the placeholder `public_key` (deterministic,
    ///    no real storage backend required).
    ///
    /// # Errors
    /// Returns `IdentityResolveError::InvalidDid` if `query` is not a
    /// canonical DID shape (legacy `did:octo:b<base32>` rejected per
    /// RFC-0010 v1.2 F4).
    pub fn handle(&self, req: &ResolveRequest) -> Result<HandlerOutput, IdentityResolveError> {
        // 1. Validate canonical DID shape; reject legacy bare form.
        let wire = octo_ident::CanonicalCodec::parse(&req.0, false)
            .map_err(|e| IdentityResolveError::InvalidDid(e.to_string()))?;

        // 2. Decode canonical wire form → RawDid for placeholder pubkey.
        let raw = octo_ident::CanonicalCodec::wire_to_raw(&wire)
            .map_err(|e| IdentityResolveError::InvalidDid(e.to_string()))?;
        let response = ResolveResponse {
            canonical_did: wire.as_str().to_owned(),
            public_key: raw.hash,
        };
        let payload = borsh::to_vec(&response)
            .map_err(|e| IdentityResolveError::Serialization(e.to_string()))?;
        Ok(
            HandlerOutput::response(payload, octo_protocol::payload_kind::IDENTITY_RESOLVE)
                .with_note(format!("resolved {}", wire.as_str())),
        )
    }
}

impl Default for ResolveHandler {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
        let handler = ResolveHandler::new();
        let req = ResolveRequest("did:octo:bad".into());
        let err = handler.handle(&req).unwrap_err();
        assert!(matches!(err, IdentityResolveError::InvalidDid(_)));
    }

    #[test]
    fn handle_rejects_legacy_bare_form() {
        // `allow_legacy_bare=false` must reject legacy `did:octo:b<base32>` form.
        let handler = ResolveHandler::new();
        let legacy = "did:octo:babcdefghijklmnopqrstuvwxyz234567abcdefghijklmnopqrstuvwxyz2345";
        let req = ResolveRequest(legacy.into());
        let err = handler.handle(&req).unwrap_err();
        assert!(matches!(err, IdentityResolveError::InvalidDid(_)));
    }

    #[test]
    fn handle_returns_canonical_did_and_placeholder_pubkey() {
        let handler = ResolveHandler::new();
        let did_str = sample_did();
        let req = ResolveRequest(did_str.clone());
        let out = handler.handle(&req).unwrap();
        let payload = out.response_payload.unwrap();
        let resp: ResolveResponse = borsh::from_slice(&payload).unwrap();
        assert_eq!(resp.canonical_did, did_str);
        // Placeholder public_key = leading 32-byte hash of RawDid (deterministic).
        let expected_hash = octo_ident::CanonicalCodec::mint(&sample_did_bytes()).hash;
        assert_eq!(resp.public_key, expected_hash);
    }

    #[test]
    fn handler_unit_constructs() {
        let _ = ResolveHandler;
    }
}
