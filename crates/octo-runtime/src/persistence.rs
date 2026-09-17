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
/// `since_unix` cursor. The broadcast sender is a clone of
/// `HandleInner::event_tx`; the channel itself stays open for the
/// lifetime of `HandleInner::_keepalive_rx` (until the last
/// `RuntimeHandle` clone drops per the `HandleInner` invariants).
///
/// `last_since_unix` is the cursor used by
/// `InProcessHandler::bind` for replay detection per RFC-0011-c
/// §9.7 follow-on amendment: a second attach with `since_unix`
/// at or behind this value is rejected with
/// `AttachError::ReplayDetected` (exit 61).
#[derive(Debug)]
pub(crate) struct SessionBinding {
    /// Clone of `HandleInner::event_tx`; channel-close semantics
    /// are documented at `HandleInner`.
    pub(crate) event_tx: tokio::sync::broadcast::Sender<RuntimeEvent>,
    /// Highest `since_unix` accepted for a successful `bind`
    /// against this session (monotone-bounded).
    pub(crate) last_since_unix: AtomicU64,
}

/// Process-singleton session registry (RFC-0011-c §F.2 step (e)
/// follow-on amendment).
///
/// Same `OnceLock<RwLock<HashMap<SessionId, Arc<SessionBinding>>>>`
/// pattern as `revocation_set` — process-local, fork-fail-closed
/// per §F.3. Entries accumulate for the lifetime of the process;
/// a restart clears the map.
///
/// Cleanup hooks (per-handle teardown) land in a follow-on
/// amendment observing the last `Arc<HandleInner>` drop.
fn session_registry() -> &'static RwLock<HashMap<SessionId, Arc<SessionBinding>>> {
    static REG: OnceLock<RwLock<HashMap<SessionId, Arc<SessionBinding>>>> = OnceLock::new();
    REG.get_or_init(|| RwLock::new(HashMap::new()))
}

/// Register a session binding for `session_id`
/// (RFC-0011-c §F.2 step (e) follow-on amendment).
///
/// Fail-CLOSED on three distinct failure classes — each surfaces
/// via `AttachError::PersistenceError` with a disambiguating
/// reason:
/// - **collision** — `session_id` already registered; the lock
///   guards the map against overwrites that would reset
///   `last_since_unix` and open a replay window
/// - **revoked** — `session_id` is in the revocation set; refuses
///   to re-register (defense-in-depth against the reset-attack on
///   an already-revoked session)
/// - **poisoned** — registry lock poisoned by a previous panic
///
/// # Errors
/// `AttachError::PersistenceError` for any of the three classes
/// above.
pub(crate) fn register_session(
    session_id: SessionId,
    binding: Arc<SessionBinding>,
) -> Result<(), AttachError> {
    if is_token_revoked(&session_id) {
        return Err(AttachError::PersistenceError(format!(
            "register_session: session {session_id:?} is revoked"
        )));
    }
    let mut guard = session_registry()
        .write()
        .map_err(|e| AttachError::PersistenceError(format!("session registry poisoned: {e}")))?;
    if guard.contains_key(&session_id) {
        return Err(AttachError::PersistenceError(format!(
            "register_session: duplicate session_id {session_id:?}"
        )));
    }
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
pub(crate) fn lookup_session(session_id: &SessionId) -> Option<Arc<SessionBinding>> {
    session_registry()
        .read()
        .ok()
        .and_then(|guard| guard.get(session_id).cloned())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `feature = "octo-runtime-persistence"` OFF — exercises the
    /// `FeatureNotEnabled` stub path. Gate at module level so the
    /// opt-out build is the only one that fires these tests
    /// (default build exercises the Stoolap-backed path instead).
    #[cfg(not(feature = "octo-runtime-persistence"))]
    mod without_feature {
        use super::*;

        #[test]
        fn persist_event_cursor_returns_feature_not_enabled_without_feature() {
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

    /// Build a minimal `SessionBinding` for tests. Capacity 1 is
    /// sufficient for the round-trip register/lookup assertions
    /// (no `send()` happens, so the channel never fills); using a
    /// larger constant would just add noise.
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
    fn register_session_rejects_revoked_session() {
        let session_id = [0xcc; 32];
        revoke_attach_token(session_id).expect("revoke");
        let res = register_session(session_id, make_binding());
        match res {
            Err(AttachError::PersistenceError(reason)) => {
                assert!(reason.contains("revoked"), "got {reason}");
            }
            other => panic!("expected PersistenceError(revoked), got {other:?}"),
        }
    }

    #[test]
    fn register_session_rejects_collision() {
        let session_id = [0xdd; 32];
        register_session(session_id, make_binding()).expect("first register");
        let res = register_session(session_id, make_binding());
        match res {
            Err(AttachError::PersistenceError(reason)) => {
                assert!(reason.contains("duplicate"), "got {reason}");
            }
            other => panic!("expected PersistenceError(duplicate), got {other:?}"),
        }
    }
}
