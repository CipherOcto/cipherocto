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
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SessionRow {
    pub user_id: String,
    pub device_id: String,
    pub homeserver_url: String,
    pub access_token: String,
    pub refresh_token: Option<String>,
    pub login_type: LoginType,
    /// Epoch seconds. Set on insert, never updated.
    pub login_timestamp: i64,
    /// Epoch seconds. Updated on every successful adapter start that
    /// loads the session; also updated by `set_latest_session`.
    pub last_used: i64,
    /// Stable multi-account ordering. Strictly monotonic on insert
    /// (`max(position) + 1`); never changes on `set_latest_session`.
    pub position: i64,
    /// Cached display name (UI hint; not authoritative).
    pub display_name: Option<String>,
    /// Cached avatar URL (mxc://, UI hint; not authoritative).
    pub avatar_url: Option<String>,
}
