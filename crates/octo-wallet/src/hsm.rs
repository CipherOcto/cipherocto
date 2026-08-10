//! HSM + hardware wallet adapter (RFC-0853 F2) — Phase H.
//!
//! Defines the canonical signer interface that delegates signing to an external
//! secure element (Ledger, YubiHSM, TPM, etc.). For S01/S02 the default
//! `InMemorySigner` adapter wraps `IdentityKey` directly (matching the existing
//! `ed25519-dalek` substrate); production deployments swap in a real HSM adapter.
//!
//! Adapter contract: sign-only (private key never leaves the device); every
//! signing operation is constant-time at the transport layer; device pubkey is
//! discoverable via `get_public_key()`. The wallet layer treats all adapters
//! uniformly via the `HsmAdapter` trait.

use ed25519_dalek::Signer;
use serde::{Deserialize, Serialize};
use zeroize::Zeroize;

/// Adapter errors.
#[derive(Debug, thiserror::Error)]
pub enum HsmError {
    #[error("device not connected: {0}")]
    NotConnected(String),
    #[error("device error: {0}")]
    Device(String),
    #[error("user rejected signing operation on device")]
    UserRejected,
    #[error("invalid public key length from device: {0}")]
    InvalidPubKey(usize),
}

/// HSM adapter trait (production: LedgerSigner / YubiHsmSigner / TpmSigner).
///
/// MVP: `InMemorySigner` wraps `crate::identity::IdentityKey`. Production swaps
/// the adapter via the wallet constructor.
pub trait HsmAdapter: Send + Sync {
    /// Discover the device's public key (32 bytes Ed25519).
    /// # Errors
    /// Returns `HsmError::Device` on transport failure, `HsmError::InvalidPubKey`
    /// on malformed response.
    fn get_public_key(&self) -> Result<[u8; 32], HsmError>;

    /// Sign `msg`. Returns 64-byte Ed25519 signature.
    /// # Errors
    /// Returns `HsmError::UserRejected` if the user denies on-device,
    /// `HsmError::Device` on transport failure.
    fn sign(&self, msg: &[u8]) -> Result<[u8; 64], HsmError>;

    /// Downcast hook for `Any` introspection. Required by callers that need
    /// to access adapter-specific state (e.g. `IdentityKey::seed_bytes_for_hkdf`
    /// downcasting to `InMemorySigner`). Each impl returns `self` cast to
    /// `&dyn Any`; the caller's `downcast_ref::<T>()` will succeed when the
    /// concrete type matches `T`.
    fn as_any(&self) -> &dyn std::any::Any;
}

/// In-memory HSM adapter (MVP default; wraps `IdentityKey`).
///
/// All `InMemorySigner` instances share the identity's public key but sign
/// locally (no hardware boundary). Production deployments MUST replace this
/// with a real HSM adapter before production rollout.
///
/// **Debug redaction (octo-wallet §Security):** `seed_bytes` is the raw
/// identity seed — must NEVER appear in Debug output (panic messages, log
/// lines, `dbg!()`). Manual `Debug` impl prints only the public key.
///
/// **Zeroize-on-drop:** `Drop` impl wipes `seed_bytes` from memory when
/// the signer is dropped (RFC-0009 §Security §Key Handling Rule 3).
pub struct InMemorySigner {
    seed_bytes: [u8; 32],
    public_key: [u8; 32],
}

impl Drop for InMemorySigner {
    fn drop(&mut self) {
        self.seed_bytes.zeroize();
    }
}

impl std::fmt::Debug for InMemorySigner {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("InMemorySigner")
            .field("seed_bytes", &"[REDACTED]")
            .field("public_key", &hex::encode(self.public_key))
            .finish()
    }
}

impl InMemorySigner {
    /// Create an in-memory signer from an identity seed (32 bytes).
    #[must_use]
    pub fn new(seed_bytes: [u8; 32], public_key: [u8; 32]) -> Self {
        Self {
            seed_bytes,
            public_key,
        }
    }

    /// Borrow the 32-byte identity seed. Restricted to crate-internal callers
    /// (mission `0009-a` A9 mitigation: prevent host-side seed exfiltration).
    /// Production hardware wallets MUST NOT expose this — the seed lives only
    /// inside the secure element.
    #[must_use]
    pub(crate) fn seed_bytes(&self) -> [u8; 32] {
        self.seed_bytes
    }
}

impl HsmAdapter for InMemorySigner {
    fn get_public_key(&self) -> Result<[u8; 32], HsmError> {
        Ok(self.public_key)
    }

    fn sign(&self, msg: &[u8]) -> Result<[u8; 64], HsmError> {
        // Reconstruct signing key from seed; sign locally (MVP). Production HSM
        // adapters delegate to hardware and return device-computed signature.
        let signing_key = ed25519_dalek::SigningKey::from_bytes(&self.seed_bytes);
        let sig = signing_key.sign(msg);
        Ok(sig.to_bytes())
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
}

/// Ledger-shaped signer (smoke-test stub; production wires real APDU transport).
///
/// Per RFC-0853 F2 §LedgerSigner: this stub produces Ed25519 signatures the
/// same way as `InMemorySigner` (real Ledger integration requires APDU over
/// USB/HID transport which lives outside this MVP module). The smoke test
/// verifies the adapter contract is satisfied: signature length 64, public key
/// discovery, deterministic behavior.
///
/// **Debug redaction:** manual `Debug` impl delegates to `InMemorySigner::fmt`
/// which redacts `seed_bytes`.
pub struct LedgerSigner {
    inner: InMemorySigner,
}

impl std::fmt::Debug for LedgerSigner {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("LedgerSigner")
            .field("inner", &self.inner)
            .finish()
    }
}

impl LedgerSigner {
    /// Construct a Ledger-shaped signer with the given identity.
    #[must_use]
    pub fn new(seed_bytes: [u8; 32], public_key: [u8; 32]) -> Self {
        Self {
            inner: InMemorySigner::new(seed_bytes, public_key),
        }
    }
}

impl HsmAdapter for LedgerSigner {
    fn get_public_key(&self) -> Result<[u8; 32], HsmError> {
        self.inner.get_public_key()
    }

    fn sign(&self, msg: &[u8]) -> Result<[u8; 64], HsmError> {
        // Production: dispatch APDU `SIGN_PAYMENT` to Ledger; user confirms
        // on-device; device returns 64-byte Ed25519 signature.
        self.inner.sign(msg)
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
}

/// No-op signer installed after `IdentityKey::revoke()`.
///
/// Defense-in-depth: replacing the live signer with `NullSigner` after
/// revocation ensures any further `sign()` calls fail at the adapter
/// level (not just at the lifecycle gate). Public key remains valid so
/// historical signature verification still succeeds.
///
/// Per RFC-0009 §Lifecycle row 4 + RFC-0009 §Security §Key Handling
/// Rule 3: revoked identities MUST NOT produce new signatures.
pub struct NullSigner {
    public_key: [u8; 32],
}

impl std::fmt::Debug for NullSigner {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("NullSigner")
            .field("public_key", &hex::encode(self.public_key))
            .finish()
    }
}

impl NullSigner {
    /// Construct a `NullSigner` carrying the original public key.
    #[must_use]
    pub fn new(public_key: [u8; 32]) -> Self {
        Self { public_key }
    }
}

impl HsmAdapter for NullSigner {
    fn get_public_key(&self) -> Result<[u8; 32], HsmError> {
        Ok(self.public_key)
    }

    fn sign(&self, _msg: &[u8]) -> Result<[u8; 64], HsmError> {
        Err(HsmError::Device(
            "signer revoked; cannot produce new signatures".to_owned(),
        ))
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
}

/// Device fingerprint (for audit + multi-device support).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeviceFingerprint {
    pub vendor: String, // "ledger" | "yubihsm" | "tpm" | ...
    pub model: String,
    pub serial: String,
}

#[cfg(test)]
mod tests {
    use super::*;
    use ed25519_dalek::Verifier;

    fn sample_seed() -> [u8; 32] {
        [
            0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18, 19, 20, 21, 22, 23,
            24, 25, 26, 27, 28, 29, 30, 31,
        ]
    }

    fn sample_pub() -> [u8; 32] {
        let sk = ed25519_dalek::SigningKey::from_bytes(&sample_seed());
        sk.verifying_key().to_bytes()
    }

    #[test]
    fn in_memory_signer_signs_and_verifies() {
        let s = InMemorySigner::new(sample_seed(), sample_pub());
        let pk = s.get_public_key().expect("get pub");
        assert_eq!(pk, sample_pub());
        let msg = b"phase h smoke test";
        let sig_bytes = s.sign(msg).expect("sign");
        assert_eq!(sig_bytes.len(), 64);
        // Verify with ed25519-dalek directly.
        let vk = ed25519_dalek::VerifyingKey::from_bytes(&sample_pub()).unwrap();
        let sig = ed25519_dalek::Signature::from_bytes(&sig_bytes);
        vk.verify(msg, &sig).expect("verify");
    }

    #[test]
    fn ledger_signer_signs_smoke_test() {
        let l = LedgerSigner::new(sample_seed(), sample_pub());
        let pk = l.get_public_key().expect("get pub");
        assert_eq!(pk, sample_pub());
        let msg = b"ledger smoke";
        let sig_bytes = l.sign(msg).expect("sign");
        assert_eq!(sig_bytes.len(), 64);
        let vk = ed25519_dalek::VerifyingKey::from_bytes(&sample_pub()).unwrap();
        vk.verify(msg, &ed25519_dalek::Signature::from_bytes(&sig_bytes))
            .expect("verify");
    }

    #[test]
    fn both_signers_produce_same_signature_for_same_input() {
        // Both adapters MUST produce identical signatures for the same input
        // (deterministic Ed25519 + identical seed).
        let in_mem = InMemorySigner::new(sample_seed(), sample_pub());
        let ledger = LedgerSigner::new(sample_seed(), sample_pub());
        let msg = b"interop test";
        let sig_in = in_mem.sign(msg).unwrap();
        let sig_ledger = ledger.sign(msg).unwrap();
        assert_eq!(sig_in, sig_ledger);
    }
}
