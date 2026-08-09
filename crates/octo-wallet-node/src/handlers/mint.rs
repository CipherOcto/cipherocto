//! `WALLET_MINT_CAPABILITY` handler (RFC-0871 §Wallet Node Lifecycle).
//!
//! Receives: `<holder_did: String, capability: [u8; 32]>`.
//! Returns: `<minted_token: [u8: wire form]>`.
//!
//! Phase 1 MVP: creates a `CapabilityToken` via the existing
//! `octo_wallet::capability::CapabilityToken::mint` substrate. Initial
//! caveat list is empty (caveats travel in subsequent attenuation
//! envelope calls). The minted token's wire form is a placeholder
//! (`CIPHEROCTO_MINT_V1:<holder_did>`); the full macaroon wire format
//! is mission 0957 Phase 2 follow-on.

use borsh::{BorshDeserialize, BorshSerialize};
use octo_ident::DidCodec;
use octo_protocol::ProtocolError;
use octo_wallet::capability::CapabilityToken;
use octo_wallet::identity::IdentityKey;

use super::{did_error_to_protocol, wallet_error_to_protocol, HandlerOutput};

/// Request payload for `WALLET_MINT_CAPABILITY`.
///
/// Wire form: borsh (`holder_did`, `capability_root`).
#[derive(Clone, Debug, BorshSerialize, BorshDeserialize, PartialEq, Eq)]
pub struct MintRequest {
    /// Canonical DID of the holder (`did:octo:z<base58btc>`).
    pub holder_did: String,
    /// 32-byte capability root secret (random nonce per mint).
    pub capability: [u8; 32],
}

impl MintRequest {
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
        borsh::to_vec(self).map_err(|e| ProtocolError::AuthorizationFailed(e.to_string()))
    }
}

/// `WALLET_MINT_CAPABILITY` handler implementation.
pub struct MintHandler<'a> {
    identity: &'a IdentityKey,
}

impl<'a> MintHandler<'a> {
    /// Construct a new `MintHandler` bound to the given identity key.
    #[must_use]
    pub const fn new(identity: &'a IdentityKey) -> Self {
        Self { identity }
    }

    /// Mint a capability token via the existing `octo_wallet` substrate.
    ///
    /// # Errors
    /// Returns `ProtocolError::InvalidDid` if `holder_did` is not a
    /// canonical DID shape.
    pub fn handle(&self, req: &MintRequest) -> Result<HandlerOutput, ProtocolError> {
        // Validate holder_did shape via canonical codec; reject bare
        // legacy form (RFC-0010 v1.2 F4 + 0010-d mission).
        octo_ident::CanonicalCodec::parse(&req.holder_did, false).map_err(did_error_to_protocol)?;

        let token = CapabilityToken::mint(&req.capability, self.identity, &req.holder_did, &[])
            .map_err(wallet_error_to_protocol)?;

        // Phase 1 MVP wire form: `CIPHEROCTO_MINT_V1:<holder_did>`.
        let minted_wire = format!("CIPHEROCTO_MINT_V1:{}", token.holder_did).into_bytes();

        Ok(HandlerOutput::response(
            minted_wire,
            octo_protocol::payload_kind::WALLET_MINT_CAPABILITY,
        )
        .with_note(format!("minted for {}", req.holder_did)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_identity() -> octo_wallet::identity::IdentityKey {
        let mut seed = [0u8; 32];
        for (i, b) in seed.iter_mut().enumerate() {
            *b = i as u8;
        }
        octo_wallet::identity::IdentityKey::from_seed(seed)
    }

    fn sample_did() -> String {
        let id = sample_identity();
        let pk = id.public_key_bytes();
        let encoded = bs58::encode(&pk).into_string();
        format!("did:octo:z{encoded}")
    }

    #[test]
    fn mint_request_borsh_round_trip() {
        let req = MintRequest {
            holder_did: sample_did(),
            capability: [0xab; 32],
        };
        let bytes = req.to_borsh().unwrap();
        let back = MintRequest::from_borsh(&bytes).unwrap();
        assert_eq!(back, req);
    }

    #[test]
    fn handle_rejects_invalid_did() {
        let id = sample_identity();
        let handler = MintHandler::new(&id);
        let req = MintRequest {
            holder_did: "did:octo:bad".into(),
            capability: [0xab; 32],
        };
        let err = handler.handle(&req).unwrap_err();
        assert!(matches!(err, ProtocolError::InvalidDid(_)));
    }

    #[test]
    fn handle_mints_with_canonical_did() {
        let id = sample_identity();
        let handler = MintHandler::new(&id);
        let req = MintRequest {
            holder_did: sample_did(),
            capability: [0xab; 32],
        };
        let out = handler.handle(&req).unwrap();
        let payload = out.response_payload.unwrap();
        let s = std::str::from_utf8(&payload).unwrap();
        assert!(s.starts_with("CIPHEROCTO_MINT_V1:did:octo:z"));
    }
}
