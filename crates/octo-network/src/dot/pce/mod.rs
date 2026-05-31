//! Proof-Carrying Envelopes (PCE) — RFC-0859
//!
//! Extends DOT envelopes with zero-knowledge proof attachment,
//! deterministic verification, and recursive aggregation.

pub mod aggregate;
pub mod envelope;
pub mod error;
pub mod policy;
pub mod proof_type;
pub mod registry;
pub mod verify;

pub use aggregate::{aggregate_proofs, AggregatedProof};
pub use envelope::ProofCarryingEnvelope;
pub use error::PceError;
pub use policy::MissionProofPolicy;
pub use proof_type::{ProofCircuitModel, ProofSystemId, ProofType, VerificationResult};
pub use registry::{VerifierEntry, VerifierRegistry};
pub use verify::{compute_merkle_root, verify_canonical_boundary, verify_pce, verify_via_dps};

/// PCE protocol version
pub const PCE_PROTOCOL_VERSION: u16 = 1;

/// Maximum proof blob size (16 MiB)
pub const MAX_PROOF_BLOB_SIZE: usize = 16 * 1024 * 1024;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dot::envelope::{DeterministicEnvelope, MessageType};

    fn make_envelope() -> DeterministicEnvelope {
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
    fn test_pce_constants() {
        assert_eq!(PCE_PROTOCOL_VERSION, 1);
        assert_eq!(MAX_PROOF_BLOB_SIZE, 16 * 1024 * 1024);
    }

    #[test]
    fn test_pce_full_pipeline() {
        // Create a PCE
        let proof_blob = vec![1u8, 2, 3, 4, 5];
        let public_inputs = vec![[0xAAu8; 32], [0xBBu8; 32]];
        let commitment = ProofCarryingEnvelope::compute_proof_commitment(&proof_blob);
        let root = compute_merkle_root(&public_inputs);

        let pce = ProofCarryingEnvelope {
            envelope: make_envelope(),
            proof_system_id: ProofSystemId::STWO as u16,
            proof_commitment: commitment,
            public_input_root: root,
            proof_blob,
            execution_model: ProofCircuitModel::AIR as u16,
            parent_proof_commitment: None,
        };

        // Verify pipeline
        let result = verify_pce(&pce, &public_inputs).unwrap();
        assert_eq!(result, VerificationResult::Valid);

        // Verify canonical boundary
        assert!(verify_canonical_boundary(&pce));
    }

    #[test]
    fn test_pce_with_parent_aggregation() {
        let proof_blob = vec![10u8, 20, 30];
        let public_inputs = vec![[0xCCu8; 32]];
        let commitment = ProofCarryingEnvelope::compute_proof_commitment(&proof_blob);
        let root = compute_merkle_root(&public_inputs);
        let parent_commitment = [0xDDu8; 32];

        let pce = ProofCarryingEnvelope {
            envelope: make_envelope(),
            proof_system_id: ProofSystemId::PLONK as u16,
            proof_commitment: commitment,
            public_input_root: root,
            proof_blob,
            execution_model: ProofCircuitModel::PLONKISH as u16,
            parent_proof_commitment: Some(parent_commitment),
        };

        let result = verify_pce(&pce, &public_inputs).unwrap();
        assert_eq!(result, VerificationResult::Valid);
    }
}
