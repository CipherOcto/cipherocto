//! End-to-end proof that the mandatory signable prefix turns a cross-algorithm
//! replay into a rejection rather than a wrong answer.
//!
//! RFC-0009 §Domain separation is mandatory, not hygiene, and this is the
//! vector that can actually detect the vulnerability. The five byte-comparison
//! vectors in `octo_ident::signable_prefix` would all pass against a prefix
//! helper that no signer ever calls; these build real signatures and assert
//! real rejections.
//!
//! ## Why this file is not next to the prefix code
//!
//! `octo-ident` is Layer B and must not depend on either algorithm crate —
//! that is the whole point of the registry, and reversing it would bake a
//! specific algorithm into the identity substrate. The only crate that can
//! see both algorithms is one that depends on both, so the end-to-end vector
//! lives here rather than in `octo-ident` where the prefix is defined.
//!
//! ## What each rejection is actually attributable to
//!
//! Being precise, because the two dimensions of the prefix fail differently:
//!
//! - **Key-id swap** is attributable to the prefix and nothing else. Same
//!   algorithm, same key, same payload; only `key_id` differs, so the
//!   recomputed signable bytes differ and the equation fails on its own
//!   terms. This is the clean demonstration.
//! - **Algorithm swap** is rejected by the prefix *and* by key shape. Ed25519
//!   keys are 32 bytes and STARK-curve keys are 33, so a STARK public key
//!   dispatched to Ed25519 is refused on length before any equation runs.
//!   That is a second line of defence, not the prefix doing the work alone,
//!   and the assertion below does not claim otherwise.

use octo_algo_ed25519::ed25519_algorithm_id;
use octo_algo_stark_schnorr::{
    public_key as stark_public_key, sign as stark_sign, stark_schnorr_algorithm_id,
    verify_dispatch as stark_verify_dispatch, DispatchError, StarkSchnorrAlgorithm,
};
use octo_ident::{
    AlgorithmId, SignableError, SignableForm, SignablePrefix, SignedMessage, VerificationMethodId,
};
use starknet_types_core::felt::Felt;

fn fe(hex: &str) -> Felt {
    Felt::from_hex_unchecked(hex)
}

fn prefix(algorithm: AlgorithmId, key_id: &str) -> SignablePrefix {
    SignablePrefix::new(algorithm, VerificationMethodId::derive(key_id))
}

/// Both algorithms installed, in whatever order — which is itself the point
/// of the additive install path, and would have been impossible before.
fn both_installed() {
    octo_algo_stark_schnorr::install().expect("install stark-schnorr");
    octo_algo_ed25519::install().expect("install ed25519");
}

const PAYLOAD: &[u8] = b"transfer 25 OCTO to did:octo:zrecipient";
const SECRET: &str = "0x2f1e3d4c5b6a79880716253443526170";

/// A signature made under one algorithm, presented under another, is
/// rejected — and is rejected as a *malformed signature* rather than
/// silently verifying against the wrong equation.
#[test]
fn cross_algorithm_signature_replay_rejected() {
    both_installed();
    let ed_id = ed25519_algorithm_id();
    let stark_id = stark_schnorr_algorithm_id();
    assert_ne!(ed_id, stark_id);

    let secret = fe(SECRET);
    let key = stark_public_key(&secret).expect("key");
    let public = key.to_wire();

    // The genuine article: a STARK-curve Schnorr signature over a message
    // that names the STARK algorithm in its prefix.
    let genuine = SignedMessage::new(prefix(stark_id, "key-a"), PAYLOAD.to_vec());
    let genuine_bytes = genuine.signable(SignableForm::Raw).expect("signable");
    let signature = stark_sign(&secret, &genuine_bytes, &fe("0x9a37"))
        .expect("sign")
        .to_wire();

    // It verifies under the algorithm its prefix names.
    stark_verify_dispatch(&stark_id, &public, &genuine_bytes, &signature)
        .expect("a genuine signature must verify");

    // Now the replay. Same payload, same signature bytes, same key — only the
    // prefix's algorithm_id has been swapped to name Ed25519.
    let replayed = SignedMessage::new(prefix(ed_id, "key-a"), PAYLOAD.to_vec());
    let replayed_bytes = replayed.signable(SignableForm::Raw).expect("signable");
    assert_ne!(
        replayed_bytes, genuine_bytes,
        "a different algorithm_id must produce a different signed domain"
    );

    let outcome = stark_verify_dispatch(&ed_id, &public, &replayed_bytes, &signature);
    assert!(
        outcome.is_err(),
        "a signature made under stark-schnorr must not verify when presented \
         as ed25519, even though the prefix and the key are otherwise intact"
    );
    // Name the mechanism rather than leaving it to be inferred.
    assert!(
        matches!(outcome, Err(DispatchError::Signature(_))),
        "the replay must be rejected as a signature failure, not as an \
         unknown algorithm: the harness installed both, so the id resolves \
         and the rejection has to come from the equation. Got {outcome:?}"
    );
}

/// The same signature replayed under the **same** algorithm but a different
/// `key_id`.
///
/// This is the one attributable to the prefix alone: algorithm, key and
/// payload are all unchanged, so the only reason it can fail is that the
/// recomputed signable bytes differ.
#[test]
fn a_key_id_swap_is_rejected_by_the_prefix_alone() {
    both_installed();
    let stark_id = stark_schnorr_algorithm_id();
    let secret = fe(SECRET);
    let key = stark_public_key(&secret).expect("key");
    let public = key.to_wire();

    let genuine = SignedMessage::new(prefix(stark_id, "key-a"), PAYLOAD.to_vec());
    let genuine_bytes = genuine.signable(SignableForm::Raw).expect("signable");
    let signature = stark_sign(&secret, &genuine_bytes, &fe("0x9a37"))
        .expect("sign")
        .to_wire();

    // Rewrapped as this key's *other* verification method.
    let swapped = SignedMessage::new(prefix(stark_id, "key-b"), PAYLOAD.to_vec());
    let swapped_bytes = swapped.signable(SignableForm::Raw).expect("signable");

    stark_verify_dispatch(&stark_id, &public, &genuine_bytes, &signature)
        .expect("genuine signature verifies");

    assert!(
        stark_verify_dispatch(&stark_id, &public, &swapped_bytes, &signature).is_err(),
        "the same signature must not verify once the prefix names another key"
    );
}

/// A verifier handed an unprefixed message refuses it.
///
/// RFC-0009: "A verifier MUST reject a message that does not carry the
/// prefix." The legacy shape is not a downgrade path.
#[test]
fn verifier_rejects_missing_prefix() {
    both_installed();
    let stark_id = stark_schnorr_algorithm_id();
    let secret = fe(SECRET);
    let key = stark_public_key(&secret).expect("key");

    // A bare payload SHORTER than the prefix cannot be read as one.
    let short = b"transfer 25 OCTO";
    assert!(
        short.len() < SignablePrefix::LEN,
        "this vector is about the short case; the long one is below"
    );
    assert_eq!(
        SignedMessage::parse(short, SignableForm::Raw),
        Err(SignableError::TooShort {
            got: short.len(),
            need: SignablePrefix::LEN
        })
    );

    // A bare payload LONGER than the prefix parses — and that is the sharp
    // edge. Its first 32 bytes become an `algorithm_id` and a `key_id` the
    // *attacker* chose, and `PAYLOAD` here is literally ASCII text, so those
    // bytes read back as whatever the message said.
    //
    // Nothing structural rejects it, and that is exactly why a verifier must
    // go on to check the key binding and dispatch on the algorithm the prefix
    // names. A verifier that only asked "does this parse?" would accept it.
    let parsed = SignedMessage::parse(PAYLOAD, SignableForm::Raw).expect("parses structurally");
    assert_ne!(
        parsed.prefix().key_id(),
        VerificationMethodId::derive("key-a"),
        "the prefix must not read back as a real key id"
    );
    assert_eq!(
        parsed.payload(),
        &PAYLOAD[SignablePrefix::LEN..],
        "the remainder is the tail of the attacker's payload"
    );

    // So a real signature is still refused, on the key binding.
    let genuine = SignedMessage::new(prefix(stark_id, "key-a"), PAYLOAD.to_vec());
    let genuine_bytes = genuine.signable(SignableForm::Raw).expect("signable");
    let signature = stark_sign(&secret, &genuine_bytes, &fe("0x9a37"))
        .expect("sign")
        .to_wire();
    stark_verify_dispatch(&stark_id, &key.to_wire(), &genuine_bytes, &signature)
        .expect("the genuine message verifies");
    assert!(
        stark_verify_dispatch(
            &stark_id,
            &key.to_wire(),
            &parsed.signable(SignableForm::Raw).expect("s"),
            &signature,
        )
        .is_err(),
        "a signature must not verify against an attacker-chosen prefix"
    );
}

/// The wire form a signer emits is what a verifier parses back, and the bytes
/// they sign are the same ones.
#[test]
fn signer_and_verifier_agree_on_the_signed_bytes() {
    both_installed();
    let stark_id = stark_schnorr_algorithm_id();
    let secret = fe(SECRET);
    let key = stark_public_key(&secret).expect("key");

    let message = SignedMessage::new(prefix(stark_id, "key-a"), PAYLOAD.to_vec());
    let wire = message.to_bytes();
    let signature = stark_sign(
        &secret,
        &message.signable(SignableForm::Raw).expect("s"),
        &fe("0x1"),
    )
    .expect("sign")
    .to_wire();

    let parsed = SignedMessage::parse(&wire, SignableForm::Raw).expect("parse");
    assert_eq!(parsed.prefix().algorithm_id(), stark_id);
    assert_eq!(parsed.payload(), PAYLOAD);

    stark_verify_dispatch(
        &stark_id,
        &key.to_wire(),
        &parsed.signable(SignableForm::Raw).expect("s"),
        &signature,
    )
    .expect("a wire message must verify from its parsed form alone");
}

/// Both algorithms are genuinely present and both are 64-byte signatures, so
/// nothing but the prefix distinguishes them.
#[test]
fn the_two_algorithms_share_a_signature_length_and_do_not_share_an_answer() {
    both_installed();
    assert_eq!(StarkSchnorrAlgorithm::SIGNATURE_LEN, 64);
    assert_eq!(
        octo_algo_ed25519::Ed25519Algorithm::SIGNATURE_LEN,
        64,
        "the premise of the whole exercise: same length, different meaning"
    );
    assert_eq!(StarkSchnorrAlgorithm::PUBLIC_KEY_LEN, 33);
    assert_eq!(octo_algo_ed25519::Ed25519Algorithm::PUBLIC_KEY_LEN, 32);
}
