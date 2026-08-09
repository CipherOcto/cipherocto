//! `CapabilitySigner` trait — abstracts over holder signing backends.
//!
//! Mission 0957 Phase 2b: enables `CapabilityToken` migration into
//! `octo-cap-macaroon` without forcing the crate to depend on
//! `octo-wallet` (which owns `IdentityKey` + `HsmAdapter` per RFC-0009
//! HSM routing mandate).
//!
//! ## Layer discipline
//!
//! `CapabilitySigner` is defined in Layer 4 (octo-cap-macaroon). The
//! `IdentityKey` blanket impl lives in Layer B (octo-wallet) — it is
//! the only canonical signer. New signer backends (hardware wallets,
//! threshold sigs, etc.) implement this trait in their own crate and
//! are wired in via dependency injection (the `mint` / `attenuate_with_signer`
//! APIs accept `&dyn CapabilitySigner`).
//!
//! ## Error model
//!
//! `sign()` returns `Result<[u8; 64], CapabilitySignerError>`. The
//! canonical `IdentityKey` impl maps `WalletError::Hsm(...)` to
//! `CapabilitySignerError::Hsm(...)`. Custom signer backends (e.g.,
//! hardware wallets) map their own errors here.

use thiserror::Error;

/// Errors returned by `CapabilitySigner::sign`.
#[derive(Debug, Error)]
pub enum CapabilitySignerError {
    /// Underlying signer rejected the operation (e.g., HSM adapter
    /// transport failure, user denied on-device, etc.). The original
    /// error is preserved as a string for caller diagnostics.
    #[error("signer rejected: {0}")]
    Signer(String),

    /// Signer produced a malformed signature (wrong length, invalid
    /// curve point, etc.). Indicates an implementation bug in the
    /// signer backend.
    #[error("malformed signature: {0}")]
    Malformed(String),
}

/// Trait for capability token holder signers.
///
/// Implementors produce 64-byte Ed25519 signatures over arbitrary byte
/// messages. The `sign` method may fail (HSM transport, user denial,
/// etc.); the failure mode is surfaced via `CapabilitySignerError`.
///
/// Implementors MUST also return the 32-byte Ed25519 public key via
/// `public_key_bytes`. The verifier reconstructs the verifying key from
/// these 32 bytes via `ed25519_dalek::VerifyingKey::from_bytes`.
pub trait CapabilitySigner {
    /// Sign `msg`; return the 64-byte Ed25519 signature.
    ///
    /// # Errors
    /// Returns `CapabilitySignerError::Signer` on backend rejection
    /// (transport / user denial / etc.) or `Malformed` on internal
    /// serializer bugs.
    fn sign(&self, msg: &[u8]) -> Result<[u8; 64], CapabilitySignerError>;

    /// Return the 32-byte Ed25519 public key.
    #[must_use]
    fn public_key_bytes(&self) -> [u8; 32];
}

/// Blanket impl for `Box<dyn CapabilitySigner>` — enables `Arc<dyn CapabilitySigner>`
/// to be passed through generic helpers without re-wrapping.
impl CapabilitySigner for Box<dyn CapabilitySigner + '_> {
    fn sign(&self, msg: &[u8]) -> Result<[u8; 64], CapabilitySignerError> {
        (**self).sign(msg)
    }
    fn public_key_bytes(&self) -> [u8; 32] {
        (**self).public_key_bytes()
    }
}

/// Blanket impl for `Arc<dyn CapabilitySigner>` — same rationale.
impl CapabilitySigner for std::sync::Arc<dyn CapabilitySigner> {
    fn sign(&self, msg: &[u8]) -> Result<[u8; 64], CapabilitySignerError> {
        (**self).sign(msg)
    }
    fn public_key_bytes(&self) -> [u8; 32] {
        (**self).public_key_bytes()
    }
}
