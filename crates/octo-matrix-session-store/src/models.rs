//! On-disk row model for the Matrix session store (mission 0850h-d).
//!
//! One row per `(user_id, device_id)`. The schema mirrors EXA's
//! `SessionData.kt` structurally (one row per device, columns for
//! tokens / homeserver / login type / position / last-used) but uses
//! the stoolap type system (TEXT for variable-length, INTEGER for
//! epoch seconds) and CipherOcto's snake_case naming.

use serde::{Deserialize, Serialize};

/// How the user authenticated to the homeserver. Drives the
/// `MatrixAdapter`'s login flow (and the `octo-matrix-onboard login`
/// subcommand dispatch).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum LoginType {
    /// Username + password login.
    Password,
    /// OpenID Connect (authorization code flow with PKCE).
    #[serde(rename = "oidc")]
    Oidc,
    /// Single sign-on (SAML, custom OIDC, etc.).
    #[serde(rename = "sso")]
    Sso,
    /// QR code cross-sign (Element-style).
    #[serde(rename = "qr")]
    Qr,
}

impl LoginType {
    /// String form for the on-disk column. Matches the serde rename
    /// (`password` / `oidc` / `sso` / `qr`).
    pub fn as_str(&self) -> &'static str {
        match self {
            LoginType::Password => "password",
            LoginType::Oidc => "oidc",
            LoginType::Sso => "sso",
            LoginType::Qr => "qr",
        }
    }
}

impl std::fmt::Display for LoginType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

impl std::str::FromStr for LoginType {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "password" => Ok(LoginType::Password),
            "oidc" => Ok(LoginType::Oidc),
            "sso" => Ok(LoginType::Sso),
            "qr" => Ok(LoginType::Qr),
            other => Err(format!("unknown login type: '{}'", other)),
        }
    }
}

/// One row in the `sessions` table. Returned by
/// `StoolapSessionStore::get_session` / `get_all_sessions` and
/// consumed by `MatrixAdapter::new` to rebuild a logged-in client.
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SessionRow {
    pub user_id: String,
    pub device_id: String,
    pub homeserver_url: String,
    pub access_token: String,
    pub refresh_token: Option<String>,
    pub login_type: LoginType,
    /// Epoch seconds. Set on insert, never updated.
    pub login_timestamp: i64,
    /// Epoch seconds. Set to the current epoch on `add_session`
    /// (initial value, equal to `login_timestamp` at insert time) and
    /// updated by the dedicated `set_latest_session` method when the
    /// operator marks a row as the most-recently-used. The session
    /// loader (`octo-adapter-matrix-sdk::session_loader::load`) does
    /// NOT touch this column — a successful load does not constitute
    /// a "use" for ordering purposes. R8-L1: a previous version of
    /// this docstring claimed `last_used` is updated on every
    /// adapter start; R6-L1 fixed the same false claim in
    /// `schema.rs` but missed this field's docstring.
    pub last_used: i64,
    /// Stable multi-account ordering. Strictly monotonic on insert
    /// (`max(position) + 1`); never changes on `set_latest_session`.
    pub position: i64,
    /// Cached display name (UI hint; not authoritative).
    pub display_name: Option<String>,
    /// Cached avatar URL (mxc://, UI hint; not authoritative).
    pub avatar_url: Option<String>,
}

/// R23-L1: hand-rolled `Debug` for `SessionRow`. The auto-derived
/// form would print `access_token` and `refresh_token` in plain
/// text, so any `dbg!(row)` or `tracing::debug!(?row)` would
/// leak the row's tokens to stderr. The redacted form matches
/// `MatrixConfig::Debug` (3-tier `redact_token`) so the four
/// session-bearing data structures (`MatrixConfig`,
/// `LoadedSession`, `OnboardConfig`, `SessionRow`) all produce
/// consistent redacted Debug output.
///
/// Note: this crate's `redact_token` is crate-private to
/// `octo-matrix-onboard-core` (the CLI's `access_token_preview`).
/// We can't call it from here, so we inline a minimal 2-tier
/// form (`first8...last4` for tokens ≥ 12 chars, `***` for
/// shorter) — same shape as the core's `access_token_preview`.
impl std::fmt::Debug for SessionRow {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SessionRow")
            .field("user_id", &self.user_id)
            .field("device_id", &self.device_id)
            .field("homeserver_url", &self.homeserver_url)
            .field("access_token", &debug_redact_token(&self.access_token))
            .field(
                "refresh_token",
                &self.refresh_token.as_deref().map(debug_redact_token),
            )
            .field("login_type", &self.login_type)
            .field("login_timestamp", &self.login_timestamp)
            .field("last_used", &self.last_used)
            .field("position", &self.position)
            .field("display_name", &self.display_name)
            .field("avatar_url", &self.avatar_url)
            .finish()
    }
}

/// 2-tier token redactor for `Debug` output. Mirrors the core
/// crate's `redact_token` (long → first8...last4, short → ***)
/// without depending on it across crate boundaries. Used by
/// `SessionRow`'s `Debug` impl above.
fn debug_redact_token(token: &str) -> String {
    if token.len() >= 12 {
        let head: String = token.chars().take(8).collect();
        let tail: String = token
            .chars()
            .rev()
            .take(4)
            .collect::<Vec<_>>()
            .into_iter()
            .rev()
            .collect();
        format!("{head}...{tail}")
    } else {
        "***".to_string()
    }
}
