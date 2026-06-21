//! Adapter lifecycle — the outer state machine the gateway
//! polls.
//!
//! The state machine is intentionally simple. The gateway only
//! cares about three transitions:
//!
//! - `Uninitialised → Connecting`: a `connect()` call was made.
//! - `Connecting → Ready`: the client is authorised and
//!   `receive_messages` is callable. (For bot mode, this is the
//!   state after `bot_sign_in`; for user mode, after
//!   `sign_in`/`check_password`.)
//! - `Ready → ShuttingDown → Stopped`: graceful teardown.
//!
//! The user-mode auth sub-flow (RequestCode → SubmitCode →
//! optional SubmitPassword) lives in `auth::AuthStateKey`; the
//! outer `Lifecycle` reports the *combined* state to the gateway
//! (e.g., a user-mode adapter in the middle of
//! `RequestCode`/`SubmitCode` is reported as
//! `Lifecycle::Authenticating`, not as
//! `AuthStateKey::CodeRequested`).
//!
//! The state is held behind a `parking_lot::Mutex` (matching
//! the rest of the workspace) and exposed via a `Status` trait
//! so tests and CLI tools can poll without owning the adapter.

use parking_lot::Mutex;
use std::fmt;
use std::sync::Arc;

use crate::auth::AuthStateKey;

/// Top-level state of the adapter, as seen by the gateway.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum AdapterLifecycle {
    /// No `connect()` call yet.
    #[default]
    Uninitialised,
    /// `connect()` was called; the underlying grammers client
    /// is establishing the MTProto session and registering with
    /// Telegram's DC.
    Connecting,
    /// Connected but not yet authenticated (user mode
    /// pre-sign-in, or bot mode pre-`bot_sign_in`).
    Connected,
    /// Sign-in in progress. For bot mode this is brief
    /// (sub-second); for user mode this is the entire
    /// RequestCode → SubmitCode → SubmitPassword flow.
    Authenticating,
    /// Authenticated and ready to send/receive.
    Ready,
    /// `shutdown()` was called; flushing pending messages.
    ShuttingDown,
    /// Shut down. `send_envelope` / `receive_messages` return
    /// `MtprotoTelegramError::NotReady`.
    Stopped,
    /// An unrecoverable error occurred (e.g., FLOOD_WAIT
    /// exceeded the retry budget, account banned, schema
    /// migration failed). The adapter is no longer usable;
    /// the gateway should construct a new one.
    Failed,
}

impl fmt::Display for AdapterLifecycle {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let s = match self {
            Self::Uninitialised => "Uninitialised",
            Self::Connecting => "Connecting",
            Self::Connected => "Connected",
            Self::Authenticating => "Authenticating",
            Self::Ready => "Ready",
            Self::ShuttingDown => "ShuttingDown",
            Self::Stopped => "Stopped",
            Self::Failed => "Failed",
        };
        f.write_str(s)
    }
}

impl AdapterLifecycle {
    /// True if the state is terminal (Stopped or Failed).
    /// The adapter is no longer usable in this state.
    pub fn is_terminal_state(&self) -> bool {
        matches!(self, Self::Stopped | Self::Failed)
    }
}

/// Valid transitions. Used by `Lifecycle::transition_to` to
/// reject out-of-order calls (e.g., `Ready → Connecting`).
const VALID_TRANSITIONS: &[(AdapterLifecycle, &[AdapterLifecycle])] = &[
    (AdapterLifecycle::Uninitialised, &[AdapterLifecycle::Connecting, AdapterLifecycle::Failed]),
    (AdapterLifecycle::Connecting, &[AdapterLifecycle::Connected, AdapterLifecycle::Authenticating, AdapterLifecycle::Failed]),
    (AdapterLifecycle::Connected, &[AdapterLifecycle::Authenticating, AdapterLifecycle::Ready, AdapterLifecycle::Failed, AdapterLifecycle::ShuttingDown]),
    (AdapterLifecycle::Authenticating, &[AdapterLifecycle::Ready, AdapterLifecycle::Failed, AdapterLifecycle::ShuttingDown]),
    (AdapterLifecycle::Ready, &[AdapterLifecycle::ShuttingDown, AdapterLifecycle::Failed]),
    (AdapterLifecycle::ShuttingDown, &[AdapterLifecycle::Stopped, AdapterLifecycle::Failed]),
    (AdapterLifecycle::Stopped, &[]),
    (AdapterLifecycle::Failed, &[AdapterLifecycle::Stopped]),
];

/// Outer state machine. Cheap to clone (`Arc<Mutex<...>>`).
#[derive(Clone, Default)]
pub struct Lifecycle {
    inner: Arc<Mutex<Inner>>,
}

#[derive(Default)]
struct Inner {
    lifecycle: AdapterLifecycle,
    auth: AuthStateKey,
}

impl Lifecycle {
    pub fn new() -> Self {
        Self::default()
    }

    /// Current outer state.
    pub fn state(&self) -> AdapterLifecycle {
        self.inner.lock().lifecycle
    }

    /// Current user-mode auth sub-state. Returns
    /// `AuthStateKey::Uninitialised` for bot mode and for
    /// adapters that have not yet entered user mode.
    pub fn auth_state(&self) -> AuthStateKey {
        self.inner.lock().auth.clone()
    }

    /// Try to transition to `next`. Returns `Ok(())` if the
    /// transition is in `VALID_TRANSITIONS`; `Err` otherwise.
    /// The `auth` sub-state is updated alongside the outer
    /// state when `next` is `Authenticating` or `Ready`.
    pub fn transition(
        &self,
        next: AdapterLifecycle,
        auth: AuthStateKey,
    ) -> Result<(), TransitionError> {
        let mut g = self.inner.lock();
        if !VALID_TRANSITIONS
            .iter()
            .find(|(from, _)| *from == g.lifecycle)
            .map(|(_, allowed)| allowed.contains(&next))
            .unwrap_or(false)
        {
            return Err(TransitionError {
                from: g.lifecycle,
                to: next,
            });
        }
        g.lifecycle = next;
        g.auth = auth;
        Ok(())
    }

    /// Force-set the state (used by the constructor and by the
    /// `sign_out` path that needs to go `Ready → Stopped`
    /// directly, skipping `ShuttingDown`).
    pub fn force(&self, next: AdapterLifecycle, auth: AuthStateKey) {
        let mut g = self.inner.lock();
        g.lifecycle = next;
        g.auth = auth;
    }

    /// True if the adapter can accept `send_envelope` /
    /// `receive_messages` calls.
    pub fn is_ready(&self) -> bool {
        matches!(self.inner.lock().lifecycle, AdapterLifecycle::Ready)
    }

    /// True if the adapter has terminated (Stopped or Failed).
    /// The gateway should drop the instance in either case.
    pub fn is_terminal(&self) -> bool {
        matches!(
            self.inner.lock().lifecycle,
            AdapterLifecycle::Stopped | AdapterLifecycle::Failed
        )
    }
}

#[derive(Debug, Clone, thiserror::Error)]
#[error("invalid lifecycle transition: {from} -> {to}")]
pub struct TransitionError {
    pub from: AdapterLifecycle,
    pub to: AdapterLifecycle,
}

impl std::fmt::Debug for Lifecycle {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let g = self.inner.lock();
        f.debug_struct("Lifecycle")
            .field("lifecycle", &g.lifecycle)
            .field("auth", &g.auth)
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fresh_lifecycle_is_uninitialised() {
        let l = Lifecycle::new();
        assert_eq!(l.state(), AdapterLifecycle::Uninitialised);
        assert!(!l.is_ready());
        assert!(!l.is_terminal());
    }

    #[test]
    fn happy_path() {
        let l = Lifecycle::new();
        l.transition(AdapterLifecycle::Connecting, AuthStateKey::Uninitialised).unwrap();
        l.transition(AdapterLifecycle::Authenticating, AuthStateKey::Uninitialised).unwrap();
        l.transition(AdapterLifecycle::Ready, AuthStateKey::SignedIn).unwrap();
        assert!(l.is_ready());
    }

    #[test]
    fn invalid_transition_rejected() {
        let l = Lifecycle::new();
        let r = l.transition(AdapterLifecycle::Ready, AuthStateKey::SignedIn);
        assert!(r.is_err());
    }

    #[test]
    fn force_bypasses_check() {
        let l = Lifecycle::new();
        l.force(AdapterLifecycle::Ready, AuthStateKey::SignedIn);
        assert_eq!(l.state(), AdapterLifecycle::Ready);
    }

    #[test]
    fn is_terminal_after_stopped() {
        let l = Lifecycle::new();
        l.force(AdapterLifecycle::Stopped, AuthStateKey::SignedOut);
        assert!(l.is_terminal());
    }

    #[test]
    fn is_terminal_after_failed() {
        let l = Lifecycle::new();
        l.force(AdapterLifecycle::Failed, AuthStateKey::SignedOut);
        assert!(l.is_terminal());
    }

    #[test]
    fn user_mode_auth_substate() {
        let l = Lifecycle::new();
        l.transition(AdapterLifecycle::Connecting, AuthStateKey::Uninitialised).unwrap();
        l.transition(AdapterLifecycle::Authenticating, AuthStateKey::CodeRequested).unwrap();
        assert_eq!(l.auth_state(), AuthStateKey::CodeRequested);
        l.transition(AdapterLifecycle::Ready, AuthStateKey::SignedIn).unwrap();
        assert_eq!(l.auth_state(), AuthStateKey::SignedIn);
    }
}
