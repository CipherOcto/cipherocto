//! PoRelay Error Types (RFC-0860 §9)

use thiserror::Error;

/// PoRelay Error Enum — 7 variants
#[derive(Error, Debug)]
pub enum PoRelayError {
    #[error("Invalid signature on proof")]
    InvalidSignature,

    #[error("Invalid epoch: {got}, expected range [{min}, {max}]")]
    InvalidEpoch { got: u64, min: u64, max: u64 },

    #[error("Gateway not found in trust registry: {gateway_id:?}")]
    GatewayNotFound { gateway_id: [u8; 32] },

    #[error("Proof replay detected for gateway {gateway_id:?}")]
    ReplayDetected { gateway_id: [u8; 32] },

    #[error("Insufficient stake: have {have}, need {need}")]
    InsufficientStake { have: u64, need: u64 },

    #[error("Diversity constraint violation: {reason}")]
    DiversityViolation { reason: &'static str },

    #[error("Slashing triggered: {reason}")]
    SlashingTriggered { reason: String },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_error_display() {
        let err = PoRelayError::InvalidSignature;
        assert!(format!("{err}").contains("Invalid signature"));

        let err = PoRelayError::InsufficientStake {
            have: 100,
            need: 500,
        };
        assert!(format!("{err}").contains("100"));
        assert!(format!("{err}").contains("500"));
    }
}
