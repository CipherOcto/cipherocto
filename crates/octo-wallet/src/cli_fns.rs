//! Layer B [ADD] free functions per RFC-0011 §Subcommand Taxonomy.
//!
//! CLI consumes these via `octo_wallet::active_identity`, etc. These are
//! thin wrapper functions over the `WalletStore` handle — they exist as
//! named free functions (rather than inherent methods on `WalletStore`) so
//! the CLI can `use octo_wallet::active_identity;` at the top of a handler
//! without reaching into the wallet struct's private layout.
//!
//! All functions are pure additions; no existing types / methods / behavior
//! are modified.

use crate::error::WalletError;
use crate::identity::IdentityKey;
use crate::identity_record::{Did, IdentityRecord, WalletStore};
use crate::lifecycle::LifecycleState;

/// Return the active identity from the store. Maps the underlying
/// `IdentityNotActive` semantics to `WalletError::NotActive` (CLI exit 2
/// for "no active identity").
///
/// # Errors
/// Returns `WalletError::NotActive` when no identity is currently active.
pub fn active_identity(store: &WalletStore) -> Result<IdentityKey, WalletError> {
    store.try_active_identity()
}

/// Look up an identity record by DID. The store holds `(DID, IdentityRecord)`
/// pairs; the CLI composes `IdentityShowOutput` from this +
/// `IdentityRotationEvent` history.
///
/// # Errors
/// Returns `WalletError::NotActive` when the DID is not registered (stub
/// behavior — real impl returns a dedicated "not found" variant).
pub fn identity_record(store: &WalletStore, did: &Did) -> Result<IdentityRecord, WalletError> {
    store.lookup_identity_record(did)
}

/// Begin rotation. Thin wrapper around `IdentityKey::begin_rotation` (the
/// underlying primitive already implements the state transition + proof
/// signature). Returns the 64-byte signature proof that the successor
/// accepted the rotation.
///
/// # Errors
/// Returns `WalletError::NotActive` if `key` is not in `Active` state,
/// `WalletError::SelfRotation` if `successor.did() == key.did()`, or any
/// HSM error surfaced from the adapter.
pub fn begin_rotation(
    key: &mut IdentityKey,
    successor: IdentityKey,
    now_unix_secs: u64,
) -> Result<[u8; 64], WalletError> {
    key.begin_rotation(successor, now_unix_secs)
}

/// Revoke. Thin wrapper around `IdentityKey::revoke`. Idempotent from
/// `Revoked` state — does NOT raise `AlreadyRevoked` (the underlying
/// `IdentityKey::revoke` already handles idempotency per RFC-0009
/// §Lifecycle row 4).
///
/// # Errors
/// Returns `WalletError::NotActive { current_state: Designated }` if the
/// identity was never activated (Designated → Revoked is not a valid edge).
pub fn revoke(key: &mut IdentityKey, now_unix_secs: u64) -> Result<(), WalletError> {
    if matches!(key.lifecycle(), LifecycleState::Revoked) {
        // Idempotent — no error, no timestamp advance.
        return Ok(());
    }
    key.revoke(now_unix_secs)
}
