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
use octo_ident::VerificationMethodId;
use octo_wallet::{IdentityKey, WalletError};

const KEY_ID: &str = "key-a";

fn revoked() -> (IdentityKey, [u8; 64]) {
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
        &key.public_key_bytes(),
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
        &key.public_key_bytes(),
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
        &key.public_key_bytes(),
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
        &pubkey,
        &legacy,
    );
    assert!(
        matches!(outcome, Err(WalletError::InvalidRevocationProof)),
        "an unprefixed proof must be refused, not stripped, got {outcome:?}"
    );
}
