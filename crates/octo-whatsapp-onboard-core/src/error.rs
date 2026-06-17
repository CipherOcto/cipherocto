//! Typed error variants for the core crate.
//!
//! R2-M1: errors are returned as typed `CoreError` variants so callers
//! (the CLI's `main` and integration tests) can `match` on the kind
//! instead of substring-matching on the message.
//!
//! R2-M2: variants are in alphabetical order to match `cargo doc` and
//! IDE jump-to-definition; documented as a project convention.

use std::path::PathBuf;

#[derive(Debug, thiserror::Error)]
pub enum CoreError {
    /// Wraps a low-level error from the adapter or runtime.
    #[error("adapter: {0}")]
    Adapter(#[source] anyhow::Error),

    /// Failed to build the `WhatsAppWebAdapter` (e.g., bad config).
    #[error("client build failed")]
    ClientBuild,

    /// `phone` did not pass E.164 validation.
    #[error("invalid phone {value:?}: {reason}")]
    InvalidPhone { value: String, reason: String },

    /// `session_path` is not usable (e.g., parent dir not creatable).
    #[error("invalid session_path {path:?}: {reason}")]
    InvalidSessionPath { path: PathBuf, reason: String },

    /// Failed to parse the on-disk config as JSON.
    #[error("parse config {path:?}: {source}")]
    Parse {
        path: PathBuf,
        #[source]
        source: serde_json::Error,
    },

    /// Failed to read the on-disk config file.
    #[error("read config {path:?}: {source}")]
    Read {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },

    /// `Event::LoggedOut` after a successful link (session expired).
    #[error("session expired")]
    SessionExpired,

    /// `wait_for_connected` / `wait_for_health` deadline exceeded.
    #[error("timeout after {secs}s")]
    Timeout { secs: u64 },

    /// Mission 0850p-a-symlink-check: `session_path` resolves to a
    /// symlink whose target is outside the requested parent directory
    /// (potential symlink-attack attempt; see D-WA-4).
    #[error("session_path {requested:?} is a symlink pointing to {resolved:?} outside the requested parent (potential symlink-attack)")]
    SessionPathSymlink { requested: String, resolved: String },
}

pub type Result<T> = std::result::Result<T, CoreError>;
