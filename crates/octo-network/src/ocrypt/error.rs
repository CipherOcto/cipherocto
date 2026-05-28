//! OCrypt error types (RFC-0853 §14)

use thiserror::Error;

/// Errors specific to the Overlay Cryptography layer
#[derive(Debug, Error)]
pub enum CryptoError {
    /// Attempted non-deterministic operation in consensus-critical path
    #[error("Consensus boundary violation: operation={operation}, context={context}")]
    ConsensusBoundaryViolation {
        operation: &'static str,
        context: &'static str,
    },

    /// Nonce reuse detected (catastrophic for ChaCha20-Poly1305)
    #[error("Nonce reuse: nonce={nonce:?}, first_use_epoch={first_use_epoch}, second_use_epoch={second_use_epoch}")]
    NonceReuse {
        nonce: [u8; 12],
        first_use_epoch: u64,
        second_use_epoch: u64,
    },

    /// Invalid nonce derivation
    #[error("Invalid nonce: {reason}")]
    InvalidNonce { reason: &'static str },

    /// Key derivation failure
    #[error("Key derivation failure at stage: {stage}")]
    KeyDerivationFailure { stage: &'static str },

    /// Signature verification failed
    #[error("Invalid signature")]
    InvalidSignature,

    /// Proof verification failed
    #[error("Invalid proof")]
    InvalidProof,

    /// Replay detected
    #[error("Replay detected: envelope_id={envelope_id:?}")]
    ReplayDetected { envelope_id: [u8; 32] },

    /// Algorithm not supported
    #[error("Unsupported algorithm: suite_id={suite_id:?}")]
    UnsupportedAlgorithm { suite_id: [u8; 10] },

    /// Encryption failed
    #[error("Encryption failed: {reason}")]
    EncryptionFailed { reason: String },

    /// Decryption failed
    #[error("Decryption failed: {reason}")]
    DecryptionFailed { reason: String },

    /// Invalid key length
    #[error("Invalid key length: expected {expected}, got {got}")]
    InvalidKeyLength { expected: usize, got: usize },
}
