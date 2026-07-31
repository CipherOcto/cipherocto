//! Capability token (macaroon v1 per RFC-0957).
//!
//! Holder-bound + caveat-chained bearer token. The verifier checks:
//! 1. Holder signature (Ed25519 over `canonical_ser(root_id || caveats_wire)`).
//! 2. HMAC chain re-derivation against the issuer's root secret.
//! 3. Caveat evaluation against the request context.

pub mod caveat;
pub mod discharge;
pub mod macaroon;
pub mod redemption;
pub mod registry;
pub mod wire;
pub mod zk_mint;

use serde::{Deserialize, Serialize};

pub use caveat::{Caveat, CaveatName, MicroOctoW, UnixTimeSecs};
pub use discharge::{DischargeChannel, DischargeMacaroon};
pub use macaroon::{
    hmac_blake3, macaroon_id, CapabilityCatalog, Macaroon, MacaroonError, MacaroonId,
};
pub use registry::{CapabilityClassRegistry, RegistryEntry, RegistryError};
pub use wire::{
    deserialize_wire, deserialize_wire_v2, serialize_wire, serialize_wire_v2, WireError, WireV2,
};
pub use zk_mint::{
    mint_with_zk, proof_bundle_from_wire, proof_bundle_to_wire, CapabilityClass, PrivateWitness,
    ProofBundle, PublicInputs, ZkMintError, COMPILED_CASM_BLAKE3_HASH,
};

use crate::identity::IdentityKey;

/// Holder-bound capability token (RFC-0957 §3.1).
///
/// Holder signature is Ed25519 over `canonical_ser(root_id || caveats_wire)`;
/// the holder DID is the audience (per RFC-0009 §Identity).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
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
    /// The catalog is required for the `WrappedOnly` chain guard (RFC-0965
    /// §3.7) — every attenuation funnels through `Macaroon::attenuate`,
    /// which walks the parent chain via the catalog. For tokens that do
    /// not carry any `WrappedOnly` caveats, an empty catalog suffices.
    ///
    /// # Errors
    /// Returns `MacaroonError::OsRng` on RNG failure, `WrappedCycle` /
    /// `WrappedDepthExceeded` / `WrappedParentNotFound` if the catalog
    /// rejects the assembled chain.
    pub fn mint(
        root_secret: &[u8; 32],
        holder: &IdentityKey,
        holder_did: impl Into<String>,
        initial_caveats: Vec<Caveat>,
        catalog: &dyn CapabilityCatalog,
    ) -> Result<Self, MintError> {
        let mut macaroon = Macaroon::mint(root_secret)?;
        for caveat in initial_caveats {
            macaroon = macaroon.attenuate(caveat, catalog)?;
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
    ///
    /// # Errors
    /// Returns `MacaroonError` (cycle / depth / parent-not-found) if the
    /// catalog rejects the assembled chain. Holder signature is stale —
    /// callers without the holder key MUST use `attenuate_with_signer`.
    pub fn attenuate(
        &self,
        caveat: Caveat,
        catalog: &dyn CapabilityCatalog,
    ) -> Result<Self, MintError> {
        let mut next = self.clone();
        next.macaroon = next.macaroon.attenuate(caveat, catalog)?;
        // Holder signature no longer covers the new caveat list — re-sign.
        // Note: holder private key is required for re-sign; this method is
        // only useful for the holder. Attenuators without the holder key can
        // produce a re-signed token by calling `attenuate_with_signer`.
        Ok(next)
    }

    /// Attenuate and re-sign with the given holder key.
    ///
    /// # Errors
    /// Returns `MacaroonError::OsRng` on RNG failure, or any `WrappedOnly`
    /// chain guard error from the catalog.
    pub fn attenuate_with_signer(
        &self,
        caveat: Caveat,
        holder: &IdentityKey,
        catalog: &dyn CapabilityCatalog,
    ) -> Result<Self, MintError> {
        let mut next = self.clone();
        next.macaroon = next.macaroon.attenuate(caveat, catalog)?;
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
    use crate::capability::macaroon::InMemoryCatalog;

    fn empty_catalog() -> InMemoryCatalog {
        InMemoryCatalog::default()
    }

    #[test]
    fn mint_and_verify_holder_sig() {
        let holder = IdentityKey::generate().unwrap();
        let root_secret = [0x42; 32];
        let caveats = vec![Caveat::Model("gpt-4".to_owned())];
        let catalog = empty_catalog();
        let token =
            CapabilityToken::mint(&root_secret, &holder, "did:octo:test", caveats, &catalog)
                .unwrap();
        token.verify_holder_sig().unwrap();
    }

    #[test]
    fn attenuate_preserves_holder_pub() {
        let holder = IdentityKey::generate().unwrap();
        let root_secret = [0x42; 32];
        let catalog = empty_catalog();
        let token = CapabilityToken::mint(&root_secret, &holder, "did:octo:test", vec![], &catalog)
            .unwrap();
        let attenuated = token
            .attenuate_with_signer(Caveat::Model("gpt-4".to_owned()), &holder, &catalog)
            .unwrap();
        assert_eq!(attenuated.holder_pub, token.holder_pub);
        attenuated.verify_holder_sig().unwrap();
    }

    #[test]
    fn attenuate_without_signer_breaks_holder_sig() {
        let holder = IdentityKey::generate().unwrap();
        let root_secret = [0x42; 32];
        let catalog = empty_catalog();
        let token = CapabilityToken::mint(&root_secret, &holder, "did:octo:test", vec![], &catalog)
            .unwrap();
        let broken = token
            .attenuate(Caveat::Model("gpt-4".to_owned()), &catalog)
            .unwrap();
        // Without re-signing, holder sig is stale.
        assert!(broken.verify_holder_sig().is_err());
    }
}
