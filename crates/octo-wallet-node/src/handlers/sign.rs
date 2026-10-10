//! `WALLET_SIGN_ED25519` handler (RFC-0871 §Wallet Node Lifecycle).
//!
//! Receives: `<payload: [u8; 32]>` — the 32-byte message to sign.
//! Returns: `<signature: [u8; 64]>` — the Ed25519 signature.
//!
//! # This is a signing oracle, so it binds its own domain
//!
//! The handler signs whatever 32 bytes it is handed. Before mission
//! `0009-e3` it did so over the bare digest, which made the output
//! indistinguishable from a signature the same key produced for any other
//! purpose — replayable anywhere a bare-message Ed25519 signature is
//! accepted. It now signs under the identity's mandatory signable prefix
//! (RFC-0009 §Domain separation), so every signature it emits names the
//! algorithm and the key it was made with.
//!
//! The prefix is applied by the signer, never chosen by the requester, and it
//! is not carried on the wire: `SignRequest` is still a bare `[u8; 32]`, so
//! RFC-0871's request contract is unchanged.

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

    /// Sign the message digest under the identity's mandatory signable prefix.
    ///
    /// The signature covers `algorithm_id ‖ key_id ‖ canonical_ser(digest)`,
    /// not the bare digest, so a signature produced here cannot be presented to
    /// a bare-message verifier or attributed to another key (RFC-0009
    /// §Domain separation).
    ///
    /// # Why the identity supplies the prefix, not the requester
    ///
    /// The alternative — the caller naming the `algorithm_id` and `key_id` to
    /// be signed under — would let any requester point this oracle at a
    /// different key or a different algorithm. On an endpoint whose entire
    /// purpose is to sign whatever it is handed, that inverts the control the
    /// prefix exists to establish. So the signer names its own domain, and
    /// `req.0` remains an opaque 32-byte value with no say in how it is
    /// framed.
    ///
    /// This is also why [`SignRequest`] stays a bare `[u8; 32]`: the prefix is
    /// not transported, so RFC-0871's request contract does not move. A
    /// verifier reconstructs the same wire form with `prefixed_message` before
    /// calling `verify_prefixed` — the prefix is still parsed and the key
    /// still checked, just against the resolved key context.
    ///
    /// # Errors
    /// Returns `ProtocolError::AuthorizationFailed` if signing fails,
    /// including `WalletError::NoKeyContext` when the identity carries no
    /// `key_id`. Refusing is intended: serving this handler with a bare
    /// signature would not be a fallback, it would be the default.
    pub fn handle(&self, req: &SignRequest) -> Result<HandlerOutput, ProtocolError> {
        let sig = self
            .identity
            .sign_prefixed(&req.0)
            .map_err(wallet_error_to_protocol)?;
        let payload =
            borsh::to_vec(&sig).map_err(|e| ProtocolError::AuthorizationFailed(e.to_string()))?;
        Ok(HandlerOutput::response(
            payload,
            octo_protocol::payload_kind::WALLET_SIGN_ED25519,
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use octo_ident::VerificationMethodId;
    use octo_wallet::identity::IdentityKey;

    fn seed(byte: u8) -> [u8; 32] {
        let mut s = [0u8; 32];
        for (i, b) in s.iter_mut().enumerate() {
            *b = byte.wrapping_add(i as u8);
        }
        s
    }

    /// An activated identity carrying a `key_id`.
    ///
    /// Both halves are attached explicitly rather than assumed. A fresh
    /// `from_seed` identity is `Designated` and cannot sign at all, and it
    /// carries no `verification_method_id` even once activated — so
    /// `sign_prefixed` refuses it with `NoKeyContext`. Production identities
    /// get the `key_id` from `identity_store::unlock`, which restores the
    /// persisted value (mission `0009-e2`); a fixture has to say so.
    fn identity_with_key(byte: u8, key_name: &str) -> IdentityKey {
        let mut id = IdentityKey::from_seed(seed(byte))
            .with_verification_method_id(VerificationMethodId::derive(key_name));
        id.activate(1_700_000_000).expect("designated → active");
        id
    }

    fn digest() -> [u8; 32] {
        let mut d = [0u8; 32];
        for (i, b) in d.iter_mut().enumerate() {
            *b = i as u8;
        }
        d
    }

    /// Ask the oracle for a signature and return `(digest, signature)`.
    fn sign_via_oracle(id: &IdentityKey) -> ([u8; 32], [u8; 64]) {
        let req = SignRequest(digest());
        let out = SignHandler::new(id)
            .handle(&req)
            .expect("oracle must serve");
        let payload = out.response_payload.expect("response payload");
        let sig: [u8; 64] = borsh::from_slice(&payload).expect("64-byte signature");
        (req.0, sig)
    }

    #[test]
    fn the_sign_request_wire_form_is_unchanged() {
        // RFC-0871 pins the request as a bare 32-byte message. The prefix is
        // applied by the signer and is not transported, so this contract must
        // not move: still 32 bytes on the wire, still a round-trip.
        let req = SignRequest([0xab; 32]);
        let bytes = req.to_borsh().unwrap();
        assert_eq!(
            bytes.len(),
            32,
            "the request contract is a bare 32-byte digest, not a prefixed message"
        );
        assert_eq!(SignRequest::from_borsh(&bytes).unwrap(), req);
    }

    #[test]
    fn the_oracle_signature_verifies_under_the_prefix() {
        let id = identity_with_key(1, "wallet-key-0");
        let (msg, sig) = sign_via_oracle(&id);

        // The verifier rebuilds the same wire form from the resolved key
        // context, then parses the prefix out of it as usual.
        let wire = id.prefixed_message(&msg).expect("key context present");
        id.verify_prefixed(&wire, &sig)
            .expect("an oracle signature must verify under the prefixed path");
    }

    #[test]
    fn the_oracle_refuses_to_emit_a_bare_digest_signature() {
        let id = identity_with_key(1, "wallet-key-0");
        let (msg, sig) = sign_via_oracle(&id);

        // The pre-`0009-e3` verification path: a bare signature over the raw
        // 32-byte digest, with the algorithm supplied out of band. It MUST
        // fail, or the signature is still replayable anywhere a bare-message
        // verifier sits.
        //
        // Read together with `the_oracle_signature_verifies_under_the_prefix`,
        // which proves the same signature is valid on the prefixed path — so
        // this is a real rejection and not an algorithm that simply fails to
        // resolve for both.
        let pk = id.public_key_bytes();
        let err = octo_wallet::verify_dispatch(&id.algorithm_id(), &pk, &msg, &sig)
            .expect_err("a bare-digest verifier must not accept this signature");
        assert!(
            err.to_string().contains("signature"),
            "expected a signature rejection, got: {err}"
        );
    }

    #[test]
    fn a_signature_from_one_identitys_oracle_is_refused_by_another() {
        let signer = identity_with_key(1, "wallet-key-0");
        let (msg, sig) = sign_via_oracle(&signer);

        // Same key material, same public key, DIFFERENT `key_id`. This
        // isolates the prefix's contribution: if only the key bytes mattered,
        // this would verify, because the verifying key is identical. It must
        // be refused on the key binding before any equation is evaluated.
        let other = identity_with_key(1, "wallet-key-1");
        let wire = signer.prefixed_message(&msg).expect("key context present");
        let err = other
            .verify_prefixed(&wire, &sig)
            .expect_err("a signature bound to another key_id must be refused");
        assert!(
            err.to_string().to_lowercase().contains("key"),
            "expected a key-binding rejection, got: {err}"
        );
    }

    #[test]
    fn an_identity_with_no_key_id_cannot_serve_the_oracle() {
        // `from_seed` attaches no verification method, and activation does not
        // add one. The handler must refuse rather than quietly fall back to a
        // bare signature — on an endpoint whose job is to sign anything, a
        // fallback would not be an edge case, it would be the behaviour.
        let mut id = IdentityKey::from_seed(seed(1));
        id.activate(1_700_000_000).expect("designated → active");
        assert!(
            id.verification_method_id().is_none(),
            "control: this identity really has no key context"
        );

        let err = SignHandler::new(&id)
            .handle(&SignRequest(digest()))
            .expect_err("an identity with no key_id must not serve the oracle");
        // Match the variant, then the reason. Asserting only on a loose
        // substring would pass on the lifecycle error too, and the lifecycle
        // check is not what is under test here — this identity is active.
        let ProtocolError::AuthorizationFailed(reason) = &err else {
            panic!("expected an authorization failure, got: {err:?}");
        };
        assert!(
            reason.contains("verification-method"),
            "the refusal must be the missing key context, not some other error: {reason}"
        );
    }
}
