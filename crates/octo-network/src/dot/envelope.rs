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

    /// Verify envelope integrity.
    pub fn verify(&self, public_key: &[u8; 32]) -> Result<(), DotError> {
        // 1. Verify envelope_id derivation
        let expected_id = self.derive_envelope_id();
        if self.envelope_id != expected_id {
            return Err(DotError::PayloadHashMismatch {
                expected: expected_id,
                actual: self.envelope_id,
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
