//! Encrypted envelope (RFC-0853 §4)

use blake3;

/// Encrypted envelope — wraps a DOT envelope with encryption.
///
/// Canonical encryption boundary: plaintext canonicalization MUST occur BEFORE encryption.
///
/// AAD = envelope_id || sender_ephemeral_public || mission_id || logical_timestamp || sequence
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EncryptedEnvelope {
    /// BLAKE3-256 hash of the plaintext (consensus-verifiable)
    pub envelope_hash: [u8; 32],
    /// Sender's ephemeral X25519 public key (32 bytes)
    pub sender_ephemeral_key: [u8; 32],
    /// 12-byte nonce for ChaCha20-Poly1305
    pub nonce: [u8; 12],
    /// Ciphertext || auth_tag (16 bytes appended)
    pub ciphertext: Vec<u8>,
}

impl EncryptedEnvelope {
    /// Build AAD from envelope context fields.
    ///
    /// aad = envelope_id || sender_ephemeral_public || mission_id || logical_timestamp || sequence
    pub fn build_aad(
        envelope_id: &[u8; 32],
        sender_ephemeral_public: &[u8; 32],
        mission_id: &[u8; 32],
        logical_timestamp: u64,
        sequence: u64,
    ) -> Vec<u8> {
        let mut aad = Vec::with_capacity(32 + 32 + 32 + 8 + 8);
        aad.extend_from_slice(envelope_id);
        aad.extend_from_slice(sender_ephemeral_public);
        aad.extend_from_slice(mission_id);
        aad.extend_from_slice(&logical_timestamp.to_be_bytes());
        aad.extend_from_slice(&sequence.to_be_bytes());
        aad
    }

    /// Compute plaintext hash = BLAKE3-256(plaintext).
    pub fn hash_plaintext(plaintext: &[u8]) -> [u8; 32] {
        *blake3::hash(plaintext).as_bytes()
    }
}

/// Encryption context for building an EncryptedEnvelope.
pub struct EncryptionContext {
    pub envelope_id: [u8; 32],
    pub mission_id: [u8; 32],
    pub logical_timestamp: u64,
    pub sequence: u64,
}

impl EncryptionContext {
    pub fn new(
        envelope_id: [u8; 32],
        mission_id: [u8; 32],
        logical_timestamp: u64,
        sequence: u64,
    ) -> Self {
        Self {
            envelope_id,
            mission_id,
            logical_timestamp,
            sequence,
        }
    }

    /// Build AAD for this context.
    pub fn build_aad(&self, sender_ephemeral_public: &[u8; 32]) -> Vec<u8> {
        EncryptedEnvelope::build_aad(
            &self.envelope_id,
            sender_ephemeral_public,
            &self.mission_id,
            self.logical_timestamp,
            self.sequence,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_build_aad_deterministic() {
        let eid = [0x01u8; 32];
        let pk = [0x02u8; 32];
        let mid = [0x03u8; 32];
        let aad1 = EncryptedEnvelope::build_aad(&eid, &pk, &mid, 100, 1);
        let aad2 = EncryptedEnvelope::build_aad(&eid, &pk, &mid, 100, 1);
        assert_eq!(aad1, aad2);
    }

    #[test]
    fn test_build_aad_size() {
        let eid = [0x01u8; 32];
        let pk = [0x02u8; 32];
        let mid = [0x03u8; 32];
        let aad = EncryptedEnvelope::build_aad(&eid, &pk, &mid, 100, 1);
        // 32 + 32 + 32 + 8 + 8 = 112
        assert_eq!(aad.len(), 112);
    }

    #[test]
    fn test_build_aad_different_envelope_ids() {
        let eid1 = [0x01u8; 32];
        let eid2 = [0x02u8; 32];
        let pk = [0x02u8; 32];
        let mid = [0x03u8; 32];
        let aad1 = EncryptedEnvelope::build_aad(&eid1, &pk, &mid, 100, 1);
        let aad2 = EncryptedEnvelope::build_aad(&eid2, &pk, &mid, 100, 1);
        assert_ne!(aad1, aad2);
    }

    #[test]
    fn test_hash_plaintext_deterministic() {
        let h1 = EncryptedEnvelope::hash_plaintext(b"hello");
        let h2 = EncryptedEnvelope::hash_plaintext(b"hello");
        assert_eq!(h1, h2);
    }

    #[test]
    fn test_hash_plaintext_different() {
        let h1 = EncryptedEnvelope::hash_plaintext(b"hello");
        let h2 = EncryptedEnvelope::hash_plaintext(b"world");
        assert_ne!(h1, h2);
    }

    #[test]
    fn test_hash_plaintext_size() {
        let h = EncryptedEnvelope::hash_plaintext(b"test");
        assert_eq!(h.len(), 32);
    }

    #[test]
    fn test_encrypted_envelope_size() {
        let env = EncryptedEnvelope {
            envelope_hash: [0u8; 32],
            sender_ephemeral_key: [0u8; 32],
            nonce: [0u8; 12],
            ciphertext: vec![0u8; 100],
        };
        assert_eq!(env.envelope_hash.len(), 32);
        assert_eq!(env.sender_ephemeral_key.len(), 32);
        assert_eq!(env.nonce.len(), 12);
        assert_eq!(env.ciphertext.len(), 100);
    }
}
