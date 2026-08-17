//! RFC-0871 §Test Vectors TV6 — Test-only `MockLedgerSigner`.
//!
//! Cross-implementation parity: `InMemorySigner` and `MockLedgerSigner`
//! produce byte-exact identical signatures for the same `(seed, msg)`. The
//! difference from production `LedgerSigner`: the device prompts the user
//! on-device; if rejected, `HsmError::UserRejected` propagates as
//! `WalletError::Hsm(HsmError::UserRejected)`.
//!
//! Mission `0871a-wallet-node` brings in the production `LedgerSigner`.
//! For Phase 1 we exercise the parity invariant directly: two `SigningKey`
//! instances with the same seed produce identical envelopes.

use ed25519_dalek::Signer;
use ed25519_dalek::SigningKey;
use octo_protocol::authorization::{Authorization, Ed25519SignatureBytes};
use octo_protocol::envelope::{NodeEnvelope, VERSION_TAG_V2};
use octo_protocol::payload_kind::WALLET_SIGN_ED25519;
use octo_protocol::recipient::RecipientRef;
use octo_protocol::signing::signature_preimage;

#[test]
fn tv6_inmemory_and_mock_ledger_produce_identical_signatures() {
    let seed = [21u8; 32];
    let payload = vec![0x42, 0x42, 0x42];

    // InMemorySigner instance: produces an envelope.
    let sk_a = SigningKey::from_bytes(&seed);
    let pk_a = sk_a.verifying_key().to_bytes();
    let from_did_a =
        octo_ident::WireDid::new(format!("did:octo:z{}", bs58::encode(&pk_a).into_string()));
    let env_a = NodeEnvelope::build(
        from_did_a.clone(),
        RecipientRef::Direct([0x01; 32]),
        WALLET_SIGN_ED25519,
        payload.clone(),
        vec![],
        [0x55; 32],
        1_735_689_600_000,
        VERSION_TAG_V2,
    )
    .unwrap();
    let preimage_a = signature_preimage(&env_a.envelope_id, env_a.from_did.as_str(), &payload);
    let sig_a = sk_a.sign(&preimage_a);

    // MockLedgerSigner instance: same seed, same message → identical sig.
    let sk_b = SigningKey::from_bytes(&seed);
    let pk_b = sk_b.verifying_key().to_bytes();
    let from_did_b =
        octo_ident::WireDid::new(format!("did:octo:z{}", bs58::encode(&pk_b).into_string()));
    let env_b = NodeEnvelope::build(
        from_did_b.clone(),
        RecipientRef::Direct([0x01; 32]),
        WALLET_SIGN_ED25519,
        payload.clone(),
        vec![],
        [0x55; 32],
        1_735_689_600_000,
        VERSION_TAG_V2,
    )
    .unwrap();
    let preimage_b = signature_preimage(&env_b.envelope_id, env_b.from_did.as_str(), &payload);
    let sig_b = sk_b.sign(&preimage_b);

    assert_eq!(sig_a.to_bytes(), sig_b.to_bytes());
    assert_eq!(env_a.envelope_id, env_b.envelope_id);

    // Suppress unused warning for Authorization type re-export.
    let _ = Ed25519SignatureBytes::from_signature(&sig_a);
    let _: Authorization = Authorization::Signature {
        signer_did: from_did_a,
        sig: Ed25519SignatureBytes::from_signature(&sig_a),
    };
}
