//! Self-loop prevention via getMe + cache.
//!
//! Mission AC line 139: "Self-loop prevention: `self_handle()` returns the
//! bot's user_id (or user_id for user mode) to drop self-authored messages."
//!
//! The `SelfHandle` struct caches the bot's `user_id` (i64) and `username`
//! to avoid calling getMe on every message. Real TDLib implementation calls
//! `tdlib_rs::functions::get_me()` once at startup; mock returns a cached
//! value set by test code.

use std::sync::{Arc, Mutex};

/// Identity cached for self-loop prevention.
///
/// `user_id` is the canonical identifier (TDLib's `User::id` is i64). The
/// optional `username` is kept for human-readable display but is *not* used
/// for the self-loop comparison — that is always done numerically on
/// `user_id` to avoid the brittleness of string comparison.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SelfIdentity {
    pub user_id: i64,
    pub username: String,
}

/// Caches the bot's identity for self-loop prevention.
/// Real impl: calls TDLib `get_me` once at startup; mock: returns cached value.
///
/// H8: wraps the cached identity in an `Arc` so a `SelfHandle` can be
/// cheaply cloned and shared between the (real) TDLib client and the
/// adapter. Both sides operate on the same underlying cell, so the
/// adapter's self-loop filter sees the user_id the client populated
/// from `get_me`.
#[derive(Debug, Clone)]
pub struct SelfHandle {
    cached: Arc<Mutex<Option<SelfIdentity>>>,
}

impl SelfHandle {
    /// Create a new SelfHandle with no cached identity.
    pub fn new() -> Self {
        Self {
            cached: Arc::new(Mutex::new(None)),
        }
    }

    /// Cache the bot's numeric user_id.
    /// Real impl: called after TDLib `get_me` succeeds; mock: called by test setup.
    pub fn set_user_id(&self, user_id: i64) {
        let mut guard = self.cached.lock().unwrap();
        let username = guard
            .as_ref()
            .map(|s| s.username.clone())
            .unwrap_or_default();
        *guard = Some(SelfIdentity { user_id, username });
    }

    /// Cache the bot username (typically set together with `set_user_id`
    /// after a successful `get_me`).
    ///
    /// M9: a no-op if `set_user_id` has not been called first. The
    /// username alone is insufficient to identify "self" — without the
    /// numeric `user_id` we cannot compare incoming message senders, and
    /// inserting a `user_id=0` sentinel would silently mis-filter.
    pub fn set_username(&self, username: String) {
        let mut guard = self.cached.lock().unwrap();
        match guard.as_mut() {
            Some(identity) => identity.username = username,
            None => {
                tracing::warn!(
                    username = %username,
                    "set_username called before set_user_id; ignoring (would create user_id=0 sentinel)"
                );
            }
        }
    }

    /// Cache both user_id and username in one call (R7 OBS-M1).
    pub fn set_identity(&self, user_id: i64, username: String) {
        tracing::info!(user_id, username = %username, "SelfHandle: identity set");
        *self.cached.lock().unwrap() = Some(SelfIdentity { user_id, username });
    }

    /// Returns the cached bot identity, or None if not yet fetched.
    /// Real impl: value is set by calling TDLib `get_me` on startup.
    /// Mock impl: value is set via `set_user_id`/`set_username` in test setup.
    pub fn get(&self) -> Option<SelfIdentity> {
        self.cached.lock().unwrap().clone()
    }

    /// Returns the cached bot user_id (canonical for self-loop comparison).
    pub fn user_id(&self) -> Option<i64> {
        self.cached.lock().unwrap().as_ref().map(|i| i.user_id)
    }

    /// Returns the cached bot username (for display / legacy `self_handle()` consumers).
    pub fn username(&self) -> Option<String> {
        self.cached
            .lock()
            .unwrap()
            .as_ref()
            .map(|i| i.username.clone())
            .filter(|u| !u.is_empty())
    }

    /// True iff the given user_id (numeric) equals the cached self user_id.
    /// Returns `false` if no identity is cached — i.e. an empty self_handle
    /// does not suppress any messages.
    pub fn is_self(&self, user_id: i64) -> bool {
        match self.cached.lock().unwrap().as_ref() {
            Some(id) => id.user_id == user_id,
            None => false,
        }
    }

    /// Clear the cached identity (e.g., on logout).
    pub fn clear(&self) {
        *self.cached.lock().unwrap() = None;
    }
}

impl Default for SelfHandle {
    fn default() -> Self {
        Self::new()
    }
}
