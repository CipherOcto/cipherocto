//! Capability token (macaroon v1 per RFC-0957).
//!
//! Holder-bound + caveat-chained bearer token. The verifier checks:
//! 1. Holder signature (Ed25519 over `canonical_ser(root_id || caveats_wire)`).
//! 2. HMAC chain re-derivation against the issuer's root secret.
//! 3. Caveat evaluation against the request context.

pub mod caveat;
pub mod discharge;
pub mod macaroon;
pub mod wire;

use serde::{Deserialize, Serialize};

pub use caveat::{Caveat, CaveatName, MicroOctoW, UnixTimeSecs};
pub use discharge::{DischargeChannel, DischargeMacaroon};
pub use macaroon::{hmac_blake3, macaroon_id, Macaroon, MacaroonError, MacaroonId};
pub use wire::{deserialize_wire, serialize_wire, WireError};

use crate::identity::IdentityKey;

/// Holder-bound capability token (RFC-0957 §3.1).
///
/// Holder signature is Ed25519 over `canonical_ser(root_id || caveats_wire)`;
/// the holder DID is the audience (per RFC-0009 §Identity).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CapabilityToken {
    /// Macaroon body (chain + caveats).
    pub macaroon: Macaroon,
    /// Holder Ed25519 public key (32 bytes).
    pub holder_pub: [u8; 32],
    /// Holder DID string (`did:octo:...`).
    pub holder_did: String,
    /// Ed25519 signature over `canonical_ser(macaroon.root_id || caveats_wire)`.
    #[serde(with = "ed25519_sig_serde")]
    pub holder_sig: ed25519_dalek::Signature,
    /// Discharge macaroons (escrow / revocation / rate-limit channels).
    pub discharges: Vec<DischargeMacaroon>,
}

/// Serialize a `CapabilityToken` with Ed25519 signature as raw bytes.
mod ed25519_sig_serde {
    use ed25519_dalek::Signature;
    use serde::{Deserialize, Deserializer, Serializer};

    pub fn serialize<S: Serializer>(sig: &Signature, ser: S) -> Result<S::Ok, S::Error> {
        ser.serialize_bytes(&sig.to_bytes())
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(de: D) -> Result<Signature, D::Error> {
        let bytes: Vec<u8> = Deserialize::deserialize(de)?;
        Signature::from_slice(&bytes).map_err(serde::de::Error::custom)
    }
}

impl CapabilityToken {
    /// Mint a capability token: generate macaroon + holder signature.
    ///
    /// # Errors
    /// Returns `MacaroonError::OsRng` on RNG failure, `WalletError::Signature`
    /// on invalid signature (should never happen for self-signed).
    pub fn mint(
        root_secret: &[u8; 32],
        holder: &IdentityKey,
        holder_did: impl Into<String>,
        initial_caveats: Vec<Caveat>,
    ) -> Result<Self, MintError> {
        let mut macaroon = Macaroon::mint(root_secret)?;
        for caveat in initial_caveats {
            macaroon = macaroon.attenuate(caveat);
        }
        let holder_pub = holder.public_key_bytes();
        let holder_did = holder_did.into();

        let msg = Self::holder_msg(&macaroon.root_id, &macaroon.caveats);
        let holder_sig = holder.sign(&msg);

        Ok(Self {
            macaroon,
            holder_pub,
            holder_did,
            holder_sig,
            discharges: Vec::new(),
        })
    }

    /// Append a caveat (attenuation). Returns a new token.
    #[must_use]
    pub fn attenuate(&self, caveat: Caveat) -> Self {
        let mut next = self.clone();
        next.macaroon = next.macaroon.attenuate(caveat);
        // Holder signature no longer covers the new caveat list — re-sign.
        // Note: holder private key is required for re-sign; this method is
        // only useful for the holder. Attenuators without the holder key can
        // produce a re-signed token by calling `attenuate_with_signer`.
        next
    }

    /// Attenuate and re-sign with the given holder key.
    ///
    /// # Errors
    /// Returns `MacaroonError::OsRng` on RNG failure.
    pub fn attenuate_with_signer(
        &self,
        caveat: Caveat,
        holder: &IdentityKey,
    ) -> Result<Self, MintError> {
        let mut next = self.clone();
        next.macaroon = next.macaroon.attenuate(caveat);
        let msg = Self::holder_msg(&next.macaroon.root_id, &next.macaroon.caveats);
        next.holder_sig = holder.sign(&msg);
        Ok(next)
    }

    /// Verify the holder signature.
    ///
    /// # Errors
    /// Returns `MintError::HolderSig` on signature failure.
    pub fn verify_holder_sig(&self) -> Result<(), MintError> {
        let msg = Self::holder_msg(&self.macaroon.root_id, &self.macaroon.caveats);
        let vk = ed25519_dalek::VerifyingKey::from_bytes(&self.holder_pub)
            .map_err(|e| MintError::HolderSig(e.to_string()))?;
        vk.verify_strict(&msg, &self.holder_sig)
            .map_err(|e| MintError::HolderSig(e.to_string()))?;
        Ok(())
    }

    /// Compose the holder signature message: `canonical_ser(root_id || caveats_wire)`.
    fn holder_msg(root_id: &MacaroonId, caveats: &[Caveat]) -> Vec<u8> {
        let mut msg = Vec::with_capacity(16 + caveats.len() * 64);
        msg.extend_from_slice(root_id);
        for caveat in caveats {
            msg.extend_from_slice(&caveat.canonical_ser());
        }
        msg
    }
}

/// Errors during capability token mint/attenuate.
#[derive(Debug, thiserror::Error)]
pub enum MintError {
    #[error("macaroon error: {0}")]
    Macaroon(#[from] MacaroonError),

    #[error("holder signature error: {0}")]
    HolderSig(String),
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::capability::caveat::Caveat;

    #[test]
    fn mint_and_verify_holder_sig() {
        let holder = IdentityKey::generate().unwrap();
        let root_secret = [0x42; 32];
        let caveats = vec![Caveat::Model("gpt-4".to_owned())];
        let token = CapabilityToken::mint(&root_secret, &holder, "did:octo:test", caveats).unwrap();
        token.verify_holder_sig().unwrap();
    }

    #[test]
    fn attenuate_preserves_holder_pub() {
        let holder = IdentityKey::generate().unwrap();
        let root_secret = [0x42; 32];
        let token = CapabilityToken::mint(&root_secret, &holder, "did:octo:test", vec![]).unwrap();
        let attenuated = token
            .attenuate_with_signer(Caveat::Model("gpt-4".to_owned()), &holder)
            .unwrap();
        assert_eq!(attenuated.holder_pub, token.holder_pub);
        attenuated.verify_holder_sig().unwrap();
    }

    #[test]
    fn attenuate_without_signer_breaks_holder_sig() {
        let holder = IdentityKey::generate().unwrap();
        let root_secret = [0x42; 32];
        let token = CapabilityToken::mint(&root_secret, &holder, "did:octo:test", vec![]).unwrap();
        let broken = token.attenuate(Caveat::Model("gpt-4".to_owned()));
        // Without re-signing, holder sig is stale.
        assert!(broken.verify_holder_sig().is_err());
    }
}
