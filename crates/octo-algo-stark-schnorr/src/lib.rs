//! Schnorr over the Cairo STARK curve as a [`SignatureAlgorithm`] —
//! RFC-0009 §Dual-format signature algorithms and the ZK-capable key.
//!
//! This is the second algorithm the registry was designed to hold. Ed25519
//! alone cannot prove the registry works: with one entry, every lookup that
//! succeeds would also succeed under a hardcoded verifier, and an
//! implementation that quietly returned Ed25519 for any unknown identifier
//! would never be caught. Two algorithms make those failures observable.
//!
//! # `ZkSupport::None`, and that is the whole point
//!
//! This crate registers as [`ZkSupport::None`] — **not**
//! [`ZkSupport::Circuit`]. `None` is a substantive claim: no constrained
//! in-circuit verifier exists, so a holder keyed here cannot produce a ZK
//! capability proof, and RFC-0009 AC#14's mint gate MUST fail closed rather
//! than fall back to another key.
//!
//! Asserting `Circuit` would turn that fail-closed gate **fail-open**, by
//! admitting exactly the key whose in-circuit verification is missing. The
//! capability circuit's holder slot is a structural commitment — it
//! Poseidon-hashes four commitments and asserts non-zero, and `stage2_main`
//! feeds all four the same commitment. It verifies nothing today. The
//! `#[cfg(test)]`-gated `stark_schnorr.cairo` module that does verify is not
//! called from the entrypoint, and wiring it changes the committed CASM
//! digest — an RFC-0958 amendment, not a registry edit.
//!
//! Promotion to `Circuit` is therefore a separate change with its own
//! approval. `zk_support_is_none_while_no_in_circuit_verifier_exists` is the
//! vector that keeps the gate shut, and it is the direct inverse of what an
//! earlier draft of this mission asserted.
//!
//! # Layer
//!
//! Layer E (per-algorithm extension) depending on Layer B (identity
//! substrate). The trait lives in `octo-ident`; the implementation lives here.
//! `octo-ident` depends on neither this crate nor any curve library.
//!
//! # What is *not* here
//!
//! Key custody. The `stark_schnorr.cairo` module records the trade
//! explicitly: holders would need a scheme only this protocol can verify,
//! which costs hardware-wallet and MPC compatibility. That is a product
//! decision, not a cryptographic one, and it belongs to whoever builds the
//! custody layer — see RFC-0009 G9.

#![forbid(unsafe_code)]
#![deny(missing_docs)]

use std::sync::Arc;

use octo_ident::{
    bootstrap_additional, global, AlgorithmId, AlgorithmRegistryError, BootstrapError,
    SignatureAlgorithm, SignatureError, ZkSupport,
};

pub mod key;
pub mod scheme;

pub use key::{StarkSchnorrKeyError, StarkSchnorrPublicKey};
pub use scheme::{
    add_mod_order, challenge, challenge_bytes, curve_order, felts_to_message, message_to_felts,
    mul_mod_order, public_key, reduce_mod_order, sign, verify, verify_with_challenge,
    StarkSchnorrSignError, StarkSchnorrSignature, CURVE_ORDER_HEX, MESSAGE_BYTES_PER_FELT,
};

/// Name this algorithm is derived from.
///
/// **Changing this string changes the wire form.** Every DID document stores
/// an `AlgorithmId`, and a document written under one name becomes unreadable
/// under another.
pub const STARK_SCHNORR_NAME: &str = "stark-schnorr";

/// The `AlgorithmId` for STARK-curve Schnorr.
///
/// Derived from [`STARK_SCHNORR_NAME`]. A function rather than a constant
/// because `Uuid::new_v5` is not const-constructible.
#[must_use]
pub fn stark_schnorr_algorithm_id() -> AlgorithmId {
    AlgorithmId::derive(STARK_SCHNORR_NAME)
}

/// Schnorr over the Cairo STARK curve.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct StarkSchnorrAlgorithm;

impl StarkSchnorrAlgorithm {
    /// Public key length in bytes: 32-byte x-coordinate plus a parity byte.
    ///
    /// **33, not 32.** The parity bit is what makes the key unambiguous —
    /// `(x, y)` and `(x, −y)` are both on the curve — so it must travel.
    pub const PUBLIC_KEY_LEN: usize = 33;
    /// Signature length in bytes: `r ‖ s`, each 32 bytes.
    pub const SIGNATURE_LEN: usize = 64;

    /// A handle to register.
    #[must_use]
    pub fn shared() -> Arc<dyn SignatureAlgorithm> {
        Arc::new(Self)
    }
}

impl SignatureAlgorithm for StarkSchnorrAlgorithm {
    fn id(&self) -> AlgorithmId {
        stark_schnorr_algorithm_id()
    }

    fn name(&self) -> &'static str {
        STARK_SCHNORR_NAME
    }

    fn public_key_len(&self) -> usize {
        Self::PUBLIC_KEY_LEN
    }

    fn signature_len(&self) -> usize {
        Self::SIGNATURE_LEN
    }

    fn zk_support(&self) -> ZkSupport {
        // See the module docs. Claiming `Circuit` here would turn RFC-0009
        // AC#14's fail-closed mint gate fail-open by admitting exactly the
        // key whose in-circuit verification does not exist.
        ZkSupport::None
    }

    fn verify(
        &self,
        public_key: &[u8],
        message: &[u8],
        signature: &[u8],
    ) -> Result<(), SignatureError> {
        // Length checks first and explicit, matching the Ed25519 registration:
        // a slice of the wrong length is a malformed input, not a signature
        // that merely fails to verify, and a caller that cannot tell those
        // apart cannot tell a bad key encoding from a bad signature.
        if public_key.len() != Self::PUBLIC_KEY_LEN {
            return Err(SignatureError::BadKeyLength {
                alg: STARK_SCHNORR_NAME,
                got: public_key.len(),
                expected: Self::PUBLIC_KEY_LEN,
            });
        }
        if signature.len() != Self::SIGNATURE_LEN {
            return Err(SignatureError::BadSignatureLength {
                alg: STARK_SCHNORR_NAME,
                got: signature.len(),
                expected: Self::SIGNATURE_LEN,
            });
        }

        let key = StarkSchnorrPublicKey::from_wire_for_verify(public_key, STARK_SCHNORR_NAME)?;
        let sig = StarkSchnorrSignature::from_wire(signature)
            .map_err(|_| SignatureError::Invalid(STARK_SCHNORR_NAME))?;
        verify(&key, message, &sig).map_err(|_| SignatureError::Invalid(STARK_SCHNORR_NAME))
    }
}

/// Install STARK-curve Schnorr into the process-wide registry.
///
/// Idempotent, and **composable**: this adds to whatever is installed rather
/// than declaring the deployment's whole algorithm set. That is what lets two
/// per-algorithm crates — which must not depend on each other to be
/// installable together — coexist in one binary. See
/// [`octo_ident::bootstrap_additional`].
///
/// A deployment that must **restrict** the algorithm set rather than extend
/// it should bootstrap the set itself and not use this; see
/// [`octo_ident::bootstrap_additional`] §What this does not do.
///
/// # Errors
/// [`BootstrapError::Registry`] if the registry rejects the algorithm.
pub fn install() -> Result<(), BootstrapError> {
    bootstrap_additional(vec![StarkSchnorrAlgorithm::shared()])
}

/// The process-wide registry, with STARK-curve Schnorr guaranteed installed.
///
/// # Errors
/// [`BootstrapError`] if the install fails.
pub fn shared_registry() -> Result<&'static octo_ident::AlgorithmRegistry, BootstrapError> {
    let registry = global();
    // Read-lock fast path: `install` takes a process-wide lock, and taking it
    // on every verification would serialise every envelope, hop and token
    // behind one mutex on what is a hot path.
    if registry.is_populated() {
        return Ok(registry);
    }
    install()?;
    Ok(registry)
}

/// Verify `signature` over `message` under `public_key` for `algorithm_id`,
/// dispatching through the process-wide registry.
///
/// # Errors
/// [`AlgorithmRegistryError`] if `algorithm_id` is not registered,
/// [`BootstrapError`] if the install conflicts, or [`SignatureError`] if a
/// registered algorithm rejects the signature.
pub fn verify_dispatch(
    algorithm_id: &AlgorithmId,
    public_key: &[u8],
    message: &[u8],
    signature: &[u8],
) -> Result<(), DispatchError> {
    let registry = shared_registry()?;
    let algorithm = registry.get(*algorithm_id)?;
    algorithm
        .verify(public_key, message, signature)
        .map_err(DispatchError::from)
}

/// Failure of [`verify_dispatch`].
///
/// Kept distinct from [`SignatureError`] so a caller can tell "no such
/// algorithm in this deployment" from "the signature is bad" without matching
/// on two unrelated enums.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum DispatchError {
    /// The registry could not be brought up.
    #[error("algorithm registry bootstrap failed: {0}")]
    Bootstrap(#[from] BootstrapError),
    /// No registered implementation for this identifier.
    #[error("no registered algorithm: {0}")]
    Unknown(#[from] AlgorithmRegistryError),
    /// A registered algorithm rejected the signature.
    #[error(transparent)]
    Signature(#[from] SignatureError),
}

#[cfg(test)]
mod tests {
    use super::*;
    use starknet_types_core::felt::Felt;

    fn fe(hex: &str) -> Felt {
        Felt::from_hex_unchecked(hex)
    }

    // ── The Cairo vectors, replayed here ──────────────────────────────────
    //
    // Every constant below is copied from `cairo/src/stark_schnorr.cairo`,
    // which produced them from an independent pure-Python implementation of
    // the STARK curve — not from Cairo's own opinion of what a Schnorr
    // signature looks like. Re-running them through the Rust verifier is what
    // turns "two implementations agree" from an assumption into a check.

    /// `V1_*` from `cairo/src/stark_schnorr.cairo`.
    const V1_PUB_X: &str = "0x314635a026b93e959147149cffb3e37cfec1b8c554e8b00b3ac8b30a7baa3c7";
    const V1_PUB_SIGN: u8 = 1;
    const V1_R: &str = "0x4baadfa62cb588e0d2d33f7c6f546fe31834eba183f0826bc91c96845b8c6f8";
    const V1_E: &str = "0xdeadbeef";
    const V1_S: &str = "0x16c341075d5b3b3e504f4d3b291704133cbf4068";

    /// `V2_*` from `cairo/src/stark_schnorr.cairo`.
    const V2_PUB_X: &str = "0x64f83f2f683af14c41cba23ca0ebadcbcd1f0718a9cde058765cb061932238f";
    const V2_PUB_SIGN: u8 = 1;
    const V2_R: &str = "0x44063f06b37c09baf2b68de7a2f44d8e1af29ed24e7ab7302c8bb8441ab8fac";
    const V2_E: &str = "0xfeedface";
    const V2_S: &str = "0x2b0e05f3123579cd53a6da0d3fc7fc414ed10c52";

    fn key(pub_x: &str, sign: u8) -> StarkSchnorrPublicKey {
        StarkSchnorrPublicKey::new(fe(pub_x).to_bytes_be(), sign == 1).expect("canonical")
    }

    /// The 7 vectors from the Cairo module, with one Rust test per case so a
    /// failure names which property broke.
    #[test]
    fn stark_schnorr_verifies_against_reference_vectors() {
        assert!(
            verify_with_challenge(&key(V1_PUB_X, V1_PUB_SIGN), &fe(V1_R), &fe(V1_E), &fe(V1_S)),
            "vector 1 must verify"
        );
        assert!(
            verify_with_challenge(&key(V2_PUB_X, V2_PUB_SIGN), &fe(V2_R), &fe(V2_E), &fe(V2_S)),
            "vector 2 must verify"
        );
    }

    #[test]
    fn stark_schnorr_rejects_wrong_public_key() {
        // The other key's signature against this pubkey: the case that stops
        // one holder riding another's authorization.
        assert!(
            !verify_with_challenge(&key(V1_PUB_X, V1_PUB_SIGN), &fe(V2_R), &fe(V2_E), &fe(V2_S)),
            "another key's signature must not verify here"
        );
    }

    #[test]
    fn stark_schnorr_rejects_wrong_message() {
        let secret = fe("0x2f1e3d4c5b6a79880716253443526170");
        let pk = public_key(&secret).expect("key");
        let sig = sign(&secret, b"transfer 1 octo", &fe("0x1234")).expect("sign");
        assert!(verify(&pk, b"transfer 1 octo", &sig).is_ok());
        assert!(verify(&pk, b"transfer 2 octo", &sig).is_err());
    }

    #[test]
    fn stark_schnorr_rejects_tampered_signature() {
        // A wrong challenge, a wrong s in both directions, and a mismatched
        // commitment r — the Cairo module's four negative cases.
        let k = key(V1_PUB_X, V1_PUB_SIGN);
        assert!(!verify_with_challenge(
            &k,
            &fe(V1_R),
            &(fe(V1_E) + Felt::ONE),
            &fe(V1_S)
        ));
        assert!(!verify_with_challenge(
            &k,
            &fe(V1_R),
            &fe(V1_E),
            &(fe(V1_S) + Felt::ONE)
        ));
        assert!(!verify_with_challenge(
            &k,
            &fe(V1_R),
            &fe(V1_E),
            &(fe(V1_S) - Felt::ONE)
        ));
        assert!(!verify_with_challenge(&k, &fe(V2_R), &fe(V1_S), &fe(V1_E)));
    }

    /// A verifier returning `true` unconditionally would pass the positive
    /// vectors. This is the anti-vacuity case for the suite above.
    #[test]
    fn a_refused_key_shape_does_not_verify_the_reference_vectors() {
        let flipped = key(V1_PUB_X, 1 - V1_PUB_SIGN);
        assert!(
            !verify_with_challenge(&flipped, &fe(V1_R), &fe(V1_E), &fe(V1_S)),
            "a flipped pubkey parity must not verify"
        );
    }

    #[test]
    fn an_off_curve_public_key_is_refused_rather_than_crashing() {
        // Roughly half of all x values have no point on the curve, so a
        // verifier cannot assume construction succeeds.
        let mut x = fe("0x2a2b3c4d5e6f708192a3b4c5d6e7f809");
        let mut checked = 0;
        while checked < 64 {
            if starknet_types_core::curve::AffinePoint::new_from_x(&x, false).is_none() {
                let k = StarkSchnorrPublicKey::new(x.to_bytes_be(), false).expect("canonical");
                assert!(!verify_with_challenge(&k, &fe(V1_R), &fe(V1_E), &fe(V1_S)));
                return;
            }
            x += Felt::ONE;
            checked += 1;
        }
        panic!("expected some x with no curve point");
    }

    // ── Registration ──────────────────────────────────────────────────────

    /// The vector that keeps RFC-0009 AC#14's fail-closed mint gate shut.
    #[test]
    fn zk_support_is_none_while_no_in_circuit_verifier_exists() {
        install().expect("install");
        let registry = shared_registry().expect("registry");
        let alg = registry
            .get(stark_schnorr_algorithm_id())
            .expect("registered");
        assert_eq!(
            alg.zk_support(),
            ZkSupport::None,
            "claiming ZkSupport::Circuit without a circuit would admit exactly \
             the key whose in-circuit verification is missing, turning the \
             fail-closed mint gate fail-open"
        );
    }

    #[test]
    fn algorithm_id_is_derived_from_the_stable_name() {
        assert_eq!(
            stark_schnorr_algorithm_id(),
            AlgorithmId::derive("stark-schnorr")
        );
        assert_ne!(
            stark_schnorr_algorithm_id(),
            octo_algo_ed25519::ed25519_algorithm_id(),
            "the two algorithms must not share an identifier"
        );
    }

    #[test]
    fn declared_lengths_match_the_encodings() {
        let alg = StarkSchnorrAlgorithm;
        assert_eq!(alg.public_key_len(), StarkSchnorrPublicKey::WIRE_LEN);
        assert_eq!(alg.signature_len(), StarkSchnorrSignature::WIRE_LEN);
    }

    #[test]
    fn a_genuine_signature_verifies_through_the_trait() {
        let secret = fe("0x2f1e3d4c5b6a79880716253443526170");
        let pk = public_key(&secret).expect("key");
        let sig = sign(&secret, b"holder payload", &fe("0x1234")).expect("sign");
        StarkSchnorrAlgorithm
            .verify(&pk.to_wire(), b"holder payload", &sig.to_wire())
            .expect("a real signature must verify");
    }

    #[test]
    fn wrong_lengths_are_length_errors_not_verification_failures() {
        assert_eq!(
            StarkSchnorrAlgorithm.verify(&[0u8; 32], b"m", &[0u8; 64]),
            Err(SignatureError::BadKeyLength {
                alg: "stark-schnorr",
                got: 32,
                expected: 33
            })
        );
        assert_eq!(
            StarkSchnorrAlgorithm.verify(&[0u8; 33], b"m", &[0u8; 63]),
            Err(SignatureError::BadSignatureLength {
                alg: "stark-schnorr",
                got: 63,
                expected: 64
            })
        );
    }

    #[test]
    fn dispatch_resolves_through_the_registry() {
        install().expect("install");
        let secret = fe("0x2f1e3d4c5b6a79880716253443526170");
        let pk = public_key(&secret).expect("key");
        let sig = sign(&secret, b"holder payload", &fe("0x1234")).expect("sign");
        verify_dispatch(
            &stark_schnorr_algorithm_id(),
            &pk.to_wire(),
            b"holder payload",
            &sig.to_wire(),
        )
        .expect("dispatch must resolve stark-schnorr through the registry");
    }

    #[test]
    fn dispatch_on_ed25519_material_is_rejected_not_silently_accepted() {
        // The cross-algorithm replay this whole registry exists to stop. Both
        // algorithms carry a 64-byte signature, so a length check cannot
        // distinguish them — only dispatch on the named identifier can.
        install().expect("install");
        let secret = fe("0x2f1e3d4c5b6a79880716253443526170");
        let pk = public_key(&secret).expect("key");
        let sig = sign(&secret, b"holder payload", &fe("0x1234")).expect("sign");
        let ed_id = octo_algo_ed25519::ed25519_algorithm_id();

        // Under the Ed25519 identifier, a STARK public key is the wrong shape.
        assert!(
            verify_dispatch(&ed_id, &pk.to_wire(), b"holder payload", &sig.to_wire()).is_err(),
            "a STARK key must not verify under the Ed25519 identifier"
        );
    }

    #[test]
    fn dispatch_on_an_unregistered_id_is_an_unknown_not_an_invalid_signature() {
        install().expect("install");
        let stranger = AlgorithmId::derive("not-a-real-algorithm");
        assert_eq!(
            verify_dispatch(&stranger, &[0u8; 33], b"m", &[0u8; 64]),
            Err(DispatchError::Unknown(
                AlgorithmRegistryError::UnknownAlgorithm(stranger)
            )),
            "an unregistered id must not fall back to a default algorithm"
        );
    }

    /// The property Ed25519 alone could never demonstrate: both algorithms
    /// installed, each dispatched by its own identifier.
    ///
    /// With one registered algorithm this is vacuous — the registry would
    /// return the right answer under a hardcoded verifier, and a cross-
    /// algorithm replay would be invisible. This is the test that makes the
    /// second registration load-bearing.
    #[test]
    fn the_registry_holds_two_algorithms_and_dispatches_each() {
        install().expect("install stark-schnorr");
        octo_algo_ed25519::install().expect("install ed25519 after stark-schnorr");

        let registry = shared_registry().expect("registry");
        assert!(
            registry.len() >= 2,
            "the registry must hold both algorithms, got {}",
            registry.len()
        );

        let ed_id = octo_algo_ed25519::ed25519_algorithm_id();
        let sn_id = stark_schnorr_algorithm_id();
        assert_ne!(ed_id, sn_id);

        // Each resolves to its own implementation, by name.
        assert_eq!(registry.get(ed_id).expect("ed25519").name(), "ed25519");
        assert_eq!(
            registry.get(sn_id).expect("stark-schnorr").name(),
            "stark-schnorr"
        );

        // And each verifies only its own material.
        let ed = octo_algo_ed25519::Ed25519Algorithm;
        use ed25519_dalek::{Signer, SigningKey};
        let signing = SigningKey::from_bytes(&[3u8; 32]);
        let ed_msg = b"ed25519 payload";
        let ed_sig = signing.sign(ed_msg).to_bytes().to_vec();

        assert_eq!(ed.public_key_len(), 32);
        assert_eq!(StarkSchnorrAlgorithm.public_key_len(), 33);

        // The STARK signature is 64 bytes, the same as Ed25519's, so replaying
        // one under the other's identifier is refused on the key shape and
        // the equation rather than on length.
        let secret = fe("0x2f1e3d4c5b6a79880716253443526170");
        let pk = public_key(&secret).expect("key");
        let sn_sig = sign(&secret, ed_msg, &fe("0x1234")).expect("sign");

        assert!(verify_dispatch(&sn_id, &pk.to_wire(), ed_msg, &sn_sig.to_wire()).is_ok());
        assert!(verify_dispatch(&ed_id, &pk.to_wire(), ed_msg, &sn_sig.to_wire()).is_err());
        assert!(
            verify_dispatch(&ed_id, signing.verifying_key().as_bytes(), ed_msg, &ed_sig).is_ok()
        );
        assert!(
            verify_dispatch(&sn_id, signing.verifying_key().as_bytes(), ed_msg, &ed_sig).is_err()
        );
    }
}
