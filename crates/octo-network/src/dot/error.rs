//! DOT error types (RFC-0850 §4.5)

use thiserror::Error;

/// Errors specific to the Deterministic Overlay Transport layer
#[derive(Debug, Error)]
pub enum DotError {
    // Envelope errors
    #[error("Invalid signature on envelope {envelope_id:?}")]
    InvalidSignature { envelope_id: [u8; 32] },

    #[error("Envelope {envelope_id:?} already seen at epoch {first_seen}")]
    ReplayDetected {
        envelope_id: [u8; 32],
        first_seen: u64,
    },

    #[error("Invalid envelope ID: expected {expected:?}, computed {computed:?}")]
    InvalidEnvelopeId {
        expected: [u8; 32],
        computed: [u8; 32],
    },

    #[error("Canonicalization failed: {reason}")]
    CanonicalizationFailed { reason: &'static str },

    #[error("Envelope too large: {size} bytes exceeds {max} bytes")]
    EnvelopeTooLarge { size: usize, max: usize },

    #[error("Unsupported protocol version: {version} (expected 1)")]
    UnsupportedVersion { version: u16 },

    // TTL
    #[error("Envelope expired: ttl_hops={ttl}, current_hops={hops}")]
    TtlExpired { ttl: u16, hops: u16 },

    // Platform adapter errors
    #[error("Platform adapter error: {0}")]
    PlatformAdapter(#[from] PlatformAdapterError),

    // Fragmentation
    #[error("Fragment reassembly timeout for envelope {envelope_id:?}")]
    FragmentTimeout { envelope_id: [u8; 32] },

    // Serialization
    #[error("Canonical serialization error: {0}")]
    Serialization(String),

    // Consensus boundary
    #[error("Consensus boundary violation: {operation}")]
    ConsensusBoundaryViolation { operation: String },
}

/// Errors from platform-specific adapters
#[derive(Debug, Error)]
pub enum PlatformAdapterError {
    #[error("Platform {platform} unreachable: {reason}")]
    Unreachable { platform: String, reason: String },

    #[error("Payload too large for platform {platform}: {size} > {max}")]
    PayloadTooLarge {
        platform: String,
        size: usize,
        max: usize,
    },

    #[error("Rate limited by platform {platform}, retry after {retry_after_ms}ms")]
    RateLimited {
        platform: String,
        retry_after_ms: u64,
    },

    #[error("Platform API error: {code} {message}")]
    ApiError { code: u16, message: String },
}
