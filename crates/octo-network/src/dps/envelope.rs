//! ProofCarryingEnvelope — RFC-0854 §4 / RFC-0859 §3.1
//!
//! Wraps a DOT envelope with proof data. 7 fields, consensus-critical.

use crate::dot::envelope::DeterministicEnvelope;
use crate::dps::suite::{ProofCircuitModel, ProofSystemId};
use crate::dps::DpsError;

/// Maximum allowed proof blob size (1 MB).
pub const MAX_PROOF_BLOB_SIZE: usize = 1_048_576;

/// Proof-carrying envelope — wraps a DOT envelope with proof attachment.
///
/// RFC-0859 §3.1: 7 fields.
#[derive(Debug, Clone)]
pub struct ProofCarryingEnvelope {
    /// The underlying deterministic envelope
    pub envelope: DeterministicEnvelope,
    /// Proof system identifier (u16 — maps to ProofSystemId)
    pub proof_system_id: u16,
    /// BLAKE3-256 commitment over the proof blob
    pub proof_commitment: [u8; 32],
    /// BLAKE3-256 Merkle root of public inputs
    pub public_input_root: [u8; 32],
    /// Serialized proof blob (opaque to consensus)
    pub proof_blob: Vec<u8>,
    /// Circuit model identifier (u16 — maps to ProofCircuitModel)
    pub execution_model: u16,
    /// Commitment to parent proof for recursive aggregation (None if leaf)
    pub parent_proof_commitment: Option<[u8; 32]>,
}

impl ProofCarryingEnvelope {
    /// Create a new proof-carrying envelope.
    ///
    /// Returns `Err` if `proof_blob` exceeds [`MAX_PROOF_BLOB_SIZE`].
    pub fn new(
        envelope: DeterministicEnvelope,
        proof_system: ProofSystemId,
        circuit_model: ProofCircuitModel,
        proof_blob: Vec<u8>,
    ) -> Result<Self, DpsError> {
        if proof_blob.len() > MAX_PROOF_BLOB_SIZE {
            return Err(DpsError::MalformedProof {
                reason: "proof blob exceeds MAX_PROOF_BLOB_SIZE (1 MB)",
            });
        }

        use blake3::Hasher;

        let proof_commitment = {
            let mut h = Hasher::new();
            h.update(&proof_blob);
            *h.finalize().as_bytes()
        };

        Ok(Self {
            envelope,
            proof_system_id: proof_system.as_u16(),
            proof_commitment,
            public_input_root: [0u8; 32], // set by caller
            proof_blob,
            execution_model: circuit_model as u16,
            parent_proof_commitment: None,
        })
    }

    /// Validate this envelope.
    ///
    /// Rejects zero `public_input_root` and proof blobs exceeding [`MAX_PROOF_BLOB_SIZE`].
    pub fn validate(&self) -> Result<(), DpsError> {
        if self.public_input_root == [0u8; 32] {
            return Err(DpsError::MalformedProof {
                reason: "public_input_root must not be zero",
            });
        }
        if self.proof_blob.len() > MAX_PROOF_BLOB_SIZE {
            return Err(DpsError::MalformedProof {
                reason: "proof blob exceeds MAX_PROOF_BLOB_SIZE (1 MB)",
            });
        }
        Ok(())
    }

    /// Set public input root.
    pub fn with_public_input_root(mut self, root: [u8; 32]) -> Self {
        self.public_input_root = root;
        self
    }

    /// Set parent proof commitment for recursive aggregation.
    pub fn with_parent(mut self, parent: [u8; 32]) -> Self {
        self.parent_proof_commitment = Some(parent);
        self
    }

    /// Verify proof_commitment matches proof_blob.
    pub fn verify_commitment(&self) -> Result<(), DpsError> {
        use blake3::Hasher;
        let mut h = Hasher::new();
        h.update(&self.proof_blob);
        let actual = *h.finalize().as_bytes();
        if actual != self.proof_commitment {
            return Err(DpsError::CommitmentMismatch {
                expected: self.proof_commitment,
                actual,
            });
        }
        Ok(())
    }

    /// Get the proof system ID as an enum.
    pub fn proof_system(&self) -> Option<ProofSystemId> {
        ProofSystemId::from_u16(self.proof_system_id)
    }

    /// Get the execution model as an enum.
    pub fn circuit_model(&self) -> Option<ProofCircuitModel> {
        match self.execution_model {
            0x0001 => Some(ProofCircuitModel::AIR),
            0x0002 => Some(ProofCircuitModel::R1CS),
            0x0003 => Some(ProofCircuitModel::PLONKISH),
            0x0004 => Some(ProofCircuitModel::ZkVm),
            0x0005 => Some(ProofCircuitModel::Recursive),
            _ => None,
        }
    }

    /// Serialize proof commitment context for signing.
    pub fn to_signing_bytes(&self) -> Vec<u8> {
        let mut buf = Vec::new();
        buf.extend_from_slice(&self.proof_system_id.to_be_bytes());
        buf.extend_from_slice(&self.proof_commitment);
        buf.extend_from_slice(&self.public_input_root);
        buf.extend_from_slice(&self.execution_model.to_be_bytes());
        if let Some(parent) = &self.parent_proof_commitment {
            buf.extend_from_slice(parent);
        }
        buf
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_test_envelope() -> DeterministicEnvelope {
        DeterministicEnvelope {
            version: 1,
            network_id: 1,
            message_type: 0x0001,
            envelope_id: [0x42u8; 32],
            mission_id: [0u8; 32],
            source_peer: [0x01u8; 32],
            origin_gateway: [0x02u8; 32],
            logical_timestamp: 100,
            ttl_hops: 10,
            payload_hash: [0x03u8; 32],
            route_trace_root: [0u8; 32],
            flags: 0,
            signature: [0u8; 64],
        }
    }

    #[test]
    fn test_pce_new() {
        let env = make_test_envelope();
        let pce = ProofCarryingEnvelope::new(
            env,
            ProofSystemId::STWO,
            ProofCircuitModel::AIR,
            vec![1, 2, 3],
        )
        .unwrap();
        assert_eq!(pce.proof_system_id, 0x0001);
        assert_eq!(pce.execution_model, 0x0001);
        assert!(pce.parent_proof_commitment.is_none());
    }

    #[test]
    fn test_pce_commitment() {
        let env = make_test_envelope();
        let pce = ProofCarryingEnvelope::new(
            env,
            ProofSystemId::PLONK,
            ProofCircuitModel::PLONKISH,
            vec![10, 20, 30],
        )
        .unwrap();
        assert!(pce.verify_commitment().is_ok());
    }

    #[test]
    fn test_pce_commitment_mismatch() {
        let env = make_test_envelope();
        let mut pce = ProofCarryingEnvelope::new(
            env,
            ProofSystemId::STWO,
            ProofCircuitModel::AIR,
            vec![1, 2, 3],
        )
        .unwrap();
        pce.proof_blob = vec![99, 99, 99]; // tamper
        assert!(pce.verify_commitment().is_err());
    }

    #[test]
    fn test_pce_with_parent() {
        let env = make_test_envelope();
        let pce = ProofCarryingEnvelope::new(
            env,
            ProofSystemId::Halo2,
            ProofCircuitModel::Recursive,
            vec![1],
        )
        .unwrap()
        .with_parent([0xAA; 32]);
        assert!(pce.parent_proof_commitment.is_some());
        assert_eq!(pce.parent_proof_commitment.unwrap(), [0xAA; 32]);
    }

    #[test]
    fn test_pce_with_public_input_root() {
        let env = make_test_envelope();
        let pce =
            ProofCarryingEnvelope::new(env, ProofSystemId::STWO, ProofCircuitModel::AIR, vec![1])
                .unwrap()
                .with_public_input_root([0xBB; 32]);
        assert_eq!(pce.public_input_root, [0xBB; 32]);
    }

    #[test]
    fn test_pce_proof_system_enum() {
        let env = make_test_envelope();
        let pce = ProofCarryingEnvelope::new(
            env,
            ProofSystemId::Groth16,
            ProofCircuitModel::R1CS,
            vec![1],
        )
        .unwrap();
        assert_eq!(pce.proof_system(), Some(ProofSystemId::Groth16));
    }

    #[test]
    fn test_pce_circuit_model_enum() {
        let env = make_test_envelope();
        let pce =
            ProofCarryingEnvelope::new(env, ProofSystemId::STWO, ProofCircuitModel::ZkVm, vec![1])
                .unwrap();
        assert_eq!(pce.circuit_model(), Some(ProofCircuitModel::ZkVm));
    }

    #[test]
    fn test_pce_signing_bytes_deterministic() {
        let env = make_test_envelope();
        let pce1 = ProofCarryingEnvelope::new(
            env.clone(),
            ProofSystemId::STWO,
            ProofCircuitModel::AIR,
            vec![1, 2, 3],
        )
        .unwrap();
        let pce2 = ProofCarryingEnvelope::new(
            env,
            ProofSystemId::STWO,
            ProofCircuitModel::AIR,
            vec![1, 2, 3],
        )
        .unwrap();
        assert_eq!(pce1.to_signing_bytes(), pce2.to_signing_bytes());
    }

    #[test]
    fn test_pce_signing_bytes_with_parent() {
        let env = make_test_envelope();
        let pce_no_parent = ProofCarryingEnvelope::new(
            env.clone(),
            ProofSystemId::STWO,
            ProofCircuitModel::AIR,
            vec![1],
        )
        .unwrap();
        let pce_with_parent =
            ProofCarryingEnvelope::new(env, ProofSystemId::STWO, ProofCircuitModel::AIR, vec![1])
                .unwrap()
                .with_parent([0xAA; 32]);
        assert_ne!(
            pce_no_parent.to_signing_bytes(),
            pce_with_parent.to_signing_bytes()
        );
    }

    #[test]
    fn test_pce_rejects_oversized_blob() {
        let env = make_test_envelope();
        let big_blob = vec![0u8; MAX_PROOF_BLOB_SIZE + 1];
        let result =
            ProofCarryingEnvelope::new(env, ProofSystemId::STWO, ProofCircuitModel::AIR, big_blob);
        assert!(result.is_err());
    }

    #[test]
    fn test_pce_validate_rejects_zero_root() {
        let env = make_test_envelope();
        let pce = ProofCarryingEnvelope::new(
            env,
            ProofSystemId::STWO,
            ProofCircuitModel::AIR,
            vec![1, 2, 3],
        )
        .unwrap();
        // public_input_root is zero by default
        assert!(pce.validate().is_err());
    }

    #[test]
    fn test_pce_validate_ok() {
        let env = make_test_envelope();
        let pce = ProofCarryingEnvelope::new(
            env,
            ProofSystemId::STWO,
            ProofCircuitModel::AIR,
            vec![1, 2, 3],
        )
        .unwrap()
        .with_public_input_root([0xBB; 32]);
        assert!(pce.validate().is_ok());
    }
}
