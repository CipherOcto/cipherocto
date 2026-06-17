//! MON Error Types (RFC-0855 §19)

use thiserror::Error;

/// Mission Overlay Network error enum — 8 variants
#[derive(Error, Debug)]
pub enum MonError {
    #[error("Invalid mission id bytes: expected {expected} bytes, got {actual}")]
    InvalidMissionIdBytes { expected: usize, actual: usize },

    #[error("Invalid mission id: {mission_hash:?}")]
    InvalidMissionId { mission_hash: [u8; 32] },

    #[error("Mission not active: current state {current_state}")]
    MissionNotActive { current_state: u16 },

    #[error("Admission denied: reason {reason}")]
    AdmissionDenied { reason: u16 },

    #[error("Topology violation: {constraint}")]
    TopologyViolation { constraint: String },

    #[error("Governance rejected: proposal {proposal_id:?}")]
    GovernanceRejected { proposal_id: [u8; 32] },

    #[error("Scope violation: required {required}, actual {actual}")]
    ScopeViolation { required: u16, actual: u16 },

    #[error("Key derivation failed: {context}")]
    KeyDerivationFailed { context: String },

    #[error("Rekeying failed: {reason}")]
    RekeyingFailed { reason: String },

    #[error("Invalid governance policy: {reason}")]
    InvalidGovernancePolicy { reason: String },

    #[error("Invalid role assignment: {reason}")]
    InvalidRoleAssignment { reason: String },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_mon_error_display() {
        let err = MonError::InvalidMissionId {
            mission_hash: [0u8; 32],
        };
        assert!(err.to_string().contains("Invalid mission id"));

        let err = MonError::MissionNotActive { current_state: 3 };
        assert!(err.to_string().contains("not active"));

        let err = MonError::KeyDerivationFailed {
            context: "test".to_string(),
        };
        assert!(err.to_string().contains("Key derivation failed"));
    }

    #[test]
    fn test_all_variants_constructible() {
        let _ = MonError::AdmissionDenied { reason: 1 };
        let _ = MonError::TopologyViolation {
            constraint: "min_participants".to_string(),
        };
        let _ = MonError::GovernanceRejected {
            proposal_id: [0u8; 32],
        };
        let _ = MonError::ScopeViolation {
            required: 1,
            actual: 2,
        };
        let _ = MonError::RekeyingFailed {
            reason: "timeout".to_string(),
        };
        let _ = MonError::InvalidGovernancePolicy {
            reason: "denominator is zero".to_string(),
        };
        let _ = MonError::InvalidRoleAssignment {
            reason: "insufficient trust".to_string(),
        };
    }
}
