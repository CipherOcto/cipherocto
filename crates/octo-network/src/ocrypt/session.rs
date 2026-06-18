//! Session key establishment (RFC-0853 §5)

use crate::ocrypt::error::CryptoError;
use chacha20poly1305::{
    aead::{AeadInPlace, KeyInit},
    ChaCha20Poly1305, Key, Nonce,
};

/// Domain separation for session key derivation
pub const SESSION_KEY_DOMAIN: &str = "ocrypt:session:v1";
/// Domain separation for nonce derivation
pub const NONCE_DOMAIN: &str = "ocrypt:nonce:v1";
/// Domain separation for envelope key derivation
pub const ENVELOPE_KEY_DOMAIN: &str = "ocrypt:envelope:v1";

/// Nonce size for ChaCha20-Poly1305 (12 bytes, not 24)
pub const NONCE_SIZE: usize = 12;
/// Key size for ChaCha20-Poly1305 (32 bytes)
pub const KEY_SIZE: usize = 32;
/// Auth tag size (16 bytes)
pub const TAG_SIZE: usize = 16;

/// Derived session key material
#[derive(Clone, Debug)]
pub struct SessionKeyMaterial {
    /// 32-byte session key for ChaCha20-Poly1305
    pub session_key: [u8; 32],
}

/// Derive a session key from X25519 shared secret.
///
/// session_key = HKDF-BLAKE3(
///   ikm = shared_secret,
///   salt = "ocrypt:session:v1",
///   info = ephemeral_public_a || ephemeral_public_b,
///   length = 32
/// )
pub fn derive_session_key(
    shared_secret: &[u8],
    ephemeral_public_a: &[u8; 32],
    ephemeral_public_b: &[u8; 32],
) -> Result<SessionKeyMaterial, CryptoError> {
    let mut info = Vec::with_capacity(64);
    info.extend_from_slice(ephemeral_public_a);
    info.extend_from_slice(ephemeral_public_b);

    let mut session_key = [0u8; KEY_SIZE];
    super::hkdf_blake3(
        SESSION_KEY_DOMAIN.as_bytes(),
        shared_secret,
        &info,
        &mut session_key,
    );

    Ok(SessionKeyMaterial { session_key })
}

/// Derive a 12-byte nonce for ChaCha20-Poly1305.
///
/// nonce = HKDF-BLAKE3(session_key, "ocrypt:nonce:v1", context)[0..12]
pub fn derive_nonce(
    session_key: &[u8; KEY_SIZE],
    context: &[u8],
) -> Result<[u8; NONCE_SIZE], CryptoError> {
    let mut full_nonce = [0u8; 32];
    super::hkdf_blake3(
        NONCE_DOMAIN.as_bytes(),
        session_key,
        context,
        &mut full_nonce,
    );

    let mut nonce = [0u8; NONCE_SIZE];
    nonce.copy_from_slice(&full_nonce[..NONCE_SIZE]);
    Ok(nonce)
}

/// Derive an envelope encryption key.
pub fn derive_envelope_key(
    shared_secret: &[u8],
    envelope_id: &[u8; 32],
) -> Result<[u8; KEY_SIZE], CryptoError> {
    let mut key = [0u8; KEY_SIZE];
    super::hkdf_blake3(
        ENVELOPE_KEY_DOMAIN.as_bytes(),
        shared_secret,
        envelope_id,
        &mut key,
    );
    Ok(key)
}

/// Encrypt plaintext with ChaCha20-Poly1305.
///
/// Returns ciphertext || auth_tag (16 bytes appended).
pub fn encrypt(
    key: &[u8; KEY_SIZE],
    nonce: &[u8; NONCE_SIZE],
    plaintext: &[u8],
    aad: &[u8],
) -> Result<Vec<u8>, CryptoError> {
    let cipher = ChaCha20Poly1305::new(Key::from_slice(key));
    let nonce_obj = Nonce::from_slice(nonce);
    let mut buffer = plaintext.to_vec();
    cipher
        .encrypt_in_place(nonce_obj, aad, &mut buffer)
        .map_err(|e| CryptoError::EncryptionFailed {
            reason: e.to_string(),
        })?;
    Ok(buffer)
}

/// Decrypt ciphertext with ChaCha20-Poly1305.
///
/// Input is ciphertext || auth_tag. Returns plaintext.
pub fn decrypt(
    key: &[u8; KEY_SIZE],
    nonce: &[u8; NONCE_SIZE],
    ciphertext_with_tag: &[u8],
    aad: &[u8],
) -> Result<Vec<u8>, CryptoError> {
    let cipher = ChaCha20Poly1305::new(Key::from_slice(key));
    let nonce_obj = Nonce::from_slice(nonce);
    let mut buffer = ciphertext_with_tag.to_vec();
    cipher
        .decrypt_in_place(nonce_obj, aad, &mut buffer)
        .map_err(|e| CryptoError::DecryptionFailed {
            reason: e.to_string(),
        })?;
    Ok(buffer)
}

/// Compute X25519 shared secret.
pub fn x25519_shared_secret(
    secret: &x25519_dalek::StaticSecret,
    public: &x25519_dalek::PublicKey,
) -> [u8; 32] {
    secret.diffie_hellman(public).to_bytes()
}

/// Derive deterministic nonce for consensus-critical paths.
///
/// nonce = HKDF-BLAKE3(session_key, "ocrypt:nonce:v1", aad || epoch_be)[0..12]
pub fn derive_consensus_nonce(
    session_key: &[u8; KEY_SIZE],
    aad: &[u8],
    epoch: u64,
) -> Result<[u8; NONCE_SIZE], CryptoError> {
    let mut context = Vec::with_capacity(aad.len() + 8);
    context.extend_from_slice(aad);
    context.extend_from_slice(&epoch.to_be_bytes());
    derive_nonce(session_key, &context)
}

#[cfg(test)]
mod tests {
    use super::*;
    use x25519_dalek::{PublicKey, StaticSecret};

    #[test]
    fn test_session_key_derivation_deterministic() {
        let shared = [0xAAu8; 32];
        let pk_a = [0x01u8; 32];
        let pk_b = [0x02u8; 32];
        let k1 = derive_session_key(&shared, &pk_a, &pk_b).unwrap();
        let k2 = derive_session_key(&shared, &pk_a, &pk_b).unwrap();
        assert_eq!(k1.session_key, k2.session_key);
    }

    #[test]
    fn test_session_key_different_inputs() {
        let shared1 = [0xAAu8; 32];
        let shared2 = [0xBBu8; 32];
        let pk_a = [0x01u8; 32];
        let pk_b = [0x02u8; 32];
        let k1 = derive_session_key(&shared1, &pk_a, &pk_b).unwrap();
        let k2 = derive_session_key(&shared2, &pk_a, &pk_b).unwrap();
        assert_ne!(k1.session_key, k2.session_key);
    }

    #[test]
    fn test_nonce_derivation() {
        let key = [0x42u8; 32];
        let n1 = derive_nonce(&key, b"context1").unwrap();
        let n2 = derive_nonce(&key, b"context2").unwrap();
        assert_ne!(n1, n2);
        assert_eq!(n1.len(), 12);
    }

    #[test]
    fn test_encrypt_decrypt_roundtrip() {
        let key = [0x42u8; 32];
        let nonce = [0x01u8; 12];
        let plaintext = b"hello world";
        let aad = b"associated data";

        let ciphertext = encrypt(&key, &nonce, plaintext, aad).unwrap();
        let decrypted = decrypt(&key, &nonce, &ciphertext, aad).unwrap();
        assert_eq!(decrypted, plaintext);
    }

    #[test]
    fn test_encrypt_wrong_key_fails() {
        let key1 = [0x42u8; 32];
        let key2 = [0x43u8; 32];
        let nonce = [0x01u8; 12];
        let plaintext = b"hello world";
        let aad = b"aad";

        let ciphertext = encrypt(&key1, &nonce, plaintext, aad).unwrap();
        let result = decrypt(&key2, &nonce, &ciphertext, aad);
        assert!(result.is_err());
    }

    #[test]
    fn test_encrypt_wrong_aad_fails() {
        let key = [0x42u8; 32];
        let nonce = [0x01u8; 12];
        let plaintext = b"hello world";

        let ciphertext = encrypt(&key, &nonce, plaintext, b"aad1").unwrap();
        let result = decrypt(&key, &nonce, &ciphertext, b"aad2");
        assert!(result.is_err());
    }

    #[test]
    fn test_consensus_nonce_derivation() {
        let key = [0x42u8; 32];
        let aad = b"test_aad";
        let n1 = derive_consensus_nonce(&key, aad, 1).unwrap();
        let n2 = derive_consensus_nonce(&key, aad, 2).unwrap();
        assert_ne!(n1, n2);
        assert_eq!(n1.len(), 12);
    }

    #[test]
    fn test_x25519_shared_secret_symmetric() {
        let secret_a = StaticSecret::from([0xA1u8; 32]);
        let secret_b = StaticSecret::from([0xB1u8; 32]);
        let pub_a = PublicKey::from(&secret_a);
        let pub_b = PublicKey::from(&secret_b);

        let shared_a = x25519_shared_secret(&secret_a, &pub_b);
        let shared_b = x25519_shared_secret(&secret_b, &pub_a);
        assert_eq!(shared_a, shared_b);
    }

    #[test]
    fn test_envelope_key_derivation() {
        let shared = [0xAAu8; 32];
        let eid1 = [0x01u8; 32];
        let eid2 = [0x02u8; 32];
        let k1 = derive_envelope_key(&shared, &eid1).unwrap();
        let k2 = derive_envelope_key(&shared, &eid2).unwrap();
        assert_ne!(k1, k2);
        assert_eq!(k1.len(), 32);
    }

    #[test]
    fn test_encrypt_empty_plaintext() {
        let key = [0x42u8; 32];
        let nonce = [0x01u8; 12];
        let ciphertext = encrypt(&key, &nonce, b"", b"aad").unwrap();
        // Empty plaintext still produces auth tag (16 bytes)
        assert_eq!(ciphertext.len(), TAG_SIZE);
        let decrypted = decrypt(&key, &nonce, &ciphertext, b"aad").unwrap();
        assert!(decrypted.is_empty());
    }

    #[test]
    fn test_nonce_size_constant() {
        assert_eq!(NONCE_SIZE, 12);
    }

    #[test]
    fn test_key_size_constant() {
        assert_eq!(KEY_SIZE, 32);
    }
}
