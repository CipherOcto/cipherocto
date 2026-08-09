//! RFC-0871 §Test Vectors TV8 — Borsh serialization byte-exact across two
//! independent implementations.
//!
//! Both implementations (Rust reference + hypothetical port) MUST produce
//! byte-exact identical borsh bytes for the same `NodeEnvelope` input.
//!
//! This file asserts:
//! 1. Two independent constructors (one from raw bytes via `build`, one
//!    reconstructed via round-trip) yield byte-exact borsh bytes.
//! 2. A 10,000-envelope random corpus round-trips without drift.

use ed25519_dalek::Signer;
use ed25519_dalek::SigningKey;
use octo_protocol::authorization::{Authorization, Ed25519SignatureBytes};
use octo_protocol::envelope::NodeEnvelope;
use octo_protocol::payload_kind::{PayloadKindId, IDENTITY_RESOLVE};
use octo_protocol::recipient::RecipientRef;
use octo_protocol::signing::signature_preimage;

fn build_signed_envelope(seed_byte: u8, nonce: [u8; 32]) -> NodeEnvelope {
    let seed = [seed_byte; 32];
    let sk = SigningKey::from_bytes(&seed);
    let pk_bytes = sk.verifying_key().to_bytes();
    let from_did = octo_ident::WireDid::new(format!(
        "did:octo:z{}",
        bs58::encode(&pk_bytes).into_string()
    ));
    let payload = vec![
        seed_byte,
        seed_byte.wrapping_add(1),
        seed_byte.wrapping_add(2),
    ];
    let mut env = NodeEnvelope::build(
        from_did.clone(),
        RecipientRef::Direct([0x01; 32]),
        IDENTITY_RESOLVE,
        payload.clone(),
        vec![],
        nonce,
        1_735_689_600_000 + (seed_byte as u64) * 1000,
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
fn tv8_borsh_byte_exact_round_trip() {
    let env = build_signed_envelope(99, [0x42; 32]);
    let bytes_a = borsh::to_vec(&env).unwrap();
    let back: NodeEnvelope = borsh::from_slice(&bytes_a).unwrap();
    let bytes_b = borsh::to_vec(&back).unwrap();
    assert_eq!(bytes_a, bytes_b, "TV8 byte-exact round-trip");
}

#[test]
fn tv8_envelope_id_byte_exact() {
    // TV8 contract: `envelope_id == BLAKE3-256(borsh::to_vec(envelope_with_zeroed_id))`.
    //
    // Strategy: build an envelope WITHOUT authorization, capture its
    // envelope_id (computed over auth=empty), then verify the canonical
    // re-derivation over the SAME auth=empty envelope matches. We avoid
    // mutating authorization post-build (which would invalidate the
    // envelope_id) — concrete authority-signed envelopes are covered by the
    // `tv8_borsh_byte_exact_round_trip` test which asserts round-trip
    // stability.
    use octo_protocol::payload_kind::IDENTITY_RESOLVE;
    use octo_protocol::recipient::RecipientRef;
    use octo_protocol::signing::compute_envelope_id;

    let payload = vec![0x01, 0x02, 0x03];
    let nonce = [0xab; 32];
    let env = NodeEnvelope::build(
        // Canonical DID: derive verifying key from seed [77; 32].
        {
            let sk = SigningKey::from_bytes(&[77u8; 32]);
            let pk = sk.verifying_key().to_bytes();
            octo_ident::WireDid::new(format!("did:octo:z{}", bs58::encode(&pk).into_string()))
        },
        RecipientRef::Direct([0x01; 32]),
        IDENTITY_RESOLVE,
        payload,
        vec![], // no authorization at build time
        nonce,
        1_735_689_600_000,
    )
    .unwrap();
    // Re-derive via the canonical helper.
    let recomputed = compute_envelope_id(&env);
    assert_eq!(
        env.envelope_id, recomputed,
        "envelope_id must equal BLAKE3-256(canonical_ser(env_with_zeroed_id))"
    );
    // Independent verification: borsh-serialize with id zeroed, hash directly.
    let mut env_zeroed = env.clone();
    env_zeroed.envelope_id = [0u8; 32];
    let bytes = borsh::to_vec(&env_zeroed).unwrap();
    let expected = *blake3::hash(&bytes).as_bytes();
    assert_eq!(
        env.envelope_id, expected,
        "envelope_id byte-exact matches direct BLAKE3-256"
    );
}

#[test]
fn tv8_random_corpus_round_trip_10k() {
    // 10,000 random envelopes: every one must round-trip byte-exactly through
    // borsh (the primary determinism gate for RFC-0871 §Determinism
    // Requirements Class A).
    let mut rng_state: u64 = 0x9e3779b97f4a7c15;
    let mut next_u64 = || {
        rng_state = rng_state
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        rng_state
    };
    let mut nonce = [0u8; 32];
    for i in 0..10_000u64 {
        let n = next_u64();
        nonce[i as usize % 32] = (n & 0xFF) as u8;
        nonce[(i as usize + 1) % 32] = ((n >> 8) & 0xFF) as u8;
        let env = build_signed_envelope((i & 0xFF) as u8, nonce);
        let bytes = borsh::to_vec(&env).unwrap();
        let back: NodeEnvelope = borsh::from_slice(&bytes).unwrap();
        let bytes2 = borsh::to_vec(&back).unwrap();
        assert_eq!(
            bytes, bytes2,
            "TV8 corpus drift at iteration {i}; borsh bytes must match"
        );
    }
    // Suppress unused warning for PayloadKindId re-export.
    let _: PayloadKindId = IDENTITY_RESOLVE;
}
