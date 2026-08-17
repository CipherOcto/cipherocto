//! RFC-0871 §Test Vectors TV2 — Receiver rejects expired envelope.

use octo_protocol::authorization::{Authorization, Ed25519SignatureBytes};
use octo_protocol::dispatch::{
    DispatcherConfig, EnvelopeDispatcher, ReferenceDispatcher, ValidationCache,
};
use octo_protocol::envelope::{NodeEnvelope, VERSION_TAG_V2};
use octo_protocol::error::ProtocolError;
use octo_protocol::payload_kind::IDENTITY_RESOLVE;
use octo_protocol::recipient::RecipientRef;
use octo_protocol::signing::signature_preimage;
use octo_protocol::time::MockClock;

use ed25519_dalek::Signer;
use ed25519_dalek::SigningKey;

#[test]
fn tv2_rejects_expired_envelope() {
    // RFC-0871 §TV2: expires_at_unix_ms = 1_000_000 (1970-01-01 + ~16 min);
    // MockClock { now_unix_ms: 1_735_689_600_000 } → must reject.
    let seed = [7u8; 32];
    let sk = SigningKey::from_bytes(&seed);
    let pk_bytes = sk.verifying_key().to_bytes();
    let from_did = octo_ident::WireDid::new(format!(
        "did:octo:z{}",
        bs58::encode(&pk_bytes).into_string()
    ));
    let payload = vec![0x01, 0x02, 0x03];
    let mut env = NodeEnvelope::build(
        from_did.clone(),
        RecipientRef::Direct([0x01; 32]),
        IDENTITY_RESOLVE,
        payload.clone(),
        vec![],
        [0xff; 32],
        1_000_000,
        VERSION_TAG_V2,
    )
    .unwrap();
    let preimage = signature_preimage(&env.envelope_id, env.from_did.as_str(), &payload);
    env.authorization = vec![Authorization::Signature {
        signer_did: from_did,
        sig: Ed25519SignatureBytes::from_signature(&sk.sign(&preimage)),
    }];

    let dispatcher = ReferenceDispatcher::new(
        ValidationCache::new(),
        Box::new(MockClock::new(1_735_689_600_000)),
        DispatcherConfig::permissive(),
    );

    let err = dispatcher.dispatch(&env).unwrap_err();
    match err {
        ProtocolError::Expired {
            now_unix_ms,
            expires_at_unix_ms,
        } => {
            assert_eq!(now_unix_ms, 1_735_689_600_000);
            assert_eq!(expires_at_unix_ms, 1_000_000);
        }
        other => panic!("expected Expired; got {other:?}"),
    }
}
