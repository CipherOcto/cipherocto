//! RFC-0871 §Test Vectors TV7 — Cross-domain envelope (identity resolve from
//! quota node context).
//!
//! Quota node receives a forward request; to authorize the asker, it
//! dispatches `WALLET_RESOLVE_DID` envelope to the wallet node over
//! NodeTransport. The envelope carries the asker's claimed DID as `payload`.
//!
//! Phase 1 shape: payload carries a DID wire string; the wallet dispatcher
//! validates the DID via `octo_ident::CanonicalCodec::parse` and returns a
//! `HandlerOutput` whose payload is the DID wire string echoed back (the
//! concrete `DidResolutionResult { did, holder_pub, audience_did }` lands in
//! mission `0871a-wallet-node` + RFC-0957-A1).

use ed25519_dalek::Signer;
use ed25519_dalek::SigningKey;
use octo_protocol::authorization::{Authorization, Ed25519SignatureBytes};
use octo_protocol::dispatch::{test_dispatcher, EnvelopeDispatcher, HandlerOutput};
use octo_protocol::envelope::NodeEnvelope;
use octo_protocol::payload_kind::WALLET_RESOLVE_DID;
use octo_protocol::recipient::RecipientRef;
use octo_protocol::signing::signature_preimage;

#[test]
fn tv7_cross_domain_envelope_echoes_payload() {
    // Sender DID (the quota node).
    let sender_seed = [33u8; 32];
    let sender_sk = SigningKey::from_bytes(&sender_seed);
    let sender_pk = sender_sk.verifying_key().to_bytes();
    let sender_did = octo_ident::WireDid::new(format!(
        "did:octo:z{}",
        bs58::encode(&sender_pk).into_string()
    ));

    // The payload is the asker's DID (canonical wire form) to be resolved.
    let asker_seed = [77u8; 32];
    let asker_sk = SigningKey::from_bytes(&asker_seed);
    let asker_pk = asker_sk.verifying_key().to_bytes();
    let asker_did = octo_ident::WireDid::new(format!(
        "did:octo:z{}",
        bs58::encode(&asker_pk).into_string()
    ));
    let payload = asker_did.as_str().as_bytes().to_vec();

    let mut env = NodeEnvelope::build(
        sender_did.clone(),
        RecipientRef::Domain(asker_did.clone()),
        WALLET_RESOLVE_DID,
        payload.clone(),
        vec![],
        [0x99; 32],
        1_735_689_600_000,
    )
    .unwrap();
    let preimage = signature_preimage(&env.envelope_id, env.from_did.as_str(), &payload);
    env.authorization = vec![Authorization::Signature {
        signer_did: sender_did,
        sig: Ed25519SignatureBytes::from_signature(&sender_sk.sign(&preimage)),
    }];

    let dispatcher = test_dispatcher(1_735_689_500_000);
    let HandlerOutput { payload: out } = dispatcher
        .dispatch(&env)
        .expect("TV7 cross-domain dispatch");
    assert_eq!(out, asker_did.as_str().as_bytes());
}
