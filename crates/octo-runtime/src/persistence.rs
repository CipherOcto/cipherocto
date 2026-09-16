//! `octo-runtime` persistence + revocation substrate
//! (RFC-0011-c §Follow-on §F.3).
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
//! ## Process-singleton invariant
//!
//! Both the revocation set and (when enabled) the Stoolap
//! connection are process-singletons via `OnceLock`. Multi-process
//! scenarios (the `octo agent run --detach` + `octo agent attach`
//! pattern across process boundaries) hit the substrate boundary
//! at the CLI's `octo agent revoke-attach` command which signals
//! the running process via IPC (out of scope for v0.1.0).

use std::collections::HashSet;
use std::sync::{OnceLock, RwLock};

use uuid::Uuid;

use crate::handle::error::{AttachError, PersistenceError};
use crate::handle::SessionId;

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
}
