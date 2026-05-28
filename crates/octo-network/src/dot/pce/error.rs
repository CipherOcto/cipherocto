//! PCE Error Types (RFC-0859 §10)

use thiserror::Error;

/// PCE Error Enum — 6 variants
#[derive(Error, Debug)]
pub enum PceError {
    #[error("Proof signature verification failed: {0}")]
    InvalidSignature(String),

    #[error("proof_commitment does not match proof_blob")]
    CommitmentMismatch,

    #[error("Unsupported proof_system_id: {0:#06x}")]
    UnsupportedSystem(u16),

    #[error("Proof blob failed parsing or structure validation: {0}")]
    MalformedProof(String),

    #[error("public_inputs do not match commitment")]
    InputMismatch,

    #[error("Aggregation failure: {reason}")]
    AggregationError { reason: String },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_pce_error_display() {
        let err = PceError::InvalidSignature("bad sig".into());
        assert!(err.to_string().contains("bad sig"));

        let err = PceError::CommitmentMismatch;
        assert!(err.to_string().contains("commitment"));

        let err = PceError::UnsupportedSystem(0x0099);
        assert!(err.to_string().contains("0x0099"));

        let err = PceError::MalformedProof("too short".into());
        assert!(err.to_string().contains("too short"));

        let err = PceError::InputMismatch;
        assert!(err.to_string().contains("public_inputs"));

        let err = PceError::AggregationError {
            reason: "count mismatch".into(),
        };
        assert!(err.to_string().contains("count mismatch"));
    }

    #[test]
    fn test_pce_error_debug() {
        let err = PceError::CommitmentMismatch;
        let debug = format!("{:?}", err);
        assert_eq!(debug, "CommitmentMismatch");
    }
}
