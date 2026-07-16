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

    /// Session 13: wacore stopped emitting `Event::PairingQrCode`
    /// after the QR ref-token budget was exhausted. The CLI was
    /// still polling `self_handle()` and would have waited the
    /// full operator `--timeout`; this variant lets the CLI bail
    /// out immediately with a clear message ("QR codes expired,
    /// no phone scanned them; restart with `--reset`").
    #[error(
        "WhatsApp QR codes expired without a phone scan \
         (idle {idle_secs}s since last QR; re-run with `--reset` to retry)"
    )]
    QrPairingStalled { idle_secs: u64 },

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

    /// Mission 0850p-a-session-export: a session bundle (tar.gz) is
    /// invalid, truncated, or failed checksum verification.
    #[error("invalid bundle {path:?}: {reason}")]
    InvalidBundle { path: PathBuf, reason: String },
}

pub type Result<T> = std::result::Result<T, CoreError>;

#[cfg(test)]
mod tests {
    use super::*;

    /// Session 13: the operator-visible message must name the cause
    /// (QR codes expired), the idle duration (so the operator can
    /// judge whether their `--timeout` should be shorter), and the
    /// recovery action (`--reset`). Automation that scrapes this
    /// string relies on the leading token being stable.
    #[test]
    fn qr_pairing_stalled_display_is_actionable() {
        let e = CoreError::QrPairingStalled { idle_secs: 60 };
        let s = e.to_string();
        assert!(s.contains("QR codes expired"), "missing cause: {s:?}");
        assert!(s.contains("60"), "missing idle_secs: {s:?}");
        assert!(s.contains("--reset"), "missing recovery hint: {s:?}");
    }
}
