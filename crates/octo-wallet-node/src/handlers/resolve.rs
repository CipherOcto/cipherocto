//! `WALLET_RESOLVE_DID` handler (RFC-0871 §Wallet Node Lifecycle).
//!
//! Receives: `<query: String>` — a DID string to resolve.
//! Returns: `<canonical_did: String, public_key: [u8; 32]>`.

use borsh::{BorshDeserialize, BorshSerialize};
use octo_ident::DidCodec;
use octo_protocol::ProtocolError;
use octo_wallet::identity::IdentityKey;

use super::{did_error_to_protocol, HandlerOutput};

/// Request payload for `WALLET_RESOLVE_DID`.
///
/// Wire form: borsh (`query`).
#[derive(Clone, Debug, BorshSerialize, BorshDeserialize, PartialEq, Eq)]
pub struct ResolveDIDRequest(pub String);

impl ResolveDIDRequest {
    /// Decode from borsh wire form.
    /// # Errors
    /// Returns `ProtocolError::AuthorizationFailed` if borsh decode fails.
    pub fn from_borsh(bytes: &[u8]) -> Result<Self, ProtocolError> {
        borsh::from_slice(bytes).map_err(|e| ProtocolError::AuthorizationFailed(e.to_string()))
    }

    /// Encode to borsh wire form.
    /// # Errors
    /// Returns `ProtocolError::AuthorizationFailed` if borsh encode fails.
    pub fn to_borsh(&self) -> Result<Vec<u8>, ProtocolError> {
        borsh::to_vec(&self.0).map_err(|e| ProtocolError::AuthorizationFailed(e.to_string()))
    }
}

/// `WALLET_RESOLVE_DID` handler.
///
/// Phase 1 MVP: only validates the canonical DID shape via
/// `octo_ident::CanonicalCodec::parse`. The actual lookup (storage,
/// upstream identity resolver node, etc.) lands in mission 0871b
/// (identity-resolver-node) and is plugged in here via the
/// `ResolverBackend` trait in the follow-on mission.
pub struct ResolveDIDHandler<'a> {
    identity: &'a IdentityKey,
}

impl<'a> ResolveDIDHandler<'a> {
    /// Construct a new `ResolveDIDHandler` bound to the given identity
    /// key. The bound identity is used for the response envelope's
    /// `from_did` (the resolver's identity, not the requester's).
    #[must_use]
    pub const fn new(identity: &'a IdentityKey) -> Self {
        Self { identity }
    }

    /// Resolve the DID query.
    ///
    /// Phase 1 MVP: validates the canonical DID shape; returns the
    /// canonical wire form + the bound identity's `public_key_bytes()`
    /// as a placeholder for the storage-pubkey form. Future mission
    /// (0871b) wires the real resolver backend.
    ///
    /// # Errors
    /// Returns `ProtocolError::InvalidDid` if `query` is not a
    /// canonical DID shape.
    pub fn handle(&self, req: &ResolveDIDRequest) -> Result<HandlerOutput, ProtocolError> {
        // Validate canonical DID shape; reject legacy bare form.
        let _parsed =
            octo_ident::CanonicalCodec::parse(&req.0, false).map_err(did_error_to_protocol)?;
        let response = ResolveDIDResponse {
            canonical_did: req.0.clone(),
            public_key: self.identity.public_key_bytes(),
        };
        let payload = borsh::to_vec(&response)
            .map_err(|e| ProtocolError::AuthorizationFailed(e.to_string()))?;
        Ok(
            HandlerOutput::response(payload, octo_protocol::payload_kind::WALLET_RESOLVE_DID)
                .with_note(format!("resolved {}", req.0)),
        )
    }
}

/// Response payload for `WALLET_RESOLVE_DID`.
///
/// Wire form: borsh (`canonical_did`, `public_key`).
#[derive(Clone, Debug, BorshSerialize, BorshDeserialize, PartialEq, Eq)]
pub struct ResolveDIDResponse {
    /// Canonical DID wire form (post-parse).
    pub canonical_did: String,
    /// 32-byte verifying key (placeholder for storage-pubkey form).
    pub public_key: [u8; 32],
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_identity() -> IdentityKey {
        let mut seed = [0u8; 32];
        for (i, b) in seed.iter_mut().enumerate() {
            *b = i as u8;
        }
        IdentityKey::from_seed(seed)
    }

    fn sample_did() -> String {
        let id = sample_identity();
        let pk = id.public_key_bytes();
        let encoded = bs58::encode(&pk).into_string();
        format!("did:octo:z{encoded}")
    }

    #[test]
    fn resolve_request_borsh_round_trip() {
        let req = ResolveDIDRequest(sample_did());
        let bytes = req.to_borsh().unwrap();
        let back = ResolveDIDRequest::from_borsh(&bytes).unwrap();
        assert_eq!(back, req);
    }

    #[test]
    fn handle_rejects_invalid_did() {
        let id = sample_identity();
        let handler = ResolveDIDHandler::new(&id);
        let req = ResolveDIDRequest("did:octo:bad".into());
        let err = handler.handle(&req).unwrap_err();
        assert!(matches!(err, ProtocolError::InvalidDid(_)));
    }

    #[test]
    fn handle_returns_canonical_did() {
        let id = sample_identity();
        let handler = ResolveDIDHandler::new(&id);
        let req = ResolveDIDRequest(sample_did());
        let out = handler.handle(&req).unwrap();
        let payload = out.response_payload.unwrap();
        let resp: ResolveDIDResponse = borsh::from_slice(&payload).unwrap();
        assert_eq!(resp.canonical_did, req.0);
        assert_eq!(resp.public_key, id.public_key_bytes());
    }
}
