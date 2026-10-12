//! RFC-0871 §Test Vectors TV7 — Cross-domain envelope (identity resolve from
//! quota node context).
//!
//! Quota node receives a forward request; to authorize the asker, it
//! dispatches `WALLET_RESOLVE_DID` envelope to the wallet node over
//! NodeTransport. The envelope carries the asker's claimed DID as `payload`.
//!
//! Phase 1 shape: payload carries a DID wire string and is echoed back.
//!
//! **What this test does not do:** it does not reach the wallet node. This
//! target has no dependency on `octo-wallet-node`, so `ResolveDIDHandler` is
//! unreachable from here, and `test_dispatcher` echoes rather than routing.
//! It covers envelope authorization, signing and dispatch — not DID resolution.
//! Resolution is covered at the node boundary in `octo-wallet-node`.
//!
//! The doc above previously said this dispatches "to the wallet node over
//! NodeTransport". It does not, and no transport is involved. Recorded rather
//! than quietly reworded, because the overclaim is what let the resolve
//! handler's substitution defect go unexamined.

use ed25519_dalek::Signer;
use ed25519_dalek::SigningKey;
use octo_protocol::authorization::{Authorization, Ed25519SignatureBytes};
use octo_protocol::dispatch::{test_dispatcher, EnvelopeDispatcher, HandlerOutput};
use octo_protocol::envelope::{NodeEnvelope, VERSION_TAG_V2};
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
        VERSION_TAG_V2,
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
