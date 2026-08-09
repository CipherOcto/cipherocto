//! Authorization enum (RFC-0871 §Data Structures).
//!
//! `Vec<Authorization>` with logical-AND verification semantics per RFC-0871
//! §Adversary Analysis A6. Each variant is one authorization mechanism;
//! callers may compose multiple (e.g., signature + capability token for
//! high-value transitions).
//!
//! `Raw { discriminator, body }` is the escape hatch for unknown future
//! authorization types — old code fails-closed if no handler is registered
//! (RFC-0871 §Compatibility, RFC-0965 §3.2 pattern).
//!
//! ## Layer discipline
//!
//! `CapabilityToken`, `ProofBundle`, and `BlsSignature` are Layer-1 opaque
//! newtype wrappers around canonical bytes. Concrete structure
//! (RFC-0957 macaroon caveat chain, RFC-0958 ZK witness format, BLS12-381
//! point encoding) lives in downstream extension crates (mission
//! `0957-ext-macaroon-crate`, `0957-ext-zk-crate`); this crate owns only the
//! wire shape.

use borsh::{BorshDeserialize, BorshSerialize};
use ed25519_dalek::Signature as Ed25519Signature;
use ed25519_dalek::Verifier;

use octo_ident::WireDid;

use crate::error::ProtocolError;

/// Ed25519 signature wrapper that owns borsh (de)serialization.
///
/// `ed25519_dalek::Signature` does not implement `BorshSerialize` directly,
/// so the wire form of `Authorization::Signature` is a flat 64-byte buffer.
/// Use [`Ed25519SignatureBytes::from_signature`] / [`to_signature`] to bridge
/// to the `ed25519_dalek` API.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, BorshSerialize, BorshDeserialize)]
pub struct Ed25519SignatureBytes(pub [u8; 64]);

impl Ed25519SignatureBytes {
    /// Wrap a `ed25519_dalek::Signature`.
    #[must_use]
    pub fn from_signature(sig: &Ed25519Signature) -> Self {
        Self(sig.to_bytes())
    }

    /// Borrow as `ed25519_dalek::Signature`.
    #[must_use]
    pub fn to_signature(&self) -> Ed25519Signature {
        Ed25519Signature::from_bytes(&self.0)
    }
}

/// Authorization mechanism carried by an envelope.
///
/// Per RFC-0871 §Adversary Analysis A6: ALL authorizations in `Vec<Authorization>`
/// MUST verify (logical AND, not OR).
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub enum Authorization {
    /// Ed25519 signature over `signature_preimage(envelope_id, from_did_wire, payload)`.
    Signature {
        /// DID of the signer (must canonical-form per RFC-0010).
        signer_did: WireDid,
        /// 64-byte Ed25519 signature bytes.
        sig: Ed25519SignatureBytes,
    },
    /// RFC-0957 capability token. Caveats gate what payload can do.
    Capability(CapabilityToken),
    /// RFC-0958 ZK proof bundle.
    Proof(ProofBundle),
    /// Threshold signature (BLS12-381 or equivalent).
    ThresholdSignature {
        /// Signer DIDs that participated.
        signers: Vec<WireDid>,
        /// BLS aggregate signature.
        sig: BlsSignature,
    },
    /// Escape hatch for unknown future authorization types (RFC-0871 §Data
    /// Structures + RFC-0965 §3.2 pattern). Old code fails-closed if no
    /// handler is registered for `discriminator`.
    Raw {
        /// 16-byte discriminator (UUID-shaped).
        discriminator: [u8; 16],
        /// Opaque body bytes.
        body: Vec<u8>,
    },
}

/// Opaque RFC-0957 capability token wrapper.
///
/// Wire form: 32-byte capability root + borsh-serialized caveat chain.
/// Concrete caveat type lives in `crates/octo-cap-macaroon/` (mission
/// `0957-ext-macaroon-crate`). This crate owns only the byte-level envelope.
#[derive(Clone, Debug, PartialEq, Eq, Hash, BorshSerialize, BorshDeserialize)]
pub struct CapabilityToken(pub Vec<u8>);

impl CapabilityToken {
    /// Wrap canonical bytes. No validation — caller must ensure the bytes
    /// were sourced from a valid RFC-0957 mint.
    #[must_use]
    pub fn from_bytes(bytes: Vec<u8>) -> Self {
        Self(bytes)
    }

    /// Borrow the canonical bytes.
    #[must_use]
    pub fn as_bytes(&self) -> &[u8] {
        &self.0
    }
}

/// Opaque RFC-0958 ZK proof bundle wrapper.
///
/// Wire form: `bundled_casm_hash || public_inputs || proof_bytes`. Concrete
/// format lives in `crates/octo-cap-zk/` (mission `0957-ext-zk-crate`).
#[derive(Clone, Debug, PartialEq, Eq, Hash, BorshSerialize, BorshDeserialize)]
pub struct ProofBundle(pub Vec<u8>);

impl ProofBundle {
    /// Wrap canonical bytes.
    #[must_use]
    pub fn from_bytes(bytes: Vec<u8>) -> Self {
        Self(bytes)
    }

    /// Borrow the canonical bytes.
    #[must_use]
    pub fn as_bytes(&self) -> &[u8] {
        &self.0
    }
}

/// 96-byte BLS12-381 G2 signature.
///
/// Wire form: little-endian 96-byte buffer (per BLS12-381 §Signature encoding
/// in IETF draft-irtf-cfrg-bls-signature-05). Concrete threshold-signature
/// semantics (BLS aggregate, key registration) land in
/// `crates/octo-cap-threshold-mpc/` per RFC-0871 §Future Work.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, BorshSerialize, BorshDeserialize)]
pub struct BlsSignature(pub [u8; 96]);

impl BlsSignature {
    /// Wrap a 96-byte signature.
    #[must_use]
    pub fn from_bytes(bytes: [u8; 96]) -> Self {
        Self(bytes)
    }

    /// Borrow the 96-byte signature.
    #[must_use]
    pub fn as_bytes(&self) -> &[u8; 96] {
        &self.0
    }
}

/// Verify an Ed25519 signature over `msg` with the public key derived from
/// `signer_did` (32-byte payload after `did:octo:z` prefix base58btc-decode
/// per RFC-0010).
///
/// Used by `EnvelopeDispatcher` to verify each `Authorization::Signature`.
/// Also exposed publicly so downstream consumers (e.g. quota-router-core
/// per RFC-0870 §NodeEnvelope Adoption) can verify RFC-0871 signatures
/// without round-tripping through the full dispatcher.
pub fn verify_ed25519_signature(
    signer_did: &WireDid,
    msg: &[u8],
    sig_bytes: &Ed25519SignatureBytes,
) -> Result<(), ProtocolError> {
    // Extract the 32-byte payload from the wire form
    // (`did:octo:z<base58btc>`).
    let prefix = "did:octo:z";
    let s = signer_did.as_str();
    let suffix = s.strip_prefix(prefix).ok_or_else(|| {
        ProtocolError::InvalidDid(format!("signer_did must start with did:octo:z; got {s:?}"))
    })?;
    let pk_bytes = bs58::decode(suffix)
        .into_vec()
        .map_err(|_| ProtocolError::InvalidDid("signer_did base58btc decode failed".into()))?;
    if pk_bytes.len() != 32 {
        return Err(ProtocolError::InvalidDid(format!(
            "signer_did payload must be 32 bytes; got {}",
            pk_bytes.len()
        )));
    }
    let mut pk_arr = [0u8; 32];
    pk_arr.copy_from_slice(&pk_bytes);
    let pk = ed25519_dalek::VerifyingKey::from_bytes(&pk_arr).map_err(|e| {
        ProtocolError::AuthorizationFailed(format!("ed25519 verifying key parse: {e}"))
    })?;
    let sig = sig_bytes.to_signature();
    pk.verify(msg, &sig)
        .map_err(|e| ProtocolError::AuthorizationFailed(format!("ed25519 verify: {e}")))
}

/// Minimal base58btc decoder (Bitcoin alphabet). No allocation beyond output.
///
/// Per RFC-0871, the signer DID payload must decode to exactly 32 bytes
/// before the verifying key can be reconstructed.
#[cfg(test)]
mod tests {
    use super::*;
    use ed25519_dalek::Signer;
    use ed25519_dalek::SigningKey;

    fn sample_pubkey(seed: u8) -> [u8; 32] {
        let mut k = [0u8; 32];
        for (i, byte) in k.iter_mut().enumerate() {
            *byte = seed.wrapping_add(i as u8);
        }
        k
    }

    #[test]
    fn borsh_round_trip_signature_variant() {
        let pk_bytes = sample_pubkey(7);
        let wire = format!("did:octo:z{}", bs58_encode(&pk_bytes));
        let sk = SigningKey::from_bytes(&pk_bytes);
        let sig = Ed25519SignatureBytes::from_signature(&sk.sign(b"hello"));
        let did = WireDid::new(wire);
        let auth = Authorization::Signature {
            signer_did: did.clone(),
            sig,
        };
        let bytes = borsh::to_vec(&auth).unwrap();
        let back: Authorization = borsh::from_slice(&bytes).unwrap();
        match back {
            Authorization::Signature { signer_did, sig: s } => {
                assert_eq!(signer_did, did);
                assert_eq!(s, sig);
            }
            _ => panic!("expected Signature variant"),
        }
    }

    #[test]
    fn borsh_round_trip_capability_token() {
        let token = CapabilityToken::from_bytes(vec![0xaa; 64]);
        let bytes = borsh::to_vec(&token).unwrap();
        let back: CapabilityToken = borsh::from_slice(&bytes).unwrap();
        assert_eq!(back, token);
    }

    #[test]
    fn borsh_round_trip_proof_bundle() {
        let proof = ProofBundle::from_bytes(vec![0xbb; 128]);
        let bytes = borsh::to_vec(&proof).unwrap();
        let back: ProofBundle = borsh::from_slice(&bytes).unwrap();
        assert_eq!(back, proof);
    }

    #[test]
    fn borsh_round_trip_threshold_signature() {
        let sk_bytes = sample_pubkey(11);
        let wire = format!("did:octo:z{}", bs58_encode(&sk_bytes));
        let did = WireDid::new(wire);
        let sig = BlsSignature::from_bytes([0xcc; 96]);
        let auth = Authorization::ThresholdSignature {
            signers: vec![did.clone()],
            sig,
        };
        let bytes = borsh::to_vec(&auth).unwrap();
        let back: Authorization = borsh::from_slice(&bytes).unwrap();
        match back {
            Authorization::ThresholdSignature { signers, sig: s } => {
                assert_eq!(signers, vec![did]);
                assert_eq!(s, sig);
            }
            _ => panic!("expected ThresholdSignature variant"),
        }
    }

    #[test]
    fn borsh_round_trip_raw() {
        let mut disc = [0u8; 16];
        disc[0] = 0xff;
        let auth = Authorization::Raw {
            discriminator: disc,
            body: vec![1, 2, 3, 4],
        };
        let bytes = borsh::to_vec(&auth).unwrap();
        let back: Authorization = borsh::from_slice(&bytes).unwrap();
        assert_eq!(back, auth);
    }

    #[test]
    fn ed25519_round_trip_via_did() {
        // The 32-byte value carried by the DID wire form is the *verifying
        // key* (public key), not the seed. Derive the verifying key from a
        // seed, encode that into the wire form, then sign with the matching
        // signing key.
        let seed = sample_pubkey(42);
        let sk = SigningKey::from_bytes(&seed);
        let pk_bytes = sk.verifying_key().to_bytes();
        let wire = format!("did:octo:z{}", bs58_encode(&pk_bytes));
        let did = WireDid::new(wire);
        let msg = b"the quick brown fox";
        let sig = Ed25519SignatureBytes::from_signature(&sk.sign(msg));
        assert!(verify_ed25519_signature(&did, msg, &sig).is_ok());
        // Tampered message must fail
        let bad = Ed25519SignatureBytes::from_signature(&sk.sign(b"the quick brown cat"));
        assert!(verify_ed25519_signature(&did, msg, &bad).is_err());
    }

    /// Minimal base58btc encoder for tests (uses the `bs58` crate so the
    /// encode/decode pair is symmetric with the production verifier).
    fn bs58_encode(input: &[u8]) -> String {
        bs58::encode(input).into_string()
    }
}
