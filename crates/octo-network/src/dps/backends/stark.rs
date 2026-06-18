//! STARK (STWO) Backend Implementation (RFC-0854 §3)
//!
//! StarkWare's STWO is a STARK prover with:
//! - Transparent (no trusted setup)
//! - AIR (Algebraic Intermediate Representation) constraints
//! - SIMD-optimized proving
//! - Massive parallelism
//!
//! STARK properties:
//! - Post-quantum secure (hash-based, no elliptic curves)
//! - Transparent setup (no toxic waste)
//! - Large proof sizes (~50-200 KB)
//! - Fast verification (~1-10 ms)

use super::super::error::DpsError;
use super::super::suite::{ProofCircuitModel, ProofSystemId};
use super::super::trait_def::DeterministicProofSystem;
use super::BackendEntry;

/// STARK (STWO) proof — serialized AIR proof bytes.
#[derive(Clone, Debug)]
pub struct StarkProof {
    /// Serialized proof bytes
    pub proof_bytes: Vec<u8>,
    /// Proof system identifier
    pub system_id: u16,
}

/// STARK verification key — commitment to the AIR constraint system.
#[derive(Clone, Debug)]
pub struct StarkVerificationKey {
    /// AIR constraint commitment
    pub air_commitment: [u8; 32],
    /// Verification key bytes
    pub vk_bytes: Vec<u8>,
}

/// STARK public inputs — committed values visible to verifier.
#[derive(Clone, Debug)]
pub struct StarkPublicInputs {
    /// Public input values (serialized)
    pub values: Vec<u8>,
    /// Trace commitment (Merkle root of execution trace)
    pub trace_commitment: [u8; 32],
}

/// STARK witness — private computation trace.
#[derive(Clone, Debug)]
pub struct StarkWitness {
    /// Execution trace (serialized AIR columns)
    pub trace: Vec<u8>,
    /// Randomness seed for Fiat-Shamir
    pub randomness_seed: [u8; 32],
}

/// STARK (STWO) proof system backend.
///
/// Implements `DeterministicProofSystem` for STARK proofs.
/// Proof generation is Class C (probabilistic); verification is Class A.
pub struct StarkBackend;

impl DeterministicProofSystem for StarkBackend {
    type Proof = StarkProof;
    type VerificationKey = StarkVerificationKey;
    type PublicInputs = StarkPublicInputs;
    type Witness = StarkWitness;

    fn prove(
        witness: &Self::Witness,
        trace_commitment: [u8; 32],
        public_inputs: &Self::PublicInputs,
    ) -> Result<Self::Proof, DpsError> {
        // Validate inputs
        if witness.trace.is_empty() {
            return Err(DpsError::WitnessGenerationFailed {
                reason: "empty trace",
            });
        }
        if public_inputs.values.is_empty() {
            return Err(DpsError::WitnessGenerationFailed {
                reason: "empty public inputs",
            });
        }

        // Verify trace commitment matches
        let computed_commitment = compute_trace_commitment(&witness.trace);
        if computed_commitment != trace_commitment {
            return Err(DpsError::TraceMismatch {
                expected: trace_commitment,
                actual: computed_commitment,
            });
        }

        // STARK proof generation (simplified):
        // In production, this would invoke STWO's SIMD-optimized prover.
        // Here we produce a deterministic proof blob from the trace + public inputs.
        let proof_bytes = generate_stark_proof_bytes(
            &witness.trace,
            &public_inputs.values,
            &witness.randomness_seed,
        );

        Ok(StarkProof {
            proof_bytes,
            system_id: ProofSystemId::STWO as u16,
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

        // STARK verification (simplified):
        // In production, this would invoke STWO's O(1) verifier.
        // Here we verify structural integrity.
        // Verification is Class A — deterministic across all implementations.
        Ok(proof.system_id == ProofSystemId::STWO as u16 && proof.proof_bytes.len() >= 32)
    }

    fn proof_commitment(proof: &Self::Proof) -> [u8; 32] {
        *blake3::hash(&proof.proof_bytes).as_bytes()
    }

    fn circuit_model() -> ProofCircuitModel {
        ProofCircuitModel::AIR
    }
}

/// Compute trace commitment: BLAKE3-256(trace_bytes).
fn compute_trace_commitment(trace: &[u8]) -> [u8; 32] {
    *blake3::hash(trace).as_bytes()
}

/// Generate STARK proof bytes (deterministic stub).
///
/// In production, this invokes STWO's SIMD-optimized prover.
/// Here we produce a deterministic proof blob for structural testing.
fn generate_stark_proof_bytes(trace: &[u8], public_inputs: &[u8], seed: &[u8; 32]) -> Vec<u8> {
    let mut hasher = blake3::Hasher::new();
    hasher.update(seed);
    hasher.update(trace);
    hasher.update(public_inputs);
    hasher.update(b"stark:proof:v1");
    let hash = hasher.finalize();
    // Produce a 256-byte proof blob (typical STARK proof is 50-200 KB)
    let mut proof = vec![0u8; 256];
    proof[..32].copy_from_slice(hash.as_bytes());
    proof
}

/// Get the backend entry for the STARK (STWO) backend.
pub fn stark_backend_entry() -> BackendEntry {
    BackendEntry {
        system_id: ProofSystemId::STWO,
        circuit_model: ProofCircuitModel::AIR,
        name: "STARK (STWO)",
        properties:
            "transparent (no trusted setup), AIR constraints, SIMD-optimized, post-quantum secure",
        typical_verify_us: 5000,  // ~5ms
        typical_proof_size: 1024, // ~1KB stub (real: 50-200KB)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_witness(trace: &[u8]) -> StarkWitness {
        StarkWitness {
            trace: trace.to_vec(),
            randomness_seed: [0x42; 32],
        }
    }

    fn make_public_inputs(values: &[u8]) -> StarkPublicInputs {
        StarkPublicInputs {
            values: values.to_vec(),
            trace_commitment: [0u8; 32], // will be overridden
        }
    }

    #[test]
    fn test_stark_prove_verify() {
        let trace = b"execution trace data";
        let commitment = compute_trace_commitment(trace);
        let witness = make_witness(trace);
        let public_inputs = StarkPublicInputs {
            values: vec![1, 2, 3],
            trace_commitment: commitment,
        };

        let proof = StarkBackend::prove(&witness, commitment, &public_inputs).unwrap();
        let vk = StarkVerificationKey {
            air_commitment: [0u8; 32],
            vk_bytes: vec![],
        };
        assert!(StarkBackend::verify(&vk, &public_inputs, &proof).unwrap());
    }

    #[test]
    fn test_stark_prove_deterministic() {
        let trace = b"deterministic trace";
        let commitment = compute_trace_commitment(trace);
        let witness = make_witness(trace);
        let public_inputs = make_public_inputs(b"inputs");

        let p1 = StarkBackend::prove(&witness, commitment, &public_inputs).unwrap();
        let p2 = StarkBackend::prove(&witness, commitment, &public_inputs).unwrap();
        assert_eq!(p1.proof_bytes, p2.proof_bytes);
    }

    #[test]
    fn test_stark_prove_empty_trace_fails() {
        let witness = make_witness(b"");
        let public_inputs = make_public_inputs(b"inputs");
        assert!(StarkBackend::prove(&witness, [0u8; 32], &public_inputs).is_err());
    }

    #[test]
    fn test_stark_prove_trace_mismatch() {
        let witness = make_witness(b"actual trace");
        let wrong_commitment = [0xFF; 32]; // doesn't match
        let public_inputs = make_public_inputs(b"inputs");
        assert!(StarkBackend::prove(&witness, wrong_commitment, &public_inputs).is_err());
    }

    #[test]
    fn test_stark_verify_empty_proof() {
        let proof = StarkProof {
            proof_bytes: vec![],
            system_id: ProofSystemId::STWO as u16,
        };
        let vk = StarkVerificationKey {
            air_commitment: [0u8; 32],
            vk_bytes: vec![],
        };
        let public_inputs = make_public_inputs(b"inputs");
        assert!(!StarkBackend::verify(&vk, &public_inputs, &proof).unwrap());
    }

    #[test]
    fn test_stark_proof_commitment() {
        let proof = StarkProof {
            proof_bytes: vec![1, 2, 3, 4, 5],
            system_id: ProofSystemId::STWO as u16,
        };
        let c1 = StarkBackend::proof_commitment(&proof);
        let c2 = StarkBackend::proof_commitment(&proof);
        assert_eq!(c1, c2);
        assert_ne!(c1, [0u8; 32]);
    }

    #[test]
    fn test_stark_circuit_model() {
        assert_eq!(StarkBackend::circuit_model(), ProofCircuitModel::AIR);
    }

    #[test]
    fn test_stark_backend_entry() {
        let entry = stark_backend_entry();
        assert_eq!(entry.system_id, ProofSystemId::STWO);
        assert_eq!(entry.name, "STARK (STWO)");
        assert!(entry.properties.contains("transparent"));
    }

    #[test]
    fn test_stark_verify_wrong_system() {
        let proof = StarkProof {
            proof_bytes: vec![1, 2, 3],
            system_id: ProofSystemId::PLONK as u16, // wrong system
        };
        let vk = StarkVerificationKey {
            air_commitment: [0u8; 32],
            vk_bytes: vec![],
        };
        let public_inputs = make_public_inputs(b"inputs");
        assert!(!StarkBackend::verify(&vk, &public_inputs, &proof).unwrap());
    }
}
