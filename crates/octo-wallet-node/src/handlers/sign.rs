//! `WALLET_SIGN_ED25519` handler (RFC-0871 §Wallet Node Lifecycle).
//!
//! Receives: `<payload: [u8; 32]>` — the 32-byte message to sign.
//! Returns: `<signature: [u8; 64]>` — the Ed25519 signature.

use borsh::{BorshDeserialize, BorshSerialize};
use octo_protocol::ProtocolError;
use octo_wallet::identity::IdentityKey;

use super::{wallet_error_to_protocol, HandlerOutput};

/// Request payload for `WALLET_SIGN_ED25519`.
///
/// Wire form: `borsh::to_vec(&[u8; 32])` — 32-byte message digest.
#[derive(Clone, Debug, BorshSerialize, BorshDeserialize, PartialEq, Eq)]
pub struct SignRequest(pub [u8; 32]);

impl SignRequest {
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

/// `WALLET_SIGN_ED25519` handler implementation.
pub struct SignHandler<'a> {
    identity: &'a IdentityKey,
}

impl<'a> SignHandler<'a> {
    /// Construct a new `SignHandler` bound to the given identity key.
    #[must_use]
    pub const fn new(identity: &'a IdentityKey) -> Self {
        Self { identity }
    }

    /// Sign the message digest via `IdentityKey::sign` (HSM-routed).
    ///
    /// # Errors
    /// Returns `ProtocolError::AuthorizationFailed` if signing fails.
    pub fn handle(&self, req: &SignRequest) -> Result<HandlerOutput, ProtocolError> {
        let sig = self
            .identity
            .sign(&req.0)
            .map_err(wallet_error_to_protocol)?;
        let payload = borsh::to_vec(&sig.to_bytes())
            .map_err(|e| ProtocolError::AuthorizationFailed(e.to_string()))?;
        Ok(HandlerOutput::response(
            payload,
            octo_protocol::payload_kind::WALLET_SIGN_ED25519,
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use octo_wallet::identity::IdentityKey;

    fn sample_identity() -> IdentityKey {
        let mut seed = [0u8; 32];
        for (i, b) in seed.iter_mut().enumerate() {
            *b = i as u8;
        }
        // RFC-0009 §Lifecycle: a fresh `from_seed` identity is
        // `Designated` and cannot sign. Activate it before the
        // HSM-routed sign call (mission 0957-phase2a fix; the test
        // was broken by 0009-l1 lifecycle state-machine enforcement).
        let mut id = IdentityKey::from_seed(seed);
        id.activate(1_700_000_000).expect("designated → active");
        id
    }

    #[test]
    fn sign_request_borsh_round_trip() {
        let req = SignRequest([0xab; 32]);
        let bytes = req.to_borsh().unwrap();
        let back = SignRequest::from_borsh(&bytes).unwrap();
        assert_eq!(back, req);
    }

    #[test]
    fn handle_signs_request_through_hsm() {
        let id = sample_identity();
        let handler = SignHandler::new(&id);
        let mut msg = [0u8; 32];
        for (i, b) in msg.iter_mut().enumerate() {
            *b = i as u8;
        }
        let req = SignRequest(msg);
        let out = handler.handle(&req).unwrap();
        let payload = out.response_payload.unwrap();
        let sig_bytes: [u8; 64] = borsh::from_slice(&payload).unwrap();
        // Verify signature against the public key via ed25519_dalek (dev-dep).
        // Production verification uses the NodeDispatcher's verifier chain.
        use ed25519_dalek::Verifier;
        let sig = ed25519_dalek::Signature::from_bytes(&sig_bytes);
        let pk = id.public_key_bytes();
        let vk = ed25519_dalek::VerifyingKey::from_bytes(&pk).unwrap();
        assert!(vk.verify(&req.0, &sig).is_ok());
    }
}
