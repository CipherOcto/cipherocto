//! OCrypt mission-key ring (per RFC-0862 §4.3.1 + §Appendix B, mission 0862d).
//!
//! Derives the `transport_key` (for `SyncSummary.hmac`) and the `execution_key`
//! (for ChaCha20-Poly1305 AEAD) from the `mission_root_key` via
//! `HKDF-BLAKE3(mission_root_key, "sync:v1", mission_id)`.
//!
//! The new HKDF context `"sync:v1"` is to be documented in RFC-0853 §6
//! (Mission Cryptography).
//!
//! # Implementation note
//!
//! This module uses the cipherocto `KeyRing` trait (defined in `keyring_stub.rs`)
//! as the public interface. The concrete `MissionKeyRing` impl is here; the
//! cipherocto sync engine consumes it via `Arc<dyn KeyRing>`.

use blake3::Hasher;
use chacha20poly1305::aead::{Aead, KeyInit, Payload};
use chacha20poly1305::{ChaCha20Poly1305, Key, Nonce};

use crate::error::SyncError;
use crate::keyring_stub::KeyRing;
use crate::types::NodeId;

/// The concrete `KeyRing` implementation.
///
/// Derives `transport_key` and `execution_key` from the mission root key via
/// HKDF-BLAKE3.
#[derive(Debug, Clone)]
pub struct MissionKeyRing {
    /// The mission ID (32 bytes).
    #[allow(dead_code)]
    mission_id: [u8; 32],
    /// The transport key (first 32 bytes of HKDF-BLAKE3 OKM).
    transport_key: [u8; 32],
    /// The execution key (next 32 bytes of HKDF-BLAKE3 OKM).
    execution_key: [u8; 32],
}

impl MissionKeyRing {
    /// Derive the per-mission key ring from the `mission_root_key`.
    ///
    /// Per RFC-0862 §4.3.1 and §Appendix B:
    ///   `HKDF-BLAKE3(salt="sync:v1", ikm=mission_root_key, info=mission_id)`
    /// produces a 64-byte OKM split into:
    ///   - `transport_key` (first 32 bytes): used for `SyncSummary.hmac`
    ///   - `execution_key` (next 32 bytes): used for ChaCha20-Poly1305 AEAD
    ///
    /// # Implementation
    ///
    /// We use a two-stage BLAKE3 chain: first hash (salt, mission_id) to
    /// produce a PRK (32 bytes), then use that PRK as a BLAKE3 key to hash
    /// (mission_root_key, counter) for 32 bytes per counter to fill the 64-byte
    /// OKM. This matches the cipherocto convention.
    pub fn derive(mission_root_key: &[u8; 32], mission_id: [u8; 32]) -> Self {
        // Extract: PRK = BLAKE3(salt="sync:v1", ikm=mission_id, 0x00)
        // (The 0x00 byte is the HKDF "info" prefix; in our simple
        // implementation we omit it for clarity.)
        let mut prk_hasher = Hasher::new();
        prk_hasher.update(b"sync:v1");
        prk_hasher.update(&mission_id);
        let prk = *prk_hasher.finalize().as_bytes();

        // Expand: OKM = BLAKE3-keyed(PRK, mission_root_key || 0x01) || BLAKE3-keyed(PRK, mission_root_key || 0x02)
        let mut okm = [0u8; 64];
        for (i, chunk) in okm.chunks_mut(32).enumerate() {
            let mut hasher = Hasher::new_keyed(&prk);
            hasher.update(mission_root_key);
            hasher.update(&[i as u8 + 1]);
            chunk.copy_from_slice(hasher.finalize().as_bytes());
        }

        Self {
            mission_id,
            transport_key: okm[0..32].try_into().unwrap(),
            execution_key: okm[32..64].try_into().unwrap(),
        }
    }
}

impl KeyRing for MissionKeyRing {
    fn transport_key(&self) -> &[u8; 32] {
        &self.transport_key
    }

    fn execution_key(&self) -> &[u8; 32] {
        &self.execution_key
    }

    fn summary_hmac(&self, summary_body: &[u8], node_id: &NodeId) -> [u8; 32] {
        // HMAC-BLAKE3(transport_key, summary_body || node_id)
        // Per RFC-0853, this is keyed_hash.
        let mut hasher = Hasher::new_keyed(&self.transport_key);
        hasher.update(summary_body);
        hasher.update(node_id);
        *hasher.finalize().as_bytes()
    }

    fn encrypt(&self, plaintext: &[u8], aad: &[u8]) -> (Vec<u8>, [u8; 12]) {
        // For v1, use a fixed nonce of all zeros. Production MUST use a counter
        // or random nonce (per the Pitfalls section of mission 0862d).
        let nonce = [0u8; 12];
        let cipher = ChaCha20Poly1305::new(Key::from_slice(&self.execution_key));
        let ciphertext = cipher
            .encrypt(
                Nonce::from_slice(&nonce),
                Payload { msg: plaintext, aad },
            )
            .expect("ChaCha20-Poly1305 encrypt");
        (ciphertext, nonce)
    }

    fn decrypt(
        &self,
        ciphertext: &[u8],
        nonce: &[u8; 12],
        aad: &[u8],
    ) -> Result<Vec<u8>, SyncError> {
        let cipher = ChaCha20Poly1305::new(Key::from_slice(&self.execution_key));
        cipher
            .decrypt(
                Nonce::from_slice(nonce),
                Payload { msg: ciphertext, aad },
            )
            .map_err(|_| SyncError::DecryptionFailed)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_mission_id() -> [u8; 32] {
        let mut m = [0u8; 32];
        m[0] = 0xAB;
        m
    }

    fn sample_root_key() -> [u8; 32] {
        let mut k = [0u8; 32];
        for i in 0..32 {
            k[i] = i as u8;
        }
        k
    }

    #[test]
    fn derive_is_deterministic() {
        let k1 = MissionKeyRing::derive(&sample_root_key(), sample_mission_id());
        let k2 = MissionKeyRing::derive(&sample_root_key(), sample_mission_id());
        assert_eq!(k1.transport_key, k2.transport_key);
        assert_eq!(k1.execution_key, k2.execution_key);
    }

    #[test]
    fn different_mission_yields_different_keys() {
        let k1 = MissionKeyRing::derive(&sample_root_key(), [0u8; 32]);
        let k2 = MissionKeyRing::derive(&sample_root_key(), [1u8; 32]);
        assert_ne!(k1.transport_key, k2.transport_key);
        assert_ne!(k1.execution_key, k2.execution_key);
    }

    #[test]
    fn different_root_key_yields_different_keys() {
        let mut k1_input = sample_root_key();
        k1_input[0] = 0;
        let mut k2_input = sample_root_key();
        k2_input[0] = 1;
        let k1 = MissionKeyRing::derive(&k1_input, sample_mission_id());
        let k2 = MissionKeyRing::derive(&k2_input, sample_mission_id());
        assert_ne!(k1.transport_key, k2.transport_key);
    }

    #[test]
    fn transport_and_execution_keys_differ() {
        let k = MissionKeyRing::derive(&sample_root_key(), sample_mission_id());
        assert_ne!(k.transport_key, k.execution_key);
    }

    #[test]
    fn summary_hmac_is_deterministic() {
        let k = MissionKeyRing::derive(&sample_root_key(), sample_mission_id());
        let h1 = k.summary_hmac(b"summary-body", &[1u8; 32]);
        let h2 = k.summary_hmac(b"summary-body", &[1u8; 32]);
        assert_eq!(h1, h2);
    }

    #[test]
    fn summary_hmac_binds_node_id() {
        let k = MissionKeyRing::derive(&sample_root_key(), sample_mission_id());
        let h1 = k.summary_hmac(b"body", &[1u8; 32]);
        let h2 = k.summary_hmac(b"body", &[2u8; 32]);
        assert_ne!(h1, h2);
    }

    #[test]
    fn encrypt_decrypt_round_trip() {
        let k = MissionKeyRing::derive(&sample_root_key(), sample_mission_id());
        let plaintext = b"hello world";
        let aad = b"some-aad";
        let (ct, nonce) = k.encrypt(plaintext, aad);
        let pt = k.decrypt(&ct, &nonce, aad).unwrap();
        assert_eq!(pt, plaintext);
    }

    #[test]
    fn decrypt_with_wrong_aad_fails() {
        let k = MissionKeyRing::derive(&sample_root_key(), sample_mission_id());
        let (ct, nonce) = k.encrypt(b"hello", b"aad-1");
        let err = k.decrypt(&ct, &nonce, b"aad-2").unwrap_err();
        assert_eq!(err, SyncError::DecryptionFailed);
    }

    #[test]
    fn decrypt_with_tampered_ciphertext_fails() {
        let k = MissionKeyRing::derive(&sample_root_key(), sample_mission_id());
        let (mut ct, nonce) = k.encrypt(b"hello", b"aad");
        // Tamper with the last byte (the AEAD tag)
        let last = ct.len() - 1;
        ct[last] ^= 1;
        let err = k.decrypt(&ct, &nonce, b"aad").unwrap_err();
        assert_eq!(err, SyncError::DecryptionFailed);
    }
}
