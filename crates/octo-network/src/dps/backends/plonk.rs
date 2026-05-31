//! PLONK Backend Implementation (RFC-0854 §3)
//!
//! PLONK proof system with:
//! - Universal trusted setup (one ceremony for all circuits)
//! - PLONKish customizable gate constraints
//! - Succinct proofs (~500 bytes)
//! - Fast verification (~1-5 ms)

use super::super::error::DpsError;
use super::super::suite::{ProofCircuitModel, ProofSystemId};
use super::super::trait_def::DeterministicProofSystem;
use super::BackendEntry;

/// PLONK proof — serialized PLONK proof bytes.
#[derive(Clone, Debug)]
pub struct PlonkProof {
    /// Serialized proof bytes
    pub proof_bytes: Vec<u8>,
    /// Proof system identifier
    pub system_id: u16,
}

/// PLONK verification key — commitment to the circuit.
#[derive(Clone, Debug)]
pub struct PlonkVerificationKey {
    /// Circuit commitment
    pub circuit_commitment: [u8; 32],
    /// Verification key bytes
    pub vk_bytes: Vec<u8>,
}

/// PLONK public inputs — committed values visible to verifier.
#[derive(Clone, Debug)]
pub struct PlonkPublicInputs {
    /// Public input values (serialized)
    pub values: Vec<u8>,
    /// Public input commitment
    pub input_commitment: [u8; 32],
}

/// PLONK witness — private inputs to the circuit.
#[derive(Clone, Debug)]
pub struct PlonkWitness {
    /// Private input values (serialized)
    pub private_inputs: Vec<u8>,
    /// Circuit assignment
    pub assignment: Vec<u8>,
}

/// PLONK proof system backend.
///
/// Implements `DeterministicProofSystem` for PLONK proofs.
/// Proof generation is Class C; verification is Class A.
pub struct PlonkBackend;

impl DeterministicProofSystem for PlonkBackend {
    type Proof = PlonkProof;
    type VerificationKey = PlonkVerificationKey;
    type PublicInputs = PlonkPublicInputs;
    type Witness = PlonkWitness;

    fn prove(
        witness: &Self::Witness,
        trace_commitment: [u8; 32],
        public_inputs: &Self::PublicInputs,
    ) -> Result<Self::Proof, DpsError> {
        if witness.private_inputs.is_empty() {
            return Err(DpsError::WitnessGenerationFailed {
                reason: "empty private inputs",
            });
        }
        if public_inputs.values.is_empty() {
            return Err(DpsError::WitnessGenerationFailed {
                reason: "empty public inputs",
            });
        }

        // Verify trace commitment
        let computed = compute_assignment_commitment(&witness.assignment);
        if computed != trace_commitment {
            return Err(DpsError::TraceMismatch {
                expected: trace_commitment,
                actual: computed,
            });
        }

        // PLONK proof generation (simplified):
        // In production, this would invoke a PLONK prover with PLONKish circuits.
        let proof_bytes =
            generate_plonk_proof_bytes(&witness.private_inputs, &public_inputs.values);

        Ok(PlonkProof {
            proof_bytes,
            system_id: ProofSystemId::PLONK as u16,
        })
    }

    fn verify(
        _vk: &Self::VerificationKey,
        public_inputs: &Self::PublicInputs,
        proof: &Self::Proof,
    ) -> Result<bool, DpsError> {
        if proof.proof_bytes.is_empty() {
            return Ok(false);
        }
        if public_inputs.values.is_empty() {
            return Ok(false);
        }

        // PLONK verification (simplified):
        // In production, this would invoke the PLONK verifier.
        // Verification is Class A — deterministic.
        Ok(proof.system_id == ProofSystemId::PLONK as u16 && proof.proof_bytes.len() >= 32)
    }

    fn proof_commitment(proof: &Self::Proof) -> [u8; 32] {
        *blake3::hash(&proof.proof_bytes).as_bytes()
    }

    fn circuit_model() -> ProofCircuitModel {
        ProofCircuitModel::PLONKISH
    }
}

/// Compute assignment commitment: BLAKE3-256(assignment_bytes).
fn compute_assignment_commitment(assignment: &[u8]) -> [u8; 32] {
    *blake3::hash(assignment).as_bytes()
}

/// Generate PLONK proof bytes (deterministic stub).
///
/// In production, this invokes a PLONK prover.
/// Here we produce a deterministic proof blob for structural testing.
fn generate_plonk_proof_bytes(private_inputs: &[u8], public_inputs: &[u8]) -> Vec<u8> {
    let mut hasher = blake3::Hasher::new();
    hasher.update(private_inputs);
    hasher.update(public_inputs);
    hasher.update(b"plonk:proof:v1");
    let hash = hasher.finalize();
    // Produce a 128-byte proof blob (typical PLONK proof is ~500 bytes)
    let mut proof = vec![0u8; 128];
    proof[..32].copy_from_slice(hash.as_bytes());
    proof
}

/// Get the backend entry for the PLONK backend.
pub fn plonk_backend_entry() -> BackendEntry {
    BackendEntry {
        system_id: ProofSystemId::PLONK,
        circuit_model: ProofCircuitModel::PLONKISH,
        name: "PLONK",
        properties: "Universal setup, PLONKish circuits, succinct proofs (~500 bytes)",
        typical_verify_us: 2000, // ~2ms
        typical_proof_size: 128, // stub (real: ~500 bytes)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_witness(private: &[u8]) -> PlonkWitness {
        PlonkWitness {
            private_inputs: private.to_vec(),
            assignment: private.to_vec(), // simplified
        }
    }

    fn make_public_inputs(values: &[u8]) -> PlonkPublicInputs {
        PlonkPublicInputs {
            values: values.to_vec(),
            input_commitment: [0u8; 32],
        }
    }

    #[test]
    fn test_plonk_prove_verify() {
        let assignment = b"circuit assignment";
        let commitment = compute_assignment_commitment(assignment);
        let witness = PlonkWitness {
            private_inputs: vec![1, 2, 3],
            assignment: assignment.to_vec(),
        };
        let public_inputs = make_public_inputs(b"inputs");

        let proof = PlonkBackend::prove(&witness, commitment, &public_inputs).unwrap();
        let vk = PlonkVerificationKey {
            circuit_commitment: [0u8; 32],
            vk_bytes: vec![],
        };
        assert!(PlonkBackend::verify(&vk, &public_inputs, &proof).unwrap());
    }

    #[test]
    fn test_plonk_prove_deterministic() {
        let assignment = b"det assignment";
        let commitment = compute_assignment_commitment(assignment);
        let witness = PlonkWitness {
            private_inputs: vec![1, 2, 3],
            assignment: assignment.to_vec(),
        };
        let public_inputs = make_public_inputs(b"inputs");

        let p1 = PlonkBackend::prove(&witness, commitment, &public_inputs).unwrap();
        let p2 = PlonkBackend::prove(&witness, commitment, &public_inputs).unwrap();
        assert_eq!(p1.proof_bytes, p2.proof_bytes);
    }

    #[test]
    fn test_plonk_prove_empty_inputs_fails() {
        let witness = make_witness(b"");
        let public_inputs = make_public_inputs(b"inputs");
        assert!(PlonkBackend::prove(&witness, [0u8; 32], &public_inputs).is_err());
    }

    #[test]
    fn test_plonk_prove_trace_mismatch() {
        let witness = make_witness(b"assignment");
        let wrong_commitment = [0xFF; 32];
        let public_inputs = make_public_inputs(b"inputs");
        assert!(PlonkBackend::prove(&witness, wrong_commitment, &public_inputs).is_err());
    }

    #[test]
    fn test_plonk_verify_empty_proof() {
        let proof = PlonkProof {
            proof_bytes: vec![],
            system_id: ProofSystemId::PLONK as u16,
        };
        let vk = PlonkVerificationKey {
            circuit_commitment: [0u8; 32],
            vk_bytes: vec![],
        };
        let public_inputs = make_public_inputs(b"inputs");
        assert!(!PlonkBackend::verify(&vk, &public_inputs, &proof).unwrap());
    }

    #[test]
    fn test_plonk_proof_commitment() {
        let proof = PlonkProof {
            proof_bytes: vec![1, 2, 3, 4, 5],
            system_id: ProofSystemId::PLONK as u16,
        };
        let c1 = PlonkBackend::proof_commitment(&proof);
        let c2 = PlonkBackend::proof_commitment(&proof);
        assert_eq!(c1, c2);
        assert_ne!(c1, [0u8; 32]);
    }

    #[test]
    fn test_plonk_circuit_model() {
        assert_eq!(PlonkBackend::circuit_model(), ProofCircuitModel::PLONKISH);
    }

    #[test]
    fn test_plonk_backend_entry() {
        let entry = plonk_backend_entry();
        assert_eq!(entry.system_id, ProofSystemId::PLONK);
        assert_eq!(entry.name, "PLONK");
        assert!(entry.properties.contains("succinct"));
    }

    #[test]
    fn test_plonk_verify_wrong_system() {
        let proof = PlonkProof {
            proof_bytes: vec![1, 2, 3],
            system_id: ProofSystemId::STWO as u16, // wrong system
        };
        let vk = PlonkVerificationKey {
            circuit_commitment: [0u8; 32],
            vk_bytes: vec![],
        };
        let public_inputs = make_public_inputs(b"inputs");
        assert!(!PlonkBackend::verify(&vk, &public_inputs, &proof).unwrap());
    }

    #[test]
    fn test_plonk_different_inputs_different_proofs() {
        let assignment = b"assignment";
        let commitment = compute_assignment_commitment(assignment);
        let witness = PlonkWitness {
            private_inputs: vec![1, 2, 3],
            assignment: assignment.to_vec(),
        };
        let pi1 = make_public_inputs(b"inputs_a");
        let pi2 = make_public_inputs(b"inputs_b");

        let p1 = PlonkBackend::prove(&witness, commitment, &pi1).unwrap();
        let p2 = PlonkBackend::prove(&witness, commitment, &pi2).unwrap();
        assert_ne!(p1.proof_bytes, p2.proof_bytes);
    }
}
