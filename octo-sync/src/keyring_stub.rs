//! `KeyRing` trait (interface only) — the full `MissionKeyRing` implementation
//! is in mission 0862d.
//!
//! Per RFC-0862 v1.1.0 §DatabaseSyncAdapter Trait, the cipherocto sync engine
//! consumes a `KeyRing` trait object (`Arc<dyn KeyRing>`) so that the actual
//! key material can be provided by 0862d without creating a Cargo dep cycle
//! (the trait is here in `octo-sync`, the impl is in 0862d which can depend
//! on `octo-sync`).

use crate::error::SyncError;
use crate::types::NodeId;

/// The `KeyRing` trait: the cipherocto sync engine's interface to per-mission
/// cryptographic material.
///
/// 5 methods:
/// - [`transport_key`](Self::transport_key) — for `SyncSummary.hmac`
/// - [`execution_key`](Self::execution_key) — for ChaCha20-Poly1305 AEAD
/// - [`summary_hmac`](Self::summary_hmac) — compute the summary HMAC
/// - [`encrypt`](Self::encrypt) — AEAD encrypt
/// - [`decrypt`](Self::decrypt) — AEAD decrypt
///
/// Implementers MUST hold the derived keys (not the mission root key) and MUST
/// be `Send + Sync` (the cipherocto sync engine uses `Arc<dyn KeyRing>` in a
/// multi-threaded async context).
pub trait KeyRing: Send + Sync + 'static {
    /// Return the 32-byte `transport_key` (first 32 bytes of HKDF-BLAKE3 OKM).
    fn transport_key(&self) -> &[u8; 32];

    /// Return the 32-byte `execution_key` (next 32 bytes of HKDF-BLAKE3 OKM).
    fn execution_key(&self) -> &[u8; 32];

    /// Compute `HMAC-BLAKE3(transport_key, summary_body || node_id)`.
    ///
    /// Used for the per-peer `SyncSummary.hmac` field. The `node_id` is the
    /// local node's [`NodeId`] (BLAKE3 of public_key || mission_id).
    fn summary_hmac(&self, summary_body: &[u8], node_id: &NodeId) -> [u8; 32];

    /// AEAD-encrypt `plaintext` with `aad` as the additional authenticated data.
    ///
    /// Returns `(ciphertext, nonce)`. The caller MUST ship the nonce alongside
    /// the ciphertext.
    fn encrypt(&self, plaintext: &[u8], aad: &[u8]) -> (Vec<u8>, [u8; 12]);

    /// AEAD-decrypt `ciphertext` with `aad` and `nonce`. Returns the plaintext
    /// on success, or [`SyncError::DecryptionFailed`] on AEAD tag mismatch.
    fn decrypt(
        &self,
        ciphertext: &[u8],
        nonce: &[u8; 12],
        aad: &[u8],
    ) -> Result<Vec<u8>, SyncError>;
}
