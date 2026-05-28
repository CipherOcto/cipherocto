//! Onion Relay Routing error types (RFC-0858 §2.4)

use thiserror::Error;

/// ORR Error Enum — 10 variants
#[derive(Error, Debug)]
pub enum OrrError {
    #[error("Invalid hop index: {index}, max {max}")]
    InvalidHopIndex { index: u16, max: u16 },

    #[error("MAC verification failed at hop {hop_index}")]
    MacVerificationFailed { hop_index: u16 },

    #[error("Decryption failed at hop {hop_index}")]
    DecryptionFailed { hop_index: u16 },

    #[error("Replay detected for route {route_id:?}, sequence {sequence}")]
    ReplayDetected { route_id: [u8; 32], sequence: u64 },

    #[error("Route {route_id:?} expired at epoch {epoch}")]
    RouteExpired { route_id: [u8; 32], epoch: u64 },

    #[error("Invalid route count: expected {expected}, got {actual}")]
    InvalidRouteCount { expected: u16, actual: u16 },

    #[error("Transport fallback exhausted at hop {hop_index}")]
    TransportFallbackExhausted { hop_index: u16 },

    #[error("Cover traffic generation failed: {reason}")]
    CoverTrafficGenerationFailed { reason: String },

    #[error("Domain isolation violation: source {source_domain:?}, target {target_domain:?}")]
    DomainIsolationViolation {
        source_domain: [u8; 32],
        target_domain: [u8; 32],
    },

    #[error("Forward secrecy violation: {detail}")]
    ForwardSecrecyViolation { detail: String },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_orr_error_display() {
        let e = OrrError::InvalidHopIndex { index: 5, max: 3 };
        assert!(e.to_string().contains("5"));
        assert!(e.to_string().contains("3"));
    }

    #[test]
    fn test_mac_verification_error() {
        let e = OrrError::MacVerificationFailed { hop_index: 2 };
        assert!(e.to_string().contains("2"));
    }

    #[test]
    fn test_replay_detected() {
        let e = OrrError::ReplayDetected {
            route_id: [0xAA; 32],
            sequence: 42,
        };
        assert!(e.to_string().contains("42"));
    }
}
