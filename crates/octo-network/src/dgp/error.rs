//! DGP error types (RFC-0852)

use thiserror::Error;

/// Errors specific to the Deterministic Gossip Protocol.
#[derive(Debug, Error)]
pub enum DgpError {
    #[error("Duplicate object: {object_hash:?} already seen at epoch {first_seen}")]
    DuplicateObject {
        object_hash: [u8; 32],
        first_seen: u64,
    },

    #[error("Invalid signature on object {object_hash:?}")]
    InvalidSignature { object_hash: [u8; 32] },

    #[error("Replay detected for object {object_hash:?}, first seen at epoch {first_seen}")]
    ReplayDetected {
        object_hash: [u8; 32],
        first_seen: u64,
    },

    #[error("TTL expired for object {object_hash:?} (ttl={ttl})")]
    TtlExpired { object_hash: [u8; 32], ttl: u16 },

    #[error("Domain mismatch: expected {expected:?}, got {actual:?}")]
    DomainMismatch {
        expected: [u8; 32],
        actual: [u8; 32],
    },

    #[error("Fragment assembly failed for {object_hash:?}: {reason}")]
    FragmentAssemblyFailed {
        object_hash: [u8; 32],
        reason: String,
    },

    #[error("Invalid object type: {object_type}")]
    InvalidObjectType { object_type: u16 },

    #[error("Serialization error: {0}")]
    Serialization(String),

    #[error("Cache full: {entries} entries, max {max}")]
    CacheFull { entries: usize, max: usize },

    #[error("Signature verification failed: {0}")]
    SignatureVerificationFailed(String),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_dgp_error_display() {
        let err = DgpError::DuplicateObject {
            object_hash: [0xAA; 32],
            first_seen: 100,
        };
        assert!(format!("{err}").contains("Duplicate object"));
    }

    #[test]
    fn test_ttl_expired() {
        let err = DgpError::TtlExpired {
            object_hash: [0xBB; 32],
            ttl: 0,
        };
        assert!(format!("{err}").contains("TTL expired"));
    }

    #[test]
    fn test_cache_full() {
        let err = DgpError::CacheFull {
            entries: 1000,
            max: 1000,
        };
        assert!(format!("{err}").contains("Cache full"));
    }
}
