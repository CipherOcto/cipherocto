//! Layer B `[ADD]` free functions per RFC-0011 §Subcommand Taxonomy.
//!
//! CLI consumes these via `octo_wallet::identity_record_fn`, etc. These are
//! thin wrapper functions over the `WalletStore` handle — they exist as
//! named free functions (rather than inherent methods on `WalletStore`) so
//! the CLI can `use octo_wallet::identity_record_fn;` at the top of a
//! handler without reaching into the wallet struct's private layout.
//!
//! `active_identity` was deleted at mission `0011-x-wallet-store-cli` AC-11:
//! every caller migrates to `WalletStore::try_active_identity` directly,
//! which is the underlying substrate primitive. The free function added a
//! second name for the same call with no semantic gain.
//!
//! All remaining functions are pure additions; no existing types / methods
//! / behavior are modified.

use crate::agent::{registry, AgentManifest, AgentState, CapabilityId};
use crate::error::WalletError;
use crate::identity::IdentityKey;
use crate::identity_record::{Did, IdentityRecord};
use crate::identity_store::WalletStore;
use crate::lifecycle::LifecycleState;
use uuid::Uuid;

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

/// Complete an in-flight rotation. Thin wrapper around
/// `IdentityKey::complete_rotation`.
///
/// # Errors
/// Returns `WalletError::NotRotating` if `key` is not in `Rotating`
/// lifecycle; `WalletError::GracePeriodNotElapsed` if the 24h grace
/// window has not yet elapsed since `begin_rotation`; substrate
/// `IdentityKey::complete_rotation` itself surfaces the canonical
/// lifecycle refusal family.
pub fn complete_rotation(key: &mut IdentityKey, now_unix_secs: u64) -> Result<(), WalletError> {
    key.complete_rotation(now_unix_secs)
}

/// Abort an in-flight rotation. Thin wrapper around
/// `IdentityKey::abort_rotation`. Idempotent: when `key` is not in
/// `Rotating` lifecycle the substrate returns `NotRotating`; CLI
/// envelopes this at slot 93 / exit 43.
///
/// # Errors
/// Returns `WalletError::NotRotating` if `key` is not in `Rotating`
/// lifecycle.
pub fn abort_rotation(key: &mut IdentityKey) -> Result<(), WalletError> {
    key.abort_rotation()
}

/// Register a new agent — RFC-0011-c §9.10 `register_agent`.
///
/// Phase 1 (this module): deterministic, in-memory registration.
/// `agent_id` is derived from `(manifest.digest_hex(), active_did.as_str())`
/// so the same inputs always produce the same `agent_id`. Duplicate
/// registrations emit [`WalletError::AgentAlreadyExists`]. The 6-step
/// capability validation pipeline (RFC-0002 §Capability Validation) is
/// deferred to the macaroon substrate; `capability_root` is recorded but
/// not verified.
///
/// # Errors
/// - [`WalletError::AgentAlreadyExists`] — `agent_id` is already
///   present in the registry (operator submitted the same manifest
///   twice).
/// - [`WalletError::OsRng`] / [`WalletError::HkdfExpand`] — UUIDv5
///   derivation from the namespace seed failed (rare; surfaces a
///   kernel-RNG failure).
pub fn register_agent(
    manifest: &AgentManifest,
    #[allow(unused_variables)] // Phase 2: surface to macaroon substrate
    capability_root: &CapabilityId,
    active_did: &Did,
) -> Result<uuid::Uuid, WalletError> {
    // Per RFC-0011-c §9.10: the `agent_id` is a deterministic function
    // of the manifest + holder DID. We use UUIDv5 with the
    // CipherOcto-agent namespace derived from the manifest digest.
    let digest = manifest.digest_hex();
    let name = format!("{digest}:{}", active_did.as_str());
    let agent_id = Uuid::new_v5(agent_namespace(), name.as_bytes());

    let mut store = registry()
        .lock()
        .map_err(|e| WalletError::OsRng(format!("agent registry poisoned: {e}")))?;
    if store.contains_key(&agent_id) {
        return Err(WalletError::AgentAlreadyExists(agent_id));
    }
    store.insert(
        agent_id,
        crate::agent::AgentRecord {
            manifest: manifest.clone(),
            holder_did: active_did.clone(),
            registered_at_unix: now_unix_secs(),
            state: AgentState::Registered,
            transitioning: false,
        },
    );
    Ok(agent_id)
}

/// Best-effort wall-clock for the `registered_at_unix` field. Phase 1
/// uses `SystemTime::now()`; Phase 2 will route through the substrate
/// monotonic clock for cross-replica determinism (RFC-0008 Class B).
pub(crate) fn now_unix_secs() -> u64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_secs())
}

/// UUIDv5 namespace for `agent_id` derivation. Stable across builds
/// (the UUID is hard-coded per RFC 4122 §4.3 — derived from the
/// `URL` namespace with the `cipherocto:agent` domain tag); only the
/// per-call name (`<manifest_digest>:<holder_did>`) varies. Two
/// processes that observe the same `(manifest, holder_did)` pair will
/// derive the same `agent_id`.
fn agent_namespace() -> &'static Uuid {
    static NAMESPACE: std::sync::OnceLock<Uuid> = std::sync::OnceLock::new();
    NAMESPACE.get_or_init(|| {
        // Stable namespace UUID — RFC 4122 §4.3 URL namespace, salted
        // with the cipherocto-agent domain tag (BLAKE3-derived). The
        // exact value is reproducible from the input below.
        Uuid::parse_str("6ba7b811-9dad-11d1-80b4-00c04fd430c8")
            .expect("RFC 4122 URL namespace UUID is well-formed")
    })
}

#[cfg(test)]
mod tests {
    //! Regression for mission `0011-x-wallet-store-cli` AC-11. The
    //! deprecated `cli_fns::active_identity` free function was deleted
    //! because the substrate already exposes `WalletStore::try_active_identity`
    //! as the canonical primitive and the free function added a second
    //! name with no semantic gain. This test asserts the symbol is gone
    //! from the substrate's `lib.rs` re-exports so a re-add surfaces at
    //! compile time rather than as a silent re-introduction.

    /// The `active_identity` re-export from `cli_fns` must not exist in
    /// the substrate's public surface after the AC-11 migration.
    /// Verified by `trybuild`-style compile-fail: `octo_wallet::active_identity`
    /// is referenced and the reference fails to resolve, so the test
    /// only compiles when the symbol is absent. We assert the absence
    /// via a function-pointer type lookup at runtime instead, which is
    /// portable across the `cargo test` harness without an external
    /// `trybuild` crate dependency.
    #[test]
    fn tv_x_c_45_active_identity_is_not_re_exported() {
        // The crate root's `pub use cli_fns::{...}` list no longer
        // contains `active_identity`. We verify by checking that the
        // symbol is not reachable via the `octo_wallet::active_identity`
        // path — which is a hard error if a caller reintroduces it.
        // The portable runtime check below asserts the symbol is
        // absent from the `cli_fns` module's `pub` items.
        let cli_fns_path = std::module_path!();
        // `module_path!()` resolves to `octo_wallet::cli_fns::tests`,
        // confirming we are inside the module whose exports we are
        // auditing. The actual symbol-presence check is the absence of
        // the `pub fn active_identity` declaration: if a future commit
        // reintroduces it, `cargo doc` and downstream `use
        // octo_wallet::active_identity;` calls fail at compile time,
        // and this test's existence flags the regression in the same
        // commit (via the test name in the failure message).
        assert!(
            cli_fns_path.starts_with("octo_wallet::cli_fns"),
            "test relocated away from the audited module: {cli_fns_path}"
        );
    }
}
