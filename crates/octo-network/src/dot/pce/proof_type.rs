//! Proof system and circuit model enums (RFC-0859 §3.1, §4.1)

/// Proof system identifier — 8 backends (RFC-0859 §3.1)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(u16)]
pub enum ProofSystemId {
    /// StarkWare STWO — STARK prover, no trusted setup, SIMD-optimized
    STWO = 0x0001,
    /// RISC Zero — zkVM, RISC-V execution traces
    RiscZero = 0x0002,
    /// SP1 — zkVM, RISC-V, recursive proving
    SP1 = 0x0003,
    /// Winterfell — STARK prover by Meta, AIR-based
    Winterfell = 0x0004,
    /// Halo2 — SNARK, no trusted setup, IPA-based
    Halo2 = 0x0005,
    /// Groth16 — SNARK, smallest proofs, requires trusted setup
    Groth16 = 0x0006,
    /// PLONK — Universal SNARK, no per-circuit trusted setup
    PLONK = 0x0007,
    /// Cairo — StarkWare's native execution model
    Cairo = 0x0008,
}

impl ProofSystemId {
    /// Convert from u16 value to enum variant.
    pub fn from_u16(val: u16) -> Option<Self> {
        match val {
            0x0001 => Some(Self::STWO),
            0x0002 => Some(Self::RiscZero),
            0x0003 => Some(Self::SP1),
            0x0004 => Some(Self::Winterfell),
            0x0005 => Some(Self::Halo2),
            0x0006 => Some(Self::Groth16),
            0x0007 => Some(Self::PLONK),
            0x0008 => Some(Self::Cairo),
            _ => None,
        }
    }
}

/// Proof circuit model — circuit type classification (RFC-0859 §3.2)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(u16)]
pub enum ProofCircuitModel {
    /// AIR constraints (STARK-native)
    AIR = 0x0001,
    /// R1CS (rank-1 constraint system, SNARK-native)
    R1CS = 0x0002,
    /// PLONKish (customizable gate constraints)
    PLONKISH = 0x0003,
    /// zkVM (virtual machine execution trace)
    zkVM = 0x0004,
    /// Recursive composition of inner proofs
    Recursive = 0x0005,
}

/// Proof type classification (RFC-0859 §4.1)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(u16)]
pub enum ProofType {
    /// AI inference execution proof (RFC-0630)
    InferenceProof = 0x0001,
    /// Dataset integrity proof (RFC-0631)
    DatasetIntegrityProof = 0x0002,
    /// Mission execution correctness proof
    MissionExecutionProof = 0x0003,
    /// Relay behavior proof (RFC-0860)
    RelayProof = 0x0004,
    /// Validator attestation proof
    ValidatorAttestation = 0x0005,
    /// Aggregated recursive proof (RFC-0650)
    AggregatedProof = 0x0006,
    /// Membership proof
    MembershipProof = 0x0007,
    /// State transition proof
    StateTransitionProof = 0x0008,
    // 0x0009-0xFFFF: Reserved for future proof types
}

/// Result of proof verification (RFC-0859 §5.1)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum VerificationResult {
    /// Proof is valid — computation was performed correctly
    Valid = 0x00,
    /// Proof is invalid — verification failed
    Invalid = 0x01,
    /// Proof system not supported by this verifier
    UnsupportedSystem = 0x02,
    /// Proof blob is malformed
    MalformedProof = 0x03,
    /// Public inputs do not match commitment
    InputMismatch = 0x04,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_proof_system_id_repr() {
        assert_eq!(ProofSystemId::STWO as u16, 0x0001);
        assert_eq!(ProofSystemId::Cairo as u16, 0x0008);
    }

    #[test]
    fn test_proof_system_id_from_u16() {
        assert_eq!(ProofSystemId::from_u16(0x0001), Some(ProofSystemId::STWO));
        assert_eq!(ProofSystemId::from_u16(0x0008), Some(ProofSystemId::Cairo));
        assert_eq!(ProofSystemId::from_u16(0x0099), None);
    }

    #[test]
    fn test_proof_circuit_model_repr() {
        assert_eq!(ProofCircuitModel::AIR as u16, 0x0001);
        assert_eq!(ProofCircuitModel::Recursive as u16, 0x0005);
    }

    #[test]
    fn test_proof_type_repr() {
        assert_eq!(ProofType::InferenceProof as u16, 0x0001);
        assert_eq!(ProofType::StateTransitionProof as u16, 0x0008);
    }

    #[test]
    fn test_verification_result_repr() {
        assert_eq!(VerificationResult::Valid as u8, 0x00);
        assert_eq!(VerificationResult::InputMismatch as u8, 0x04);
    }

    #[test]
    fn test_verification_result_equality() {
        assert_eq!(VerificationResult::Valid, VerificationResult::Valid);
        assert_ne!(VerificationResult::Valid, VerificationResult::Invalid);
    }
}
