//! DpsError — error types for Deterministic Proof Substrate (RFC-0854 §10)
//!
//! All variants include contextual fields for diagnostics.

/// Errors specific to the DPS proof layer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DpsError {
    /// Proof signature verification failed
    InvalidSignature { proof_system: u16 },
    /// proof_commitment does not match proof_blob
    CommitmentMismatch {
        expected: [u8; 32],
        actual: [u8; 32],
    },
    /// Unsupported proof_system_id
    UnsupportedSystem { system_id: u16 },
    /// Proof blob failed parsing or structure validation
    MalformedProof { reason: &'static str },
    /// public_inputs do not match proof
    InputMismatch,
    /// Aggregation failure
    AggregationError { reason: &'static str },
    /// Witness generation failed
    WitnessGenerationFailed { reason: &'static str },
    /// Verification timeout
    VerificationTimeout { elapsed_ms: u64, limit_ms: u64 },
    /// Registry entry not found
    RegistryEntryNotFound { system_id: u16 },
    /// Invalid circuit model for this proof system
    InvalidCircuitModel {
        expected: &'static str,
        actual: &'static str,
    },
    /// Route trace does not match expected trace
    TraceMismatch {
        expected: [u8; 32],
        actual: [u8; 32],
    },
    /// Proof generation backend failed
    ProofGenerationFailed { reason: &'static str },
    /// Proof verification failed
    VerificationFailed { reason: &'static str },
    /// Verification key is invalid or malformed
    InvalidVerificationKey { reason: &'static str },
    /// Consensus boundary violation detected
    ConsensusBoundaryViolation { reason: &'static str },
    /// No backend registered for this proof system
    UnsupportedBackend { system_id: u16 },
}

impl std::fmt::Display for DpsError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidSignature { proof_system } => {
                write!(
                    f,
                    "Invalid signature for proof system {:#06x}",
                    proof_system
                )
            }
            Self::CommitmentMismatch { expected, actual } => {
                write!(
                    f,
                    "Commitment mismatch: expected {}, got {}",
                    hex8(expected),
                    hex8(actual)
                )
            }
            Self::UnsupportedSystem { system_id } => {
                write!(f, "Unsupported proof system: {:#06x}", system_id)
            }
            Self::MalformedProof { reason } => write!(f, "Malformed proof: {}", reason),
            Self::InputMismatch => write!(f, "Public input mismatch"),
            Self::AggregationError { reason } => write!(f, "Aggregation error: {}", reason),
            Self::WitnessGenerationFailed { reason } => {
                write!(f, "Witness generation failed: {}", reason)
            }
            Self::VerificationTimeout {
                elapsed_ms,
                limit_ms,
            } => {
                write!(
                    f,
                    "Verification timeout: {}ms exceeded {}ms limit",
                    elapsed_ms, limit_ms
                )
            }
            Self::RegistryEntryNotFound { system_id } => {
                write!(f, "Registry entry not found for system {:#06x}", system_id)
            }
            Self::InvalidCircuitModel { expected, actual } => {
                write!(
                    f,
                    "Invalid circuit model: expected {}, got {}",
                    expected, actual
                )
            }
            Self::TraceMismatch { expected, actual } => {
                write!(
                    f,
                    "Trace mismatch: expected {}, got {}",
                    hex8(expected),
                    hex8(actual)
                )
            }
            Self::ProofGenerationFailed { reason } => {
                write!(f, "Proof generation failed: {}", reason)
            }
            Self::VerificationFailed { reason } => {
                write!(f, "Verification failed: {}", reason)
            }
            Self::InvalidVerificationKey { reason } => {
                write!(f, "Invalid verification key: {}", reason)
            }
            Self::ConsensusBoundaryViolation { reason } => {
                write!(f, "Consensus boundary violation: {}", reason)
            }
            Self::UnsupportedBackend { system_id } => {
                write!(f, "No backend registered for system {:#06x}", system_id)
            }
        }
    }
}

impl std::error::Error for DpsError {}

fn hex8(b: &[u8; 32]) -> String {
    format!("{:02x}{:02x}..{:02x}{:02x}", b[0], b[1], b[30], b[31])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_error_display() {
        let e = DpsError::InvalidSignature {
            proof_system: 0x0001,
        };
        assert!(e.to_string().contains("0x0001"));
    }

    #[test]
    fn test_commitment_mismatch() {
        let e = DpsError::CommitmentMismatch {
            expected: [0xAA; 32],
            actual: [0xBB; 32],
        };
        assert!(e.to_string().contains("mismatch"));
    }

    #[test]
    fn test_unsupported_system() {
        let e = DpsError::UnsupportedSystem { system_id: 0xFFFF };
        assert!(e.to_string().contains("0xffff"));
    }

    #[test]
    fn test_malformed_proof() {
        let e = DpsError::MalformedProof {
            reason: "missing header",
        };
        assert!(e.to_string().contains("missing header"));
    }

    #[test]
    fn test_input_mismatch() {
        let e = DpsError::InputMismatch;
        assert!(e.to_string().contains("input"));
    }

    #[test]
    fn test_aggregation_error() {
        let e = DpsError::AggregationError {
            reason: "incompatible proof types",
        };
        assert!(e.to_string().contains("incompatible"));
    }

    #[test]
    fn test_witness_generation_failed() {
        let e = DpsError::WitnessGenerationFailed {
            reason: "missing private input",
        };
        assert!(e.to_string().contains("missing"));
    }

    #[test]
    fn test_verification_timeout() {
        let e = DpsError::VerificationTimeout {
            elapsed_ms: 5000,
            limit_ms: 500,
        };
        assert!(e.to_string().contains("5000"));
        assert!(e.to_string().contains("500"));
    }

    #[test]
    fn test_registry_entry_not_found() {
        let e = DpsError::RegistryEntryNotFound { system_id: 0x0099 };
        assert!(e.to_string().contains("0x0099"));
    }

    #[test]
    fn test_invalid_circuit_model() {
        let e = DpsError::InvalidCircuitModel {
            expected: "R1CS",
            actual: "AIR",
        };
        assert!(e.to_string().contains("R1CS"));
        assert!(e.to_string().contains("AIR"));
    }

    #[test]
    fn test_trace_mismatch() {
        let e = DpsError::TraceMismatch {
            expected: [0xAA; 32],
            actual: [0xBB; 32],
        };
        assert!(e.to_string().contains("Trace mismatch"));
    }

    #[test]
    fn test_proof_generation_failed() {
        let e = DpsError::ProofGenerationFailed {
            reason: "backend unavailable",
        };
        assert!(e.to_string().contains("backend unavailable"));
    }

    #[test]
    fn test_verification_failed() {
        let e = DpsError::VerificationFailed {
            reason: "invalid proof structure",
        };
        assert!(e.to_string().contains("invalid proof structure"));
    }

    #[test]
    fn test_invalid_verification_key() {
        let e = DpsError::InvalidVerificationKey {
            reason: "wrong length",
        };
        assert!(e.to_string().contains("wrong length"));
    }

    #[test]
    fn test_consensus_boundary_violation() {
        let e = DpsError::ConsensusBoundaryViolation {
            reason: "cross-boundary proof",
        };
        assert!(e.to_string().contains("cross-boundary proof"));
    }

    #[test]
    fn test_error_is_send_sync() {
        fn assert_send_sync<T: Send + Sync>() {}
        assert_send_sync::<DpsError>();
    }
}
