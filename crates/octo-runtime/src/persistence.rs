//! `octo-runtime` persistence + revocation + session-registry
//! substrate (RFC-0011-c §Follow-on §F.3 + §F.2 step (e) follow-on
//! amendment).
//!
//! ## Stoolap cursor persistence (gated on feature)
//!
//! `persist_event_cursor` + `load_event_cursor` are gated on the
//! `octo-runtime-persistence` feature flag. When the feature is OFF
//! (default build), they return `PersistenceError::FeatureNotEnabled`.
//! When ON, they round-trip through a CipherOcto-fork Stoolap
//! database per [[stoolap-fork-persistence]].
//!
//! ## In-memory revocation set
//!
//! `revoke_attach_token` + `is_token_revoked` maintain a
//! process-singleton `RwLock<HashSet<SessionId>>` revocation set.
//! The set is in-memory only — it does NOT survive process restart
//! (RFC-0011-c §F.3 documents fork-fail-closed semantics:
//! "the revocation set is process-local; a restarted process
//! must re-revoke via the CLI surface"). This is a deliberate
//! simplification for v0.1.0; future amendments may persist the
//! set via the Stoolap feature flag.
//!
//! ## In-memory session registry
//!
//! `register_session` + `lookup_session` + `unregister_session`
//! maintain a process-singleton `RwLock<HashMap<SessionId,
//! Arc<SessionBinding>>>` registry. `RuntimeHandle::new` registers
//! a `SessionBinding` for the spawned session; `InProcessHandler::bind`
//! looks it up to mint a fresh broadcast `Receiver` for the
//! caller (the substrate-faithful step (e) surface per RFC-0011-c
//! §F.2 follow-on amendment).
//!
//! Entries accumulate for the process lifetime — there is no
//! per-handle teardown hook in v0.1.0 (the cleanup path lands via
//! a follow-on amendment that observes the last
//! `Arc<HandleInner>` drop). A process restart clears the
//! registry (per the §F.3 fork-fail-closed semantics).
//!
//! ## Process-singleton invariant
//!
//! The revocation set, the session registry, and the Stoolap
//! connection (when the `octo-runtime-persistence` feature is
//! enabled) are all process-singletons via `OnceLock`. Multi-process
//! scenarios — the `octo agent run --detach` + `octo agent attach`
//! pattern across process boundaries — hit the substrate boundary
//! at the CLI's `octo agent revoke-attach` surface, which signals
//! the running process via IPC; this IPC pathway is out of scope
//! for v0.1.0.

use std::collections::{HashMap, HashSet};
use std::sync::atomic::AtomicU64;
use std::sync::{Arc, OnceLock, RwLock};

use uuid::Uuid;

use crate::handle::error::{AttachError, PersistenceError};
use crate::handle::{RuntimeEvent, SessionId};

/// Per-agent cursor persistence (RFC-0011-c §F.3).
///
/// Gated on `cfg(feature = "octo-runtime-persistence")`. When the
/// feature is OFF (default build), returns
/// `PersistenceError::FeatureNotEnabled`.
///
/// # Errors
/// Returns `PersistenceError::FeatureNotEnabled` when the feature
/// flag is OFF, or `PersistenceError::StoolapWriteFailed` /
/// `StoolapReadFailed` on Stoolap transport failures.
#[cfg(feature = "octo-runtime-persistence")]
pub fn persist_event_cursor(_agent_id: Uuid, _cursor: u64) -> Result<(), PersistenceError> {
    // Phase 1 stub: the Stoolap cursor schema is wired in the
    // follow-on mission `0011-c-octo-runtime-persistence-schema`.
    // For now, the Stoolap transport exists in the workspace but
    // the schema is not yet committed.
    //
    // The placeholder body is intentionally minimal so the
    // substrate-faithful contract (signatures + tests) lands
    // before the schema migration. The function signature is
    // permanent; only the body advances.
    Err(PersistenceError::StoolapWriteFailed(
        "cursor persistence schema pending (0011-c-octo-runtime-persistence-schema)".to_string(),
    ))
}

/// Per-agent cursor persistence (stub when feature OFF).
///
/// Returns `PersistenceError::FeatureNotEnabled` so the caller can
/// surface a substrate-faithful error to the CLI boundary.
#[cfg(not(feature = "octo-runtime-persistence"))]
pub fn persist_event_cursor(_agent_id: Uuid, _cursor: u64) -> Result<(), PersistenceError> {
    Err(PersistenceError::FeatureNotEnabled)
}

/// Load a per-agent cursor (RFC-0011-c §F.3).
///
/// Gated on `cfg(feature = "octo-runtime-persistence")`.
///
/// # Errors
/// Returns `PersistenceError::FeatureNotEnabled` when the feature
/// flag is OFF, or `PersistenceError::StoolapReadFailed` on Stoolap
/// transport failures.
#[cfg(feature = "octo-runtime-persistence")]
pub fn load_event_cursor(_agent_id: Uuid) -> Result<Option<u64>, PersistenceError> {
    // Phase 1 stub (see `persist_event_cursor` rationale).
    Ok(None)
}

/// Load a per-agent cursor (stub when feature OFF).
#[cfg(not(feature = "octo-runtime-persistence"))]
pub fn load_event_cursor(_agent_id: Uuid) -> Result<Option<u64>, PersistenceError> {
    Err(PersistenceError::FeatureNotEnabled)
}

/// Process-singleton revocation set.
///
/// `OnceLock` + `RwLock<HashSet<SessionId>>` keeps the substrate
/// fork-fail-closed: any process restart clears the set (the
/// operator must re-revoke via the CLI surface — out of scope for
/// v0.1.0).
fn revocation_set() -> &'static RwLock<HashSet<SessionId>> {
    static SET: OnceLock<RwLock<HashSet<SessionId>>> = OnceLock::new();
    SET.get_or_init(|| RwLock::new(HashSet::new()))
}

/// Add a session id to the revocation set (RFC-0011-c §F.3).
///
/// Once added, `is_token_revoked` returns `true` for that session id
/// indefinitely (until process restart).
///
/// # Errors
/// Returns `AttachError::RevocationError(reason)` when the
/// underlying `RwLock` was poisoned by a previous panic during a
/// revocation operation. The canonical revocation-set envelope
/// lives on `AttachError` per RFC-0011-c §F.4 (the standalone
/// `RevocationError` enum was folded into `AttachError`).
pub fn revoke_attach_token(session_id: SessionId) -> Result<(), AttachError> {
    let mut guard = revocation_set()
        .write()
        .map_err(|e| AttachError::RevocationError(format!("revocation set poisoned: {e}")))?;
    guard.insert(session_id);
    Ok(())
}

/// Fast-path check invoked at step (b) of `attach_with_token`
/// (RFC-0011-c §F.2).
///
/// # Returns
/// `true` iff the session id has been previously added to the
/// revocation set via `revoke_attach_token` (and the process has
/// not been restarted).
///
/// Fail-CLOSED on poisoned `RwLock`: if the revocation set is
/// unreadable (a previous panic during a revocation operation
/// poisoned the lock), the function returns `true` so the token is
/// treated as revoked. The alternative (fail-open) would silently
/// bypass explicit operator revocations; fail-CLOSED preserves the
/// revocation guarantee at the cost of denying attach operations
/// until the operator restarts the process.
#[must_use]
pub fn is_token_revoked(session_id: &SessionId) -> bool {
    revocation_set()
        .read()
        .map(|guard| guard.contains(session_id))
        .unwrap_or(true)
}

/// Per-session binding registered on `RuntimeHandle::new`
/// (RFC-0011-c §F.2 step (e) follow-on amendment).
///
/// Substrate-side cache of the broadcast sender + last-observed
/// `since_unix`. The broadcast sender is a clone of
/// `HandleInner::event_tx`; the channel itself stays open for the
/// lifetime of `HandleInner::_keepalive_rx` (until the last
/// `RuntimeHandle` clone drops per the `HandleInner` invariants).
///
/// `last_since_unix` is the cursor used by
/// `InProcessHandler::bind` for replay detection per RFC-0011-c
/// §9.7 follow-on amendment: a second attach with `since_unix`
/// behind this value is rejected with `AttachError::ReplayDetected`
/// (exit 61).
#[derive(Debug)]
pub struct SessionBinding {
    /// Clone of the broadcast sender. `bind` calls `.subscribe()`
    /// on this to mint a fresh `Receiver<RuntimeEvent>` for the
    /// caller. Sender is a no-op to drop — the underlying channel
    /// closes only when ALL senders + the keepalive receiver
    /// drop, which the `HandleInner` invariant guarantees cannot
    /// happen while any `RuntimeHandle` clone is alive.
    pub event_tx: tokio::sync::broadcast::Sender<RuntimeEvent>,
    /// Last-observed `since_unix` for a successful `bind` against
    /// this session. Initialized to `0` on registration; bumped
    /// to the caller's `since_unix` on every successful attach.
    pub last_since_unix: AtomicU64,
}

/// Process-singleton session registry (RFC-0011-c §F.2 step (e)
/// follow-on amendment).
///
/// Same `OnceLock<RwLock<HashMap<SessionId, Arc<SessionBinding>>>>`
/// pattern as `revocation_set` — process-local, fork-fail-closed
/// per §F.3. Entries accumulate for the lifetime of the process;
/// a restart clears the map (per the §F.3 fork-fail-closed
/// semantics).
///
/// Cleanup of individual entries when a session ends is OUT OF
/// SCOPE for v0.1.0 — the substrate does not yet observe per-handle
/// teardown; a follow-on amendment will hook the last
/// `Arc<HandleInner>` drop and call `unregister_session`.
fn session_registry() -> &'static RwLock<HashMap<SessionId, Arc<SessionBinding>>> {
    static REG: OnceLock<RwLock<HashMap<SessionId, Arc<SessionBinding>>>> = OnceLock::new();
    REG.get_or_init(|| RwLock::new(HashMap::new()))
}

/// Register a session binding for `session_id`
/// (RFC-0011-c §F.2 step (e) follow-on amendment).
///
/// Idempotent: re-registering the same `session_id` overwrites the
/// prior entry. The caller (typically `RuntimeHandle::new`) is
/// responsible for using a unique `session_id` per spawn;
/// collisions are a programmer error (the substrate cannot
/// distinguish two concurrent spawns of the same
/// `(agent_id, spawned_at)` pair — `derive_session_id` is
/// deterministic over those inputs).
///
/// # Errors
/// Returns `AttachError::PersistenceError` when the registry lock
/// is poisoned by a previous panic. Fail-CLOSED per the
/// revocation-set discipline.
pub fn register_session(
    session_id: SessionId,
    binding: Arc<SessionBinding>,
) -> Result<(), AttachError> {
    let mut guard = session_registry()
        .write()
        .map_err(|e| AttachError::PersistenceError(format!("session registry poisoned: {e}")))?;
    guard.insert(session_id, binding);
    Ok(())
}

/// Look up a previously-registered session binding.
///
/// Returns `None` if no binding is registered for `session_id`.
/// Fail-CLOSED on poisoned lock: returns `None` so the caller
/// surfaces `AttachError::UnknownSession` (the same surface a
/// legitimate "no session" lookup would produce — the
/// poisoned-lock case is indistinguishable from a missing entry
/// and the substrate treats both as "session not found").
#[must_use]
pub fn lookup_session(session_id: &SessionId) -> Option<Arc<SessionBinding>> {
    session_registry()
        .read()
        .ok()
        .and_then(|guard| guard.get(session_id).cloned())
}

/// Remove a session binding (RFC-0011-c §F.2 step (e) follow-on).
///
/// Currently UNCALLED from the substrate — cleanup hooks land via
/// a follow-on amendment that observes the last
/// `Arc<HandleInner>` drop. Exposed here so follow-on tests +
/// extension crates can exercise the registry surface without
/// depending on the `RuntimeHandle` lifecycle.
///
/// # Errors
/// Returns `AttachError::PersistenceError` when the registry lock
/// is poisoned by a previous panic.
pub fn unregister_session(session_id: &SessionId) -> Result<(), AttachError> {
    let mut guard = session_registry()
        .write()
        .map_err(|e| AttachError::PersistenceError(format!("session registry poisoned: {e}")))?;
    guard.remove(session_id);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn persist_event_cursor_returns_feature_not_enabled_without_feature() {
        // When the feature is OFF, `persist_event_cursor` returns
        // `FeatureNotEnabled` so the CLI can map the substrate
        // error to `OctoCliError::PersistenceError` (exit 57).
        let res = persist_event_cursor(Uuid::new_v4(), 0);
        assert!(
            matches!(res, Err(PersistenceError::FeatureNotEnabled)),
            "got {res:?}"
        );
    }

    #[test]
    fn load_event_cursor_returns_feature_not_enabled_without_feature() {
        let res = load_event_cursor(Uuid::new_v4());
        assert!(
            matches!(res, Err(PersistenceError::FeatureNotEnabled)),
            "got {res:?}"
        );
    }

    #[test]
    fn is_token_revoked_returns_false_for_unrevoked_session() {
        let session_id = [0xaa; 32];
        assert!(!is_token_revoked(&session_id));
    }

    #[test]
    fn revoke_then_is_revoked_round_trip() {
        let session_id = [0xbb; 32];
        // Idempotent: revoke twice is fine.
        revoke_attach_token(session_id).expect("revoke 1");
        revoke_attach_token(session_id).expect("revoke 2 (idempotent)");
        assert!(is_token_revoked(&session_id));
    }

    #[test]
    fn other_session_not_revoked() {
        // Adding session A must not affect session B.
        let a = [0x01; 32];
        let b = [0x02; 32];
        revoke_attach_token(a).expect("revoke a");
        assert!(is_token_revoked(&a));
        assert!(!is_token_revoked(&b));
    }

    /// Build a minimal `SessionBinding` for tests. The broadcast
    /// channel is created with capacity 1 (sufficient for
    /// `subscribe()` round-trip assertions without depending on
    /// tokio runtime).
    fn make_binding() -> Arc<SessionBinding> {
        let (tx, _rx) = tokio::sync::broadcast::channel::<RuntimeEvent>(1);
        Arc::new(SessionBinding {
            event_tx: tx,
            last_since_unix: AtomicU64::new(0),
        })
    }

    #[test]
    fn session_registry_register_lookup_round_trip() {
        let session_id = [0x42; 32];
        let binding = make_binding();
        register_session(session_id, Arc::clone(&binding)).expect("register");
        let got = lookup_session(&session_id).expect("lookup yields Some");
        // Same `Arc` payload — same broadcast sender identity.
        assert!(
            Arc::ptr_eq(&got, &binding),
            "register_session returns the same Arc on lookup"
        );
        // Cursor is the initial value (0) — no bind has fired yet.
        assert_eq!(
            got.last_since_unix
                .load(std::sync::atomic::Ordering::Relaxed),
            0
        );
    }

    #[test]
    fn session_registry_lookup_returns_none_when_not_registered() {
        let session_id = [0x99; 32];
        // No prior register_session call — lookup must yield None.
        assert!(lookup_session(&session_id).is_none());
    }
}
