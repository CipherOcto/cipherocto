//! Per-mission key isolation (per RFC-0862 Phase 4, mission 0862l).
//!
//! Adds privacy-aware encryption to the carrier layer. PRIVATE missions
//! encrypt sync payloads with mission-specific keys (ChaCha20-Poly1305 AEAD);
//! PUBLIC missions send in clear text.
//!
//! This module wraps the existing `MissionKeyRing` (from 0862d) to provide
//! a privacy-level abstraction for the carrier layer.

use std::sync::Arc;

use crate::error::SyncError;
use crate::keyring::{KeyRing, MissionKeyRing};

/// Mission privacy level (per RFC-0862 §4.3.1).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MissionPrivacy {
    /// Encrypted with mission-specific key. Only trusted peers can decrypt.
    Private,
    /// Sent in clear text. Any peer can read.
    Public,
}

/// Wrapper that adds privacy-aware encryption to the carrier layer.
///
/// Uses the existing `MissionKeyRing` for AEAD operations. PUBLIC missions
/// pass payloads through unchanged; PRIVATE missions encrypt with the
/// mission's execution key.
#[derive(Debug, Clone)]
pub struct MissionCrypto {
    /// The mission's key ring (already has encrypt/decrypt).
    keyring: Arc<MissionKeyRing>,
    /// The mission's privacy level.
    privacy: MissionPrivacy,
}

impl MissionCrypto {
    /// Create a new `MissionCrypto` for the given privacy level.
    pub fn new(keyring: Arc<MissionKeyRing>, privacy: MissionPrivacy) -> Self {
        Self { keyring, privacy }
    }

    /// Return the mission's privacy level.
    pub fn privacy(&self) -> MissionPrivacy {
        self.privacy
    }

    /// Encrypt a payload.
    ///
    /// PUBLIC missions return plaintext passthrough with a zero nonce.
    /// PRIVATE missions encrypt with ChaCha20-Poly1305 AEAD.
    ///
    /// Returns `(ciphertext, nonce)`. The caller MUST ship the nonce
    /// alongside the ciphertext (prepended as first 12 bytes).
    pub fn encrypt(&self, plaintext: &[u8], aad: &[u8]) -> (Vec<u8>, [u8; 12]) {
        match self.privacy {
            MissionPrivacy::Public => (plaintext.to_vec(), [0u8; 12]),
            MissionPrivacy::Private => self.keyring.encrypt(plaintext, aad),
        }
    }

    /// Decrypt a payload.
    ///
    /// PUBLIC missions return ciphertext passthrough (no decryption needed).
    /// PRIVATE missions decrypt with the mission's execution key.
    pub fn decrypt(
        &self,
        ciphertext: &[u8],
        nonce: &[u8; 12],
        aad: &[u8],
    ) -> Result<Vec<u8>, SyncError> {
        match self.privacy {
            MissionPrivacy::Public => Ok(ciphertext.to_vec()),
            MissionPrivacy::Private => self.keyring.decrypt(ciphertext, nonce, aad),
        }
    }

    /// Prepare a payload for transmission.
    ///
    /// For PRIVATE missions, prepends the 12-byte nonce to the ciphertext.
    /// For PUBLIC missions, returns plaintext as-is.
    ///
    /// Wire format: `[12-byte nonce][ciphertext]` (PRIVATE) or `[plaintext]` (PUBLIC).
    pub fn prepare_for_send(&self, plaintext: &[u8], aad: &[u8]) -> Vec<u8> {
        let (payload, nonce) = self.encrypt(plaintext, aad);
        match self.privacy {
            MissionPrivacy::Public => payload,
            MissionPrivacy::Private => {
                let mut wire = Vec::with_capacity(12 + payload.len());
                wire.extend_from_slice(&nonce);
                wire.extend_from_slice(&payload);
                wire
            }
        }
    }

    /// Extract and decrypt a received payload.
    ///
    /// Expects the wire format from `prepare_for_send`: `[12-byte nonce][ciphertext]`
    /// for PRIVATE missions, or `[plaintext]` for PUBLIC missions.
    pub fn receive(&self, wire: &[u8], aad: &[u8]) -> Result<Vec<u8>, SyncError> {
        match self.privacy {
            MissionPrivacy::Public => Ok(wire.to_vec()),
            MissionPrivacy::Private => {
                if wire.len() < 12 {
                    return Err(SyncError::DecryptionFailed);
                }
                let nonce: [u8; 12] = wire[..12]
                    .try_into()
                    .map_err(|_| SyncError::DecryptionFailed)?;
                let ciphertext = &wire[12..];
                self.keyring.decrypt(ciphertext, &nonce, aad)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_keyring() -> Arc<MissionKeyRing> {
        Arc::new(MissionKeyRing::derive(&[0x42u8; 32], [0xABu8; 32]))
    }

    #[test]
    fn public_passthrough_encrypt() {
        let crypto = MissionCrypto::new(test_keyring(), MissionPrivacy::Public);
        let (ct, nonce) = crypto.encrypt(b"hello", b"aad");
        assert_eq!(ct, b"hello");
        assert_eq!(nonce, [0u8; 12]);
    }

    #[test]
    fn public_passthrough_decrypt() {
        let crypto = MissionCrypto::new(test_keyring(), MissionPrivacy::Public);
        let pt = crypto.decrypt(b"hello", &[0u8; 12], b"aad").unwrap();
        assert_eq!(pt, b"hello");
    }

    #[test]
    fn private_encrypt_decrypt_roundtrip() {
        let crypto = MissionCrypto::new(test_keyring(), MissionPrivacy::Private);
        let plaintext = b"secret sync data";
        let aad = b"mission-aad";
        let (ciphertext, nonce) = crypto.encrypt(plaintext, aad);
        assert_ne!(ciphertext, plaintext.to_vec());
        let decrypted = crypto.decrypt(&ciphertext, &nonce, aad).unwrap();
        assert_eq!(decrypted, plaintext);
    }

    #[test]
    fn private_wrong_key_fails() {
        let crypto1 = MissionCrypto::new(test_keyring(), MissionPrivacy::Private);
        let crypto2 = MissionCrypto::new(
            Arc::new(MissionKeyRing::derive(&[0x99u8; 32], [0xABu8; 32])),
            MissionPrivacy::Private,
        );
        let (ciphertext, nonce) = crypto1.encrypt(b"secret", b"aad");
        let result = crypto2.decrypt(&ciphertext, &nonce, b"aad");
        assert!(result.is_err());
    }

    #[test]
    fn private_wrong_aad_fails() {
        let crypto = MissionCrypto::new(test_keyring(), MissionPrivacy::Private);
        let (ciphertext, nonce) = crypto.encrypt(b"secret", b"correct-aad");
        let result = crypto.decrypt(&ciphertext, &nonce, b"wrong-aad");
        assert!(result.is_err());
    }

    #[test]
    fn prepare_for_send_public() {
        let crypto = MissionCrypto::new(test_keyring(), MissionPrivacy::Public);
        let wire = crypto.prepare_for_send(b"hello", b"aad");
        assert_eq!(wire, b"hello");
    }

    #[test]
    fn prepare_for_send_private_prepends_nonce() {
        let crypto = MissionCrypto::new(test_keyring(), MissionPrivacy::Private);
        let wire = crypto.prepare_for_send(b"secret", b"aad");
        assert!(wire.len() > 12);
        // First 12 bytes are nonce, rest is ciphertext
        let nonce: [u8; 12] = wire[..12].try_into().unwrap();
        assert_ne!(nonce, [0u8; 12]);
    }

    #[test]
    fn receive_roundtrip() {
        let crypto = MissionCrypto::new(test_keyring(), MissionPrivacy::Private);
        let wire = crypto.prepare_for_send(b"secret", b"aad");
        let pt = crypto.receive(&wire, b"aad").unwrap();
        assert_eq!(pt, b"secret");
    }

    #[test]
    fn receive_too_short_fails() {
        let crypto = MissionCrypto::new(test_keyring(), MissionPrivacy::Private);
        let result = crypto.receive(&[0u8; 5], b"aad");
        assert!(result.is_err());
    }
}
