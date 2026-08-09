//! Identity substrate.
//!
//! Per RFC-0009 §Identity Key Format + §Capability Keys + RFC-0009 v1.1
//! §HsmAdapter Integration:
//! - `IdentityKey` holds an `Arc<dyn HsmAdapter>` for signing operations
//!   (host memory for `InMemorySigner`, secure element for `LedgerSigner`).
//! - The raw seed NEVER leaves the adapter; production `LedgerSigner` signs
//!   on-device with explicit user confirmation.
//! - `CapabilityKey` is a 32-byte symmetric key derived per-(audience, channel)
//!   via HKDF-BLAKE3; for `InMemorySigner`-backed identities the derivation
//!   uses the adapter's seed (crate-internal accessor only).

use std::str::FromStr;
use std::sync::Arc;

use ed25519_dalek::{Signature, Verifier, VerifyingKey};
use serde::{Deserialize, Serialize};
use zeroize::{Zeroize, ZeroizeOnDrop};

use crate::error::WalletError;
use crate::hsm::{HsmAdapter, InMemorySigner};

use octo_ident::DidCodec;

/// Ed25519 identity keypair. Wraps an `Arc<dyn HsmAdapter>` + cached
/// 32-byte public key.
///
/// Per RFC-0009 v1.1 §HsmAdapter Integration:
/// - `sign()` delegates to `self.signer.sign(msg)`; never touches host memory
///   for production `LedgerSigner` (seed lives in secure element).
/// - The raw seed is accessible only via the adapter — `InMemorySigner` exposes
///   it crate-internally for HKDF + keystore-export paths; hardware adapters
///   do not expose it at all.
pub struct IdentityKey {
    signer: Arc<dyn HsmAdapter>,
    public_key: [u8; 32],
}

impl std::fmt::Debug for IdentityKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("IdentityKey")
            .field("public_key", &hex::encode(self.public_key))
            .field("signer", &"<dyn HsmAdapter>")
            .finish_non_exhaustive()
    }
}

impl Clone for IdentityKey {
    fn clone(&self) -> Self {
        Self {
            signer: self.signer.clone(),
            public_key: self.public_key,
        }
    }
}

impl IdentityKey {
    /// Generate a fresh identity key from OS CSPRNG (defaults to
    /// `InMemorySigner` for MVP; production swaps in a hardware adapter via
    /// [`IdentityKey::with_signer`]).
    ///
    /// # Errors
    /// Returns `WalletError::OsRng` if the OS RNG fails (extremely rare).
    pub fn generate() -> Result<Self, WalletError> {
        let mut seed = [0u8; 32];
        getrandom::getrandom(&mut seed).map_err(|e| WalletError::OsRng(e.to_string()))?;
        let k = Self::from_seed(seed);
        seed.zeroize();
        Ok(k)
    }

    /// Restore from a 32-byte seed using the default `InMemorySigner`.
    pub fn from_seed(seed: [u8; 32]) -> Self {
        let sk = ed25519_dalek::SigningKey::from_bytes(&seed);
        let public_key = sk.verifying_key().to_bytes();
        Self {
            signer: Arc::new(InMemorySigner::new(seed, public_key)),
            public_key,
        }
    }

    /// Construct an `IdentityKey` backed by an arbitrary `HsmAdapter` impl
    /// (e.g. `LedgerSigner`). The public key is sourced from the adapter.
    ///
    /// # Errors
    /// Returns `WalletError::Hsm(HsmError::Device)` if `get_public_key`
    /// fails on the adapter (transport failure).
    pub fn with_signer(signer: Arc<dyn HsmAdapter>) -> Result<Self, WalletError> {
        let public_key = signer.get_public_key()?;
        Ok(Self { signer, public_key })
    }

    /// Crate-internal seed accessor for HKDF derivation. Returns the seed
    /// when the adapter IS an `InMemorySigner`; otherwise returns an error.
    /// This is the ONLY path that exposes the raw seed bytes; production
    /// hardware adapters cannot satisfy it.
    ///
    /// # Errors
    /// Returns `WalletError::Hsm(HsmError::Device)` when the adapter is not
    /// an `InMemorySigner` (production hardware wallets MUST NOT export seed).
    ///
    /// **Note:** `pub` (not `pub(crate)`) because the `octo-wallet` CLI binary
    /// (`src/bin/octo-wallet.rs`) needs to export the seed for vault
    /// initialization. Any other consumer should use `IdentityKey::sign()`
    /// which delegates to the adapter without exposing the seed.
    pub fn seed_bytes_for_hkdf(&self) -> Result<[u8; 32], WalletError> {
        // MVP shortcut: every `IdentityKey::from_seed` constructs with
        // `InMemorySigner` as the adapter. Production wirings using real
        // `LedgerSigner` would need a separate `LedgerSigner::export_seed`
        // APDU (deliberately not implemented — seed export is what hardware
        // wallets REFUSE to do). For now, when a non-InMemorySigner is in
        // use, the caller must catch this error.
        let any = self.signer.as_any();
        if let Some(in_mem) = any.downcast_ref::<InMemorySigner>() {
            Ok(in_mem.seed_bytes())
        } else {
            Err(WalletError::Hsm(crate::hsm::HsmError::Device(
                "seed export not supported by hardware adapter".to_owned(),
            )))
        }
    }

    /// Public verifying key (32 bytes Ed25519 public key).
    #[must_use]
    pub fn public_key_bytes(&self) -> [u8; 32] {
        self.public_key
    }

    /// Ed25519 signature over `msg`. Delegates to the underlying
    /// `HsmAdapter::sign` — production `LedgerSigner` will prompt the user
    /// on-device; rejection propagates as `WalletError::Hsm(HsmError::UserRejected)`.
    ///
    /// # Errors
    /// Returns `WalletError::Hsm(_)` on any adapter failure (transport,
    /// user rejection, device error).
    pub fn sign(&self, msg: &[u8]) -> Result<Signature, WalletError> {
        let sig_bytes = self.signer.sign(msg)?;
        Ok(Signature::from_bytes(&sig_bytes))
    }

    /// Borrow the underlying signer (e.g. for tests asserting parity with
    /// direct adapter calls).
    #[must_use]
    pub fn signer(&self) -> &Arc<dyn HsmAdapter> {
        &self.signer
    }

    /// Verify a signature (useful for self-tests).
    ///
    /// # Errors
    /// Returns `WalletError::Signature` if the signature is invalid.
    pub fn verify(&self, msg: &[u8], sig: &Signature) -> Result<(), WalletError> {
        let vk = VerifyingKey::from_bytes(&self.public_key)
            .map_err(|e| WalletError::Signature(e.to_string()))?;
        vk.verify(msg, sig)
            .map_err(|e| WalletError::Signature(e.to_string()))
    }
}

/// Capability key (per-(audience, channel) symmetric key). 32 bytes.
///
/// Derived via HKDF-BLAKE3 per RFC-0009 §Capability Keys:
/// `capability_root = HKDF-BLAKE3(salt=identity_seed, info="cipherocto/cap/v1/{channel_id}", ikm=audience_did)`
///
/// `ZeroizeOnDrop` ensures the key bytes are wiped from memory when the value
/// goes out of scope (RFC-0009 §Security Considerations).
#[derive(Clone, ZeroizeOnDrop)]
pub struct CapabilityKey([u8; 32]);

impl std::fmt::Debug for CapabilityKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CapabilityKey")
            .field("bytes", &"[REDACTED]")
            .finish()
    }
}

impl AsRef<[u8]> for CapabilityKey {
    fn as_ref(&self) -> &[u8] {
        &self.0
    }
}

impl CapabilityKey {
    /// Raw 32 key bytes. Caller MUST NOT log, persist, or copy. Zeroized on drop.
    #[must_use]
    pub fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }
}

/// Audience identifier (canonical DID form). Used as HKDF IKM.
///
/// Per RFC-0010 v1.2 F4 (Wallet audience validation): `AudienceId::from_str`
/// MUST validate via `octo_ident::CanonicalCodec::parse(s, allow_legacy_bare: false)`.
/// Legacy `did:octo:b<base32>` form is rejected post-deprecation window.
/// Bare `did:octo:<suffix>` literals are rejected (`DidError::LegacyFormExpired`).
/// Only canonical `did:octo:z<base58btc>` form (43-44 char suffix) is valid.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(transparent)]
pub struct AudienceId(String);

impl FromStr for AudienceId {
    type Err = WalletError;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        if s.is_empty() {
            return Err(WalletError::InvalidAudienceId("empty".to_owned()));
        }
        // RFC-0010 v1.2 F4: validate canonical DID shape via the codec.
        // Production paths MUST use `allow_legacy_bare: false`; legacy form
        // is rejected post-deprecation window. Test fixtures that need the
        // legacy form can call `CanonicalCodec::parse(s, true)` directly.
        octo_ident::CanonicalCodec::parse(s, false)
            .map(|_| Self(s.to_owned()))
            .map_err(|e| {
                WalletError::InvalidAudienceId(format!("canonical DID validation failed: {e}"))
            })
    }
}

impl std::fmt::Display for AudienceId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl AsRef<str> for AudienceId {
    fn as_ref(&self) -> &str {
        &self.0
    }
}

/// Channel identifier (per-(audience, channel) domain separator).
/// Used as HKDF info suffix.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ChannelId(String);

impl ChannelId {
    /// Wire-format version. Bump on info-string change.
    pub const VERSION: &'static str = "v1";

    /// HKDF info prefix per RFC-0009 §Capability Keys.
    pub const INFO_PREFIX: &'static str = "cipherocto/cap/";

    /// Compose the HKDF info string: `cipherocto/cap/v1/{channel_id}`.
    #[must_use]
    pub fn info_string(&self) -> String {
        format!("{}{}/{}", Self::INFO_PREFIX, Self::VERSION, self.0)
    }
}

impl FromStr for ChannelId {
    type Err = WalletError;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        if s.is_empty() {
            return Err(WalletError::InvalidChannelId("empty".to_owned()));
        }
        Ok(Self(s.to_owned()))
    }
}

impl std::fmt::Display for ChannelId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

/// Derive a per-(audience, channel) capability key from an identity.
///
/// `capability_root = HKDF-BLAKE3(salt=identity_seed, info="cipherocto/cap/v1/{channel_id}", ikm=audience_did)`
///
/// Per RFC-0009 §Capability Keys:
/// - salt = identity_seed (per-identity domain separation; sourced from
///   adapter for `InMemorySigner`; returns `WalletError::Hsm` for hardware
///   adapters that cannot export the seed — those adapters are NOT supported
///   for HKDF-derived capability roots; mission `0871a` defines the
///   hardware-wallet capability mint path)
/// - info = `cipherocto/cap/v1/{channel_id}` (versioned namespace)
/// - ikm = audience_did (audience unlinkability)
///
/// Same `(identity, audience, channel)` triple → same capability key (deterministic).
/// Different `(audience, channel)` → independent keys (SimpleX-style unlinkability).
///
/// # Errors
/// Returns `WalletError::InvalidAudienceId` / `WalletError::InvalidChannelId`
/// if the inputs fail validation, or `WalletError::Hsm` when the identity's
/// adapter cannot export the seed.
pub fn derive_capability_key(
    identity: &IdentityKey,
    audience: &AudienceId,
    channel: &ChannelId,
) -> Result<CapabilityKey, WalletError> {
    // RFC-0009 §Capability Keys: HKDF-BLAKE3(salt=identity_seed,
    // info="cipherocto/cap/v1/{channel_id}", ikm=audience_did).
    // We use `blake3::derive_key` (HKDF-style Extract-and-Expand with BLAKE3).
    // Salt is sourced from the adapter (InMemorySigner exposes it crate-internally;
    // hardware adapters refuse to export it).
    let info = channel.info_string();
    let ikm = audience.to_string();
    let context = format!("{info}:{ikm}");

    let seed = identity.seed_bytes_for_hkdf()?;
    let mut salted_ikm = Vec::with_capacity(32 + ikm.len());
    salted_ikm.extend_from_slice(&seed);
    salted_ikm.extend_from_slice(ikm.as_bytes());

    let derived = blake3::derive_key(&context, &salted_ikm);
    let mut okm = [0u8; 32];
    okm.copy_from_slice(&derived);
    Ok(CapabilityKey(okm))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::hsm::InMemorySigner;
    use ed25519_dalek::Signer;

    #[test]
    fn generate_sign_verify_roundtrip() {
        let k = IdentityKey::generate().expect("generate");
        let msg = b"hello world";
        let sig = k.sign(msg).expect("sign");
        k.verify(msg, &sig).expect("verify");
    }

    #[test]
    fn from_seed_signs_deterministically() {
        let seed = [42u8; 32];
        let k = IdentityKey::from_seed(seed);
        let msg = b"deterministic";
        let sig1 = k.sign(msg).expect("sign 1");
        let sig2 = k.sign(msg).expect("sign 2");
        assert_eq!(sig1, sig2, "Ed25519 sign must be deterministic");
    }

    #[test]
    fn in_memory_signer_byte_identical_to_raw_signing_key() {
        // RFC-0009 byte-identical parity: HsmAdapter-mediated signing MUST
        // produce the same bytes as a direct ed25519_dalek::SigningKey call.
        let seed = [99u8; 32];
        let raw_sk = ed25519_dalek::SigningKey::from_bytes(&seed);
        let raw_sig = raw_sk.sign(b"parity check");

        let id = IdentityKey::from_seed(seed);
        let adapter_sig = id.sign(b"parity check").expect("adapter sign");
        assert_eq!(raw_sig.to_bytes(), adapter_sig.to_bytes());
    }

    #[test]
    fn public_key_matches_signing_key_derivation() {
        let seed = [7u8; 32];
        let expected_pk = ed25519_dalek::SigningKey::from_bytes(&seed)
            .verifying_key()
            .to_bytes();
        let id = IdentityKey::from_seed(seed);
        assert_eq!(id.public_key_bytes(), expected_pk);
    }

    #[test]
    fn seed_export_blocked_for_hardware_adapter() {
        // RFC-0009 A9 mitigation: a hardware-backed IdentityKey MUST NOT
        // expose its seed via `seed_bytes_for_hkdf`. We simulate a hardware
        // adapter with a custom HsmAdapter that returns a fixed public key.
        struct FakeHwAdapter {
            pk: [u8; 32],
        }
        impl HsmAdapter for FakeHwAdapter {
            fn get_public_key(&self) -> Result<[u8; 32], crate::hsm::HsmError> {
                Ok(self.pk)
            }
            fn sign(&self, _msg: &[u8]) -> Result<[u8; 64], crate::hsm::HsmError> {
                Ok([0u8; 64])
            }
            fn as_any(&self) -> &dyn std::any::Any {
                self
            }
        }
        let pk = [1u8; 32];
        let id = IdentityKey::with_signer(Arc::new(FakeHwAdapter { pk })).unwrap();
        let r = id.seed_bytes_for_hkdf();
        assert!(matches!(r, Err(WalletError::Hsm(_))));
    }

    #[test]
    fn derive_capability_key_succeeds_for_in_memory() {
        let k = IdentityKey::from_seed([7u8; 32]);
        let aud: AudienceId = octo_ident::test_helpers::sample_did(7)
            .parse()
            .expect("canonical audience");
        let ch: ChannelId = "channel-1".parse().unwrap();
        let _ = derive_capability_key(&k, &aud, &ch).expect("hkdf derive");
    }

    #[test]
    fn derive_deterministic() {
        let k = IdentityKey::generate().unwrap();
        let aud: AudienceId = octo_ident::test_helpers::sample_did(188).parse().unwrap();
        let ch: ChannelId = "channel-1".parse().unwrap();
        let cap1 = derive_capability_key(&k, &aud, &ch).unwrap();
        let cap2 = derive_capability_key(&k, &aud, &ch).unwrap();
        assert_eq!(cap1.as_bytes(), cap2.as_bytes());
    }

    #[test]
    fn derive_independent_channels() {
        let k = IdentityKey::generate().unwrap();
        let aud: AudienceId = octo_ident::test_helpers::sample_did(188).parse().unwrap();
        let cap_a = derive_capability_key(&k, &aud, &"ch-a".parse().unwrap()).unwrap();
        let cap_b = derive_capability_key(&k, &aud, &"ch-b".parse().unwrap()).unwrap();
        assert_ne!(cap_a.as_bytes(), cap_b.as_bytes());
    }

    #[test]
    fn derive_independent_audiences() {
        let k = IdentityKey::generate().unwrap();
        let ch: ChannelId = "channel-1".parse().unwrap();
        let cap_a = derive_capability_key(
            &k,
            &octo_ident::test_helpers::sample_did(85).parse().unwrap(),
            &ch,
        )
        .unwrap();
        // Use a second canonical DID (different pubkey → different hash)
        // rather than the legacy `did:octo:b` form, which RFC-0010 v1.2 F4
        // rejects post-deprecation. The HKDF IKM is the wire string, so
        // distinct valid DIDs produce distinct capability keys.
        let cap_b = derive_capability_key(
            &k,
            &octo_ident::test_helpers::sample_did(99).parse().unwrap(),
            &ch,
        )
        .unwrap();
        assert_ne!(cap_a.as_bytes(), cap_b.as_bytes());
    }

    #[test]
    fn empty_audience_rejected() {
        assert!("".parse::<AudienceId>().is_err());
    }

    #[test]
    fn canonical_did_audience_accepted() {
        // RFC-0010 v1.2 F4: canonical `did:octo:z<base58btc>` accepted.
        let canonical = octo_ident::test_helpers::sample_did(7);
        let audience: AudienceId = canonical.parse().expect("canonical accepted");
        assert_eq!(audience.to_string(), canonical);
    }

    #[test]
    fn legacy_did_b_form_rejected_post_deprecation() {
        // RFC-0010 v1.2 F4: legacy `did:octo:b<base32>` rejected.
        // 52-char base32 payload satisfies `parse` step 2 *only when*
        // `allow_legacy_bare: true`; the wallet surface uses `false`, so the
        // legacy form must be rejected. We construct a syntactically valid
        // 62-char legacy form (did:octo:b + 52 base32 chars).
        let legacy = format!(
            "did:octo:b{}",
            (0..52)
                .map(|i| match i % 32 {
                    0 => b'a',
                    1 => b'b',
                    _ => b'c',
                } as char)
                .collect::<String>()
        );
        let r: Result<AudienceId, _> = legacy.parse();
        assert!(r.is_err(), "legacy did:octo:b must be rejected");
    }

    #[test]
    fn bare_did_octo_suffix_rejected_post_window() {
        // RFC-0010 §`parse` step 3: bare `did:octo:<name>` is rejected
        // post-deprecation window (allow_legacy_bare: false).
        let r: Result<AudienceId, _> = "did:octo:evil".parse();
        assert!(r.is_err(), "bare did:octo: suffix must be rejected");
    }

    #[test]
    fn wrong_prefix_did_rejected() {
        let r: Result<AudienceId, _> = "did:foo:zSomeBase58String".parse();
        assert!(r.is_err(), "non-octo DID prefix must be rejected");
    }

    #[test]
    fn malformed_did_truncated_rejected() {
        let r: Result<AudienceId, _> = "did:octo:zabc".parse();
        assert!(r.is_err(), "truncated canonical DID must be rejected");
    }

    #[test]
    fn empty_channel_rejected() {
        assert!("".parse::<ChannelId>().is_err());
    }

    #[test]
    fn channel_info_string() {
        let ch: ChannelId = "test".parse().unwrap();
        assert_eq!(ch.info_string(), "cipherocto/cap/v1/test");
    }

    #[test]
    fn in_memory_signer_factory_round_trips() {
        // Sanity check the construction path used by `IdentityKey::from_seed`.
        let seed = [11u8; 32];
        let s = InMemorySigner::new(seed, [0u8; 32]);
        assert_eq!(s.seed_bytes(), seed);
    }
}
