//! RFC-0871 §Test Vectors TV4 — `Vec<Authorization>` with capability + signature.
//!
//! Per RFC-0871 §Adversary Analysis A6: ALL authorizations in `Vec<Authorization>`
//! MUST verify (logical AND). TV4 asserts the mixed-vec shape accepts when
//! both authorizations verify, and rejects when either fails.
//!
//! Note: `Authorization::Capability` HMAC verification lives in
//! `crates/octo-cap-macaroon/` (mission `0957-ext-macaroon-crate`). For TV4
//! the crate ships the capability-token wrapper; the concrete attenuation
//! invariant check is a Phase 4 addition. TV4 here exercises the
//! `verify_all` flow with a Capability + Signature pair.

use ed25519_dalek::Signer;
use ed25519_dalek::SigningKey;
use octo_protocol::authorization::{Authorization, CapabilityToken, Ed25519SignatureBytes};
use octo_protocol::dispatch::{test_dispatcher, EnvelopeDispatcher};
use octo_protocol::envelope::NodeEnvelope;
use octo_protocol::error::ProtocolError;
use octo_protocol::payload_kind::IDENTITY_RESOLVE;
use octo_protocol::recipient::RecipientRef;
use octo_protocol::signing::signature_preimage;

#[test]
fn tv4_accepts_capability_plus_signature() {
    let seed = [11u8; 32];
    let sk = SigningKey::from_bytes(&seed);
    let pk_bytes = sk.verifying_key().to_bytes();
    let from_did = octo_ident::WireDid::new(format!(
        "did:octo:z{}",
        bs58::encode(&pk_bytes).into_string()
    ));
    let payload = vec![0x10, 0x20, 0x30];
    let mut env = NodeEnvelope::build(
        from_did.clone(),
        RecipientRef::Direct([0x01; 32]),
        IDENTITY_RESOLVE,
        payload.clone(),
        vec![
            // Per RFC-0957 §3: capability carries PaymentCaveat + ValidAfter.
            // Phase 1 placeholder bytes; the concrete attenuation check lands
            // in mission 0957-ext-macaroon-crate.
            Authorization::Capability(CapabilityToken::from_bytes(vec![
                0xc0, 0xff, 0xee, 0xca, 0xfe,
            ])),
        ],
        [0x33; 32],
        1_735_689_600_000,
    )
    .unwrap();
    let preimage = signature_preimage(&env.envelope_id, env.from_did.as_str(), &payload);
    env.authorization.push(Authorization::Signature {
        signer_did: from_did,
        sig: Ed25519SignatureBytes::from_signature(&sk.sign(&preimage)),
    });
    assert_eq!(env.authorization.len(), 2);

    let dispatcher = test_dispatcher(1_735_689_500_000);
    dispatcher.dispatch(&env).expect("TV4 mixed-auth dispatch");
}

#[test]
fn tv4_rejects_when_signature_fails() {
    let seed = [13u8; 32];
    let sk = SigningKey::from_bytes(&seed);
    let pk_bytes = sk.verifying_key().to_bytes();
    let from_did = octo_ident::WireDid::new(format!(
        "did:octo:z{}",
        bs58::encode(&pk_bytes).into_string()
    ));
    let payload = vec![0x10, 0x20, 0x30];
    let mut env = NodeEnvelope::build(
        from_did.clone(),
        RecipientRef::Direct([0x01; 32]),
        IDENTITY_RESOLVE,
        payload.clone(),
        vec![Authorization::Capability(CapabilityToken::from_bytes(
            vec![0xc0, 0xff, 0xee],
        ))],
        [0x44; 32],
        1_735_689_600_000,
    )
    .unwrap();
    // Sign with a DIFFERENT key (seed=99) → signature verification fails;
    // the capability is still present (would pass Phase 4 verification),
    // but logical-AND rejects the envelope.
    let wrong_seed = [99u8; 32];
    let wrong_sk = SigningKey::from_bytes(&wrong_seed);
    let preimage = signature_preimage(&env.envelope_id, env.from_did.as_str(), &payload);
    env.authorization.push(Authorization::Signature {
        signer_did: from_did,
        sig: Ed25519SignatureBytes::from_signature(&wrong_sk.sign(&preimage)),
    });
    let dispatcher = test_dispatcher(1_735_689_500_000);
    let err = dispatcher.dispatch(&env).unwrap_err();
    assert!(matches!(err, ProtocolError::AuthorizationFailed(_)));
}
