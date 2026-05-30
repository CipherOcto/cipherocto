//! Deterministic Envelope
//!
//! RFC-0850 §3.3: Deterministic Envelope (DEN)

use blake3;
use ed25519_dalek::{Signature, Verifier, VerifyingKey};

use super::error::DotError;

/// Message types for DOT envelopes
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(u16)]
pub enum MessageType {
    Message = 0x0001,
    Command = 0x0002,
    MissionSignal = 0x0003,
    StateUpdate = 0x0004,
    Heartbeat = 0x0005,
    ConsensusFragment = 0x0006,
    RouteAnnouncement = 0x0007,
    GatewayAdvertisement = 0x0008,
    GossipObject = 0x0009,
    ProofSubmission = 0x000A,
    Discovery = 0x000B,
}

/// Deterministic Envelope for DOT
///
/// All messages transported through DOT MUST use this canonical envelope.
/// Does NOT use serde — uses RFC-0126 canonical serialization (to_signing_bytes).
#[derive(Debug, Clone)]
#[repr(C)]
pub struct DeterministicEnvelope {
    /// Protocol version (current: 1)
    pub version: u16,
    /// Network identifier
    pub network_id: u32,
    /// Message type
    pub message_type: u16,
    /// Globally unique envelope identifier
    pub envelope_id: [u8; 32],
    /// Mission identifier (zero if not mission-scoped)
    pub mission_id: [u8; 32],
    /// Source peer identifier
    pub source_peer: [u8; 32],
    /// Gateway that first injected this envelope
    pub origin_gateway: [u8; 32],
    /// Logical timestamp (NOT wall-clock)
    pub logical_timestamp: u64,
    /// Maximum hop count before discard
    pub ttl_hops: u16,
    /// BLAKE3-256 of canonical payload bytes
    pub payload_hash: [u8; 32],
    /// Merkle root of route trace
    pub route_trace_root: [u8; 32],
    /// Protocol flags (bitmask)
    pub flags: u64,
    /// Ed25519 signature over canonical envelope bytes
    pub signature: [u8; 64],
}

impl DeterministicEnvelope {
    /// Derive envelope_id from canonical fields.
    ///
    /// envelope_id = BLAKE3-256(
    ///     network_id || message_type ||
    ///     source_peer || origin_gateway || logical_timestamp || payload_hash
    /// )
    ///
    /// Note: `version` is EXCLUDED from envelope_id derivation to ensure
    /// envelope identity remains stable across protocol version upgrades.
    pub fn derive_envelope_id(&self) -> [u8; 32] {
        let mut hasher = blake3::Hasher::new();
        hasher.update(&self.network_id.to_be_bytes());
        hasher.update(&self.message_type.to_be_bytes());
        hasher.update(&self.source_peer);
        hasher.update(&self.origin_gateway);
        hasher.update(&self.logical_timestamp.to_be_bytes());
        hasher.update(&self.payload_hash);
        *hasher.finalize().as_bytes()
    }

    /// Serialize to canonical bytes for signing/verification.
    /// Excludes signature field.
    pub fn to_signing_bytes(&self) -> Vec<u8> {
        let mut buf = Vec::with_capacity(256);
        buf.extend_from_slice(&self.version.to_be_bytes());
        buf.extend_from_slice(&self.network_id.to_be_bytes());
        buf.extend_from_slice(&self.message_type.to_be_bytes());
        buf.extend_from_slice(&self.envelope_id);
        buf.extend_from_slice(&self.mission_id);
        buf.extend_from_slice(&self.source_peer);
        buf.extend_from_slice(&self.origin_gateway);
        buf.extend_from_slice(&self.logical_timestamp.to_be_bytes());
        buf.extend_from_slice(&self.ttl_hops.to_be_bytes());
        buf.extend_from_slice(&self.payload_hash);
        buf.extend_from_slice(&self.route_trace_root);
        buf.extend_from_slice(&self.flags.to_be_bytes());
        buf
    }

    /// Serialize to canonical wire bytes including signature.
    /// This is the complete envelope for transport across platforms.
    pub fn to_wire_bytes(&self) -> Vec<u8> {
        let mut buf = self.to_signing_bytes();
        buf.extend_from_slice(&self.signature);
        buf
    }

    /// Deserialize from wire bytes (must include 64-byte signature at end).
    pub fn from_wire_bytes(data: &[u8]) -> Result<Self, DotError> {
        // Signing bytes: 2+4+2+32+32+32+32+8+2+32+32+8 = 218 bytes
        const SIGNING_LEN: usize = 218;
        const SIGNATURE_LEN: usize = 64;
        const WIRE_LEN: usize = SIGNING_LEN + SIGNATURE_LEN;

        if data.len() != WIRE_LEN {
            return Err(DotError::Serialization(format!(
                "Invalid wire envelope length: expected {}, got {}",
                WIRE_LEN,
                data.len()
            )));
        }

        let mut offset = 0;
        let read_u16 = |data: &[u8], off: &mut usize| -> u16 {
            let v = u16::from_be_bytes([data[*off], data[*off + 1]]);
            *off += 2;
            v
        };
        let read_u32 = |data: &[u8], off: &mut usize| -> u32 {
            let v =
                u32::from_be_bytes([data[*off], data[*off + 1], data[*off + 2], data[*off + 3]]);
            *off += 4;
            v
        };
        let read_u64 = |data: &[u8], off: &mut usize| -> u64 {
            let v = u64::from_be_bytes([
                data[*off],
                data[*off + 1],
                data[*off + 2],
                data[*off + 3],
                data[*off + 4],
                data[*off + 5],
                data[*off + 6],
                data[*off + 7],
            ]);
            *off += 8;
            v
        };
        let read_bytes = |data: &[u8], off: &mut usize, len: usize| -> [u8; 32] {
            let mut arr = [0u8; 32];
            arr.copy_from_slice(&data[*off..*off + len]);
            *off += len;
            arr
        };

        let version = read_u16(data, &mut offset);
        let network_id = read_u32(data, &mut offset);
        let message_type = read_u16(data, &mut offset);
        let envelope_id = read_bytes(data, &mut offset, 32);
        let mission_id = read_bytes(data, &mut offset, 32);
        let source_peer = read_bytes(data, &mut offset, 32);
        let origin_gateway = read_bytes(data, &mut offset, 32);
        let logical_timestamp = read_u64(data, &mut offset);
        let ttl_hops = read_u16(data, &mut offset);
        let payload_hash = read_bytes(data, &mut offset, 32);
        let route_trace_root = read_bytes(data, &mut offset, 32);
        let flags = read_u64(data, &mut offset);

        let mut signature = [0u8; 64];
        signature.copy_from_slice(&data[SIGNING_LEN..WIRE_LEN]);

        Ok(Self {
            version,
            network_id,
            message_type,
            envelope_id,
            mission_id,
            source_peer,
            origin_gateway,
            logical_timestamp,
            ttl_hops,
            payload_hash,
            route_trace_root,
            flags,
            signature,
        })
    }

    /// Verify envelope integrity.
    pub fn verify(&self, public_key: &[u8; 32]) -> Result<(), DotError> {
        // 1. Verify envelope_id derivation
        let expected_id = self.derive_envelope_id();
        if self.envelope_id != expected_id {
            return Err(DotError::InvalidEnvelopeId {
                expected: expected_id,
                computed: self.envelope_id,
            });
        }

        // 2. Verify signature (Ed25519)
        let signing_bytes = self.to_signing_bytes();
        let signature = Signature::from_bytes(&self.signature);
        let verifying_key = VerifyingKey::from_bytes(public_key)
            .map_err(|e| DotError::Serialization(e.to_string()))?;
        verifying_key
            .verify(&signing_bytes, &signature)
            .map_err(|_| DotError::InvalidSignature {
                envelope_id: self.envelope_id,
            })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ed25519_dalek::{Signer, SigningKey};

    fn create_test_envelope() -> (DeterministicEnvelope, SigningKey) {
        let signing_key = SigningKey::from_bytes(&[42u8; 32]);
        let mut envelope = DeterministicEnvelope {
            version: 1,
            network_id: 1,
            message_type: MessageType::Message as u16,
            envelope_id: [0u8; 32],
            mission_id: [0u8; 32],
            source_peer: [1u8; 32],
            origin_gateway: [2u8; 32],
            logical_timestamp: 1000,
            ttl_hops: 10,
            payload_hash: *blake3::hash(b"test payload").as_bytes(),
            route_trace_root: [0u8; 32],
            flags: 0,
            signature: [0u8; 64],
        };
        envelope.envelope_id = envelope.derive_envelope_id();
        let signing_bytes = envelope.to_signing_bytes();
        envelope.signature = signing_key.sign(&signing_bytes).to_bytes();
        (envelope, signing_key)
    }

    #[test]
    fn test_envelope_id_derivation() {
        let (envelope, _) = create_test_envelope();
        let derived = envelope.derive_envelope_id();
        assert_eq!(envelope.envelope_id, derived);
    }

    #[test]
    fn test_envelope_signing_bytes() {
        let (envelope, _) = create_test_envelope();
        let bytes = envelope.to_signing_bytes();
        assert!(!bytes.is_empty());
        assert!(bytes.len() > 200);
    }

    #[test]
    fn test_envelope_verify_valid() {
        let (envelope, signing_key) = create_test_envelope();
        let public_key = signing_key.verifying_key().to_bytes();
        assert!(envelope.verify(&public_key).is_ok());
    }

    #[test]
    fn test_envelope_verify_invalid_signature() {
        let (mut envelope, signing_key) = create_test_envelope();
        envelope.signature[0] ^= 0xFF;
        let public_key = signing_key.verifying_key().to_bytes();
        assert!(envelope.verify(&public_key).is_err());
    }

    #[test]
    fn test_envelope_verify_wrong_key() {
        let (envelope, _) = create_test_envelope();
        let wrong_key = [99u8; 32];
        assert!(envelope.verify(&wrong_key).is_err());
    }

    #[test]
    fn test_envelope_verify_corrupted_id() {
        let (mut envelope, signing_key) = create_test_envelope();
        envelope.envelope_id[0] ^= 0xFF;
        let public_key = signing_key.verifying_key().to_bytes();
        assert!(envelope.verify(&public_key).is_err());
    }
}

// ============================================================
// Privacy and Encryption (RFC-0850 §10)
// ============================================================

/// Bitmask flags for envelope privacy features.
/// Stored in DeterministicEnvelope.flags.
pub mod envelope_flags {
    /// Payload is encrypted (ciphertext in payload_hash-verified blob)
    pub const ENCRYPTED: u64 = 0x0001;
    /// Envelope is sealed (metadata minimized)
    pub const SEALED: u64 = 0x0002;
    /// Transport obfuscation enabled
    pub const OBFUSCATED: u64 = 0x0004;
    /// End-to-end encrypted (no relay decryption)
    pub const E2E: u64 = 0x0008;
    /// Stealth mode (mission existence hidden)
    pub const STEALTH: u64 = 0x0010;
}

/// A sealed envelope with encrypted payload and minimized metadata.
///
/// Platforms observe only ciphertext and relay metadata.
/// Mission data is NEVER plaintext on the carrier platform.
#[derive(Debug, Clone)]
pub struct SealedEnvelope {
    /// The outer DOT envelope (metadata visible to platform)
    pub envelope: DeterministicEnvelope,
    /// Encrypted payload bytes (ciphertext)
    pub encrypted_payload: Vec<u8>,
    /// Nonce used for encryption (12 bytes for ChaCha20-Poly1305)
    pub nonce: [u8; 12],
    /// Sender's ephemeral public key (for ECDH)
    pub sender_ephemeral: [u8; 32],
}

/// Metadata-minimized envelope for transport obfuscation.
/// Platforms see only opaque bytes.
#[derive(Debug, Clone)]
pub struct ObfuscatedEnvelope {
    /// Opaque wire bytes (envelope + payload serialized together)
    pub wire_bytes: Vec<u8>,
    /// BLAKE3-256 of the original envelope_id (for dedup)
    pub envelope_hash: [u8; 32],
}

/// Privacy configuration for envelope sealing.
#[derive(Debug, Clone, Copy, Default)]
pub struct PrivacyConfig {
    /// Enable end-to-end encryption
    pub e2e_encryption: bool,
    /// Enable metadata minimization
    pub metadata_minimization: bool,
    /// Enable transport obfuscation
    pub transport_obfuscation: bool,
}

impl DeterministicEnvelope {
    /// Check if this envelope has encrypted payload.
    pub fn is_encrypted(&self) -> bool {
        (self.flags & envelope_flags::ENCRYPTED) != 0
    }

    /// Check if this envelope is sealed (metadata minimized).
    pub fn is_sealed(&self) -> bool {
        (self.flags & envelope_flags::SEALED) != 0
    }

    /// Check if this envelope uses transport obfuscation.
    pub fn is_obfuscated(&self) -> bool {
        (self.flags & envelope_flags::OBFUSCATED) != 0
    }

    /// Check if this envelope is E2E encrypted.
    pub fn is_e2e(&self) -> bool {
        (self.flags & envelope_flags::E2E) != 0
    }

    /// Check if this envelope is in stealth mode.
    pub fn is_stealth(&self) -> bool {
        (self.flags & envelope_flags::STEALTH) != 0
    }

    /// Set privacy flags on this envelope.
    pub fn set_privacy(&mut self, config: &PrivacyConfig) {
        if config.e2e_encryption {
            self.flags |= envelope_flags::ENCRYPTED | envelope_flags::E2E;
        }
        if config.metadata_minimization {
            self.flags |= envelope_flags::SEALED;
        }
        if config.transport_obfuscation {
            self.flags |= envelope_flags::OBFUSCATED;
        }
    }

    /// Derive a sealing key from shared secret.
    /// Uses HKDF-BLAKE3 with domain separation.
    pub fn derive_sealing_key(shared_secret: &[u8; 32], envelope_id: &[u8; 32]) -> [u8; 32] {
        let mut hasher = blake3::Hasher::new();
        hasher.update(shared_secret);
        hasher.update(envelope_id);
        hasher.update(b"dot:seal:v1");
        *hasher.finalize().as_bytes()
    }

    /// Compute the wire format hash for obfuscation.
    /// wire_hash = BLAKE3-256(envelope_bytes || encrypted_payload)
    pub fn compute_wire_hash(signing_bytes: &[u8], encrypted_payload: &[u8]) -> [u8; 32] {
        let mut hasher = blake3::Hasher::new();
        hasher.update(signing_bytes);
        hasher.update(encrypted_payload);
        *hasher.finalize().as_bytes()
    }
}

impl SealedEnvelope {
    /// Create a new sealed envelope from a plaintext envelope.
    ///
    /// In a full implementation, this would encrypt the payload using
    /// ChaCha20-Poly1305 with the shared secret. Here we provide the
    /// structural framework; actual encryption requires OCrypt session keys.
    pub fn new(
        envelope: DeterministicEnvelope,
        encrypted_payload: Vec<u8>,
        nonce: [u8; 12],
        sender_ephemeral: [u8; 32],
    ) -> Self {
        Self {
            envelope,
            encrypted_payload,
            nonce,
            sender_ephemeral,
        }
    }

    /// Derive the decryption key from receiver's secret and sender's ephemeral.
    pub fn derive_decryption_key(
        receiver_secret: &[u8; 32],
        sender_ephemeral: &[u8; 32],
    ) -> [u8; 32] {
        let mut hasher = blake3::Hasher::new();
        hasher.update(receiver_secret);
        hasher.update(sender_ephemeral);
        hasher.update(b"dot:decrypt:v1");
        *hasher.finalize().as_bytes()
    }

    /// Verify that the encrypted payload hash matches the envelope's payload_hash.
    pub fn verify_payload_hash(&self) -> bool {
        let computed = blake3::hash(&self.encrypted_payload);
        *computed.as_bytes() == self.envelope.payload_hash
    }
}

impl ObfuscatedEnvelope {
    /// Create an obfuscated envelope from wire bytes.
    pub fn from_wire(wire_bytes: Vec<u8>) -> Self {
        let envelope_hash = *blake3::hash(&wire_bytes).as_bytes();
        Self {
            wire_bytes,
            envelope_hash,
        }
    }

    /// Get the envelope hash for deduplication.
    pub fn dedup_key(&self) -> [u8; 32] {
        self.envelope_hash
    }
}

#[cfg(test)]
mod privacy_tests {
    use super::*;
    use ed25519_dalek::{Signer, SigningKey};

    fn make_envelope() -> DeterministicEnvelope {
        let signing_key = SigningKey::from_bytes(&[42u8; 32]);
        let mut envelope = DeterministicEnvelope {
            version: 1,
            network_id: 1,
            message_type: MessageType::Message as u16,
            envelope_id: [0u8; 32],
            mission_id: [0u8; 32],
            source_peer: [1u8; 32],
            origin_gateway: [2u8; 32],
            logical_timestamp: 1000,
            ttl_hops: 10,
            payload_hash: *blake3::hash(b"test payload").as_bytes(),
            route_trace_root: [0u8; 32],
            flags: 0,
            signature: [0u8; 64],
        };
        envelope.envelope_id = envelope.derive_envelope_id();
        let signing_bytes = envelope.to_signing_bytes();
        envelope.signature = signing_key.sign(&signing_bytes).to_bytes();
        envelope
    }

    #[test]
    fn test_envelope_flags_default_none() {
        let env = make_envelope();
        assert!(!env.is_encrypted());
        assert!(!env.is_sealed());
        assert!(!env.is_obfuscated());
        assert!(!env.is_e2e());
        assert!(!env.is_stealth());
    }

    #[test]
    fn test_set_privacy_encrypted() {
        let mut env = make_envelope();
        env.set_privacy(&PrivacyConfig {
            e2e_encryption: true,
            ..Default::default()
        });
        assert!(env.is_encrypted());
        assert!(env.is_e2e());
        assert!(!env.is_sealed());
    }

    #[test]
    fn test_set_privacy_sealed() {
        let mut env = make_envelope();
        env.set_privacy(&PrivacyConfig {
            metadata_minimization: true,
            ..Default::default()
        });
        assert!(env.is_sealed());
        assert!(!env.is_encrypted());
    }

    #[test]
    fn test_set_privacy_all() {
        let mut env = make_envelope();
        env.set_privacy(&PrivacyConfig {
            e2e_encryption: true,
            metadata_minimization: true,
            transport_obfuscation: true,
        });
        assert!(env.is_encrypted());
        assert!(env.is_e2e());
        assert!(env.is_sealed());
        assert!(env.is_obfuscated());
    }

    #[test]
    fn test_derive_sealing_key_deterministic() {
        let secret = [1u8; 32];
        let eid = [2u8; 32];
        let k1 = DeterministicEnvelope::derive_sealing_key(&secret, &eid);
        let k2 = DeterministicEnvelope::derive_sealing_key(&secret, &eid);
        assert_eq!(k1, k2);
    }

    #[test]
    fn test_derive_sealing_key_different_inputs() {
        let k1 = DeterministicEnvelope::derive_sealing_key(&[1u8; 32], &[2u8; 32]);
        let k2 = DeterministicEnvelope::derive_sealing_key(&[1u8; 32], &[3u8; 32]);
        assert_ne!(k1, k2);
    }

    #[test]
    fn test_sealed_envelope_payload_hash() {
        let env = make_envelope();
        let payload = b"encrypted data";
        let sealed = SealedEnvelope::new(env.clone(), payload.to_vec(), [0u8; 12], [9u8; 32]);
        // Payload hash won't match since env.payload_hash is for plaintext
        // but the function correctly computes and compares
        assert!(!sealed.verify_payload_hash());

        // Create sealed envelope with matching hash
        let encrypted = b"ciphertext here";
        let mut env2 = env.clone();
        env2.payload_hash = *blake3::hash(encrypted).as_bytes();
        let sealed2 = SealedEnvelope::new(env2, encrypted.to_vec(), [0u8; 12], [9u8; 32]);
        assert!(sealed2.verify_payload_hash());
    }

    #[test]
    fn test_sealed_envelope_derive_decryption_key() {
        let receiver_secret = [1u8; 32];
        let sender_ephemeral = [2u8; 32];
        let k1 = SealedEnvelope::derive_decryption_key(&receiver_secret, &sender_ephemeral);
        let k2 = SealedEnvelope::derive_decryption_key(&receiver_secret, &sender_ephemeral);
        assert_eq!(k1, k2);

        // Different sender ephemeral produces different key
        let k3 = SealedEnvelope::derive_decryption_key(&receiver_secret, &[3u8; 32]);
        assert_ne!(k1, k3);
    }

    #[test]
    fn test_obfuscated_envelope_dedup() {
        let wire = vec![1u8, 2, 3, 4, 5];
        let obf = ObfuscatedEnvelope::from_wire(wire.clone());
        let obf2 = ObfuscatedEnvelope::from_wire(wire);
        assert_eq!(obf.dedup_key(), obf2.dedup_key());

        let obf3 = ObfuscatedEnvelope::from_wire(vec![6u8, 7, 8]);
        assert_ne!(obf.dedup_key(), obf3.dedup_key());
    }

    #[test]
    fn test_compute_wire_hash_deterministic() {
        let sb = b"signing bytes";
        let ep = b"encrypted payload";
        let h1 = DeterministicEnvelope::compute_wire_hash(sb, ep);
        let h2 = DeterministicEnvelope::compute_wire_hash(sb, ep);
        assert_eq!(h1, h2);
    }
}
