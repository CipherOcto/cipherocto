//! `octo-matrix-onboard-core` — library half of `octo-matrix-onboard`.
//!
//! Mission 0850h-a: authenticate a human operator against any Matrix
//! homeserver in four modes (password, OIDC, SSO, QR) and write a JSON
//! config file matching the extended `MatrixConfig` schema consumed by
//! `octo-adapter-matrix-sdk`.
//!
//! The binary crate (`octo-matrix-onboard`) imports this lib to drive the
//! actual flows; the integration test also imports it directly so it can
//! run the auth code without spawning a subprocess.

pub mod client_from_config;
pub mod oauth_listener;
pub mod qrcode_render;
pub mod session;

/// Typed error variants for the core crate. R2-M1: a previous
/// shape returned `anyhow::Error` from `client_from_config`, which
/// forced the CLI to substring-match on the message to route
/// errors to the right exit code. The typed variants let callers
/// `match` directly.
#[derive(Debug, thiserror::Error)]
pub enum CoreError {
    /// Failed to read the on-disk config file.
    #[error("read config {path:?}: {source}")]
    Read {
        path: std::path::PathBuf,
        #[source]
        source: std::io::Error,
    },
    /// Failed to parse the on-disk config as JSON / `OnboardConfig`.
    #[error("parse config {path:?}: {source}")]
    Parse {
        path: std::path::PathBuf,
        #[source]
        source: serde_json::Error,
    },
    /// `user_id` did not pass MXID validation.
    #[error("invalid user_id {value}: {source}")]
    InvalidUserId {
        value: String,
        #[source]
        source: matrix_sdk::ruma::IdParseError,
    },
    /// Failed to build the `matrix_sdk::Client` against the
    /// configured homeserver. The inner `ClientBuildError` is
    /// `Box`-wrapped because the SDK's error type is 184+ bytes
    /// (`result_large_err` clippy lint).
    #[error("build client against {homeserver}: {source}")]
    ClientBuild {
        homeserver: String,
        #[source]
        source: Box<matrix_sdk::ClientBuildError>,
    },
    /// `restore_session` failed on a freshly built client. The
    /// inner error is also `Box`-wrapped for the same reason.
    #[error("restore_session: {0}")]
    RestoreSession(#[source] Box<matrix_sdk::Error>),
}

/// Captured session material — what the SDK returns after a successful
/// login. The on-disk JSON written by the binary is built directly from
/// this struct.
///
/// R1-L1: the `access_token` field is private (not `pub`); the
/// binary's `output` module reads it via the `to_disk_json` method,
/// which is the single sanctioned path to the on-disk config. CLI
/// display code that wants a token preview should go through
/// `access_token_preview` (redacted).
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct Session {
    /// Matrix homeserver URL (e.g. `https://matrix.example.com`).
    pub homeserver_url: String,
    /// Authenticated user MXID (e.g. `@bot:matrix.example.com`).
    pub user_id: String,
    /// Device ID assigned by the homeserver.
    pub device_id: String,
    /// Access token. R1-L1: private; access via `to_disk_json` (for
    /// writing) or `access_token_preview` (for redacted display).
    access_token: String,
    /// Refresh token, when the homeserver issued one.
    pub refresh_token: Option<String>,
}

impl Session {
    /// R1-L1: build a `Session` from material captured during login.
    /// This is the only public path that constructs a `Session` —
    /// the `access_token` field is private, so callers (the binary
    /// and integration tests) must hand the token in through this
    /// constructor at the moment of capture rather than after the
    /// fact. The token is then read out only via `to_disk_json`
    /// (for writing the on-disk config) or `access_token_preview`
    /// (for redacted display).
    pub fn new(
        homeserver_url: String,
        user_id: String,
        device_id: String,
        access_token: String,
        refresh_token: Option<String>,
    ) -> Self {
        Self {
            homeserver_url,
            user_id,
            device_id,
            access_token,
            refresh_token,
        }
    }

    /// R1-L1: read-only access to the access token. This is the
    /// sanctioned path for integration tests and the adapter's
    /// session-load path that need the raw token to hand to the
    /// Matrix SDK. The token is NOT exposed via a public field;
    /// callers must go through this method.
    pub fn access_token(&self) -> &str {
        &self.access_token
    }

    /// R1-L1: build the on-disk JSON shape (mission 0850h-a §Output
    /// contract). This is the ONLY way the access_token leaves the
    /// library; the binary calls this and writes the result to disk.
    pub fn to_disk_json(&self) -> serde_json::Value {
        serde_json::json!({
            "homeserver_url": self.homeserver_url,
            "user_id": self.user_id,
            "device_id": self.device_id,
            "access_token": self.access_token,
            "refresh_token": self.refresh_token,
            "rooms": Vec::<String>::new(),
        })
    }

    /// R1-L1: redacted access token preview for CLI display. For
    /// tokens longer than 16 chars, shows the first 8 + "..." + the
    /// last 4. For shorter tokens, shows the first 4 + "...".
    pub fn access_token_preview(&self) -> String {
        redact_token(&self.access_token)
    }
}

/// R1-L1: token redaction helper. Shows the first 8 chars + "..." +
/// last 4 chars (the standard `syt_…XXXX` form operators expect).
/// For tokens shorter than 16 chars, returns the first 4 + "...".
///
/// R5-L1: this is one of three `redact_token` implementations
/// across the four mission crates. Each site has a deliberately
/// different format policy because each display context calls
/// for a different balance of brevity and operator-recognizability:
///
/// - `crates/octo-matrix-onboard-core/src/lib.rs` (THIS FILE) —
///   the one-time "logged in" confirmation message
///   (`Session::access_token_preview`). Uses a 2-tier form
///   (first8...last4 / first4...) that reveals slightly more
///   of short tokens.
/// - `crates/octo-adapter-matrix-sdk/src/lib.rs:55` — free-form
///   diagnostic output (error messages, debug logs). Uses a
///   3-tier form (first8...last4 / all*** / ***) so operators
///   can correlate the start AND end of a long token against
///   the homeserver's UI.
/// - `crates/octo-matrix-onboard/src/modes/session.rs:50` —
///   tabular `session list` output. Uses a compact 2-tier form
///   (first8*** / ***) that keeps the column width
///   predictable.
///
/// If you change this implementation, audit the other two for
/// consistency. The per-site policies are intentional; the
/// cross-reference is the missing piece a future maintainer
/// needs to avoid silent divergence.
fn redact_token(token: &str) -> String {
    if token.len() <= 16 {
        let prefix: String = token.chars().take(4).collect();
        format!("{prefix}...")
    } else {
        let prefix: String = token.chars().take(8).collect();
        let suffix: String = token
            .chars()
            .rev()
            .take(4)
            .collect::<Vec<_>>()
            .into_iter()
            .rev()
            .collect();
        format!("{prefix}...{suffix}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn redact_token_long() {
        // "syt_abcdefgh_long_token_xyz" — 27 chars (>16), so the
        // long-form redaction is used. First 8 chars: "syt_abcd";
        // last 4 chars: "_xyz". The middle is elided.
        let r = redact_token("syt_abcdefgh_long_token_xyz");
        assert_eq!(r, "syt_abcd..._xyz", "got: {r}");
    }

    #[test]
    fn redact_token_short() {
        let r = redact_token("short");
        assert_eq!(r, "shor...");
    }

    #[test]
    fn to_disk_json_includes_access_token() {
        let s = Session {
            homeserver_url: "https://matrix.example.com".into(),
            user_id: "@bot:matrix.example.com".into(),
            device_id: "ABCDEFGHIJ".into(),
            access_token: "syt_real_token_xyz".into(),
            refresh_token: Some("syr_y".into()),
        };
        let v = s.to_disk_json();
        assert_eq!(v["access_token"], "syt_real_token_xyz");
        assert_eq!(v["refresh_token"], "syr_y");
        assert_eq!(v["rooms"].as_array().unwrap().len(), 0);
    }
}
