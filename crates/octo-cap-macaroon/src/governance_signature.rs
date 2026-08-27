//! `GovernanceSignature` substrate — canonical home per RFC-0105 §3.12.
//!
//! Per §3.12 (NEW in v3.5-r5): `verify_governance_signature` and `blake3_hash`
//! are defined ONCE in `octo-cap-macaroon` (Layer A substrate). Consumer
//! crates import via `use octo_cap_macaroon::{verify_governance_signature,
//! blake3_hash};` OR via the re-export `use octo_vault::{
//! verify_governance_signature, blake3_hash};` (octo-vault re-exports both).
//! Both paths resolve to the canonical octo-cap-macaroon definition.
//!
//! Consumers MUST NOT re-declare these functions locally (single-source-of-truth
//! rule per §3.12).

use thiserror::Error;

/// 32-byte governance public key (Ed25519).
pub type GovernancePubkey = [u8; 32];

/// 64-byte governance signature (Ed25519).
pub type GovernanceSignatureBytes = [u8; 64];

/// Errors emitted by [`verify_governance_signature`].
#[derive(Debug, Error, PartialEq, Eq)]
pub enum GovernanceSignatureError {
    /// Signature bytes length was not 64.
    #[error("governance signature must be 64 bytes (got {0})")]
    InvalidLength(usize),
    /// Signature verification failed (Ed25519 verify returned Err).
    #[error("governance signature verification failed")]
    InvalidSignature,
    /// Public key bytes length was not 32.
    #[error("governance pubkey must be 32 bytes (got {0})")]
    InvalidPubkeyLength(usize),
}

/// Verify a 64-byte Ed25519 signature over the given `body_hash` using
/// `governance_pubkey`. Returns `Ok(())` on success, `Err(_)` on failure.
///
/// **Single-source-of-truth (NEW in v3.5-r5, RFC-0105 §3.12):** consumers
/// MUST import this function from `octo-cap-macaroon` (or its
/// `octo-vault` re-export). Local re-declarations are forbidden — the
/// single-source rule exists so the substrate's signature semantics
/// can be evolved in one place (e.g., PQC migration per layer model).
///
/// **Why `body_hash` parameter:** per RFC-0105 §3.6,
/// `GovernanceSignature` carries the raw Ed25519 signature over the
/// canonical body_hash. The body_hash is computed by the caller per
/// the event-type-specific RFC (e.g., RFC-0960 §2.2 for
/// BurnEventRef; RFC-0959 §2.2 for SettlementEvent; RFC-0965
/// v2.1 §2.3 for PaymentCaveat). This function is the substrate-
/// canonical verifier that all event types share.
pub fn verify_governance_signature(
    sig: &GovernanceSignatureBytes,
    body_hash: &[u8; 32],
    governance_pubkey: &GovernancePubkey,
) -> Result<(), GovernanceSignatureError> {
    use ed25519_dalek::{Signature, Verifier, VerifyingKey};
    let pk = VerifyingKey::from_bytes(governance_pubkey)
        .map_err(|_| GovernanceSignatureError::InvalidPubkeyLength(governance_pubkey.len()))?;
    let s = Signature::from_bytes(sig);
    pk.verify(body_hash, &s)
        .map_err(|_| GovernanceSignatureError::InvalidSignature)
}

/// Canonical BLAKE3 hash primitive (Layer A frozen substrate per §3.12).
///
/// Thin wrapper around `blake3::hash` that returns the fixed-array
/// `[u8; 32]` output (not the `Hash` newtype).
///
/// **Single-source-of-truth (NEW in v3.5-r5, RFC-0105 §3.12):** consumers
/// MUST import this function from `octo-cap-macaroon` (or its
/// `octo-vault` re-export). Local re-declarations are forbidden.
#[must_use]
pub fn blake3_hash(input: &[u8]) -> [u8; 32] {
    let h = blake3::hash(input);
    let mut out = [0u8; 32];
    out.copy_from_slice(h.as_bytes());
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use ed25519_dalek::{Signer, SigningKey};
    use rand::RngCore;

    fn sample_key() -> (SigningKey, [u8; 32]) {
        let mut secret = [0u8; 32];
        rand::rng().fill_bytes(&mut secret);
        let sk = SigningKey::from_bytes(&secret);
        let pk_bytes = sk.verifying_key().to_bytes();
        (sk, pk_bytes)
    }

    #[test]
    fn blake3_hash_is_deterministic() {
        let a = blake3_hash(b"hello");
        let b = blake3_hash(b"hello");
        assert_eq!(a, b);
    }

    #[test]
    fn blake3_hash_changes_with_input() {
        let a = blake3_hash(b"a");
        let b = blake3_hash(b"b");
        assert_ne!(a, b);
    }

    #[test]
    fn verify_governance_signature_accepts_valid_signature() {
        let (sk, pk) = sample_key();
        let body_hash = blake3_hash(b"test body");
        let sig = sk.sign(&body_hash).to_bytes();
        assert_eq!(verify_governance_signature(&sig, &body_hash, &pk), Ok(()));
    }

    #[test]
    fn verify_governance_signature_rejects_invalid_signature() {
        let (_sk, pk) = sample_key();
        let body_hash = blake3_hash(b"test body");
        let bogus_sig = [0u8; 64];
        assert_eq!(
            verify_governance_signature(&bogus_sig, &body_hash, &pk),
            Err(GovernanceSignatureError::InvalidSignature)
        );
    }

    #[test]
    fn verify_governance_signature_rejects_bad_pubkey() {
        let mut bad_pk = [0u8; 32];
        bad_pk[31] = 1; // not on the Ed25519 curve
        let body_hash = blake3_hash(b"test body");
        let sig = [0u8; 64];
        // Off-curve pubkey is rejected at `VerifyingKey::from_bytes` BEFORE
        // signature verification — the substrate reports `InvalidPubkeyLength`
        // in this rejection stage. Either error variant is a valid
        // "rejection of malformed pubkey" outcome; the substrate fails-closed.
        assert!(matches!(
            verify_governance_signature(&sig, &body_hash, &bad_pk),
            Err(GovernanceSignatureError::InvalidPubkeyLength(_))
                | Err(GovernanceSignatureError::InvalidSignature)
        ));
    }
}
