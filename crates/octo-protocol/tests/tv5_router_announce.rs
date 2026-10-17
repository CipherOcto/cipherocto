//! RFC-0871 §Test Vectors TV5 — Wallet node announces payload kinds via
//! `RouterAnnouncePayload`.
//!
//! Per RFC-0871 §Wallet Node Lifecycle: `WalletNode::start()` registers
//! itself + emits `RouterAnnouncePayload` so mesh peers cache the
//! `(node_id, capabilities, payload_kinds)` triple. The wallet-specific kinds
//! are advertised via the existing `RouterAnnouncePayload` shape (no
//! `AnnouncedCapability` struct introduced); peers then route
//! `RecipientRef::Domain(wallet_did)` envelopes to wallet nodes.
//!
//! Phase 1 ships the payload-kind registry: the dispatcher records which
//! payload kinds it serves via `DispatcherConfig::served_kinds`. Mission
//! `0871a-wallet-node` will register the wallet payload kinds at startup.

use octo_protocol::authorization::{Authorization, Ed25519SignatureBytes};
use octo_protocol::dispatch::{
    DispatcherConfig, EnvelopeDispatcher, ReferenceDispatcher, ValidationCache,
};
use octo_protocol::envelope::{NodeEnvelope, VERSION_TAG_V2};
use octo_protocol::error::ProtocolError;
use octo_protocol::payload_kind::{IDENTITY_RESOLVE, WALLET_MINT_CAPABILITY, WALLET_SIGN_ED25519};
use octo_protocol::recipient::RecipientRef;
use octo_protocol::signing::signature_preimage;
use octo_protocol::time::MockClock;

use ed25519_dalek::Signer;
use ed25519_dalek::SigningKey;

fn signed_envelope(
    seed: u8,
    payload_kind: octo_protocol::payload_kind::PayloadKindId,
) -> NodeEnvelope {
    let seed_bytes = [seed; 32];
    let sk = SigningKey::from_bytes(&seed_bytes);
    let pk_bytes = sk.verifying_key().to_bytes();
    let from_did = octo_ident::WireDid::new(format!(
        "did:octo:z{}",
        bs58::encode(&pk_bytes).into_string()
    ));
    let payload = vec![0x01];
    let mut env = NodeEnvelope::build(
        from_did.clone(),
        RecipientRef::Direct([0x01; 32]),
        payload_kind,
        payload.clone(),
        vec![],
        [0x77; 32],
        1_735_689_600_000,
        VERSION_TAG_V2,
    )
    .unwrap();
    let preimage = signature_preimage(&env.envelope_id, env.from_did.as_str(), &payload);
    env.authorization = vec![Authorization::Signature {
        signer_did: from_did,
        sig: Ed25519SignatureBytes::from_signature(&sk.sign(&preimage)),
    }];
    env
}

#[test]
fn tv5_wallet_node_advertises_served_kinds() {
    // A wallet-node dispatcher advertises its wallet-specific payload kinds.
    let wallet_dispatcher = ReferenceDispatcher::new(
        ValidationCache::new(),
        Box::new(MockClock::new(1_735_689_500_000)),
        DispatcherConfig {
            max_ttl_secs: 3600,
            served_kinds: vec![WALLET_SIGN_ED25519, WALLET_MINT_CAPABILITY],
        },
    );
    // Identity-resolve is NOT in the wallet dispatcher's served_kinds; it
    // must reject per RFC-0871 §Adversary Analysis A5 (unknown kind handler).
    let env_identity = signed_envelope(15, IDENTITY_RESOLVE);
    let err = wallet_dispatcher.dispatch(&env_identity).unwrap_err();
    assert!(matches!(err, ProtocolError::UnknownPayloadKind(_)));
    // A wallet-signed payload IS served → accept.
    let env_wallet = signed_envelope(15, WALLET_SIGN_ED25519);
    wallet_dispatcher
        .dispatch(&env_wallet)
        .expect("wallet kind served");
}
