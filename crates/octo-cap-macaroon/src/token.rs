//! `CapabilityToken` — holder-bound + caveat-chained bearer token
//! (RFC-0957 §3.1). The verifier checks:
//! 1. Holder signature (Ed25519 over `canonical_ser(root_id || caveats_wire)`).
//! 2. HMAC chain re-derivation against the issuer's root secret.
//! 3. Caveat evaluation against the request context.
//!
//! Layer 4 extension crate per RFC-0965. This module owns the holder-
//! bound envelope around a `Macaroon` + `holder_sig` + `discharges`.
//! It depends on `crate::macaroon` + `crate::caveat` + `crate::signer`
//! (the `CapabilitySigner` trait abstraction for the holder key — the
//! `IdentityKey` blanket impl lives in `octo-wallet`).

use ed25519_dalek::Signature;
use serde::{Deserialize, Serialize};

use crate::caveat::Caveat;
use crate::macaroon::{CapabilityCatalog, Macaroon, MacaroonError, MacaroonId};
use crate::signer::CapabilitySigner;

/// Discharge macaroon body. Channel-tagged (escrow / revocation /
/// rate-limit). Per RFC-0957 §3.4, discharges satisfy third-party
/// caveats attached to the parent capability.
///
/// Migration note (mission 0957 Phase 2b-3): `DischargeMacaroon` moved
/// into the extension crate alongside `CapabilityToken` because the
/// parent token references discharges by value (`Vec<DischargeMacaroon>`).
/// The other `discharge.rs` types (ChannelProvider, EscrowDischargeProvider,
/// RateLimitDischargeProvider, RevocationDischargeProvider, etc.) remain
/// in `octo-wallet::capability::discharge` for now — Phase 2b follow-on
/// migrates them too.
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DischargeMacaroon {
    /// Channel identifier (`"escrow"`, `"revocation"`, etc.).
    pub channel: String,
    /// Discharge macaroon body (32-byte root secret hash + caveats).
    pub root_secret_hash: [u8; 32],
    /// Chain HMACs (same format as `Macaroon.chain`).
    pub chain: Vec<[u8; 32]>,
    /// Caveats on the discharge (e.g., time bounds).
    pub caveats: Vec<Caveat>,
}

/// Manual redacting `Debug` impl (octo-wallet §Security). `chain` is the
/// HMAC chain (redacted) and `root_secret_hash` is the discharge root
/// secret hash (redacted).
impl std::fmt::Debug for DischargeMacaroon {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("DischargeMacaroon")
            .field("channel", &self.channel)
            .field("root_secret_hash", &"[REDACTED 32 bytes]")
            .field("chain_len", &self.chain.len())
            .field("caveats", &self.caveats)
            .finish()
    }
}

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
    /// Initial caveats are appended via `Macaroon::extend_chain` (pub)
    /// helper WITHOUT the catalog-based `WrappedOnly` chain guard. The
    /// guard is enforced at `attenuate` / `attenuate_with_signer` time
    /// (caller responsibility) and at `verify_full` time (verifier
    /// responsibility) — NOT at mint. This breaks the double-insert
    /// contradiction between prior 5-arg `mint` (post-write hook auto-
    /// inserted into `HolderRegistry`) and RFC-0969 `mint_dual` (atomic
    /// pair insert).
    ///
    /// # Errors
    /// Returns `MintError::Macaroon` on RNG failure. Initial caveat
    /// append via `extend_chain` cannot fail (no catalog check). Returns
    /// `MintError::Signer` if the `CapabilitySigner` rejects the operation.
    pub fn mint(
        root_secret: &[u8; 32],
        holder: &dyn CapabilitySigner,
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
        let sig_bytes = holder.sign(&msg)?;
        let holder_sig = Signature::from_bytes(&sig_bytes);

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
    /// chain guard error from the catalog. Returns `MintError::Signer` on
    /// holder key rejection.
    pub fn attenuate_with_signer(
        &self,
        caveat: Caveat,
        holder: &dyn CapabilitySigner,
        catalog: &dyn CapabilityCatalog,
    ) -> Result<Self, MintError> {
        let mut next = self.clone();
        next.macaroon = next.macaroon.attenuate(caveat, catalog)?;
        let msg = Self::holder_msg(&next.macaroon.root_id, &next.macaroon.caveats);
        let sig_bytes = holder.sign(&msg)?;
        next.holder_sig = Signature::from_bytes(&sig_bytes);
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
    /// Length-prefixed per field (matching `Macaroon::canonical_ser_unsigned`)
    /// to prevent concatenation-collision attacks: without prefixes,
    /// `canonical_ser(caveat_A) || canonical_ser(caveat_B)` can collide
    /// with `canonical_ser(caveat_A') || canonical_ser(caveat_B')` when
    /// byte boundaries align. The Ed25519 signature would then be signing
    /// ambiguity — distinct caveat lists would produce identical
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
/// `Macaroon::canonical_ser_unsigned` field-prefixing).
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

    /// Signer backend rejection (mission 0957 Phase 2b-2 `CapabilitySigner`
    /// trait). Propagates from `holder.sign(&msg)` when the signer rejects
    /// the operation (HSM transport failure, user denied on-device, etc.).
    #[error("signer error: {0}")]
    Signer(#[from] crate::signer::CapabilitySignerError),
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::macaroon::InMemoryCatalog;

    fn empty_catalog() -> InMemoryCatalog {
        InMemoryCatalog::default()
    }

    /// Test signer backing a fixed seed (no HSM, no failure modes).
    struct TestSigner {
        key: [u8; 32],
        pub_bytes: [u8; 32],
    }

    impl CapabilitySigner for TestSigner {
        fn sign(&self, msg: &[u8]) -> Result<[u8; 64], crate::signer::CapabilitySignerError> {
            use ed25519_dalek::{Signer, SigningKey};
            let sk = SigningKey::from_bytes(&self.key);
            let sig = sk.sign(msg);
            Ok(sig.to_bytes())
        }
        fn public_key_bytes(&self) -> [u8; 32] {
            self.pub_bytes
        }
    }

    fn sample_signer() -> TestSigner {
        let key = [0x42u8; 32];
        use ed25519_dalek::SigningKey;
        let sk = SigningKey::from_bytes(&key);
        let vk = sk.verifying_key();
        TestSigner {
            key,
            pub_bytes: vk.to_bytes(),
        }
    }

    #[test]
    fn mint_and_verify_holder_sig() {
        let holder = sample_signer();
        let root_secret = [0x42; 32];
        let caveats = [Caveat::Model("gpt-4".to_owned())];
        let token =
            CapabilityToken::mint(&root_secret, &holder, "did:octo:zTestHolder", &caveats).unwrap();
        token.verify_holder_sig().unwrap();
    }

    #[test]
    fn attenuate_preserves_holder_pub() {
        let holder = sample_signer();
        let root_secret = [0x42; 32];
        let catalog = empty_catalog();
        let token =
            CapabilityToken::mint(&root_secret, &holder, "did:octo:zTestHolder", &[]).unwrap();
        let attenuated = token
            .attenuate_with_signer(Caveat::Model("gpt-4".to_owned()), &holder, &catalog)
            .unwrap();
        assert_eq!(attenuated.holder_pub, token.holder_pub);
        attenuated.verify_holder_sig().unwrap();
    }

    #[test]
    fn attenuate_without_signer_breaks_holder_sig() {
        let holder = sample_signer();
        let root_secret = [0x42; 32];
        let catalog = empty_catalog();
        let token =
            CapabilityToken::mint(&root_secret, &holder, "did:octo:zTestHolder", &[]).unwrap();
        let broken = token
            .attenuate(Caveat::Model("gpt-4".to_owned()), &catalog)
            .unwrap();
        // Without re-signing, holder sig is stale.
        assert!(broken.verify_holder_sig().is_err());
    }

    /// R6 finding: `CapabilityToken` derives `Serialize/Deserialize`. The
    /// `wire_roundtrip` test in `wire.rs` exercises the full token through
    /// the base64-wrapped wire format, but a direct `serde_json` roundtrip
    /// pins JSON-specific concerns (field tag preservation, signature
    /// byte-array encoding via `ed25519_sig_serde`).
    #[test]
    fn capability_token_serde_json_roundtrip() {
        let holder = sample_signer();
        let root_secret = [0x42; 32];
        let _catalog = empty_catalog();
        let token = CapabilityToken::mint(
            &root_secret,
            &holder,
            "did:octo:zSerdeTest",
            &[Caveat::Model("gpt-4".to_owned())],
        )
        .unwrap();

        let json = serde_json::to_string(&token).expect("serialize");
        let restored: CapabilityToken = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(restored, token, "serde_json roundtrip must be exact");

        // Holder signature must still verify after round-trip.
        restored
            .verify_holder_sig()
            .expect("holder sig still verifies");
    }

    /// R6 finding: `DischargeMacaroon` derives `Serialize/Deserialize`.
    /// Direct `serde_json` roundtrip pins the wire encoding (channel
    /// string, fixed-size byte arrays, caveat enum tags).
    #[test]
    fn discharge_macaroon_serde_json_roundtrip() {
        let discharge = DischargeMacaroon {
            channel: "escrow".to_owned(),
            root_secret_hash: [0xaau8; 32],
            chain: vec![[0x01; 32], [0x02; 32]],
            caveats: vec![
                Caveat::Before(2_000_000_000),
                Caveat::Model("gpt-4".to_owned()),
            ],
        };
        let json = serde_json::to_string(&discharge).expect("serialize");
        let restored: DischargeMacaroon = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(restored, discharge, "serde_json roundtrip must be exact");
    }
}
