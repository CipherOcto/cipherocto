//! Persistent multi-account session store (mission 0850h-d).
//!
//! Backed by CipherOcto's [`stoolap`] fork, the canonical project-wide
//! persistence layer (per RFC-0914-a and the convention documented in
//! `crates/quota-router-core/src/storage.rs`). Replaces the
//! one-file-per-identity model of missions 0850h-a / 0850h-c with a
//! single queryable inventory of Matrix `(user_id, device_id)` rows.
//!
//! ## Layout
//!
//! - [`models`] — `SessionRow`, `LoginType`. The on-disk shape mirrors
//!   EXA's `SessionData` (one row per `(user_id, device_id)`, columns
//!   for tokens / homeserver / login type / position / last-used),
//!   adapted to stoolap's `INTEGER`/`TEXT` type system.
//! - [`schema`] — `init_schema(&&db)`: idempotent `CREATE TABLE IF NOT
//!   EXISTS` + indexes. The store calls this on `new`.
//! - [`store`] — `SessionStore` trait + `StoolapSessionStore` impl.
//!   The trait surface (`add_session`, `update_data`, `get_session`,
//!   `get_all_sessions`, `get_latest_session`, `number_of_sessions`,
//!   `set_latest_session`, `remove_session`) is the direct-getter
//!   subset of EXA's `SessionStore.kt` — the Flow-based observers are
//!   intentionally NOT adopted because a CLI does not need a reactive
//!   stream.
//!
//! ## Multi-account ordering
//!
//! `position` is strictly monotonic across inserts (`max(position) + 1`)
//! and never changes on `set_latest_session` (which only updates
//! `last_used`). `login_timestamp` is set on `add_session` and is
//! immutable thereafter. This pattern is from EXA's
//! `DatabaseSessionStore.addSession` and preserves chronological
//! multi-account ordering across devices.

pub mod models;
pub mod schema;
pub mod store;

pub use models::{LoginType, SessionRow};
pub use schema::init_schema;
pub use store::{SessionStore, SessionStoreError, StoolapSessionStore};

/// Current epoch seconds.
///
/// R6-L3: this is the canonical implementation of "now in epoch
/// seconds" for the four mission crates. The `octo-matrix-session-store`
/// crate is the leaf — both `octo-adapter-matrix-sdk` and
/// `octo-matrix-onboard` depend on it — so making the function
/// `pub` here is the natural single source of truth. Previous
/// shape: three near-identical copies
/// (the local `now_epoch` in `octo-matrix-session-store/src/store.rs`,
/// `octo-matrix-onboard/src/modes/session.rs`,
/// and `unix_epoch_now` in `octo-adapter-matrix-sdk/src/lib.rs`
/// returning `u64`) diverged only on the `i64` vs `u64` return
/// type. R6-L3 removed all three duplicates; the `u64` call site
/// in the adapter now casts from `i64` at the call boundary,
/// with a short comment explaining why.
///
/// Returns 0 if the system clock is before the Unix epoch
/// (defensive — never expected in practice, but a `SystemTime`
/// earlier than `UNIX_EPOCH` would otherwise produce a negative
/// duration and a panic on `as_secs`).
pub fn now_epoch() -> i64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

/// Default on-disk location of the store: per-platform project data
/// dir + `sessions.db`. On Linux this is
/// `$XDG_DATA_HOME/cipherocto/sessions.db` (typically
/// `~/.local/share/cipherocto/sessions.db`).
///
/// Returns `Err(SessionStoreError::NoDefaultPath)` on platforms where
/// `directories::ProjectDirs::from(...)` cannot derive a per-platform
/// project data directory (no `$XDG_DATA_HOME` and no `$HOME`). On
/// such platforms the caller must pass an explicit path to
/// `StoolapSessionStore::new(path)` — we deliberately do NOT fall
/// back to a bare relative `sessions.db` in the cwd, which would
/// silently create a file in whatever directory the CLI happened to
/// be in (potentially world-writable on shared systems).
pub fn default_store_path() -> Result<std::path::PathBuf, SessionStoreError> {
    directories::ProjectDirs::from("com", "cipherocto", "cipherocto")
        .map(|p| p.data_dir().join("sessions.db"))
        .ok_or(SessionStoreError::NoDefaultPath)
}
