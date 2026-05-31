//! Deterministic Proof System trait (RFC-0854 §1)
//!
//! The core abstraction for proof system backends. All proof system
//! implementations MUST implement this trait to be usable within
//! the CipherOcto protocol.

use super::error::DpsError;
use super::suite::ProofCircuitModel;

/// Core trait for deterministic proof systems (RFC-0854 §1).
///
/// Defines the interface that all proof backends (STARK, PLONK, Halo2, etc.)
/// must implement. Proof generation is Class C (probabilistic), but
/// verification MUST be Class A (deterministic across all implementations).
pub trait DeterministicProofSystem {
    /// The proof type produced by this system.
    type Proof: Clone + Send + Sync;

    /// The verification key type.
    type VerificationKey: Clone + Send + Sync;

    /// The public inputs type.
    type PublicInputs: Clone + Send + Sync;

    /// The witness type (private inputs for proving).
    type Witness: Clone + Send + Sync;

    /// Generate a proof given witness data, trace commitment, and public inputs.
    ///
    /// - `witness`: computation trace, intermediate values, randomness seed
    /// - `trace_commitment`: Merkle root of the computation trace
    /// - `public_inputs`: inputs visible to verifier
    ///
    /// Note: Proof generation is RFC-0008 Class C (probabilistic).
    fn prove(
        witness: &Self::Witness,
        trace_commitment: [u8; 32],
        public_inputs: &Self::PublicInputs,
    ) -> Result<Self::Proof, DpsError>;

    /// Verify a proof -- MUST be deterministic across all implementations.
    ///
    /// This is RFC-0008 Class A (protocol deterministic).
    fn verify(
        vk: &Self::VerificationKey,
        public_inputs: &Self::PublicInputs,
        proof: &Self::Proof,
    ) -> Result<bool, DpsError>;

    /// Compute proof commitment (hash of proof for Merkle trees).
    fn proof_commitment(proof: &Self::Proof) -> [u8; 32];

    /// Return the circuit model for this proof system.
    fn circuit_model() -> ProofCircuitModel;
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dps::suite::ProofCircuitModel;

    /// Minimal mock proof system for testing the trait.
    struct MockProofSystem;

    #[derive(Clone)]
    struct MockProof([u8; 32]);
    #[derive(Clone)]
    struct MockVk([u8; 32]);
    #[derive(Clone)]
    struct MockPublicInputs(Vec<u8>);
    #[derive(Clone)]
    struct MockWitness(Vec<u8>);

    impl DeterministicProofSystem for MockProofSystem {
        type Proof = MockProof;
        type VerificationKey = MockVk;
        type PublicInputs = MockPublicInputs;
        type Witness = MockWitness;

        fn prove(
            witness: &Self::Witness,
            trace_commitment: [u8; 32],
            _public_inputs: &Self::PublicInputs,
        ) -> Result<Self::Proof, DpsError> {
            // Mock: proof = BLAKE3(witness || trace_commitment)
            use blake3::Hasher;
            let mut h = Hasher::new();
            h.update(&witness.0);
            h.update(&trace_commitment);
            Ok(MockProof(*h.finalize().as_bytes()))
        }

        fn verify(
            _vk: &Self::VerificationKey,
            _public_inputs: &Self::PublicInputs,
            proof: &Self::Proof,
        ) -> Result<bool, DpsError> {
            // Mock: always valid if proof is non-zero
            Ok(proof.0 != [0u8; 32])
        }

        fn proof_commitment(proof: &Self::Proof) -> [u8; 32] {
            proof.0
        }

        fn circuit_model() -> ProofCircuitModel {
            ProofCircuitModel::AIR
        }
    }

    #[test]
    fn test_deterministic_proof_system_prove() {
        let witness = MockWitness(vec![1, 2, 3]);
        let public = MockPublicInputs(vec![4, 5, 6]);
        let trace = [0xAA; 32];

        let proof1 = MockProofSystem::prove(&witness, trace, &public).unwrap();
        let proof2 = MockProofSystem::prove(&witness, trace, &public).unwrap();
        assert_eq!(proof1.0, proof2.0, "prove must be deterministic");
    }

    #[test]
    fn test_deterministic_proof_system_verify() {
        let vk = MockVk([0u8; 32]);
        let public = MockPublicInputs(vec![4, 5, 6]);
        let proof = MockProof([0xBB; 32]);

        assert!(MockProofSystem::verify(&vk, &public, &proof).unwrap());
    }

    #[test]
    fn test_deterministic_proof_system_verify_rejects_zero() {
        let vk = MockVk([0u8; 32]);
        let public = MockPublicInputs(vec![4, 5, 6]);
        let proof = MockProof([0u8; 32]);

        assert!(!MockProofSystem::verify(&vk, &public, &proof).unwrap());
    }

    #[test]
    fn test_deterministic_proof_system_proof_commitment() {
        let proof = MockProof([0xCC; 32]);
        let commitment = MockProofSystem::proof_commitment(&proof);
        assert_eq!(commitment, [0xCC; 32]);
    }

    #[test]
    fn test_deterministic_proof_system_circuit_model() {
        assert_eq!(
            MockProofSystem::circuit_model(),
            ProofCircuitModel::AIR
        );
    }

    #[test]
    fn test_proof_generate_different_witnesses() {
        let witness1 = MockWitness(vec![1, 2, 3]);
        let witness2 = MockWitness(vec![4, 5, 6]);
        let public = MockPublicInputs(vec![7, 8, 9]);
        let trace = [0xAA; 32];

        let proof1 = MockProofSystem::prove(&witness1, trace, &public).unwrap();
        let proof2 = MockProofSystem::prove(&witness2, trace, &public).unwrap();
        assert_ne!(proof1.0, proof2.0, "different witnesses must produce different proofs");
    }

    #[test]
    fn test_proof_generate_different_trace_commitments() {
        let witness = MockWitness(vec![1, 2, 3]);
        let public = MockPublicInputs(vec![4, 5, 6]);

        let proof1 = MockProofSystem::prove(&witness, [0xAA; 32], &public).unwrap();
        let proof2 = MockProofSystem::prove(&witness, [0xBB; 32], &public).unwrap();
        assert_ne!(proof1.0, proof2.0, "different trace commitments must produce different proofs");
    }
}
