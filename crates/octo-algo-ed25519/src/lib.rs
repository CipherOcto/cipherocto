//! Ed25519 as a [`SignatureAlgorithm`] — RFC-0009 AC#11.
//!
//! This crate exists so that `octo-ident` can describe *which* algorithm a
//! verification site should use without naming one. It is the Ed25519
//! registration AC#11 refers to when it excludes "the Ed25519 registration
//! itself" from its prohibition: this is the one place a `VerifyingKey` is
//! allowed to be constructed from bytes.
//!
//! # Layer
//!
//! Layer E (per-algorithm extension) depending on Layer B (identity
//! substrate). The trait lives in `octo-ident`; the implementation lives
//! here. `octo-ident` does not depend on this crate and carries no
//! `ed25519-dalek` dependency — if it did, every crate depending on the
//! identity substrate would inherit a specific algorithm, and the registry
//! would be a wrapper over a hardcoded choice rather than a way to remove
//! one.
//!
//! # Install before verifying
//!
//! [`install`] must run before any verification. A dispatch against an
//! unbootstrapped registry fails closed with
//! [`AlgorithmRegistryError::NotPopulated`], which is deliberately distinct
//! from [`AlgorithmRegistryError::UnknownAlgorithm`] — "this deployment
//! supports nothing yet" and "this identifier names something I do not
//! implement" are different facts.
//!
//! ```no_run
//! octo_algo_ed25519::install().expect("registry bootstrap");
//! ```

#![forbid(unsafe_code)]
#![deny(missing_docs)]

use std::sync::Arc;

use ed25519_dalek::{Signature, Verifier, VerifyingKey};
use octo_ident::{
    bootstrap, global, AlgorithmId, AlgorithmRegistryError, BootstrapError, SignatureAlgorithm,
    SignatureError, ZkSupport,
};

/// Name this algorithm is derived from.
///
/// **Changing this string changes the wire form.** Every DID document
/// stores an `AlgorithmId`, and a document written under one name becomes
/// unreadable under another. The name is also what the storage migration
/// maps legacy kind byte `0x01` onto, so it is not a free label — it is
/// part of the persisted format.
pub const ED25519_NAME: &str = "ed25519";

/// The `AlgorithmId` for Ed25519.
///
/// Derived from [`ED25519_NAME`]. Exposed as a function rather than a
/// constant because `Uuid::new_v5` is not const-constructible.
#[must_use]
pub fn ed25519_algorithm_id() -> AlgorithmId {
    AlgorithmId::derive(ED25519_NAME)
}

/// Ed25519 (RFC 8032) verification.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Ed25519Algorithm;

impl Ed25519Algorithm {
    /// Public key length in bytes.
    pub const PUBLIC_KEY_LEN: usize = 32;
    /// Signature length in bytes.
    pub const SIGNATURE_LEN: usize = 64;

    /// A handle to register.
    #[must_use]
    pub fn shared() -> Arc<dyn SignatureAlgorithm> {
        Arc::new(Self)
    }
}

impl SignatureAlgorithm for Ed25519Algorithm {
    fn id(&self) -> AlgorithmId {
        ed25519_algorithm_id()
    }

    fn name(&self) -> &'static str {
        ED25519_NAME
    }

    fn public_key_len(&self) -> usize {
        Self::PUBLIC_KEY_LEN
    }

    fn signature_len(&self) -> usize {
        Self::SIGNATURE_LEN
    }

    fn zk_support(&self) -> ZkSupport {
        // No constrained in-circuit verifier for RFC-8032 Ed25519 in this
        // codebase. `ZkSupport::Circuit` is an affirmative claim that some
        // circuit backs the algorithm, and claiming it without one turns a
        // fail-closed mint gate fail-open — so the honest default stands.
        ZkSupport::None
    }

    fn verify(
        &self,
        public_key: &[u8],
        message: &[u8],
        signature: &[u8],
    ) -> Result<(), SignatureError> {
        // Length checks come first and are explicit. Ed25519 fixes both
        // lengths, and a slice of the wrong length is a malformed input,
        // not a signature that merely fails to verify. Reporting the two
        // differently is what lets a caller tell "wrong key encoding" from
        // "wrong signature".
        if public_key.len() != Self::PUBLIC_KEY_LEN {
            return Err(SignatureError::BadKeyLength {
                alg: ED25519_NAME,
                got: public_key.len(),
                expected: Self::PUBLIC_KEY_LEN,
            });
        }
        if signature.len() != Self::SIGNATURE_LEN {
            return Err(SignatureError::BadSignatureLength {
                alg: ED25519_NAME,
                got: signature.len(),
                expected: Self::SIGNATURE_LEN,
            });
        }

        let key_bytes: &[u8; Self::PUBLIC_KEY_LEN] = public_key
            .try_into()
            .expect("length checked above, so this cannot fail");
        let sig_bytes: &[u8; Self::SIGNATURE_LEN] = signature
            .try_into()
            .expect("length checked above, so this cannot fail");

        // A 32-byte string is a well-formed Ed25519 point encoding, but not
        // necessarily one on the curve or in the prime-order subgroup. Both
        // failures are the caller's error and must not verify.
        let verifying_key = VerifyingKey::from_bytes(key_bytes)
            .map_err(|_| SignatureError::Invalid(ED25519_NAME))?;
        let sig = Signature::from_bytes(sig_bytes);
        verifying_key
            .verify(message, &sig)
            .map_err(|_| SignatureError::Invalid(ED25519_NAME))
    }
}

/// Install Ed25519 into the process-wide registry.
///
/// Idempotent: calling it from several initialising crates is fine. Install
/// **before** the first verification — an unbootstrapped registry fails
/// closed rather than guessing.
///
/// # Errors
/// [`BootstrapError::ConflictingBootstrap`] if some other caller already
/// installed a different algorithm set.
pub fn install() -> Result<(), BootstrapError> {
    bootstrap(vec![Ed25519Algorithm::shared()])
}

/// The process-wide registry, with Ed25519 guaranteed installed.
///
/// A convenience over [`install`] followed by [`global`] for the common
/// "verify through the shared registry" call, where the caller would
/// otherwise have to bootstrap defensively at every site and repeat the
/// same error handling.
///
/// # Errors
/// [`BootstrapError`] if the install conflicts with an existing set.
pub fn shared_registry() -> Result<&'static octo_ident::AlgorithmRegistry, BootstrapError> {
    install()?;
    Ok(global())
}

/// Verify `signature` over `message` under `public_key` for `algorithm_id`,
/// dispatching through the process-wide registry.
///
/// The single call a verification site should make. It installs on first
/// use, so a site cannot forget to bootstrap and get a confusing
/// `NotPopulated` instead of a verdict.
///
/// # Errors
/// [`AlgorithmRegistryError`] if `algorithm_id` is not registered, or
/// [`BootstrapError`] if the install conflicts. Propagates
/// [`SignatureError`] as-is from the algorithm implementation.
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
/// algorithm in this deployment" from "the signature is bad" without
/// matching on two unrelated enums.
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
    use ed25519_dalek::{Signer, SigningKey};

    fn signed() -> (SigningKey, Vec<u8>, Vec<u8>) {
        let key = SigningKey::from_bytes(&[7u8; 32]);
        let msg = b"holder signature payload";
        let sig = key.sign(msg).to_bytes();
        (key, msg.to_vec(), sig.to_vec())
    }

    #[test]
    fn algorithm_id_is_derived_from_the_stable_name() {
        // The migration maps legacy kind byte 0x01 onto
        // AlgorithmId::derive("ed25519"). If this drifts, every migrated
        // document names an algorithm nobody has registered.
        assert_eq!(ed25519_algorithm_id(), AlgorithmId::derive("ed25519"));
    }

    #[test]
    fn a_genuine_signature_verifies() {
        let (_key, msg, sig) = signed();
        Ed25519Algorithm
            .verify(signed().0.verifying_key().as_bytes(), &msg, &sig)
            .expect("a real signature must verify");
    }

    #[test]
    fn a_tampered_message_is_rejected() {
        let (key, _, sig) = signed();
        assert_eq!(
            Ed25519Algorithm.verify(key.verifying_key().as_bytes(), b"other message", &sig),
            Err(SignatureError::Invalid("ed25519"))
        );
    }

    #[test]
    fn a_signature_from_another_key_is_rejected() {
        let (_, msg, sig) = signed();
        let other = SigningKey::from_bytes(&[8u8; 32]);
        assert_eq!(
            Ed25519Algorithm.verify(other.verifying_key().as_bytes(), &msg, &sig),
            Err(SignatureError::Invalid("ed25519"))
        );
    }

    #[test]
    fn wrong_lengths_are_length_errors_not_verification_failures() {
        // Distinguishing these is the point: a caller that only sees
        // "invalid" cannot tell a malformed key from a bad signature.
        assert_eq!(
            Ed25519Algorithm.verify(&[0u8; 31], b"m", &[0u8; 64]),
            Err(SignatureError::BadKeyLength {
                alg: "ed25519",
                got: 31,
                expected: 32
            })
        );
        assert_eq!(
            Ed25519Algorithm.verify(&[0u8; 32], b"m", &[0u8; 63]),
            Err(SignatureError::BadSignatureLength {
                alg: "ed25519",
                got: 63,
                expected: 64
            })
        );
    }

    #[test]
    fn a_short_slice_is_never_accepted_as_a_key() {
        // The pre-refactor sites sliced a fixed-size array out of a
        // caller-supplied slice, which panics on a short input. A verifier
        // must return an error, not abort the process.
        assert!(Ed25519Algorithm
            .verify(&[0u8; 8], b"m", &[0u8; 64])
            .is_err());
        assert!(Ed25519Algorithm
            .verify(&[0u8; 32], b"m", &[0u8; 8])
            .is_err());
    }

    #[test]
    fn an_off_curve_key_is_rejected_not_verified() {
        // 32 bytes is a well-formed encoding length; these bytes are not
        // a point on the curve. Constructing the key must fail rather than
        // silently yielding a key that verifies nothing.
        let all_ff = [0xFFu8; 32];
        assert!(Ed25519Algorithm.verify(&all_ff, b"m", &[0u8; 64]).is_err());
    }

    #[test]
    fn concurrent_install_is_serialised() {
        // The check-then-act race this pins: `bootstrap` reads the
        // ready-flag, fills the map, and sets the flag, and without a lock
        // across all three, two threads can both observe "not populated"
        // and both try to populate. One wins, the other sees
        // `AlreadyRegistered` and reports a conflict where there was only
        // agreement. Two of these eight threads ran concurrently when
        // this vector was first written and the suite went red about 2 runs
        // in 25 — so it is a real race, not a theoretical one.
        let handles: Vec<_> = (0..8).map(|_| std::thread::spawn(install)).collect();
        for handle in handles {
            handle
                .join()
                .expect("thread must not panic")
                .expect("concurrent install of the same set must not conflict");
        }
    }

    #[test]
    fn zk_support_is_none() {
        assert_eq!(Ed25519Algorithm.zk_support(), ZkSupport::None);
    }

    #[test]
    fn dispatch_resolves_through_the_registry() {
        install().expect("install");
        let (key, msg, sig) = signed();
        verify_dispatch(
            &ed25519_algorithm_id(),
            key.verifying_key().as_bytes(),
            &msg,
            &sig,
        )
        .expect("dispatch must resolve Ed25519 through the registry");
    }

    #[test]
    fn dispatch_on_an_unregistered_id_is_an_unknown_not_an_invalid_signature() {
        install().expect("install");
        let (key, msg, sig) = signed();
        let stranger = AlgorithmId::derive("not-a-real-algorithm");
        assert_eq!(
            verify_dispatch(&stranger, key.verifying_key().as_bytes(), &msg, &sig),
            Err(DispatchError::Unknown(
                AlgorithmRegistryError::UnknownAlgorithm(stranger)
            )),
            "an unregistered id must not fall back to a default algorithm"
        );
    }
}
