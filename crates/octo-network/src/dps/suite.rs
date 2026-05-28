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
    zkVM = 0x0004,
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
    fn test_aggregation_flags() {
        assert_eq!(AGG_NONE, 0x00);
        assert_eq!(AGG_RECURSIVE, 0x01);
        assert_eq!(AGG_PLONK_COMPOSE, 0x02);
        assert_eq!(AGG_STARK_FRI, 0x04);
        // Can be combined
        assert_eq!(AGG_RECURSIVE | AGG_STARK_FRI, 0x05);
    }
}
