//! Recursive Proof Aggregation (RFC-0859 §7)

use crate::dot::pce::envelope::ProofCarryingEnvelope;
use crate::dot::pce::error::PceError;
use crate::dot::pce::proof_type::ProofSystemId;
use crate::dot::pce::verify::compute_merkle_root;

/// An aggregated proof combining multiple envelope proofs (RFC-0859 §7.2).
#[derive(Debug, Clone)]
pub struct AggregatedProof {
    /// Commitments to the inner proofs being aggregated
    pub inner_proof_commitments: Vec<[u8; 32]>,
    /// The aggregated proof blob
    pub aggregated_blob: Vec<u8>,
    /// The aggregation proof system
    pub aggregation_system: u16,
    /// The aggregated public input root
    pub aggregated_public_input_root: [u8; 32],
    /// Number of proofs aggregated
    pub proof_count: u32,
}

impl AggregatedProof {
    /// Compute the aggregated proof commitment.
    pub fn compute_aggregated_commitment(&self) -> [u8; 32] {
        *blake3::hash(&self.aggregated_blob).as_bytes()
    }

    /// Verify the aggregated proof structure.
    pub fn verify_structure(&self) -> Result<(), PceError> {
        if self.proof_count == 0 {
            return Err(PceError::AggregationError {
                reason: "zero proofs aggregated".into(),
            });
        }
        if self.inner_proof_commitments.len() != self.proof_count as usize {
            return Err(PceError::AggregationError {
                reason: format!(
                    "commitment count {} != proof_count {}",
                    self.inner_proof_commitments.len(),
                    self.proof_count
                ),
            });
        }
        if self.aggregated_blob.is_empty() {
            return Err(PceError::MalformedProof("empty aggregated_blob".into()));
        }
        if ProofSystemId::from_u16(self.aggregation_system).is_none() {
            return Err(PceError::UnsupportedSystem(self.aggregation_system));
        }
        Ok(())
    }
}

/// Aggregate multiple Proof-Carrying Envelopes into a single AggregatedProof.
///
/// RFC-0859 §7.1: Multiple proofs across multiple envelopes MAY be aggregated
/// into a single recursive proof.
///
/// This creates the structural aggregation — actual proof aggregation
/// requires a backend (STARK/PLONK) and is Class C.
pub fn aggregate_proofs(
    pces: &[ProofCarryingEnvelope],
    aggregation_system: ProofSystemId,
) -> Result<AggregatedProof, PceError> {
    if pces.is_empty() {
        return Err(PceError::AggregationError {
            reason: "no envelopes to aggregate".into(),
        });
    }

    let inner_proof_commitments: Vec<[u8; 32]> =
        pces.iter().map(|pce| pce.proof_commitment).collect();

    // Compute aggregated public input root from all envelopes' public input roots
    let input_roots: Vec<[u8; 32]> = pces.iter().map(|pce| pce.public_input_root).collect();
    let aggregated_public_input_root = compute_merkle_root(&input_roots);

    // Concatenate proof blobs as the aggregated blob
    // (In production, this would be a recursive proof from the backend)
    let mut aggregated_blob = Vec::new();
    for pce in pces {
        aggregated_blob.extend_from_slice(&pce.proof_blob);
    }

    Ok(AggregatedProof {
        inner_proof_commitments,
        aggregated_blob: aggregated_blob.clone(),
        aggregation_system: aggregation_system as u16,
        aggregated_public_input_root,
        proof_count: pces.len() as u32,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dot::envelope::{DeterministicEnvelope, MessageType};

    fn make_pce(proof_data: &[u8], input_root: [u8; 32]) -> ProofCarryingEnvelope {
        let blob = proof_data.to_vec();
        let commitment = ProofCarryingEnvelope::compute_proof_commitment(&blob);
        ProofCarryingEnvelope {
            envelope: DeterministicEnvelope {
                version: 1,
                network_id: 1,
                message_type: MessageType::Message as u16,
                envelope_id: [0u8; 32],
                mission_id: [0u8; 32],
                source_peer: [0u8; 32],
                origin_gateway: [0u8; 32],
                logical_timestamp: 0,
                ttl_hops: 10,
                payload_hash: [0u8; 32],
                route_trace_root: [0u8; 32],
                flags: 0,
                signature: [0u8; 64],
            },
            proof_system_id: ProofSystemId::STWO as u16,
            proof_commitment: commitment,
            public_input_root: input_root,
            proof_blob: blob,
            execution_model: 0x0001,
            parent_proof_commitment: None,
        }
    }

    #[test]
    fn test_aggregate_single_proof() {
        let pce = make_pce(&[1, 2, 3], [0xAAu8; 32]);
        let agg = aggregate_proofs(&[pce], ProofSystemId::STWO).unwrap();
        assert_eq!(agg.proof_count, 1);
        assert_eq!(agg.inner_proof_commitments.len(), 1);
    }

    #[test]
    fn test_aggregate_multiple_proofs() {
        let pces = vec![
            make_pce(&[1, 2], [0xAAu8; 32]),
            make_pce(&[3, 4], [0xBBu8; 32]),
            make_pce(&[5, 6], [0xCCu8; 32]),
        ];
        let agg = aggregate_proofs(&pces, ProofSystemId::STWO).unwrap();
        assert_eq!(agg.proof_count, 3);
        assert_eq!(agg.inner_proof_commitments.len(), 3);
        assert!(!agg.aggregated_blob.is_empty());
    }

    #[test]
    fn test_aggregate_empty_fails() {
        let result = aggregate_proofs(&[], ProofSystemId::STWO);
        assert!(matches!(result, Err(PceError::AggregationError { .. })));
    }

    #[test]
    fn test_aggregated_proof_verify_structure_valid() {
        let pces = vec![make_pce(&[1], [0xAAu8; 32]), make_pce(&[2], [0xBBu8; 32])];
        let agg = aggregate_proofs(&pces, ProofSystemId::STWO).unwrap();
        assert!(agg.verify_structure().is_ok());
    }

    #[test]
    fn test_aggregated_proof_verify_structure_zero_proofs() {
        let agg = AggregatedProof {
            inner_proof_commitments: vec![],
            aggregated_blob: vec![1, 2, 3],
            aggregation_system: ProofSystemId::STWO as u16,
            aggregated_public_input_root: [0u8; 32],
            proof_count: 0,
        };
        assert!(matches!(
            agg.verify_structure(),
            Err(PceError::AggregationError { .. })
        ));
    }

    #[test]
    fn test_aggregated_proof_verify_structure_count_mismatch() {
        let agg = AggregatedProof {
            inner_proof_commitments: vec![[0u8; 32]], // 1 commitment
            aggregated_blob: vec![1, 2, 3],
            aggregation_system: ProofSystemId::STWO as u16,
            aggregated_public_input_root: [0u8; 32],
            proof_count: 2, // but claims 2
        };
        assert!(matches!(
            agg.verify_structure(),
            Err(PceError::AggregationError { .. })
        ));
    }

    #[test]
    fn test_aggregated_proof_verify_empty_blob() {
        let agg = AggregatedProof {
            inner_proof_commitments: vec![[0u8; 32]],
            aggregated_blob: vec![],
            aggregation_system: ProofSystemId::STWO as u16,
            aggregated_public_input_root: [0u8; 32],
            proof_count: 1,
        };
        assert!(matches!(
            agg.verify_structure(),
            Err(PceError::MalformedProof(_))
        ));
    }

    #[test]
    fn test_aggregated_proof_verify_unsupported_system() {
        let agg = AggregatedProof {
            inner_proof_commitments: vec![[0u8; 32]],
            aggregated_blob: vec![1, 2, 3],
            aggregation_system: 0x0099,
            aggregated_public_input_root: [0u8; 32],
            proof_count: 1,
        };
        assert!(matches!(
            agg.verify_structure(),
            Err(PceError::UnsupportedSystem(0x0099))
        ));
    }

    #[test]
    fn test_aggregate_deterministic_root() {
        let pces = vec![make_pce(&[1], [0xAAu8; 32]), make_pce(&[2], [0xBBu8; 32])];
        let agg1 = aggregate_proofs(&pces, ProofSystemId::STWO).unwrap();
        let agg2 = aggregate_proofs(&pces, ProofSystemId::STWO).unwrap();
        assert_eq!(
            agg1.aggregated_public_input_root,
            agg2.aggregated_public_input_root
        );
    }
}
