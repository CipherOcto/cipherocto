//! Self-handle tracking for the MTProto adapter (mirrors
//! `octo-adapter-telegram::self_handle`).
//!
//! The `PlatformAdapter` trait requires a `self_handle()` accessor
//! so the gateway can drop self-authored messages and avoid relay
//! loops (ZeroClaw pattern, RFC-0850 §8.4). Telegram is a chat
//! platform — its messages have a numeric `from_id`, so the
//! "self handle" is the logged-in user's numeric `user_id`. The
//! `self_handle()` method returns that as a `String` ("user:12345")
//! in the same format the TDLib adapter uses, so the gateway's
//! self-loop filter is identical across both adapters.
//!
//! ## Threading
//!
//! `SelfHandle` wraps an `Arc<Mutex<Option<SelfIdentity>>>` so it
//! can be cheaply cloned and shared between the `PlatformAdapter`
//! impl (which calls `self_handle()`) and the `TelegramMtprotoClient`
//! impl (which writes the identity after `connect()`). The lock is
//! `parking_lot::Mutex` (matching the rest of the workspace).

use parking_lot::Mutex;
use std::sync::Arc;

/// Identity used by the self-loop filter. `user_id` is the
/// Telegram-issued numeric user identifier (`from_id` in MTProto
/// message types). `username` is the optional `@handle` (without
/// the leading `@`); bots and some users do not have one.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct MtprotoSelfIdentity {
    pub user_id: i64,
    pub username: Option<String>,
}

impl MtprotoSelfIdentity {
    pub fn is_set(&self) -> bool {
        self.user_id != 0
    }
    // NB (R15-C15): there is intentionally NO `handle()`
    // method on `MtprotoSelfIdentity`. The canonical
    // self-handle string is built by
    // `PlatformAdapter::self_handle()` (adapter.rs), which
    // returns the `"telegram:user:<id>"` form consumed by
    // the gateway's self-loop filter. A previous version
    // had `handle()` returning `"user:<id>"` (no platform
    // prefix) and a deprecated test; both have been
    // removed so there is a single source of truth for
    // the handle form.
}

/// Shared self-handle. Cheap to clone (`Arc` inside).
#[derive(Clone, Default)]
pub struct MtprotoSelfHandle {
    inner: Arc<Mutex<Option<MtprotoSelfIdentity>>>,
}

impl MtprotoSelfHandle {
    pub fn new() -> Self {
        Self::default()
    }

    /// Set the logged-in user's identity. Called by the
    /// `TelegramMtprotoClient` impl after `connect()` resolves
    /// `get_me()`. Idempotent — the last call wins.
    pub fn set_identity(&self, user_id: i64, username: Option<String>) {
        let mut g = self.inner.lock();
        *g = Some(MtprotoSelfIdentity { user_id, username });
    }

    /// Set just the numeric user_id (used when `get_me()` returns
    /// a user without a username, e.g. a freshly-created bot).
    pub fn set_user_id(&self, user_id: i64) {
        let mut g = self.inner.lock();
        let entry = g.get_or_insert_with(MtprotoSelfIdentity::default);
        entry.user_id = user_id;
    }

    /// Read the current identity. `None` if the adapter has not
    /// yet resolved `get_me()`.
    pub fn get(&self) -> Option<MtprotoSelfIdentity> {
        self.inner.lock().clone()
    }

    /// Drop the cached identity (called from `sign_out` so a
    /// subsequent `connect()` re-resolves it).
    pub fn clear(&self) {
        *self.inner.lock() = None;
    }
}

impl std::fmt::Debug for MtprotoSelfHandle {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let g = self.inner.lock();
        f.debug_struct("MtprotoSelfHandle")
            .field("identity", &g.as_ref())
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identity_is_unset_by_default() {
        let h = MtprotoSelfHandle::new();
        assert!(h.get().is_none());
    }

    #[test]
    fn set_identity_round_trips() {
        let h = MtprotoSelfHandle::new();
        h.set_identity(42, Some("alice".into()));
        let id = h.get().unwrap();
        assert_eq!(id.user_id, 42);
        assert_eq!(id.username.as_deref(), Some("alice"));
    }

    #[test]
    fn set_user_id_alone() {
        let h = MtprotoSelfHandle::new();
        h.set_user_id(99);
        assert_eq!(h.get().unwrap().user_id, 99);
    }

    #[test]
    fn clear_removes_identity() {
        let h = MtprotoSelfHandle::new();
        h.set_user_id(1);
        h.clear();
        assert!(h.get().is_none());
    }

    #[test]
    fn clone_shares_state() {
        let h = MtprotoSelfHandle::new();
        let h2 = h.clone();
        h.set_user_id(7);
        assert_eq!(h2.get().unwrap().user_id, 7);
    }
}
