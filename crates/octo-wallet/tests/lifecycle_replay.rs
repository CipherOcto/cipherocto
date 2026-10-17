//! A revocation proof made under one algorithm, presented under another.
//!
//! This is the decisive vector for the lifecycle-proof migration. The proof
//! used to be the bare constant `b"revoke"`, and the verifier was handed the
//! algorithm to use by its caller — so the signed bytes said nothing about
//! which equation produced them.
//!
//! It needs a second real algorithm, so it lives here as an integration test
//! rather than in the unit tests: `octo-wallet` carries the algorithm as a
//! dev-dependency only, and depending on it at runtime would invert the
//! registry's layering.

use octo_algo_ed25519::ed25519_algorithm_id;
use octo_algo_stark_schnorr::stark_schnorr_algorithm_id;
use octo_algo_stark_schnorr::{public_key, sign, Felt};
use octo_ident::VerificationMethodId;
use octo_wallet::{IdentityKey, WalletError};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

const KEY_ID: &str = "key-a";

fn revoked() -> (IdentityKey, Vec<u8>) {
    octo_algo_ed25519::install().expect("install ed25519");
    let mut key = IdentityKey::from_seed([51u8; 32])
        .with_verification_method_id(VerificationMethodId::derive(KEY_ID));
    key.activate(1_000).expect("activate");
    key.revoke(2_000).expect("revoke");
    let proof = key
        .revoked_proof()
        .expect("a key context yields a bound proof");
    (key, proof)
}

/// The proof verifies under the algorithm it was made with.
#[test]
fn a_revocation_proof_verifies_under_its_own_algorithm() {
    let (key, proof) = revoked();
    IdentityKey::verify_revocation_proof(
        &ed25519_algorithm_id(),
        &VerificationMethodId::derive(KEY_ID),
        key.public_key_bytes(),
        &proof,
    )
    .expect("genuine");
}

/// And it does not verify when the same proof is presented as the other
/// algorithm's.
///
/// Before the prefix, this call could not distinguish the two: the signed
/// bytes were the same six constants in both cases, and the only thing that
/// differed was which verifier the caller chose. That is the property
/// RFC-0009 §Domain separation exists to remove.
#[test]
fn a_revocation_proof_replayed_as_the_other_algorithm_is_rejected() {
    let (key, proof) = revoked();

    let outcome = IdentityKey::verify_revocation_proof(
        &stark_schnorr_algorithm_id(),
        &VerificationMethodId::derive(KEY_ID),
        key.public_key_bytes(),
        &proof,
    );
    assert!(
        matches!(outcome, Err(WalletError::InvalidRevocationProof)),
        "an Ed25519 revocation proof must not verify as stark-schnorr, \
         got {outcome:?}"
    );

    // And the same signature under the right algorithm still verifies, so
    // the refusal above is about the algorithm and not a broken key.
    IdentityKey::verify_revocation_proof(
        &ed25519_algorithm_id(),
        &VerificationMethodId::derive(KEY_ID),
        key.public_key_bytes(),
        &proof,
    )
    .expect("the genuine form must still verify");
}

/// The bare six-byte constant, signed directly, is refused. This is the
/// downgrade path: a verifier that accepted it would keep honouring proofs
/// minted before the prefix existed.
#[test]
fn the_legacy_bare_constant_is_refused() {
    let mut key = IdentityKey::from_seed([52u8; 32])
        .with_verification_method_id(VerificationMethodId::derive(KEY_ID));
    key.activate(1_000).expect("activate");
    let pubkey = key.public_key_bytes();
    let legacy = key.signer().sign(b"revoke").expect("sign the bare literal");

    let outcome = IdentityKey::verify_revocation_proof(
        &ed25519_algorithm_id(),
        &VerificationMethodId::derive(KEY_ID),
        pubkey,
        &legacy,
    );
    assert!(
        matches!(outcome, Err(WalletError::InvalidRevocationProof)),
        "an unprefixed proof must be refused, not stripped, got {outcome:?}"
    );
}

/// **The vector that mutation proved missing, and the one that finally
/// exercises a second algorithm end to end.**
///
/// `a_revocation_proof_replayed_as_the_other_algorithm_is_rejected` above
/// looks like cross-algorithm coverage and is not. It hands the verifier an
/// Ed25519 key and an Ed25519 proof named as stark-schnorr, so both a
/// dispatching verifier and a hardcoded Ed25519 one refuse it — the rejection
/// is guaranteed by the input and says nothing about whether the algorithm
/// argument was honoured. Proved by mutation: replacing the `algorithm_id`
/// passed to `verify_dispatch` with a literal Ed25519 id leaves all three
/// tests in that file green.
///
/// This one signs with stark-schnorr and expects `Ok`, which a hardcoded
/// Ed25519 verifier cannot produce from a 33-byte key and a stark-schnorr
/// signature. That is what makes it mutation-sensitive.
#[test]
fn a_stark_schnorr_revocation_proof_verifies_through_the_dispatched_path() {
    use octo_algo_stark_schnorr::{public_key, sign, Felt};

    octo_algo_ed25519::install().expect("install ed25519");
    octo_algo_stark_schnorr::install().expect("install stark-schnorr");

    let secret = Felt::from_hex_unchecked("0x2f1e3d4c5b6a79880716253443526170");
    let nonce = Felt::from_hex_unchecked("0x13579bdf02468ace");
    let key_id = VerificationMethodId::derive(KEY_ID);
    let algorithm = stark_schnorr_algorithm_id();

    // 33 bytes, not 32. This is the whole point: a `[u8; 32]` parameter
    // cannot express this key, which is why mission `0009-cc` widened the
    // verification sites to take a slice.
    let public_key = public_key(&secret).expect("derives a public key");
    let wire = public_key.to_wire();
    assert_eq!(
        wire.len(),
        33,
        "stark-schnorr public keys are 33 bytes. If this ever reads 32 the \
         vector has stopped testing what it claims to."
    );

    // Build the exact message `IdentityKey::verify_lifecycle_proof` re-derives,
    // so the vector exercises the production preimage rather than a fixture.
    let message = octo_ident::SignedMessage::new(
        octo_ident::SignablePrefix::new(algorithm, key_id),
        b"revoke".to_vec(),
    );
    let signable = message
        .signable(octo_ident::SignableForm::Raw)
        .expect("raw signable form");

    let signature = sign(&secret, &signable, &nonce).expect("sign");
    let proof: [u8; 64] = signature.to_wire();

    IdentityKey::verify_revocation_proof(&algorithm, &key_id, &wire, &proof)
        .expect("a stark-schnorr proof must verify under stark-schnorr");

    // And the same bytes under Ed25519 must not, so the acceptance above came
    // from the algorithm rather than from the signature merely parsing.
    assert!(
        IdentityKey::verify_revocation_proof(&ed25519_algorithm_id(), &key_id, &wire, &proof,)
            .is_err(),
        "a stark-schnorr proof must not verify as Ed25519"
    );
}

/// A `HsmAdapter` for an algorithm whose key is not 32 bytes.
///
/// **This impl is the acceptance criterion.** While `HsmAdapter::get_public_key`
/// returned `[u8; 32]` and `sign` returned `[u8; 64]`, no adapter for a
/// 33-byte-key algorithm could be written at all — the trait's own docs promise
/// that adding an algorithm means adding an adapter, and the signatures made
/// that promise false. Widened by mission 0009-cd.
struct StarkSigner {
    secret: Felt,
    /// A fresh nonce per signature. A fixed nonce would make two signatures
    /// over different messages identical bytes, which this vector would then
    /// not be able to tell apart.
    nonce_counter: AtomicU64,
}

impl octo_wallet::hsm::HsmAdapter for StarkSigner {
    fn get_public_key(&self) -> Result<Vec<u8>, octo_wallet::hsm::HsmError> {
        Ok(public_key(&self.secret)
            .expect("derives a public key")
            .to_wire()
            .to_vec())
    }

    fn sign(&self, msg: &[u8]) -> Result<Vec<u8>, octo_wallet::hsm::HsmError> {
        let n = self.nonce_counter.fetch_add(1, Ordering::SeqCst);
        Ok(sign(&self.secret, msg, &Felt::from(n))
            .expect("sign")
            .to_wire()
            .to_vec())
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    fn algorithm_id(&self) -> octo_ident::AlgorithmId {
        stark_schnorr_algorithm_id()
    }
}

/// The vector that proves the trait widening was real: an identity whose signer
/// is not Ed25519, verifying its own signatures.
///
/// Every other vector in this file hands the verifier an explicit algorithm.
/// This one lets the identity carry its key and its algorithm together through
/// `HsmAdapter`, which is the path production uses — so it fails if the key
/// stored on `IdentityKey` is still a 32-byte copy rather than the adapter's
/// own bytes.
#[test]
fn an_identity_backed_by_a_non_ed25519_adapter_verifies_its_own_signatures() {
    octo_algo_ed25519::install().expect("install ed25519");
    octo_algo_stark_schnorr::install().expect("install stark-schnorr");

    let secret = Felt::from_hex_unchecked("0x2f1e3d4c5b6a79880716253443526170");
    let signer = Arc::new(StarkSigner {
        secret,
        nonce_counter: AtomicU64::new(1),
    });
    let mut key = IdentityKey::with_signer(signer).expect("with_signer");
    key.activate(1_000).expect("activate");

    assert_eq!(
        key.public_key_bytes().len(),
        33,
        "the identity must carry the adapter's 33-byte key whole. A 32 here \
         means IdentityKey is still making an independent fixed-size copy."
    );

    let sig = key.sign(b"hello").expect("sign");
    assert_eq!(sig.len(), 64);
    key.verify(b"hello", &sig)
        .expect("a stark-schnorr identity must verify its own signature");

    // The control, and the reason the first assertion is not enough: a verifier
    // that ignored the key entirely would also pass it.
    assert!(
        key.verify(b"goodbye", &sig).is_err(),
        "a signature over different bytes must not verify"
    );
    assert_ne!(
        sig,
        key.sign(b"hello").expect("sign again"),
        "two signatures over the same message must differ (nonce discipline)"
    );
}
