//! Schema migration for the Matrix session store (mission 0850h-d).
//!
//! The schema is modeled after EXA's `SessionData` (one row per
//! `(user_id, device_id)`, columns for tokens / homeserver / login
//! type / position / last-used) and adapted to stoolap's type system.
//! `stoolap` uses `INTEGER` for epoch seconds and `TEXT` for variable
//! length strings; BLOB columns are not used because the on-disk
//! shape is purely textual (Matrix `@user:server` and `access_token`
//! are both strings).
//!
//! `init_schema` is idempotent (`CREATE TABLE IF NOT EXISTS` + `CREATE
//! INDEX IF NOT EXISTS`); the store calls it on `new`.

use crate::store::{stoolap_err, SessionStoreError};

/// Create the `sessions` table and its indexes. Safe to call on a
/// fresh database (the typical case) and on an existing one (the
/// `IF NOT EXISTS` clauses make every statement a no-op when the
/// schema is already at the latest version).
///
/// Column reference:
/// - `user_id` / `device_id` — composite primary key. Matrix
///   `@user:server` is the natural user identifier; the device ID is
///   the 10-char uppercase alphanumeric assigned at login.
/// - `homeserver_url` — full URL of the homeserver (e.g.,
///   `https://matrix.example.com`). Cached for offline CLI use.
/// - `access_token` — the long-lived (or refresh-rotated) bearer
///   token. `refresh_token` is NULL for password-only logins that
///   don't issue refresh tokens.
/// - `login_type` — see `LoginType`. Drives adapter login dispatch.
/// - `login_timestamp` — set on `add_session` (epoch seconds),
///   immutable thereafter.
/// - `last_used` — set to the current epoch seconds on `add_session`
///   (initial value, equal to `login_timestamp` at insert time) and
///   updated by the dedicated `set_latest_session` method when the
///   operator marks a row as the most-recently-used. The session
///   loader (`octo-adapter-matrix-sdk::session_loader::load`) does
///   NOT touch this column — a successful load does not constitute
///   a "use" for ordering purposes. R6-L1: a previous version of
///   this docstring claimed `last_used` is updated on every
///   adapter start; the SQL in `store.rs:set_latest_session` only
///   updates it from `set_latest_session`.
/// - `position` — strictly monotonic on insert. Never changes on
///   `set_latest_session`. Drives stable multi-account ordering.
/// - `display_name` / `avatar_url` — UI hints cached from the
///   homeserver; not authoritative.
pub fn init_schema(db: &stoolap::Database) -> Result<(), SessionStoreError> {
    db.execute(
        "CREATE TABLE IF NOT EXISTS sessions (
            user_id TEXT NOT NULL,
            device_id TEXT NOT NULL,
            homeserver_url TEXT NOT NULL,
            access_token TEXT NOT NULL,
            refresh_token TEXT,
            login_type TEXT NOT NULL,
            login_timestamp INTEGER NOT NULL,
            last_used INTEGER NOT NULL,
            position INTEGER NOT NULL,
            display_name TEXT,
            avatar_url TEXT,
            PRIMARY KEY (user_id, device_id)
        )",
        [],
    )
    .map_err(stoolap_err)?;

    // Index on position for stable ordering in get_all_sessions.
    db.execute(
        "CREATE INDEX IF NOT EXISTS idx_sessions_position ON sessions(position)",
        [],
    )
    .map_err(stoolap_err)?;

    // R1-M25: UNIQUE on `position` so a same-process race between two
    // `StoolapSessionStore` instances (each computing
    // `max(position) + 1` from the same snapshot) surfaces as an
    // `AlreadyExists`-style error from the second insert, not as a
    // silent collision. The store's documented single-process model
    // means this is defense in depth, not a fix for a routine case.
    db.execute(
        "CREATE UNIQUE INDEX IF NOT EXISTS idx_sessions_position_unique
         ON sessions(position)",
        [],
    )
    .map_err(stoolap_err)?;

    // Index on last_used for get_latest_session.
    db.execute(
        "CREATE INDEX IF NOT EXISTS idx_sessions_last_used ON sessions(last_used)",
        [],
    )
    .map_err(stoolap_err)?;

    Ok(())
}
