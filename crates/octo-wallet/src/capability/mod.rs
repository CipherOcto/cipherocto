//! Capability token (macaroon v1 per RFC-0957).
//!
//! Holder-bound + caveat-chained bearer token. The verifier checks:
//! 1. Holder signature (Ed25519 over `canonical_ser(root_id || caveats_wire)`).
//! 2. HMAC chain re-derivation against the issuer's root secret.
//! 3. Caveat evaluation against the request context.

pub mod audit_log;
pub mod audit_replay_log;
pub mod bearer_capsule_re_export;
pub mod caveat;
pub mod destination_nonce_store;
pub mod discharge;
pub mod dispatch;
pub mod dual_issuance;
pub mod federation;
pub mod gateway_authenticator;
pub mod gc;
pub mod gossip;
pub mod hop_envelope;
pub mod macaroon;
pub mod market_delivery;
pub mod redemption;
pub mod registry;
pub mod verify;
pub mod wire;
pub mod zk_mint;

use serde::{Deserialize, Serialize};

pub use bearer_capsule_re_export::BearerCapsule;
pub use caveat::{Caveat, CaveatName, MicroOctoW, UnixTimeSecs};
pub use discharge::{
    verify_discharges, ChannelProvider, ChannelProviderRegistry, ChannelProviderResolver,
    DischargeChannel, DischargeError, DischargeMacaroon, DischargeRequest, DischargeVerification,
    EscrowBalance, EscrowDischargeProvider, RateLimitContext, RateLimitDischargeProvider,
    RevocationDischargeProvider, REVOCATION_DISCHARGE_TTL_SECS,
};
pub use macaroon::{
    hmac_blake3, macaroon_id, CapabilityCatalog, Macaroon, MacaroonError, MacaroonId,
};
pub use market_delivery::{
    DealSettled, DealSettledPayload, DeliveryError, EnvelopeId, MarketDeliveryEnvelope,
    MarketDeliveryEnvelopePreimage, RoleTag,
};
pub use registry::{CapabilityClassRegistry, RegistryEntry, RegistryError};
pub use verify::{verify_with_resolve, VerifiedToken, VerifyContext, VerifyError};
pub use wire::{
    compute_cap_root_hash_from_wire, deserialize_wire, deserialize_wire_v2, serialize_wire,
    serialize_wire_v2, WireError, WireV2,
};
pub use zk_mint::{
    bundled_casm_hash, mint_with_zk, mint_with_zk_and_signers, proof_bundle_from_wire,
    proof_bundle_to_wire, CapabilityClass, ExecutionTrace, PrivateWitness, ProofBundle,
    PublicInputs, TraceStep, ZkMintError, COMPILED_CASM_BLAKE3_HASH,
};

use crate::identity::IdentityKey;

/// Holder-bound capability token (RFC-0957 §3.1).
///
/// Holder signature is Ed25519 over `canonical_ser(root_id || caveats_wire)`;
/// the holder DID is the audience (per RFC-0009 §Identity).
///
/// **Debug redaction (octo-wallet §Security):** `holder_sig` is the bearer
/// Ed25519 signature over the macaroon; `macaroon.chain` is the HMAC chain
/// (redacted by `Macaroon::Debug`); `discharges[*].chain` is the discharge
/// HMAC chain (redacted by `DischargeMacaroon::Debug`). Manual `Debug` impl
/// prints only the public `holder_pub` + `holder_did` + `holder_sig_stale`
/// flag + a count of attached discharges. Never log `holder_sig` bytes.
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
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
    /// True iff the holder signature was invalidated by a subsequent
    /// `attenuate()` without re-signing. `verify_holder_sig` rejects
    /// tokens with `holder_sig_stale = true` regardless of the embedded
    /// signature bytes. Mission 0957-a R6 fix: the previous design
    /// returned Ok-with-stale-sig silently, which downstream code could
    /// forget to re-validate; the explicit flag forces the verifier to
    /// notice. Set to `false` on `mint` and `attenuate_with_signer`;
    /// set to `true` on `attenuate` (without signer).
    #[serde(default)]
    pub holder_sig_stale: bool,
}

impl std::fmt::Debug for CapabilityToken {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CapabilityToken")
            .field("macaroon", &self.macaroon)
            .field("holder_pub", &hex::encode(self.holder_pub))
            .field("holder_did", &self.holder_did)
            .field("holder_sig", &"[REDACTED 64 bytes]")
            .field("discharges_count", &self.discharges.len())
            .field("holder_sig_stale", &self.holder_sig_stale)
            .finish()
    }
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
    /// **0957-e amendment (mission 0957-e; RFC-0957-A1 §Persistence-Free Mint):**
    /// 4-arg persistence-free signature per RFC-0957-A1 G3. The `catalog`
    /// and `Option<&mut Transaction>` parameters are REMOVED; mint is pure
    /// crypto (R6-C3 fix). Persistence is handled by the caller via
    /// `TransactionExt::insert_holder_record` (single) or
    /// `TransactionExt::insert_dual` (atomic pair insert per RFC-0969).
    ///
    /// Initial caveats are appended via `Macaroon::extend_chain` (pub(crate)
    /// helper) WITHOUT the catalog-based `WrappedOnly` chain guard. The
    /// guard is enforced at `attenuate` / `attenuate_with_signer` time
    /// (caller responsibility) and at `verify_full` time (verifier
    /// responsibility) — NOT at mint. This breaks the double-insert
    /// contradiction between prior 5-arg `mint` (post-write hook auto-
    /// inserted into `HolderRegistry`) and RFC-0969 `mint_dual` (atomic
    /// pair insert).
    ///
    /// # Errors
    /// Returns `MacaroonError::OsRng` on RNG failure. Initial caveat
    /// append via `extend_chain` cannot fail (no catalog check).
    pub fn mint(
        root_secret: &[u8; 32],
        holder: &IdentityKey,
        holder_did: &str,
        initial_caveats: &[Caveat],
    ) -> Result<Self, MintError> {
        let mut macaroon = Macaroon::mint(root_secret)?;
        for caveat in initial_caveats {
            macaroon = macaroon.extend_chain(caveat.clone());
        }
        let holder_pub = holder.public_key_bytes();
        let holder_did = holder_did.to_owned();

        let msg = Self::holder_msg(&macaroon.root_id, &macaroon.caveats);
        let holder_sig = holder.sign(&msg);

        Ok(Self {
            macaroon,
            holder_pub,
            holder_did,
            holder_sig,
            discharges: Vec::new(),
            holder_sig_stale: false,
        })
    }

    /// Append a caveat (attenuation). Returns a new token with
    /// `holder_sig_stale = true` — the embedded signature does NOT cover
    /// the new caveat list. Callers MUST either:
    /// 1. Re-sign by calling `attenuate_with_signer(caveat, holder, ...)`
    ///    instead (preferred), or
    /// 2. Recognize the stale flag and re-validate downstream.
    ///
    /// `verify_holder_sig` rejects tokens with `holder_sig_stale = true`
    /// unconditionally; this surfaces the broken state at the verify
    /// boundary rather than letting it propagate silently.
    ///
    /// # Errors
    /// Returns `MacaroonError` (cycle / depth / parent-not-found /
    /// `UnknownRawName`) if the catalog rejects the assembled chain.
    pub fn attenuate(
        &self,
        caveat: Caveat,
        catalog: &dyn CapabilityCatalog,
    ) -> Result<Self, MintError> {
        let mut next = self.clone();
        next.macaroon = next.macaroon.attenuate(caveat, catalog)?;
        next.holder_sig_stale = true;
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
        next.holder_sig_stale = false;
        Ok(next)
    }

    /// Verify the holder signature.
    ///
    /// # Errors
    /// Returns `MintError::HolderSig` on signature failure OR when the
    /// token has `holder_sig_stale = true` (set by `attenuate` without
    /// re-signing — mission 0957-a R6 fix surfaces stale state at the
    /// verify boundary rather than letting it propagate).
    pub fn verify_holder_sig(&self) -> Result<(), MintError> {
        if self.holder_sig_stale {
            return Err(MintError::HolderSig(
                "holder_sig_stale: token was attenuated without re-signing; \
                 call attenuate_with_signer or re-mint"
                    .to_owned(),
            ));
        }
        let msg = Self::holder_msg(&self.macaroon.root_id, &self.macaroon.caveats);
        let vk = ed25519_dalek::VerifyingKey::from_bytes(&self.holder_pub)
            .map_err(|e| MintError::HolderSig(e.to_string()))?;
        vk.verify_strict(&msg, &self.holder_sig)
            .map_err(|e| MintError::HolderSig(e.to_string()))?;
        Ok(())
    }

    /// Compose the holder signature message:
    /// `u32(16) || root_id || u32(|caveats_wire|) || caveats_wire`.
    ///
    /// Length-prefixed per field (matching `Macaroon::canonical_ser_unsigned`
    /// at `macaroon.rs:172`) to prevent concatenation-collision attacks:
    /// without prefixes, `canonical_ser(caveat_A) || canonical_ser(caveat_B)`
    /// can collide with `canonical_ser(caveat_A') || canonical_ser(caveat_B')`
    /// when byte boundaries align. The Ed25519 signature would then be
    /// signing ambiguity — distinct caveat lists would produce identical
    /// signatures.
    ///
    /// `caveats_wire` itself is a single length-prefixed concatenation of
    /// the per-caveat canonical JSON (the inner field is the
    /// caveat-variant-tagged serialization). The outer `u32(|caveats_wire|)`
    /// removes the outer ambiguity; inner per-caveat boundaries remain
    /// length-prefixed by each caveat's own `canonical_ser`.
    fn holder_msg(root_id: &MacaroonId, caveats: &[Caveat]) -> Vec<u8> {
        let mut inner = Vec::with_capacity(caveats.len() * 64);
        for caveat in caveats {
            inner.extend_from_slice(&caveat.canonical_ser());
        }
        let mut msg = Vec::with_capacity(4 + 16 + 4 + inner.len());
        msg.extend_from_slice(&u32_len_field(root_id.len()));
        msg.extend_from_slice(root_id);
        msg.extend_from_slice(&u32_len_field(inner.len()));
        msg.extend_from_slice(&inner);
        msg
    }
}

/// Big-endian `u32` length prefix used by `holder_msg` (mirrors
/// `Macaroon::canonical_ser_unsigned` field-prefixing at `macaroon.rs`).
/// Holder signature path is bounded by `caveats.len() < 2^16` and
/// per-caveat `canonical_ser() < 2^16` in practice; `u32` is the
/// safe upper bound.
fn u32_len_field(n: usize) -> [u8; 4] {
    u32::try_from(n)
        .expect("holder_msg field length fits in u32")
        .to_be_bytes()
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
        let caveats = [Caveat::Model("gpt-4".to_owned())];
        let token = CapabilityToken::mint(
            &root_secret,
            &holder,
            &octo_ident::test_helpers::sample_did(104),
            &caveats,
        )
        .unwrap();
        token.verify_holder_sig().unwrap();
    }

    #[test]
    fn attenuate_preserves_holder_pub() {
        let holder = IdentityKey::generate().unwrap();
        let root_secret = [0x42; 32];
        let catalog = empty_catalog();
        let token = CapabilityToken::mint(
            &root_secret,
            &holder,
            &octo_ident::test_helpers::sample_did(104),
            &[],
        )
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
        let token = CapabilityToken::mint(
            &root_secret,
            &holder,
            &octo_ident::test_helpers::sample_did(104),
            &[],
        )
        .unwrap();
        let broken = token
            .attenuate(Caveat::Model("gpt-4".to_owned()), &catalog)
            .unwrap();
        // Without re-signing, holder sig is stale.
        assert!(broken.verify_holder_sig().is_err());
    }
}
