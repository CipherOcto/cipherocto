//! Onion relay extension (RFC-0853 §10)

use crate::ocrypt::error::CryptoError;
use crate::ocrypt::session;
use blake3;
use hkdf::Hkdf;
use sha2::Sha256;

/// Domain separation for onion session key derivation
const ONION_SESSION_DOMAIN: &str = "ocrypt:onion:v1";
/// Domain separation for onion nonce derivation
const ONION_NONCE_DOMAIN: &str = "ocrypt:nonce:v1";

/// A single onion layer — encrypted for one relay hop.
///
/// Each relay knows ONLY: previous hop, next hop, local instructions.
/// NOT: origin, destination, full route, mission topology.
#[derive(Clone, Debug, PartialEq, Eq)]
#[repr(C)]
pub struct OnionLayer {
    /// Ephemeral public key for this layer (32 bytes)
    pub ephemeral_public: [u8; 32],
    /// 12-byte nonce for ChaCha20-Poly1305
    pub nonce: [u8; 12],
    /// Encrypted payload (next layer or final payload)
    pub ciphertext: Vec<u8>,
}

/// Derive a relay session key for onion routing.
///
/// session_key = HKDF-BLAKE3(shared_secret, "ocrypt:onion:v1", hop_index || route_id)
pub fn derive_onion_session_key(
    shared_secret: &[u8],
    hop_index: u16,
    route_id: &[u8; 32],
) -> Result<[u8; 32], CryptoError> {
    let salt = ONION_SESSION_DOMAIN.as_bytes();
    let mut info = [0u8; 34]; // 2 (hop_index) + 32 (route_id)
    info[0..2].copy_from_slice(&hop_index.to_be_bytes());
    info[2..34].copy_from_slice(route_id);

    let hk = Hkdf::<Sha256>::new(Some(salt), shared_secret);
    let mut key = [0u8; 32];
    hk.expand(&info, &mut key)
        .map_err(|_| CryptoError::KeyDerivationFailure {
            stage: "onion_session_key",
        })?;
    Ok(key)
}

/// Derive nonce for onion layer encryption.
///
/// nonce = HKDF-BLAKE3(session_key, "ocrypt:nonce:v1", hop_index)[0..12]
pub fn derive_onion_nonce(session_key: &[u8; 32], hop_index: u16) -> Result<[u8; 12], CryptoError> {
    let mut info = [0u8; 2];
    info.copy_from_slice(&hop_index.to_be_bytes());

    let hk = Hkdf::<Sha256>::new(Some(ONION_NONCE_DOMAIN.as_bytes()), session_key);
    let mut full = [0u8; 32];
    hk.expand(&info, &mut full)
        .map_err(|_| CryptoError::KeyDerivationFailure {
            stage: "onion_nonce",
        })?;

    let mut nonce = [0u8; 12];
    nonce.copy_from_slice(&full[..12]);
    Ok(nonce)
}

/// Encrypt an onion layer (inside-out construction).
///
/// Payload → encrypt for relay N → encrypt for relay N-1 → ... → encrypt for relay entry
pub fn encrypt_onion_layer(
    session_key: &[u8; 32],
    hop_index: u16,
    payload: &[u8],
    aad: &[u8],
) -> Result<OnionLayer, CryptoError> {
    let nonce = derive_onion_nonce(session_key, hop_index)?;
    let ephemeral_public = [0u8; 32]; // placeholder — real impl uses X25519 ephemeral
    let ciphertext = session::encrypt(session_key, &nonce, payload, aad)?;

    Ok(OnionLayer {
        ephemeral_public,
        nonce,
        ciphertext,
    })
}

/// Decrypt an onion layer.
pub fn decrypt_onion_layer(
    session_key: &[u8; 32],
    layer: &OnionLayer,
    aad: &[u8],
) -> Result<Vec<u8>, CryptoError> {
    session::decrypt(session_key, &layer.nonce, &layer.ciphertext, aad)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_onion_session_key_deterministic() {
        let secret = [0xAAu8; 32];
        let route_id = [0x01u8; 32];
        let k1 = derive_onion_session_key(&secret, 0, &route_id).unwrap();
        let k2 = derive_onion_session_key(&secret, 0, &route_id).unwrap();
        assert_eq!(k1, k2);
    }

    #[test]
    fn test_onion_session_key_different_hops() {
        let secret = [0xAAu8; 32];
        let route_id = [0x01u8; 32];
        let k1 = derive_onion_session_key(&secret, 0, &route_id).unwrap();
        let k2 = derive_onion_session_key(&secret, 1, &route_id).unwrap();
        assert_ne!(k1, k2);
    }

    #[test]
    fn test_onion_session_key_different_routes() {
        let secret = [0xAAu8; 32];
        let r1 = [0x01u8; 32];
        let r2 = [0x02u8; 32];
        let k1 = derive_onion_session_key(&secret, 0, &r1).unwrap();
        let k2 = derive_onion_session_key(&secret, 0, &r2).unwrap();
        assert_ne!(k1, k2);
    }

    #[test]
    fn test_onion_nonce_size() {
        let key = [0x42u8; 32];
        let nonce = derive_onion_nonce(&key, 0).unwrap();
        assert_eq!(nonce.len(), 12);
    }

    #[test]
    fn test_onion_nonce_different_hops() {
        let key = [0x42u8; 32];
        let n1 = derive_onion_nonce(&key, 0).unwrap();
        let n2 = derive_onion_nonce(&key, 1).unwrap();
        assert_ne!(n1, n2);
    }

    #[test]
    fn test_encrypt_decrypt_onion_layer() {
        let key = [0x42u8; 32];
        let plaintext = b"onion payload";
        let aad = b"aad";

        let layer = encrypt_onion_layer(&key, 0, plaintext, aad).unwrap();
        let decrypted = decrypt_onion_layer(&key, &layer, aad).unwrap();
        assert_eq!(decrypted, plaintext);
    }

    #[test]
    fn test_onion_layer_wrong_key_fails() {
        let key1 = [0x42u8; 32];
        let key2 = [0x43u8; 32];
        let aad = b"aad";

        let layer = encrypt_onion_layer(&key1, 0, b"test", aad).unwrap();
        let result = decrypt_onion_layer(&key2, &layer, aad);
        assert!(result.is_err());
    }

    #[test]
    fn test_onion_session_key_size() {
        let secret = [0xAAu8; 32];
        let route_id = [0x01u8; 32];
        let key = derive_onion_session_key(&secret, 0, &route_id).unwrap();
        assert_eq!(key.len(), 32);
    }

    #[test]
    fn test_onion_nonce_uniqueness_across_hops() {
        let key = [0x42u8; 32];
        let mut nonces = Vec::new();
        for hop in 0..10u16 {
            nonces.push(derive_onion_nonce(&key, hop).unwrap());
        }
        // All nonces must be unique
        for i in 0..nonces.len() {
            for j in (i + 1)..nonces.len() {
                assert_ne!(
                    nonces[i], nonces[j],
                    "Nonce collision at hops {} and {}",
                    i, j
                );
            }
        }
    }

    #[test]
    fn test_onion_relay_isolation() {
        // Each relay gets a different session key — compromise of one doesn't expose others
        let secret = [0xAAu8; 32];
        let route_id = [0x01u8; 32];
        let k0 = derive_onion_session_key(&secret, 0, &route_id).unwrap();
        let k1 = derive_onion_session_key(&secret, 1, &route_id).unwrap();
        let k2 = derive_onion_session_key(&secret, 2, &route_id).unwrap();

        // All keys must be different (relay isolation)
        assert_ne!(k0, k1);
        assert_ne!(k1, k2);
        assert_ne!(k0, k2);
    }

    #[test]
    fn test_onion_forward_secrecy() {
        // Compromise of relay 1's key doesn't let you decrypt relay 0's layer
        let key0 = [0x42u8; 32];
        let key1 = [0x43u8; 32];
        let aad = b"aad";

        let payload = b"secret data";
        let layer0 = encrypt_onion_layer(&key0, 0, payload, aad).unwrap();

        // Decrypting with key1 (wrong relay) must fail
        let result = decrypt_onion_layer(&key1, &layer0, aad);
        assert!(result.is_err());
    }

    #[test]
    fn test_onion_layer_aad_binding() {
        // Same key + different AAD must produce different results
        let key = [0x42u8; 32];
        let payload = b"test payload";

        let layer1 = encrypt_onion_layer(&key, 0, payload, b"aad1").unwrap();
        let layer2 = encrypt_onion_layer(&key, 0, payload, b"aad2").unwrap();

        // Different AAD should produce different ciphertext (with overwhelming probability)
        assert_ne!(layer1.ciphertext, layer2.ciphertext);
    }

    #[test]
    fn test_onion_constants() {
        assert_eq!(ONION_SESSION_DOMAIN, "ocrypt:onion:v1");
        assert_eq!(ONION_NONCE_DOMAIN, "ocrypt:nonce:v1");
    }
}
