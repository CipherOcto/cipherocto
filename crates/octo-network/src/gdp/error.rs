//! GDP Error Types (RFC-0851 §12)

use thiserror::Error;

/// GDP Error Enum — 11 variants
#[derive(Error, Debug)]
pub enum GdpError {
    #[error("Invalid advertisement: {reason}")]
    InvalidAdvertisement { reason: String },

    #[error("Stale sequence: got {got}, expected >= {expected}")]
    StaleSequence { got: u64, expected: u64 },

    #[error("Replay detected for gateway {gateway_id:?}")]
    ReplayDetected { gateway_id: [u8; 32] },

    #[error("Invalid signature")]
    InvalidSignature,

    #[error("Capability mismatch: required {required:?}, available {available:?}")]
    CapabilityMismatch { required: u64, available: u64 },

    #[error("Insufficient stake: have {have}, need {need}")]
    InsufficientStake { have: u64, need: u64 },

    #[error("Cache full (max {max_entries})")]
    CacheFull { max_entries: u32 },

    #[error("Heartbeat timeout for gateway {gateway_id:?} after {missed} missed")]
    HeartbeatTimeout { gateway_id: [u8; 32], missed: u32 },

    #[error("Heartbeat out of order: got seq {got}, expected >= {expected}")]
    HeartbeatOutOfOrder { got: u64, expected: u64 },

    #[error("Scope not permitted: {scope:?}")]
    ScopeNotPermitted { scope: u16 },

    #[error("Diversity violation: {dimension} score {score} < minimum {minimum}")]
    DiversityViolation {
        dimension: String,
        score: u32,
        minimum: u32,
    },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_gdp_error_display() {
        let err = GdpError::StaleSequence {
            got: 5,
            expected: 10,
        };
        assert!(err.to_string().contains("5"));
        assert!(err.to_string().contains("10"));
    }

    #[test]
    fn test_gdp_error_variants() {
        let errors: Vec<GdpError> = vec![
            GdpError::InvalidAdvertisement {
                reason: "test".into(),
            },
            GdpError::StaleSequence {
                got: 1,
                expected: 2,
            },
            GdpError::ReplayDetected {
                gateway_id: [0u8; 32],
            },
            GdpError::InvalidSignature,
            GdpError::CapabilityMismatch {
                required: 1,
                available: 0,
            },
            GdpError::InsufficientStake {
                have: 100,
                need: 500,
            },
            GdpError::CacheFull { max_entries: 100 },
            GdpError::HeartbeatTimeout {
                gateway_id: [0u8; 32],
                missed: 3,
            },
            GdpError::HeartbeatOutOfOrder {
                got: 1,
                expected: 2,
            },
            GdpError::ScopeNotPermitted { scope: 0x0004 },
            GdpError::DiversityViolation {
                dimension: "transport".into(),
                score: 1,
                minimum: 2,
            },
        ];
        assert_eq!(errors.len(), 11);
    }
}
