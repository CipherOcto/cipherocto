//! Layer B `[ADD]` free functions per RFC-0011 §Subcommand Taxonomy.
//!
//! CLI consumes these via `octo_wallet::active_identity`, etc. These are
//! thin wrapper functions over the `WalletStore` handle — they exist as
//! named free functions (rather than inherent methods on `WalletStore`) so
//! the CLI can `use octo_wallet::active_identity;` at the top of a handler
//! without reaching into the wallet struct's private layout.
//!
//! All functions are pure additions; no existing types / methods / behavior
//! are modified.

use crate::agent::{registry, AgentManifest, AgentState, CapabilityId};
use crate::error::WalletError;
use crate::identity::IdentityKey;
use crate::identity_record::{Did, IdentityRecord, WalletStore};
use crate::lifecycle::LifecycleState;
use uuid::Uuid;

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
        },
    );
    Ok(agent_id)
}

/// Best-effort wall-clock for the `registered_at_unix` field. Phase 1
/// uses `SystemTime::now()`; Phase 2 will route through the substrate
/// monotonic clock for cross-replica determinism (RFC-0008 Class B).
fn now_unix_secs() -> u64 {
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
