//! Recursive proof aggregation — RFC-0854 §8

use crate::dps::suite::ProofSystemId;
use crate::dps::DpsError;

/// Aggregation method identifiers.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(u16)]
pub enum AggregationMethod {
    /// Recursive STARK composition
    Recursive = 0x0001,
    /// PLONK inner/outer composition
    PLONKCompose = 0x0002,
    /// STARK FRI folding
    StarkFri = 0x0003,
}

/// An aggregated proof combining multiple constituent proofs.
///
/// RFC-0854 §8 / RFC-0859 §7.
#[derive(Debug, Clone)]
#[repr(C)]
pub struct AggregatedProof {
    /// Proof system used for aggregation
    pub aggregation_system: ProofSystemId,
    /// Aggregation method
    pub method: AggregationMethod,
    /// Number of constituent proofs
    pub proof_count: u32,
    /// Merkle root of constituent proof commitments
    pub constituent_root: [u8; 32],
    /// Aggregated proof blob
    pub aggregated_blob: Vec<u8>,
    /// Aggregated public input root
    pub aggregated_public_input_root: [u8; 32],
}

impl AggregatedProof {
    /// Create a new aggregated proof.
    pub fn new(
        aggregation_system: ProofSystemId,
        method: AggregationMethod,
        constituent_root: [u8; 32],
        aggregated_blob: Vec<u8>,
        aggregated_public_input_root: [u8; 32],
        proof_count: u32,
    ) -> Self {
        Self {
            aggregation_system,
            method,
            proof_count,
            constituent_root,
            aggregated_blob,
            aggregated_public_input_root,
        }
    }

    /// Verify aggregated proof commitment.
    pub fn verify_blob_commitment(&self, expected: &[u8; 32]) -> Result<(), DpsError> {
        use blake3::Hasher;
        let mut h = Hasher::new();
        h.update(&self.aggregated_blob);
        let actual = *h.finalize().as_bytes();
        if &actual != expected {
            return Err(DpsError::CommitmentMismatch {
                expected: *expected,
                actual,
            });
        }
        Ok(())
    }

    /// Serialize for commitment computation.
    pub fn to_commitment_bytes(&self) -> Vec<u8> {
        let mut buf = Vec::new();
        buf.extend_from_slice(&(self.aggregation_system as u16).to_le_bytes());
        buf.extend_from_slice(&(self.method as u16).to_le_bytes());
        buf.extend_from_slice(&self.proof_count.to_le_bytes());
        buf.extend_from_slice(&self.constituent_root);
        buf.extend_from_slice(&self.aggregated_public_input_root);
        buf
    }

    /// Compute commitment over this aggregated proof.
    pub fn compute_commitment(&self) -> [u8; 32] {
        use blake3::Hasher;
        let mut h = Hasher::new();
        h.update(&self.to_commitment_bytes());
        h.update(&self.aggregated_blob);
        *h.finalize().as_bytes()
    }
}

/// Recursive proof aggregator — builds a Merkle tree of constituent proofs.
pub struct RecursiveAggregator {
    /// Constituent proof commitments
    leaves: Vec<[u8; 32]>,
    /// Aggregation system to use
    aggregation_system: ProofSystemId,
    /// Aggregation method
    method: AggregationMethod,
}

impl RecursiveAggregator {
    /// Create a new aggregator.
    pub fn new(aggregation_system: ProofSystemId, method: AggregationMethod) -> Self {
        Self {
            leaves: Vec::new(),
            aggregation_system,
            method,
        }
    }

    /// Add a proof commitment to the aggregation.
    pub fn add_proof(&mut self, commitment: [u8; 32]) {
        self.leaves.push(commitment);
    }

    /// Number of constituent proofs.
    pub fn len(&self) -> usize {
        self.leaves.len()
    }

    /// Check if aggregator is empty.
    pub fn is_empty(&self) -> bool {
        self.leaves.is_empty()
    }

    /// Compute the Merkle root of constituent proofs.
    /// Uses BLAKE3-256 with binary tree construction.
    pub fn compute_constituent_root(&self) -> [u8; 32] {
        if self.leaves.is_empty() {
            return [0u8; 32];
        }
        let mut level = self.leaves.clone();
        while level.len() > 1 {
            let mut next = Vec::with_capacity((level.len() + 1) / 2);
            for chunk in level.chunks(2) {
                use blake3::Hasher;
                let mut h = Hasher::new();
                h.update(&chunk[0]);
                if chunk.len() > 1 {
                    h.update(&chunk[1]);
                } else {
                    // Duplicate last leaf for odd count
                    h.update(&chunk[0]);
                }
                next.push(*h.finalize().as_bytes());
            }
            level = next;
        }
        level[0]
    }

    /// Build the aggregated proof (placeholder — actual proving is backend-specific).
    pub fn build(&self) -> Result<AggregatedProof, DpsError> {
        if self.leaves.is_empty() {
            return Err(DpsError::AggregationError {
                reason: "no constituent proofs",
            });
        }
        let root = self.compute_constituent_root();
        Ok(AggregatedProof::new(
            self.aggregation_system,
            self.method,
            root,
            Vec::new(), // blob filled by backend
            [0u8; 32],  // public input root filled by caller
            self.leaves.len() as u32,
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_aggregation_method_variants() {
        assert_eq!(AggregationMethod::Recursive as u16, 0x0001);
        assert_eq!(AggregationMethod::PLONKCompose as u16, 0x0002);
        assert_eq!(AggregationMethod::StarkFri as u16, 0x0003);
    }

    #[test]
    fn test_aggregated_proof_commitment_bytes() {
        let ap = AggregatedProof::new(
            ProofSystemId::STWO,
            AggregationMethod::Recursive,
            [0xAA; 32],
            vec![1, 2, 3],
            [0xBB; 32],
            5,
        );
        let bytes = ap.to_commitment_bytes();
        assert!(!bytes.is_empty());
    }

    #[test]
    fn test_aggregated_proof_compute_commitment() {
        let ap = AggregatedProof::new(
            ProofSystemId::STWO,
            AggregationMethod::Recursive,
            [0xAA; 32],
            vec![1, 2, 3],
            [0xBB; 32],
            5,
        );
        let c1 = ap.compute_commitment();
        let c2 = ap.compute_commitment();
        assert_eq!(c1, c2);
    }

    #[test]
    fn test_aggregator_empty() {
        let agg = RecursiveAggregator::new(ProofSystemId::STWO, AggregationMethod::Recursive);
        assert!(agg.is_empty());
        assert_eq!(agg.len(), 0);
    }

    #[test]
    fn test_aggregator_single_proof() {
        let mut agg = RecursiveAggregator::new(ProofSystemId::STWO, AggregationMethod::Recursive);
        agg.add_proof([0x01; 32]);
        assert_eq!(agg.len(), 1);
        let root = agg.compute_constituent_root();
        assert_eq!(root, [0x01; 32]); // single leaf = root
    }

    #[test]
    fn test_aggregator_two_proofs() {
        let mut agg = RecursiveAggregator::new(ProofSystemId::STWO, AggregationMethod::Recursive);
        agg.add_proof([0x01; 32]);
        agg.add_proof([0x02; 32]);
        let root = agg.compute_constituent_root();
        assert_ne!(root, [0x01; 32]); // hashed, not raw
        assert_ne!(root, [0x02; 32]);
    }

    #[test]
    fn test_aggregator_deterministic() {
        let mut agg1 = RecursiveAggregator::new(ProofSystemId::STWO, AggregationMethod::Recursive);
        agg1.add_proof([0x01; 32]);
        agg1.add_proof([0x02; 32]);
        agg1.add_proof([0x03; 32]);

        let mut agg2 = RecursiveAggregator::new(ProofSystemId::STWO, AggregationMethod::Recursive);
        agg2.add_proof([0x01; 32]);
        agg2.add_proof([0x02; 32]);
        agg2.add_proof([0x03; 32]);

        assert_eq!(
            agg1.compute_constituent_root(),
            agg2.compute_constituent_root()
        );
    }

    #[test]
    fn test_aggregator_odd_count() {
        let mut agg = RecursiveAggregator::new(ProofSystemId::STWO, AggregationMethod::Recursive);
        agg.add_proof([0x01; 32]);
        agg.add_proof([0x02; 32]);
        agg.add_proof([0x03; 32]);
        let root = agg.compute_constituent_root();
        assert_ne!(root, [0u8; 32]); // should produce a valid root
    }

    #[test]
    fn test_aggregator_empty_root() {
        let agg = RecursiveAggregator::new(ProofSystemId::STWO, AggregationMethod::Recursive);
        assert_eq!(agg.compute_constituent_root(), [0u8; 32]);
    }

    #[test]
    fn test_aggregator_build_empty() {
        let agg = RecursiveAggregator::new(ProofSystemId::STWO, AggregationMethod::Recursive);
        assert!(agg.build().is_err());
    }

    #[test]
    fn test_aggregator_build_ok() {
        let mut agg = RecursiveAggregator::new(ProofSystemId::STWO, AggregationMethod::Recursive);
        agg.add_proof([0x01; 32]);
        let ap = agg.build().unwrap();
        assert_eq!(ap.proof_count, 1);
        assert_eq!(ap.aggregation_system, ProofSystemId::STWO);
    }

    #[test]
    fn test_aggregated_proof_verify_blob_commitment() {
        let blob = vec![1, 2, 3, 4, 5];
        let mut ap = AggregatedProof::new(
            ProofSystemId::STWO,
            AggregationMethod::Recursive,
            [0xAA; 32],
            blob,
            [0xBB; 32],
            1,
        );
        // Compute correct commitment
        use blake3::Hasher;
        let mut h = Hasher::new();
        h.update(&ap.aggregated_blob);
        let expected = *h.finalize().as_bytes();
        assert!(ap.verify_blob_commitment(&expected).is_ok());

        // Tamper
        ap.aggregated_blob = vec![99, 99];
        assert!(ap.verify_blob_commitment(&expected).is_err());
    }
}
