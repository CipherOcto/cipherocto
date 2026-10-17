//! RFC-0871 §Test Vectors TV3 — Receiver rejects replayed envelope.

use ed25519_dalek::Signer;
use ed25519_dalek::SigningKey;
use octo_protocol::authorization::{Authorization, Ed25519SignatureBytes};
use octo_protocol::dispatch::{test_dispatcher, EnvelopeDispatcher};
use octo_protocol::envelope::{NodeEnvelope, VERSION_TAG_V2};
use octo_protocol::error::ProtocolError;
use octo_protocol::payload_kind::IDENTITY_RESOLVE;
use octo_protocol::recipient::RecipientRef;
use octo_protocol::signing::signature_preimage;

#[test]
fn tv3_rejects_replayed_envelope() {
    let seed = [9u8; 32];
    let sk = SigningKey::from_bytes(&seed);
    let pk_bytes = sk.verifying_key().to_bytes();
    let from_did = octo_ident::WireDid::new(format!(
        "did:octo:z{}",
        bs58::encode(&pk_bytes).into_string()
    ));
    let payload = vec![0xaa, 0xbb, 0xcc];
    let mut env = NodeEnvelope::build(
        from_did.clone(),
        RecipientRef::Direct([0x01; 32]),
        IDENTITY_RESOLVE,
        payload.clone(),
        vec![],
        [0xee; 32],
        1_735_689_600_000,
        VERSION_TAG_V2,
    )
    .unwrap();
    let preimage = signature_preimage(&env.envelope_id, env.from_did.as_str(), &payload);
    env.authorization = vec![Authorization::Signature {
        signer_did: from_did,
        sig: Ed25519SignatureBytes::from_signature(&sk.sign(&preimage)),
    }];

    let dispatcher = test_dispatcher(1_735_689_500_000);
    // First dispatch succeeds.
    dispatcher.dispatch(&env).expect("first dispatch");
    // Replay must reject with ReplayDetected carrying the original envelope_id.
    let err = dispatcher.dispatch(&env).unwrap_err();
    match err {
        ProtocolError::ReplayDetected(id) => {
            assert_eq!(id, env.envelope_id);
        }
        other => panic!("expected ReplayDetected; got {other:?}"),
    }
}
