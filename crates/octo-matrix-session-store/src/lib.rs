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
//! - [`schema`] — `init_schema(&db)`: idempotent `CREATE TABLE IF NOT
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

/// Default on-disk location of the store: per-platform project data
/// dir + `sessions.db`. On Linux this is
/// `$XDG_DATA_HOME/cipherocto/sessions.db` (typically
/// `~/.local/share/cipherocto/sessions.db`).
///
/// The location can be overridden via `StoolapSessionStore::new(path)`
/// — used by tests (in-memory) and by deployments that store sessions
/// in a non-standard location (e.g., a read-only network mount).
pub fn default_store_path() -> std::path::PathBuf {
    directories::ProjectDirs::from("com", "cipherocto", "cipherocto")
        .map(|p| p.data_dir().join("sessions.db"))
        .unwrap_or_else(|| std::path::PathBuf::from("sessions.db"))
}
