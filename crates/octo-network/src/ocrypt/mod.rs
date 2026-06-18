//! Overlay Cryptography (OCrypt) — RFC-0853
//!
//! Sovereign cryptographic layer for CipherOcto overlay networking.
//!
//! Provides:
//! - Sovereign overlay identity (platform-independent)
//! - Deterministic envelope encryption (ChaCha20-Poly1305)
//! - Session key establishment (X25519 + HKDF-BLAKE3)
//! - Mission-scoped key hierarchy
//! - Gateway attestation
//! - Onion relay extension
//! - Deterministic randomness derivation
//!
//! Core invariant: External platforms MUST NEVER be trusted for
//! confidentiality, authenticity, ordering, or integrity.

pub mod attestation;
pub mod envelope;
pub mod error;
pub mod identity;
pub mod mission;
pub mod onion;
pub mod randomness;
pub mod session;
pub mod suite;

pub use attestation::GatewayAttestation;
pub use envelope::{EncryptedEnvelope, EncryptionContext};
pub use error::CryptoError;
pub use identity::{OverlayIdentity, PlatformBinding};
pub use mission::MissionKeyHierarchy;
pub use randomness::{derive_deterministic_nonce, derive_deterministic_random};
pub use session::{
    decrypt, derive_consensus_nonce, derive_envelope_key, derive_nonce, derive_session_key,
    encrypt, x25519_shared_secret, SessionKeyMaterial, KEY_SIZE, NONCE_SIZE, TAG_SIZE,
};
pub use suite::{CryptoSuiteId, DEFAULT_SUITE};

/// HKDF-BLAKE3 key derivation (RFC-0853).
///
/// Uses BLAKE3 keyed hashing as the PRF for HKDF extract-then-expand.
/// This replaces HKDF-SHA256 to maintain CipherOcto's BLAKE3-only policy.
pub fn hkdf_blake3(salt: &[u8], ikm: &[u8], info: &[u8], output: &mut [u8]) {
    // Extract: PRK = BLAKE3::keyed(salt, ikm)
    let mut salt_key = [0u8; 32];
    let len = salt.len().min(32);
    salt_key[..len].copy_from_slice(&salt[..len]);
    let mut extractor = blake3::Hasher::new_keyed(&salt_key);
    extractor.update(ikm);
    let prk = *extractor.finalize().as_bytes();

    // Expand: use BLAKE3 XOF for arbitrary-length output
    // T(counter) = BLAKE3::keyed(PRK, info || counter)
    // Max output: 255 * 32 = 8160 bytes (u8 counter limit)
    assert!(
        output.len() <= 255 * 32,
        "hkdf_blake3 output too large: {} > 8160",
        output.len()
    );
    let mut offset = 0;
    let mut counter = 1u8;
    while offset < output.len() {
        let mut expander = blake3::Hasher::new_keyed(&prk);
        expander.update(info);
        expander.update(&[counter]);
        let mut xof_reader = expander.finalize_xof();
        let chunk_len = (output.len() - offset).min(32);
        let mut chunk = [0u8; 32];
        xof_reader.fill(&mut chunk[..chunk_len]);
        output[offset..offset + chunk_len].copy_from_slice(&chunk[..chunk_len]);
        offset += chunk_len;
        counter = counter.wrapping_add(1);
    }
}
