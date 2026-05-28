//! DRS Error Types (RFC-0856 Error Types section)

use thiserror::Error;

/// DRS Error Enum — 8 variants
#[derive(Error, Debug)]
pub enum DrsError {
    #[error("Route not found: {route_id:?}")]
    RouteNotFound { route_id: [u8; 32] },

    #[error("Scoring overflow in component: {component}")]
    ScoringOverflow { component: String },

    #[error("Invalid weights — field: {field}")]
    InvalidWeights { field: String },

    #[error("Route cache full: max {max_entries} entries")]
    CacheFull { max_entries: u32 },

    #[error("Route revocation failed: {reason}")]
    RevocationFailed { reason: String },

    #[error("Trust computation failed — factor: {factor}")]
    TrustComputationFailed { factor: String },

    #[error("Invalid route domain: {domain:?}")]
    InvalidRouteDomain { domain: [u8; 32] },

    #[error("Signature verification failed")]
    SignatureVerificationFailed,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_drs_error_display() {
        let err = DrsError::RouteNotFound {
            route_id: [0xAA; 32],
        };
        assert!(format!("{err}").contains("Route not found"));

        let err = DrsError::ScoringOverflow {
            component: "trust".into(),
        };
        assert!(format!("{err}").contains("trust"));

        let err = DrsError::CacheFull { max_entries: 1000 };
        assert!(format!("{err}").contains("1000"));

        let err = DrsError::SignatureVerificationFailed;
        assert!(format!("{err}").contains("Signature"));
    }

    #[test]
    fn test_drs_error_debug() {
        let err = DrsError::InvalidWeights {
            field: "trust_weight".into(),
        };
        let debug = format!("{:?}", err);
        assert!(debug.contains("InvalidWeights"));
    }
}
