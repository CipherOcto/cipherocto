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
    /// Signature verification failed under the named algorithm.
    ///
    /// Also covers a registry lookup that found nothing, since a caller
    /// learns the same thing either way and cannot probe which algorithms
    /// a deployment supports.
    #[error("governance signature verification failed")]
    InvalidSignature,
    /// Public key length does not match what the named algorithm requires.
    #[error("governance pubkey length {0} does not match the algorithm")]
    InvalidPubkeyLength(usize),
}

/// The algorithm governance signatures are verified under by default.
///
/// The verifier below takes the algorithm as a parameter so it genuinely
/// dispatches (RFC-0009 AC#11); this is the value current callers pass,
/// kept in one place so a consumer does not have to add its own dependency
/// just to name the algorithm. The honest home is a field alongside the
/// governance key on the signed body, which this slice does not change.
#[must_use]
pub fn governance_algorithm_id() -> octo_ident::AlgorithmId {
    octo_algo_ed25519::ed25519_algorithm_id()
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
    algorithm_id: &octo_ident::AlgorithmId,
    sig: &[u8],
    body_hash: &[u8; 32],
    governance_pubkey: &[u8],
) -> Result<(), GovernanceSignatureError> {
    // One way in. This used to reach for `shared_registry()` itself, take the
    // algorithm out by hand and call `verify` on it, so that a wrong key
    // length could be reported as `InvalidPubkeyLength` rather than folded
    // into "signature bad". That distinction is worth keeping — it tells a
    // caller holding a 33-byte key under a 32-byte algorithm what actually
    // went wrong — but it did not need a second route to the registry.
    //
    // The registered algorithm reports the same length error, and the dispatch
    // error carries it intact, so the specific case is recovered by matching
    // rather than by bypassing the entry point.
    octo_algo_ed25519::verify_dispatch(algorithm_id, governance_pubkey, body_hash, sig).map_err(
        |e| match e {
            octo_algo_ed25519::DispatchError::Signature(
                octo_ident::SignatureError::BadKeyLength { got, .. },
            ) => GovernanceSignatureError::InvalidPubkeyLength(got),
            _ => GovernanceSignatureError::InvalidSignature,
        },
    )
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
    /// The algorithm these fixtures sign under. Centralised so a second
    /// algorithm is one edit, not a hunt through the test bodies.
    fn ed25519_id() -> octo_ident::AlgorithmId {
        octo_algo_ed25519::ed25519_algorithm_id()
    }

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

    /// The vector that tells dispatch apart from a 32-and-64-byte assumption.
    ///
    /// Every other test in this module reaches the verifier with a 32-byte
    /// Ed25519 key and a 64-byte Ed25519 signature, through
    /// `governance_algorithm_id()`. A verifier that hardcoded those two widths
    /// would pass all of them, and would pass this crate's other consumers too.
    ///
    /// **This vector could not have existed before the parameters were
    /// widened.** `sig` was `&GovernanceSignatureBytes` and `governance_pubkey`
    /// was `&GovernancePubkey` — aliases for `[u8; 64]` and `[u8; 32]` — so a
    /// 33-byte key was a compile error at the call rather than a runtime
    /// verdict, and the algorithm parameter could only ever be consulted to
    /// reject. This module is the substrate's shared governance verifier, so
    /// that reached every governance-signed event type in the workspace.
    ///
    /// It also pins the error split. A wrong-width key must still be reported
    /// as `InvalidPubkeyLength` rather than folded into `InvalidSignature` —
    /// the caller needs to know it handed over the wrong kind of key.
    #[test]
    fn a_governance_signature_verifies_under_stark_schnorr() {
        use octo_algo_stark_schnorr::{public_key, sign, Felt};

        octo_algo_ed25519::install().expect("install ed25519");
        octo_algo_stark_schnorr::install().expect("install stark-schnorr");

        let secret = Felt::from_hex_unchecked("0x6b1c2d3e4f506172");
        let pk = public_key(&secret).expect("derives a public key").to_wire();
        assert_eq!(
            pk.len(),
            33,
            "a stark-schnorr governance key is 33 bytes — that is what the \
             widened parameter makes expressible"
        );

        let body_hash = blake3_hash(b"governance body");
        let sig = sign(
            &secret,
            &body_hash,
            &Felt::from_hex_unchecked("0x0099887766554433"),
        )
        .expect("sign")
        .to_wire();

        assert!(
            verify_governance_signature(
                &octo_algo_stark_schnorr::stark_schnorr_algorithm_id(),
                &sig,
                &body_hash,
                &pk,
            )
            .is_ok(),
            "a genuine stark-schnorr governance signature must not be refused \
             by the dispatched path"
        );

        // The error split survives: the right algorithm with the wrong-width
        // key is a key-length problem, not a bad signature.
        let ed = octo_algo_ed25519::ed25519_algorithm_id();
        assert!(
            matches!(
                verify_governance_signature(&ed, &sig, &body_hash, &pk),
                Err(GovernanceSignatureError::InvalidPubkeyLength(33))
            ),
            "a 33-byte key under ed25519 must be reported as a key-length \
             problem, not as an invalid signature"
        );

        // And a genuine Ed25519 governance signature under the same key width
        // still works, so the vector is not passing by refusing everything.
        let sk = ed25519_dalek::SigningKey::from_bytes(&[0x51u8; 32]);
        let good = sk.sign(&body_hash).to_bytes();
        assert!(
            verify_governance_signature(&ed, &good, &body_hash, &sk.verifying_key().to_bytes())
                .is_ok(),
            "a genuine ed25519 governance signature must still verify"
        );
    }

    #[test]
    fn verify_governance_signature_accepts_valid_signature() {
        let (sk, pk) = sample_key();
        let body_hash = blake3_hash(b"test body");
        let sig = sk.sign(&body_hash).to_bytes();
        assert_eq!(
            verify_governance_signature(&ed25519_id(), &sig, &body_hash, &pk),
            Ok(())
        );
    }

    #[test]
    fn verify_governance_signature_rejects_invalid_signature() {
        let (_sk, pk) = sample_key();
        let body_hash = blake3_hash(b"test body");
        let bogus_sig = [0u8; 64];
        assert_eq!(
            verify_governance_signature(&ed25519_id(), &bogus_sig, &body_hash, &pk),
            Err(GovernanceSignatureError::InvalidSignature)
        );
    }

    #[test]
    fn verify_governance_signature_rejects_bad_pubkey() {
        let mut bad_pk = [0u8; 32];
        bad_pk[31] = 1; // not on the Ed25519 curve
        let body_hash = blake3_hash(b"test body");
        let sig = [0u8; 64];
        // An off-curve pubkey is rejected before signature verification — the
        // substrate reports `InvalidSignature` in this rejection stage, since
        // the length check passes (32 bytes) and the algorithm itself refuses
        // the encoding. Either error variant is a valid "rejection of
        // malformed pubkey" outcome; the substrate fails-closed.
        assert!(matches!(
            verify_governance_signature(&ed25519_id(), &sig, &body_hash, &bad_pk),
            Err(GovernanceSignatureError::InvalidPubkeyLength(_))
                | Err(GovernanceSignatureError::InvalidSignature)
        ));
    }
}
