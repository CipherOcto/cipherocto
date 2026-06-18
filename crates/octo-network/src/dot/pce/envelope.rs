//! Proof-Carrying Envelope (RFC-0859 §3.1)

use crate::dot::envelope::DeterministicEnvelope;
#[cfg(test)]
use crate::dot::pce::proof_type::ProofSystemId;

/// Proof-Carrying Envelope — wraps a DOT envelope with a zero-knowledge proof.
///
/// Fields match RFC-0859 §3.1 (7 fields):
/// 1. envelope — the underlying DOT envelope
/// 2. proof_system_id — which proof backend (ProofSystemId)
/// 3. proof_commitment — BLAKE3-256(proof_blob)
/// 4. public_input_root — Merkle root of public inputs
/// 5. proof_blob — the serialized proof bytes
/// 6. execution_model — ProofCircuitModel variant
/// 7. parent_proof_commitment — optional, for recursive aggregation
#[derive(Debug, Clone)]
pub struct ProofCarryingEnvelope {
    /// The underlying DOT envelope
    pub envelope: DeterministicEnvelope,
    /// Proof system identifier (ProofSystemId enum)
    pub proof_system_id: u16,
    /// BLAKE3-256 commitment to proof_blob
    pub proof_commitment: [u8; 32],
    /// Merkle root of public inputs
    pub public_input_root: [u8; 32],
    /// Serialized proof blob
    pub proof_blob: Vec<u8>,
    /// Proof circuit model (ProofCircuitModel enum)
    pub execution_model: u16,
    /// Parent proof commitment for recursive aggregation (None if top-level)
    pub parent_proof_commitment: Option<[u8; 32]>,
}

impl ProofCarryingEnvelope {
    /// Compute the proof commitment from the proof blob.
    /// proof_commitment = BLAKE3-256(proof_blob)
    pub fn compute_proof_commitment(proof_blob: &[u8]) -> [u8; 32] {
        *blake3::hash(proof_blob).as_bytes()
    }

    /// Verify the proof commitment matches the proof blob.
    pub fn verify_commitment(&self) -> bool {
        let expected = Self::compute_proof_commitment(&self.proof_blob);
        expected == self.proof_commitment
    }

    /// Create a signing byte representation for the PCE.
    /// aad = envelope_id || proof_system_id || proof_commitment || public_input_root
    ///        || parent_proof_commitment (or zeros if None)
    pub fn to_signing_bytes(&self) -> Vec<u8> {
        let mut bytes = Vec::new();
        bytes.extend_from_slice(&self.envelope.envelope_id);
        bytes.extend_from_slice(&self.proof_system_id.to_be_bytes());
        bytes.extend_from_slice(&self.proof_commitment);
        bytes.extend_from_slice(&self.public_input_root);
        match &self.parent_proof_commitment {
            Some(parent) => bytes.extend_from_slice(parent),
            None => bytes.extend_from_slice(&[0u8; 32]),
        }
        bytes
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dot::envelope::MessageType;

    fn make_test_envelope() -> DeterministicEnvelope {
        DeterministicEnvelope {
            version: 1,
            network_id: 1,
            message_type: MessageType::Message as u16,
            envelope_id: [1u8; 32],
            mission_id: [0u8; 32],
            source_peer: [2u8; 32],
            origin_gateway: [3u8; 32],
            logical_timestamp: 1000,
            ttl_hops: 10,
            payload_hash: [4u8; 32],
            route_trace_root: [0u8; 32],
            flags: 0,
            signature: [0u8; 64],
        }
    }

    #[test]
    fn test_pce_compute_proof_commitment() {
        let blob = b"test proof data";
        let commitment = ProofCarryingEnvelope::compute_proof_commitment(blob);
        assert_eq!(commitment, *blake3::hash(blob).as_bytes());
        assert_ne!(commitment, [0u8; 32]);
    }

    #[test]
    fn test_pce_verify_commitment_valid() {
        let blob = vec![1u8, 2, 3, 4, 5];
        let commitment = ProofCarryingEnvelope::compute_proof_commitment(&blob);
        let pce = ProofCarryingEnvelope {
            envelope: make_test_envelope(),
            proof_system_id: ProofSystemId::STWO as u16,
            proof_commitment: commitment,
            public_input_root: [0u8; 32],
            proof_blob: blob,
            execution_model: 0x0001,
            parent_proof_commitment: None,
        };
        assert!(pce.verify_commitment());
    }

    #[test]
    fn test_pce_verify_commitment_mismatch() {
        let pce = ProofCarryingEnvelope {
            envelope: make_test_envelope(),
            proof_system_id: ProofSystemId::STWO as u16,
            proof_commitment: [0xFFu8; 32],
            public_input_root: [0u8; 32],
            proof_blob: vec![1u8, 2, 3],
            execution_model: 0x0001,
            parent_proof_commitment: None,
        };
        assert!(!pce.verify_commitment());
    }

    #[test]
    fn test_pce_signing_bytes_length() {
        let pce = ProofCarryingEnvelope {
            envelope: make_test_envelope(),
            proof_system_id: ProofSystemId::PLONK as u16,
            proof_commitment: [0xAAu8; 32],
            public_input_root: [0xBBu8; 32],
            proof_blob: vec![],
            execution_model: 0x0003,
            parent_proof_commitment: None,
        };
        let bytes = pce.to_signing_bytes();
        // 32 (envelope_id) + 2 (proof_system_id) + 32 (proof_commitment)
        // + 32 (public_input_root) + 32 (parent=zeros) = 130
        assert_eq!(bytes.len(), 130);
    }

    #[test]
    fn test_pce_signing_bytes_with_parent() {
        let pce = ProofCarryingEnvelope {
            envelope: make_test_envelope(),
            proof_system_id: ProofSystemId::STWO as u16,
            proof_commitment: [0xAAu8; 32],
            public_input_root: [0xBBu8; 32],
            proof_blob: vec![],
            execution_model: 0x0005,
            parent_proof_commitment: Some([0xCCu8; 32]),
        };
        let bytes_with = pce.to_signing_bytes();
        let pce_no_parent = ProofCarryingEnvelope {
            parent_proof_commitment: None,
            ..pce
        };
        let bytes_without = pce_no_parent.to_signing_bytes();
        // With parent: 32 + 2 + 32 + 32 + 32 = 130
        // Without parent: 32 + 2 + 32 + 32 + 32 = 130 (zeros instead)
        assert_eq!(bytes_with.len(), 130);
        assert_eq!(bytes_without.len(), 130);
        // But the last 32 bytes differ
        assert_ne!(
            &bytes_with[bytes_with.len() - 32..],
            &bytes_without[bytes_without.len() - 32..]
        );
    }
}
