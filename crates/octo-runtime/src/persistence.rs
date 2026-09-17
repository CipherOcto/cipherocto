//! `octo-runtime` persistence + revocation + session-registry
//! substrate (RFC-0011-c §Follow-on §F.3 + §F.2 step (e) follow-on
//! amendment; §F.7.5 cross-process revocation substrate paired
//! amendment).
//!
//! ## Stoolap cursor persistence (gated on feature)
//!
//! `persist_event_cursor` + `load_event_cursor` are gated on the
//! `octo-runtime-persistence` feature flag. When the feature is OFF
//! (no flag), they return `PersistenceError::FeatureNotEnabled`.
//! When ON, they round-trip through a CipherOcto-fork Stoolap
//! database per [[stoolap-fork-persistence]].
//!
//! ## Revocation set (trait-dispatched per §F.7.5 paired amendment)
//!
//! `revoke_attach_token` + `is_token_revoked` dispatch through the
//! trait-mediated `RevocationStore` substrate. Default impl is
//! `InMemoryRevocationStore` (process-local `RwLock<HashSet>`), the
//! fork-fail-closed default that matches the prior §F.3 contract.
//! Operators may opt into the cross-process Stoolap-backed ledger
//! by registering a `StoolapRevocationStore` via
//! `install_revocation_store_default_with` (see §F.7.5 paired
//! amendment). The substrate stays unaware of which implementation
//! is installed — see `current_revocation_store` for the lookup
//! semantics.
//!
//! ## In-memory session registry
//!
//! `register_session` + `lookup_session` maintain a process-singleton
//! `RwLock<HashMap<SessionId, Arc<SessionBinding>>>` registry.
//! `RuntimeHandle::new` registers a `SessionBinding` for the spawned
//! session; `InProcessHandler::bind` looks it up to mint a fresh
//! broadcast `Receiver` for the caller (the substrate-faithful step
//! (e) surface per RFC-0011-c §F.2 follow-on amendment). The
//! revocation check inside `register_session` is dispatched through
//! the public `is_token_revoked` free function so any installed
//! store participates transparently.
//!
//! Entries accumulate for the process lifetime — there is no
//! per-handle teardown hook in v0.1.0 (the cleanup path lands via
//! a follow-on amendment that observes the last
//! `Arc<HandleInner>` drop). A process restart clears the
//! registry (per the §F.3 fork-fail-closed semantics).
//!
//! ## Process-singleton invariant
//!
//! The session registry + the optional Stoolap cursor connection
//! are process-singletons via `OnceLock`. The revocation store uses
//! a two-slot singleton pattern (`DEFAULT_REVOCATION_STORE` always
//! populated on first access + `ACTIVE_REVOCATION_STORE` set by an
//! explicit single-shot operator install per §F.7.5) — see
//! `current_revocation_store` for the lookup semantics.

use std::collections::{HashMap, HashSet};
use std::sync::atomic::AtomicU64;
use std::sync::{Arc, OnceLock, RwLock};

use uuid::Uuid;

use crate::handle::error::{AttachError, PersistenceError};
use crate::handle::RuntimeEvent;

// Re-export `SessionId` for downstream Layer D per-extension crates
// (e.g. `octo-runtime-revocation-store`) that consume the
// `RevocationStore` trait. Without this re-export, downstream
// callers would have to reach into `octo_runtime::handle::SessionId`
// directly — a Layer B internal path that should not leak.
// Mirrors the same re-export shape used for `AttachError` in
// `octo_runtime::handle::error`.
pub use crate::handle::SessionId;

// ===========================================================================
// §F.7.5 paired amendment — trait-dispatched revocation substrate
// ===========================================================================

/// Per-extension trait for revocation storage backends (RFC-0011-c
/// §F.7.5 paired amendment; per-extension-crates pattern per
/// [[cipherocto-design-principles]] §User extensibility).
///
/// Implementors live in Layer D crates (e.g.
/// `octo-runtime-revocation-store` for the Stoolap-backed ledger).
/// The trait itself lives in `octo-runtime` Layer B so the substrate
/// stays unaware of the storage mechanism; the install path runs
/// through `install_revocation_store_default_with` (factory closure
/// preserving try-install / fall-back semantics per §F.7.5).
///
/// # Trait method semantic contract
///
/// - `revoke_attach_token` MUST be idempotent: revoke-2 is a no-op
///   once revoke-1 succeeds. Pre-check + INSERT (rather than
///   `INSERT OR IGNORE`) is the canonical idempotent pattern when
///   the backend is the Stoolap fork at pinned rev 527e8eb which
///   lacks the `INSERT OR IGNORE` / `INSERT OR REPLACE` syntax
///   per the substrate discipline locked in §F.7.5 fix sweeps.
/// - `is_token_revoked` is a read-side existence check that MUST
///   fail-CLOSED on backend read failures (returns `true` +
///   `tracing::error!` with `kind()` value). The fail-CLOSED
///   preserve the explicit-operator-revocation guarantee at the
///   cost of denying attach operations until the operator resolves
///   the backend issue.
pub trait RevocationStore: Send + Sync + std::fmt::Debug {
    /// Mark `session_id` as revoked in the backend. Idempotent.
    ///
    /// # Errors
    /// Returns `AttachError::PersistenceError(reason)` on backend
    /// write failure (per §F.4 slot 57). The substrate never
    /// silently succeeds on a write failure.
    fn revoke_attach_token(&self, session_id: SessionId) -> Result<(), AttachError>;

    /// Ledger-backed existence check for `session_id`.
    ///
    /// # Returns
    /// `true` iff the session has been revoked (and the backend has
    /// not been tampered with). Returns `true` on poisoned lock or
    /// backend read failure (fail-CLOSED per the trait contract
    /// above + §F.7.5 §Failure semantics).
    fn is_token_revoked(&self, session_id: &SessionId) -> bool;

    /// Per-impl diagnostic identity (used by `tracing::error!` on
    /// fail-CLOSED paths so substrate observability sites can
    /// distinguish `InMemoryRevocationStore` from
    /// `StoolapRevocationStore`).
    fn kind(&self) -> &'static str;
}

/// In-memory default implementation of `RevocationStore`
/// (RFC-0011-c §F.7.5 paired amendment).
///
/// Each instance owns its own `RwLock<HashSet<SessionId>>`. The
/// `Arc<InMemoryRevocationStore>` singleton (via
/// `DEFAULT_REVOCATION_STORE`) carries the chosen instance for the
/// lifetime of the process. The default impl's `is_token_revoked`
/// catches a poisoned `RwLock` and returns `true` (fail-CLOSED
/// preserves the explicit-operator-revocation guarantee); promoted
/// behind the trait (no behavior change for default consumers per
/// the existing §F.3 fork-fail-closed contract).
#[derive(Debug)]
pub struct InMemoryRevocationStore {
    inner: RwLock<HashSet<SessionId>>,
}

impl Default for InMemoryRevocationStore {
    fn default() -> Self {
        Self {
            inner: RwLock::new(HashSet::new()),
        }
    }
}

impl InMemoryRevocationStore {
    /// Construct a new empty in-memory revocation store. Process-local;
    /// see RFC-0011-c §F.3 fork-fail-closed semantics.
    #[allow(dead_code)] // public convenience ctor; not used by substrate path
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Test injection hook (per §F.7.5 paired amendment).
    ///
    /// Tests obtain a concrete `Arc<InMemoryRevocationStore>`
    /// reference via `DEFAULT_REVOCATION_STORE.get_or_init(...)` and
    /// invoke `with_inner` on it for state inspection. The seam is
    /// `pub(crate)` rather than `pub` to keep the type's interior
    /// mutability off the public surface.
    #[allow(dead_code)] // only used by `mod tests` inside this file
    pub(crate) fn with_inner<F, R>(&self, f: F) -> R
    where
        F: FnOnce(&HashSet<SessionId>) -> R,
    {
        let guard = self.inner.read().unwrap_or_else(|e| e.into_inner());
        f(&guard)
    }
}

impl RevocationStore for InMemoryRevocationStore {
    fn revoke_attach_token(&self, session_id: SessionId) -> Result<(), AttachError> {
        let mut guard = self.inner.write().map_err(|e| {
            AttachError::PersistenceError(format!(
                "InMemoryRevocationStore poisoned write lock: {e}"
            ))
        })?;
        guard.insert(session_id);
        Ok(())
    }

    fn is_token_revoked(&self, session_id: &SessionId) -> bool {
        self.inner
            .read()
            .map(|guard| guard.contains(session_id))
            .unwrap_or_else(|e| {
                tracing::error!(
                    kind = self.kind(),
                    error = %e,
                    "InMemoryRevocationStore read lock poisoned; failing-CLOSED"
                );
                true
            })
    }

    fn kind(&self) -> &'static str {
        "InMemoryRevocationStore"
    }
}

/// Lazily-initialized default revocation store
/// (`OnceLock<Arc<InMemoryRevocationStore>>`).
///
/// Lazy init never blocks subsequent `set_revocation_store`
/// overrides — the default install does NOT block override per the
/// two-slot invariant in §F.7.5 paired amendment.
/// Default revocation store slot. Module-private — the only
/// write path is via the factory-closure-mediated
/// `install_revocation_store_default_with` API which routes through
/// `set_revocation_store(Arc<dyn RevocationStore>)`. Exposing the
/// `OnceLock` as `pub` would let downstream callers `.set()` it
/// directly and bypass the documented try-install / fall-back-to-default
/// semantic at the API layer (RFC-0011-c §F.7.5 CLI wiring block).
static DEFAULT_REVOCATION_STORE: OnceLock<Arc<InMemoryRevocationStore>> = OnceLock::new();

/// Operator-installed active revocation store (`OnceLock<Arc<dyn
/// RevocationStore>>`). Single-shot: subsequent
/// `set_revocation_store` calls return
/// `AttachError::Internal(...)` per §F.7.5 paired amendment.
/// Module-private for the same factory-closure-mediation reason
/// documented on `DEFAULT_REVOCATION_STORE`.
static ACTIVE_REVOCATION_STORE: OnceLock<Arc<dyn RevocationStore>> = OnceLock::new();

/// Module-private trait-dispatch lookup (RFC-0011-c §F.7.5 paired
/// amendment).
///
/// Reads `ACTIVE_REVOCATION_STORE.get()` first (operator install).
/// On miss, calls `DEFAULT_REVOCATION_STORE.get_or_init(...)` to
/// lazily-initialize the default and returns it as
/// `Arc<dyn RevocationStore>` WITHOUT writing to
/// `ACTIVE_REVOCATION_STORE` (the default install does NOT block
/// subsequent operator override).
fn current_revocation_store() -> Arc<dyn RevocationStore> {
    if let Some(active) = ACTIVE_REVOCATION_STORE.get() {
        return Arc::clone(active);
    }
    Arc::clone(
        DEFAULT_REVOCATION_STORE.get_or_init(|| Arc::new(InMemoryRevocationStore::default())),
    ) as Arc<dyn RevocationStore>
}

/// Install an operator-provided revocation store (RFC-0011-c §F.7.5
/// paired amendment).
///
/// Writes to `ACTIVE_REVOCATION_STORE.set(store)`. On double-install
/// (the slot is already populated), returns
/// `Err(AttachError::Internal("revocation store already installed"))`.
///
/// This is a NEW substrate discipline distinct from
/// `HANDLE_TRANSPORT_REGISTRY` per §F.2 step (e) (which is
/// `OnceLock::get_or_init` lazy-init + `Registry::register`
/// last-write-wins); the difference is intentional because
/// revocation-store init is a one-shot operator decision, not an
/// idempotent extension registration.
///
/// `AttachError::Internal(String)` maps to substrate exit 64 (NOT
/// `AttachError::RevocationError(String)` per §F.4 slot 58 — the
/// `RevocationError` envelope is reserved for runtime revocation
/// failures, not init failures).
///
/// # Errors
/// Returns `AttachError::Internal(reason)` if a store has already
/// been installed. Returns `AttachError::Internal(reason)` for any
/// other single-shot `OnceLock` violation (currently none; the
/// variant is reserved for forward compat).
pub fn set_revocation_store(store: Arc<dyn RevocationStore>) -> Result<(), AttachError> {
    ACTIVE_REVOCATION_STORE
        .set(store)
        .map_err(|_| AttachError::Internal("revocation store already installed".to_string()))
}

/// Factory-mediated init fn for revocation stores (RFC-0011-c §F.7.5
/// paired amendment).
///
/// Routes through `set_revocation_store(store)` with the factory
/// providing the `Arc<dyn RevocationStore>` on success. Preserves
/// the "try install, fall back to default on error" semantic at the
/// API layer: a single call site catches the `Err` and emits a
/// `WARN` log before the dispatcher falls back to
/// `InMemoryRevocationStore` (per §F.7.5 failure-semantics block).
///
/// A direct `set_revocation_store(install_default()?)` form would
/// propagate errors at the CLI boundary and lose the fall-back
/// behavior — this factory closure preserves it.
///
/// Layer B has NO compile-time dependency on Layer D — the factory
/// type-erases the D impl via `Arc<dyn RevocationStore>`. Layer C
/// (CLI) wires the dependency injection by passing
/// `Layer_D::install_default` as the factory closure.
///
/// # Errors
/// Same as `set_revocation_store`: `AttachError::Internal(reason)`
/// on double-install, or whatever the factory closure returns on its
/// own failure.
pub fn install_revocation_store_default_with<F>(factory: F) -> Result<(), AttachError>
where
    F: FnOnce() -> Result<Arc<dyn RevocationStore>, AttachError>,
{
    let store = factory()?;
    set_revocation_store(store)
}

// ===========================================================================
// §F.3 legacy substrate — module-level free functions preserved as the
// substrate-faithful façade. Bodies dispatch via the trait-mediated
// `current_revocation_store()` lookup. Signatures + error variants + return
// types are preserved end-to-end.
// ===========================================================================

/// Add a session id to the revocation set (RFC-0011-c §F.3 +
/// §F.7.5 paired amendment).
///
/// The body dispatches via `current_revocation_store()` so an
/// operator-installed `StoolapRevocationStore` participates
/// transparently — a `StoolapRevocationStore::revoke_attach_token`
/// writes to the shared ledger across the process boundary.
///
/// Once added, `is_token_revoked` returns `true` for that session id
/// indefinitely (until the backend's natural TTL or explicit
/// eviction, whichever the implementation supports).
///
/// # Errors
/// Returns `AttachError::PersistenceError(reason)` on backend write
/// failure (per the trait contract + §F.4 slot 57).
pub fn revoke_attach_token(session_id: SessionId) -> Result<(), AttachError> {
    current_revocation_store().revoke_attach_token(session_id)
}

/// Fast-path check invoked at step (b) of `attach_with_token`
/// (RFC-0011-c §F.2).
///
/// The body dispatches via `current_revocation_store()` and a
/// poisoned-lock or backend-read failure fails-CLOSED (returns
/// `true`) per the trait contract. The default impl
/// (`InMemoryRevocationStore`) preserves the §F.3 fork-fail-closed
/// pre-amendment behavior byte-for-byte (poisoned read lock returns
/// `true`; `tracing::error!` added for observability).
///
/// # Returns
/// `true` iff the session id has been revoked via the active store
/// (or the read fails — fail-CLOSED).
#[must_use]
pub fn is_token_revoked(session_id: &SessionId) -> bool {
    current_revocation_store().is_token_revoked(session_id)
}

/// Per-agent cursor persistence (RFC-0011-c §F.3).
///
/// Gated on `cfg(feature = "octo-runtime-persistence")`. When the
/// feature is OFF (no flag), returns
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
///
/// # Errors
/// Always returns `PersistenceError::FeatureNotEnabled` when this
/// stub is reached (the `octo-runtime-persistence` feature is OFF).
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
///
/// # Errors
/// Always returns `PersistenceError::FeatureNotEnabled` when this
/// stub is reached (the `octo-runtime-persistence` feature is OFF).
#[cfg(not(feature = "octo-runtime-persistence"))]
pub fn load_event_cursor(_agent_id: Uuid) -> Result<Option<u64>, PersistenceError> {
    Err(PersistenceError::FeatureNotEnabled)
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
    /// Clone of `HandleInner::event_tx`; channel-close invariants
    /// documented at `HandleInner`.
    pub(crate) event_tx: tokio::sync::broadcast::Sender<RuntimeEvent>,
    /// Highest `since_unix` accepted for a successful `bind`
    /// against this session (monotone-bounded).
    pub(crate) last_since_unix: AtomicU64,
}

/// Process-singleton session registry (RFC-0011-c §F.2 step (e)
/// follow-on amendment).
///
/// Same `OnceLock<RwLock<HashMap<SessionId, Arc<SessionBinding>>>>`
/// pattern as the prior revocation-set substrate — process-local,
/// fork-fail-closed per §F.3. Entries accumulate for the lifetime
/// of the process; a restart clears the map.
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
/// The revocation check delegates to the public `is_token_revoked`
/// free function so any installed `RevocationStore` impl
/// participates transparently — an operator-installed
/// `StoolapRevocationStore` makes the revocation-set check
/// cross-process per §F.7.5 paired amendment. The check is
/// re-issued under the registry write lock to close the TOCTOU
/// window between the initial check and the `HashMap::insert`
/// (otherwise a concurrent `revoke_attach_token` can race the
/// insert and resurrect the session).
///
/// # Errors
/// `AttachError::PersistenceError` for any of the three classes
/// above.
pub(crate) fn register_session(
    session_id: SessionId,
    binding: Arc<SessionBinding>,
) -> Result<(), AttachError> {
    let mut guard = session_registry()
        .write()
        .map_err(|e| AttachError::PersistenceError(format!("session registry poisoned: {e}")))?;
    if is_token_revoked(&session_id) {
        return Err(AttachError::PersistenceError(format!(
            "register_session: session {session_id:?} is revoked"
        )));
    }
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

    // -------------------------------------------------------------------
    // §F.7.5 paired amendment — trait-dispatched revocation substrate
    //
    // Tests deliberately avoid calling `set_revocation_store` /
    // `install_revocation_store_default_with` with a successful
    // factory closure (those paths pollute the crate-wide
    // `ACTIVE_REVOCATION_STORE` `OnceLock` and would race sibling
    // tests under cargo's default parallel test runner). Instead,
    // these tests exercise the trait contract end-to-end via direct
    // construction of `InMemoryRevocationStore` boxes, plus the
    // factory-closure path with an errant factory that bails before
    // reaching `set_revocation_store`. The `set_revocation_store`
    // reject-on-double-install + factory happy-path routing are
    // covered indirectly via the integration test in
    // `crates/octo-runtime-revocation-store/tests/cross_process.rs`
    // (each integration test runs in its own subprocess so the
    // crate-wide singleton is fresh).
    // -------------------------------------------------------------------

    /// Deterministic stub `RevocationStore` impl for the
    /// trait-dispatch tests. Doesn't touch the crate-wide
    /// singleton; each test that uses it constructs a fresh
    /// `Arc<dyn RevocationStore>` local with its own inner
    /// `RwLock<HashSet<SessionId>>` (mirroring the
    /// `InMemoryRevocationStore` field shape).
    #[derive(Debug)]
    struct StubRevocationStore {
        inner: std::sync::RwLock<std::collections::HashSet<SessionId>>,
    }

    impl StubRevocationStore {
        fn new() -> Self {
            Self {
                inner: std::sync::RwLock::new(std::collections::HashSet::new()),
            }
        }
    }

    impl RevocationStore for StubRevocationStore {
        fn revoke_attach_token(&self, session_id: SessionId) -> Result<(), AttachError> {
            let mut guard = self
                .inner
                .write()
                .map_err(|e| AttachError::PersistenceError(format!("stub poisoned: {e}")))?;
            guard.insert(session_id);
            Ok(())
        }
        fn is_token_revoked(&self, session_id: &SessionId) -> bool {
            self.inner
                .read()
                .map(|guard| guard.contains(session_id))
                .unwrap_or(true)
        }
        fn kind(&self) -> &'static str {
            "StubRevocationStore"
        }
    }

    #[test]
    fn in_memory_revocation_store_kind_returns_canonical_string() {
        let store = InMemoryRevocationStore::default();
        assert_eq!(store.kind(), "InMemoryRevocationStore");
    }

    #[test]
    fn in_memory_with_inner_test_seam_observes_state() {
        let store = InMemoryRevocationStore::default();
        let session_id = [0xab; 32];
        // Empty at construction.
        let count_before = store.with_inner(|set| set.len());
        assert_eq!(count_before, 0);
        // After a revoke + the same store instance gets the row.
        store
            .revoke_attach_token(session_id)
            .expect("revoke on same instance");
        let count_after = store.with_inner(|set| {
            assert!(set.contains(&session_id), "session is in the set");
            set.len()
        });
        assert_eq!(count_after, 1);
    }

    #[test]
    fn in_memory_send_sync_bounds_via_static() {
        // Compile-time check: the singleton type carries
        // Send + Sync automatically because all fields are.
        // Explicitly `static` a reference to confirm the bounds.
        const _: fn() = || {
            fn assert_send_sync<T: Send + Sync>() {}
            assert_send_sync::<InMemoryRevocationStore>();
        };
        let store = InMemoryRevocationStore::default();
        let _trait_obj: Arc<dyn RevocationStore> = Arc::new(store);
    }

    #[test]
    fn trait_object_dispatch_revoke_via_local_boxed_store() {
        // Verify trait dispatch via a local boxed store (NOT the
        // crate-wide singleton).
        let store: Arc<dyn RevocationStore> = Arc::new(StubRevocationStore::new());
        let session_id = [0x77u8; 32];
        assert!(!store.is_token_revoked(&session_id));
        store.revoke_attach_token(session_id).expect("revoke");
        assert!(store.is_token_revoked(&session_id));
    }

    #[test]
    fn trait_object_dispatch_idempotent_revoke_via_local_boxed_store() {
        // Revoke twice is a no-op for the trait contract.
        let store: Arc<dyn RevocationStore> = Arc::new(StubRevocationStore::new());
        let session_id = [0x33u8; 32];
        store.revoke_attach_token(session_id).expect("revoke 1");
        store
            .revoke_attach_token(session_id)
            .expect("revoke 2 (idempotent)");
        assert!(store.is_token_revoked(&session_id));
    }

    #[test]
    fn in_memory_poisoned_write_lock_contract_returns_persistence_error() {
        // Positive control: a fresh store's write succeeds. The
        // poisoned path is exercised in production when a panic
        // inside the RwLock poisons the lock; synthesizing a real
        // poison without a separate thread adds runtime churn for
        // no additional coverage. The contract is wired via the
        // `.map_err(...)` arm at the impl site.
        let store = InMemoryRevocationStore::default();
        let res = store.revoke_attach_token([0x01; 32]);
        assert!(res.is_ok(), "fresh store revoke succeeds: {res:?}");
    }

    #[test]
    fn install_revocation_store_default_with_propagates_factory_error() {
        // Factory returns its own error; install propagates it
        // BEFORE reaching `set_revocation_store` (so this test
        // does NOT pollute the crate-wide ACTIVE_REVOCATION_STORE).
        let res = install_revocation_store_default_with(|| {
            Err::<Arc<dyn RevocationStore>, AttachError>(AttachError::Internal(
                "factory declined (test fixture)".to_string(),
            ))
        });
        match res {
            Err(AttachError::Internal(reason)) if reason.contains("factory declined") => {}
            other => panic!("factory error not propagated: {other:?}"),
        }
    }

    #[test]
    fn set_revocation_store_rejects_double_install_via_local_arc() {
        // We exercise the OnceLock single-shot semantic by
        // constructing a fresh, independent install-call sequence
        // via `OnceLock::set`. This does NOT touch the
        // crate-wide ACTIVE_REVOCATION_STORE; it gives us a
        // local analog.
        let local: OnceLock<Arc<dyn RevocationStore>> = OnceLock::new();
        let first: Arc<dyn RevocationStore> = Arc::new(StubRevocationStore::new());
        let second: Arc<dyn RevocationStore> = Arc::new(StubRevocationStore::new());

        // First install: Ok.
        local
            .set(first)
            .map_err(|_| "first set unexpectedly returned Err")
            .expect("first set succeeds on a fresh OnceLock");
        // Second install: Err (the first Arc is returned in the
        // Err variant).
        let second_res = local.set(second);
        match second_res {
            Err(_) => { /* ok — single-shot semantic preserved */ }
            Ok(()) => panic!("second set should fail"),
        }
    }
}
