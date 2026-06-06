//! Integration tests for Overlay Cryptography (OCrypt).
//!
//! Tests the full crypto lifecycle: identity derivation → session key
//! establishment → encrypt/decrypt → mission key hierarchy → HKDF-BLAKE3.

use octo_network::ocrypt::identity::{OverlayIdentity, PlatformBinding};
use octo_network::ocrypt::mission::MissionKeyHierarchy;
use octo_network::ocrypt::session::{
    decrypt, derive_consensus_nonce, derive_envelope_key, derive_nonce, derive_session_key,
    encrypt, KEY_SIZE, NONCE_SIZE,
};
use octo_network::ocrypt::suite::DEFAULT_SUITE;

// ── HKDF-BLAKE3 ──

#[test]
fn test_hkdf_blake3_deterministic() {
    let mut out1 = [0u8; 32];
    let mut out2 = [0u8; 32];
    octo_network::ocrypt::hkdf_blake3(b"salt", b"ikm", b"info", &mut out1);
    octo_network::ocrypt::hkdf_blake3(b"salt", b"ikm", b"info", &mut out2);
    assert_eq!(out1, out2);
}

#[test]
fn test_hkdf_blake3_different_inputs_different_output() {
    let mut out1 = [0u8; 32];
    let mut out2 = [0u8; 32];
    octo_network::ocrypt::hkdf_blake3(b"salt1", b"ikm", b"info", &mut out1);
    octo_network::ocrypt::hkdf_blake3(b"salt2", b"ikm", b"info", &mut out2);
    assert_ne!(out1, out2);
}

#[test]
fn test_hkdf_blake3_variable_length() {
    let mut out_16 = [0u8; 16];
    let mut out_64 = [0u8; 64];
    octo_network::ocrypt::hkdf_blake3(b"s", b"i", b"n", &mut out_16);
    octo_network::ocrypt::hkdf_blake3(b"s", b"i", b"n", &mut out_64);
    // First 16 bytes should match
    assert_eq!(&out_16[..], &out_64[..16]);
}

// ── Overlay Identity ──

#[test]
fn test_identity_derivation_deterministic() {
    let pk = [0x42; 32];
    let id1 = OverlayIdentity::derive_peer_id(&pk, 0);
    let id2 = OverlayIdentity::derive_peer_id(&pk, 0);
    assert_eq!(id1, id2);
}

#[test]
fn test_identity_different_keys_different_id() {
    let id1 = OverlayIdentity::derive_peer_id(&[0x42; 32], 0);
    let id2 = OverlayIdentity::derive_peer_id(&[0x43; 32], 0);
    assert_ne!(id1, id2);
}

#[test]
fn test_identity_different_epoch_different_id() {
    let pk = [0x42; 32];
    let id1 = OverlayIdentity::derive_peer_id(&pk, 0);
    let id2 = OverlayIdentity::derive_peer_id(&pk, 100);
    assert_ne!(id1, id2);
}

#[test]
fn test_identity_builder_and_signing() {
    let identity = OverlayIdentity::new([0x42; 32], 100).with_capabilities_root([0xAA; 32]);

    assert_eq!(
        identity.peer_id,
        OverlayIdentity::derive_peer_id(&[0x42; 32], 100)
    );
    assert_eq!(identity.capabilities_root, [0xAA; 32]);

    let signing_bytes = identity.to_signing_bytes();
    assert!(!signing_bytes.is_empty());
    assert_eq!(signing_bytes.len(), 32 + 32 + 8 + 32);
}

// ── Platform Binding ──

#[test]
fn test_platform_binding_signing() {
    let binding = PlatformBinding::new(0x0001, [0xAA; 32]);
    let bytes = binding.to_signing_bytes();
    assert_eq!(bytes.len(), 2 + 32);
}

// ── Session Key Establishment ──

#[test]
fn test_session_key_derivation_deterministic() {
    let shared_secret = vec![0xAB; 32];
    let ep_a = [0x01; 32];
    let ep_b = [0x02; 32];

    let sk1 = derive_session_key(&shared_secret, &ep_a, &ep_b).unwrap();
    let sk2 = derive_session_key(&shared_secret, &ep_a, &ep_b).unwrap();
    assert_eq!(sk1.session_key, sk2.session_key);
}

#[test]
fn test_session_key_different_ephemeral() {
    let shared_secret = vec![0xAB; 32];
    let sk1 = derive_session_key(&shared_secret, &[0x01; 32], &[0x02; 32]).unwrap();
    let sk2 = derive_session_key(&shared_secret, &[0x03; 32], &[0x04; 32]).unwrap();
    assert_ne!(sk1.session_key, sk2.session_key);
}

// ── Nonce derivation ──

#[test]
fn test_nonce_derivation_deterministic() {
    let key = [0xAA; KEY_SIZE];
    let n1 = derive_nonce(&key, b"context").unwrap();
    let n2 = derive_nonce(&key, b"context").unwrap();
    assert_eq!(n1, n2);
    assert_eq!(n1.len(), NONCE_SIZE);
}

#[test]
fn test_consensus_nonce_derivation() {
    let key = [0xAA; KEY_SIZE];
    let n1 = derive_consensus_nonce(&key, b"aad_data", 100).unwrap();
    let n2 = derive_consensus_nonce(&key, b"aad_data", 100).unwrap();
    assert_eq!(n1, n2);

    // Different epoch → different nonce
    let n3 = derive_consensus_nonce(&key, b"aad_data", 101).unwrap();
    assert_ne!(n1, n3);
}

// ── Envelope key derivation ──

#[test]
fn test_envelope_key_derivation() {
    let shared_secret = vec![0xAB; 32];
    let envelope_id = [0xBB; 32];

    let k1 = derive_envelope_key(&shared_secret, &envelope_id).unwrap();
    let k2 = derive_envelope_key(&shared_secret, &envelope_id).unwrap();
    assert_eq!(k1, k2);
    assert_eq!(k1.len(), KEY_SIZE);
}

// ── Encrypt/Decrypt roundtrip ──

#[test]
fn test_encrypt_decrypt_roundtrip() {
    let key = [0xAA; KEY_SIZE];
    let nonce = derive_nonce(&key, b"test_context").unwrap();
    let plaintext = b"hello cipherocto world";
    let aad = b"additional data";

    let ciphertext = encrypt(&key, &nonce, plaintext, aad).unwrap();
    assert_ne!(&ciphertext[..plaintext.len()], plaintext);
    assert!(ciphertext.len() > plaintext.len()); // includes auth tag

    let decrypted = decrypt(&key, &nonce, &ciphertext, aad).unwrap();
    assert_eq!(&decrypted, plaintext);
}

#[test]
fn test_encrypt_decrypt_wrong_key_fails() {
    let key1 = [0xAA; KEY_SIZE];
    let key2 = [0xBB; KEY_SIZE];
    let nonce = derive_nonce(&key1, b"ctx").unwrap();

    let ciphertext = encrypt(&key1, &nonce, b"secret", b"aad").unwrap();
    let result = decrypt(&key2, &nonce, &ciphertext, b"aad");
    assert!(result.is_err());
}

#[test]
fn test_encrypt_decrypt_wrong_aad_fails() {
    let key = [0xAA; KEY_SIZE];
    let nonce = derive_nonce(&key, b"ctx").unwrap();

    let ciphertext = encrypt(&key, &nonce, b"secret", b"aad_correct").unwrap();
    let result = decrypt(&key, &nonce, &ciphertext, b"aad_wrong");
    assert!(result.is_err());
}

#[test]
fn test_encrypt_decrypt_tampered_ciphertext_fails() {
    let key = [0xAA; KEY_SIZE];
    let nonce = derive_nonce(&key, b"ctx").unwrap();

    let mut ciphertext = encrypt(&key, &nonce, b"secret", b"aad").unwrap();
    ciphertext[0] ^= 0xFF; // tamper
    let result = decrypt(&key, &nonce, &ciphertext, b"aad");
    assert!(result.is_err());
}

// ── Mission Key Hierarchy ──

#[test]
fn test_mission_key_hierarchy_derivation() {
    let seed = [0xF1; 32];
    let mission_id = [0x01; 32];

    let h = MissionKeyHierarchy::derive(&seed, &mission_id).unwrap();

    // All keys should be different
    assert_ne!(h.mission_root_key, h.transport_keys_root);
    assert_ne!(h.transport_keys_root, h.relay_keys_root);
    assert_ne!(h.relay_keys_root, h.execution_keys_root);
}

#[test]
fn test_mission_key_hierarchy_deterministic() {
    let seed = [0xF1; 32];
    let mission_id = [0x01; 32];

    let h1 = MissionKeyHierarchy::derive(&seed, &mission_id).unwrap();
    let h2 = MissionKeyHierarchy::derive(&seed, &mission_id).unwrap();
    assert_eq!(h1, h2);
}

#[test]
fn test_mission_key_hierarchy_per_mission_isolation() {
    let seed = [0xF1; 32];
    let h1 = MissionKeyHierarchy::derive(&seed, &[0x01; 32]).unwrap();
    let h2 = MissionKeyHierarchy::derive(&seed, &[0x02; 32]).unwrap();
    assert_ne!(h1.mission_root_key, h2.mission_root_key);
}

#[test]
fn test_mission_seed_derivation() {
    let coord_key = [0x42; 32];
    let mission_id = [0x01; 32];

    let seed1 = MissionKeyHierarchy::derive_seed(&coord_key, &mission_id).unwrap();
    let seed2 = MissionKeyHierarchy::derive_seed(&coord_key, &mission_id).unwrap();
    assert_eq!(seed1, seed2);
    assert_ne!(seed1, [0u8; 32]);
}

// ── Crypto Suite ──

#[test]
fn test_default_suite() {
    use octo_network::ocrypt::suite::algorithms;
    assert_eq!(DEFAULT_SUITE.hash_id, algorithms::HASH_BLAKE3_256);
    assert_eq!(DEFAULT_SUITE.signature_id, algorithms::SIG_ED25519);
    assert_eq!(DEFAULT_SUITE.kex_id, algorithms::KEX_X25519);
    assert_eq!(DEFAULT_SUITE.aead_id, algorithms::AEAD_CHACHA20_POLY1305);
    assert_eq!(DEFAULT_SUITE.kdf_id, algorithms::KDF_HKDF_BLAKE3);
}
