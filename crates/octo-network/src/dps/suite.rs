//! Proof system identification and circuit model enums (RFC-0854 §3)

/// Proof system identifiers — 8 supported backends.
///
/// Matches RFC-0859 ProofSystemId exactly.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(u16)]
pub enum ProofSystemId {
    STWO = 0x0001,
    RiscZero = 0x0002,
    SP1 = 0x0003,
    Winterfell = 0x0004,
    Halo2 = 0x0005,
    Groth16 = 0x0006,
    PLONK = 0x0007,
    Cairo = 0x0008,
    // 0x0009-0xFFFF: reserved for future backends
}

impl ProofSystemId {
    /// Try to convert from u16 value.
    pub fn from_u16(v: u16) -> Option<Self> {
        match v {
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

    /// Return the u16 discriminant.
    pub fn as_u16(self) -> u16 {
        self as u16
    }
}

/// Circuit model classification (RFC-0854 §3.2).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(u16)]
pub enum ProofCircuitModel {
    AIR = 0x0001,
    R1CS = 0x0002,
    PLONKISH = 0x0003,
    ZkVm = 0x0004,
    Recursive = 0x0005,
}

/// Execution class from RFC-0008.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(u16)]
pub enum ProofExecutionClass {
    /// Deterministic with canonical algorithms
    ClassA = 0x0001,
    /// Deterministic off-chain with canonical kernels
    ClassB = 0x0002,
    /// Probabilistic / human-in-the-loop
    ClassC = 0x0003,
}

/// Proof suite 4-field composite key (RFC-0854 §3).
///
/// Uniquely identifies a proof suite by its proof system, field, hash, and recursion scheme.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ProofSuiteId {
    /// Proof system identifier
    pub proof_system: u16,
    /// Field identifier
    pub field_id: u16,
    /// Hash identifier
    pub hash_id: u16,
    /// Recursion scheme identifier
    pub recursion_scheme: u16,
}

impl ProofSuiteId {
    /// Create a new proof suite ID.
    pub fn new(proof_system: u16, field_id: u16, hash_id: u16, recursion_scheme: u16) -> Self {
        Self {
            proof_system,
            field_id,
            hash_id,
            recursion_scheme,
        }
    }

    /// Compute BLAKE3-256 hash of this composite key.
    pub fn to_hash(&self) -> [u8; 32] {
        use blake3::Hasher;
        let mut h = Hasher::new();
        h.update(&self.proof_system.to_be_bytes());
        h.update(&self.field_id.to_be_bytes());
        h.update(&self.hash_id.to_be_bytes());
        h.update(&self.recursion_scheme.to_be_bytes());
        *h.finalize().as_bytes()
    }
}

/// A registered proof suite with its capabilities.
#[derive(Debug, Clone)]
pub struct ProofSuite {
    /// Unique backend identifier
    pub system_id: ProofSystemId,
    /// Circuit model supported by this backend
    pub circuit_model: ProofCircuitModel,
    /// Execution class for this proof type
    pub execution_class: ProofExecutionClass,
    /// Maximum verification latency in milliseconds
    pub max_verification_latency_ms: u64,
    /// Supported aggregation methods (bitmask)
    pub aggregation_support: u8,
}

/// Aggregation capability flags for ProofSuite.
pub const AGG_NONE: u8 = 0x00;
pub const AGG_RECURSIVE: u8 = 0x01;
pub const AGG_PLONK_COMPOSE: u8 = 0x02;
pub const AGG_STARK_FRI: u8 = 0x04;

impl ProofSuite {
    /// Create a new proof suite.
    pub fn new(
        system_id: ProofSystemId,
        circuit_model: ProofCircuitModel,
        execution_class: ProofExecutionClass,
    ) -> Self {
        Self {
            system_id,
            circuit_model,
            execution_class,
            max_verification_latency_ms: 5000,
            aggregation_support: AGG_NONE,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_proof_system_id_from_u16() {
        assert_eq!(ProofSystemId::from_u16(0x0001), Some(ProofSystemId::STWO));
        assert_eq!(ProofSystemId::from_u16(0x0008), Some(ProofSystemId::Cairo));
        assert_eq!(ProofSystemId::from_u16(0x0099), None);
        assert_eq!(ProofSystemId::from_u16(0x0000), None);
    }

    #[test]
    fn test_proof_system_id_as_u16() {
        assert_eq!(ProofSystemId::STWO.as_u16(), 0x0001);
        assert_eq!(ProofSystemId::Cairo.as_u16(), 0x0008);
    }

    #[test]
    fn test_proof_system_id_roundtrip() {
        for i in 1..=8u16 {
            let id = ProofSystemId::from_u16(i).unwrap();
            assert_eq!(id.as_u16(), i);
        }
    }

    #[test]
    fn test_proof_circuit_model_variants() {
        assert_eq!(ProofCircuitModel::AIR as u16, 0x0001);
        assert_eq!(ProofCircuitModel::Recursive as u16, 0x0005);
    }

    #[test]
    fn test_proof_execution_class_variants() {
        assert_eq!(ProofExecutionClass::ClassA as u16, 0x0001);
        assert_eq!(ProofExecutionClass::ClassC as u16, 0x0003);
    }

    #[test]
    fn test_proof_suite_new() {
        let suite = ProofSuite::new(
            ProofSystemId::STWO,
            ProofCircuitModel::AIR,
            ProofExecutionClass::ClassA,
        );
        assert_eq!(suite.system_id, ProofSystemId::STWO);
        assert_eq!(suite.max_verification_latency_ms, 5000);
        assert_eq!(suite.aggregation_support, AGG_NONE);
    }

    #[test]
    fn test_proof_suite_id_new() {
        let id = ProofSuiteId::new(0x0001, 0x0002, 0x0003, 0x0004);
        assert_eq!(id.proof_system, 0x0001);
        assert_eq!(id.field_id, 0x0002);
        assert_eq!(id.hash_id, 0x0003);
        assert_eq!(id.recursion_scheme, 0x0004);
    }

    #[test]
    fn test_proof_suite_id_to_hash_deterministic() {
        let id = ProofSuiteId::new(0x0001, 0x0002, 0x0003, 0x0004);
        let h1 = id.to_hash();
        let h2 = id.to_hash();
        assert_eq!(h1, h2);
    }

    #[test]
    fn test_proof_suite_id_to_hash_different_inputs() {
        let id1 = ProofSuiteId::new(0x0001, 0x0002, 0x0003, 0x0004);
        let id2 = ProofSuiteId::new(0x0001, 0x0002, 0x0003, 0x0005);
        assert_ne!(id1.to_hash(), id2.to_hash());
    }

    #[test]
    fn test_proof_suite_id_equality() {
        let id1 = ProofSuiteId::new(1, 2, 3, 4);
        let id2 = ProofSuiteId::new(1, 2, 3, 4);
        let id3 = ProofSuiteId::new(1, 2, 3, 5);
        assert_eq!(id1, id2);
        assert_ne!(id1, id3);
    }

    #[test]
    fn test_aggregation_flags() {
        assert_eq!(AGG_NONE, 0x00);
        assert_eq!(AGG_RECURSIVE, 0x01);
        assert_eq!(AGG_PLONK_COMPOSE, 0x02);
        assert_eq!(AGG_STARK_FRI, 0x04);
        // Can be combined
        assert_eq!(AGG_RECURSIVE | AGG_STARK_FRI, 0x05);
    }
}
