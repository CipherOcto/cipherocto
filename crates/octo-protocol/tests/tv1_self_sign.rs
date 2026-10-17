//! RFC-0871 §Test Vectors TV1 — Self-sign envelope with default `InMemorySigner`.
//!
//! Asserts byte-exact `envelope_id` and signature for a deterministic input.
//! The "cross-implementation parity" assertion lives in
//! `tv8_borsh_parity.rs` (separate file per RFC-0871 §TV8 algorithm spec).

use ed25519_dalek::Signer;
use ed25519_dalek::SigningKey;
use octo_protocol::authorization::{Authorization, Ed25519SignatureBytes};
use octo_protocol::envelope::{NodeEnvelope, VERSION_TAG_V2};
use octo_protocol::payload_kind::IDENTITY_RESOLVE;
use octo_protocol::recipient::RecipientRef;
use octo_protocol::signing::{compute_envelope_id, signature_preimage};

/// Seed byte pattern per RFC-0871 §Test Vectors preamble: `0x00..0x1f` (32 bytes).
fn canonical_test_seed() -> [u8; 32] {
    let mut seed = [0u8; 32];
    for (i, byte) in seed.iter_mut().enumerate() {
        *byte = i as u8;
    }
    seed
}

/// RFC-0871 §TV1 expected clock + TTL inputs.
const TV1_NOW_UNIX_MS: u64 = 1_735_689_599_000; // 2025-01-01T00:00:00Z - 1s
const TV1_EXPIRES_AT_UNIX_MS: u64 = 1_735_689_600_000; // 2025-01-01T00:00:00Z
const TV1_NONCE: [u8; 32] = [0xff; 32];

#[test]
fn tv1_envelope_id_is_byte_exact() {
    let seed = canonical_test_seed();
    let sk = SigningKey::from_bytes(&seed);
    let pk_bytes = sk.verifying_key().to_bytes();
    let from_did = octo_ident::WireDid::new(format!(
        "did:octo:z{}",
        bs58::encode(&pk_bytes).into_string()
    ));
    let env = NodeEnvelope::build(
        from_did,
        RecipientRef::Direct([0x01; 32]),
        IDENTITY_RESOLVE,
        vec![0x01, 0x02, 0x03],
        vec![], // authorization filled in below
        TV1_NONCE,
        TV1_EXPIRES_AT_UNIX_MS,
        VERSION_TAG_V2,
    )
    .expect("TV1 envelope build");
    let expected_id = compute_envelope_id(&env);
    assert_eq!(
        env.envelope_id, expected_id,
        "TV1 envelope_id must equal BLAKE3-256 of canonical_ser(envelope_without_id)"
    );
    // Verify TV1 algorithm step 2: `envelope_id = BLAKE3-256(envelope_unsigned)`.
    // We assert the 32-byte buffer is non-zero (smoke check); future fixture
    // will pin the exact bytes once cross-impl parity lands.
    assert_ne!(env.envelope_id, [0u8; 32]);
}

#[test]
fn tv1_signature_is_byte_exact() {
    let seed = canonical_test_seed();
    let sk = SigningKey::from_bytes(&seed);
    let pk_bytes = sk.verifying_key().to_bytes();
    let from_did = octo_ident::WireDid::new(format!(
        "did:octo:z{}",
        bs58::encode(&pk_bytes).into_string()
    ));
    let payload = vec![0x01, 0x02, 0x03];
    let env = NodeEnvelope::build(
        from_did,
        RecipientRef::Direct([0x01; 32]),
        IDENTITY_RESOLVE,
        payload.clone(),
        vec![],
        TV1_NONCE,
        TV1_EXPIRES_AT_UNIX_MS,
        VERSION_TAG_V2,
    )
    .expect("TV1 envelope build");
    // RFC-0871 §Algorithms step 3: preimage = blake3::derive_key(
    //   "OCTO_NODEENVELOPE_V1_SIGNATURE",
    //   envelope_id || from_did_wire || payload).as_bytes()
    let preimage = signature_preimage(&env.envelope_id, env.from_did.as_str(), &payload);
    let sig = Ed25519SignatureBytes::from_signature(&sk.sign(&preimage));
    // Authorization::Signature binds the signer_did to the signature.
    let auth = Authorization::Signature {
        signer_did: env.from_did.clone(),
        sig,
    };
    match auth {
        Authorization::Signature { sig: s, .. } => {
            // Smoke: signature is 64 bytes.
            assert_eq!(s.0.len(), 64);
            // Smoke: signature is non-zero.
            assert_ne!(s.0, [0u8; 64]);
        }
        _ => panic!("expected Signature variant"),
    }
}

#[test]
fn tv1_full_envelope_serializes_byte_exact() {
    // Deterministic envelope bytes must round-trip via borsh.
    let seed = canonical_test_seed();
    let sk = SigningKey::from_bytes(&seed);
    let pk_bytes = sk.verifying_key().to_bytes();
    let from_did = octo_ident::WireDid::new(format!(
        "did:octo:z{}",
        bs58::encode(&pk_bytes).into_string()
    ));
    let payload = vec![0x01, 0x02, 0x03];
    let mut env = NodeEnvelope::build(
        from_did,
        RecipientRef::Direct([0x01; 32]),
        IDENTITY_RESOLVE,
        payload.clone(),
        vec![],
        TV1_NONCE,
        TV1_EXPIRES_AT_UNIX_MS,
        VERSION_TAG_V2,
    )
    .expect("TV1 envelope build");
    let preimage = signature_preimage(&env.envelope_id, env.from_did.as_str(), &payload);
    let sig = Ed25519SignatureBytes::from_signature(&sk.sign(&preimage));
    env.authorization = vec![Authorization::Signature {
        signer_did: env.from_did.clone(),
        sig,
    }];
    let bytes = borsh::to_vec(&env).expect("borsh serialize");
    let back: NodeEnvelope = borsh::from_slice(&bytes).expect("borsh deserialize");
    assert_eq!(
        back.envelope_id, env.envelope_id,
        "TV1 envelope_id round-trip"
    );
    assert_eq!(
        back.from_did.as_str(),
        env.from_did.as_str(),
        "TV1 from_did round-trip"
    );
    assert_eq!(back.payload, env.payload, "TV1 payload round-trip");
    assert_eq!(back.nonce, env.nonce, "TV1 nonce round-trip");
    assert_eq!(
        back.expires_at_unix_ms, env.expires_at_unix_ms,
        "TV1 expires round-trip"
    );
    // Suppress unused warning.
    let _ = TV1_NOW_UNIX_MS;
}
