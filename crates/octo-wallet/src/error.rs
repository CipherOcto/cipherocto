//! Wallet error type.

use thiserror::Error;

use crate::hsm::HsmError;
use crate::lifecycle::LifecycleState;

/// Top-level error for `octo-wallet`.
#[derive(Debug, Error)]
pub enum WalletError {
    #[error("OS RNG failure: {0}")]
    OsRng(String),

    #[error("invalid audience ID: {0}")]
    InvalidAudienceId(String),

    #[error("invalid channel ID: {0}")]
    InvalidChannelId(String),

    #[error("HKDF expand failed: {0}")]
    HkdfExpand(String),

    #[error("signature verification failed: {0}")]
    Signature(String),

    #[error("HSM error: {0}")]
    Hsm(#[from] HsmError),

    #[error("vault slot not found: {0}")]
    VaultSlotNotFound(String),

    #[error("vault decryption failed (wrong passphrase or corrupted slot)")]
    VaultDecryptionFailed,

    #[error("vault KDF timed out")]
    VaultKdfTimeout,

    #[error("invalid slot ID `{0}` (must match [a-zA-Z0-9._-]+ and length 1..=128)")]
    InvalidSlotId(String),

    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    #[error("keystore parse error: {0}")]
    KeystoreParse(String),

    #[error("keystore version mismatch: expected {expected}, got {got}")]
    KeystoreVersion { expected: String, got: String },

    #[error("config error: {0}")]
    Config(String),

    // ----- Identity lifecycle errors (RFC-0009 §Lifecycle Requirements) -----
    /// `sign()` called when lifecycle state is not `Active` or `Rotating`
    /// (i.e. `Designated` or `Revoked`).
    #[error("identity not active (current state: {current_state:?})")]
    NotActive { current_state: LifecycleState },

    /// `activate()` called on a `Revoked` identity (terminal state).
    #[error("identity already revoked; cannot activate")]
    AlreadyRevoked,

    /// `activate()` or `revoke()` called while identity is in the
    /// `Rotating` state. Caller must complete or abort the rotation first
    /// (l2 mission owns rotation transitions).
    #[error("identity rotation in progress; complete or abort rotation first")]
    RotationInProgress,
}
