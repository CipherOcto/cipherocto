//! Stable, machine-readable description of an onboarding run.
//!
//! The CLI prints this to stdout (or a `--output` path) so that
//! automation (e.g. a deploy script) can drive onboarding without
//! parsing log lines.
//!
//! Schema (JSON, versioned via `schema_version`):
//!
//! ```json
//! {
//!   "schema_version": 1,
//!   "mode": "bot_token" | "user_code" | "qr_login" | "whoami",
//!   "self_id": 123456789,
//!   "self_username": "my_bot",          // null for user accounts
//!   "is_bot": true,
//!   "data_dir": "/var/lib/octo/mtproto/0",
//!   "config_path": "/var/lib/octo/mtproto/0/config.json",
//!   "elapsed_ms": 4521
//! }
//! ```

use serde::{Deserialize, Serialize};

/// Onboarding mode — selects which adapter connect path was
/// used. Mirrors the CLI's `--mode` enum.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OnboardMode {
    /// Bot token mode (`connect_bot_token`).
    BotToken,
    /// User phone + SMS code (+ optional 2FA) mode
    /// (`connect_user`).
    UserCode,
    /// QR login mode (`connect_qr_login` + `poll_qr_login`).
    QrLogin,
    /// Read-only: print the `self_handle` of an existing session.
    Whoami,
}

/// Successful onboarding result. Serializes to JSON for the
/// `--output` path or for stdout when `--json` is set.
///
/// R2-ARCH-6: marked `#[non_exhaustive]` so adding a new
/// field is a backward-compatible change for downstream
/// crates (a future `OnboardOutput { ..., created_at: ... }`
/// won't break every external `match` against the struct).
/// Construction inside the workspace still works — only
/// external `let x = OnboardOutput { ... }` from a
/// downstream crate becomes a compile error (which is the
/// desired effect: external callers should use the
/// `SCHEMA_VERSION` constant + `to_json_pretty` + JSON
/// parsing for forward-compatibility).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[non_exhaustive]
pub struct OnboardOutput {
    /// Schema version. Bump on backward-incompatible changes to
    /// this struct.
    pub schema_version: u32,
    /// Mode that produced this output.
    pub mode: OnboardMode,
    /// Telegram user-id (or bot-id) of the authenticated
    /// principal. `i64` to match `MtprotoSelfHandle::id`.
    pub self_id: i64,
    /// `@username` if the principal has one (`None` for users
    /// without a public username, including most bots created
    /// without one).
    pub self_username: Option<String>,
    /// `true` for bot tokens, `false` for user accounts and QR
    /// logins. Mirrors Telegram's own `User::bot` flag.
    pub is_bot: bool,
    /// Resolved on-disk data directory. CLI uses this as the
    /// authoritative hint for where to find the session file.
    pub data_dir: String,
    /// Path to the JSON config file the CLI just wrote (or, in
    /// `Whoami` mode, the file it just read).
    pub config_path: String,
    /// Wall-clock time spent in the connect loop, in
    /// milliseconds. For `Whoami` this is always 0.
    pub elapsed_ms: u64,
}

impl OnboardOutput {
    /// Current schema version. Bump on breaking changes.
    pub const SCHEMA_VERSION: u32 = 1;

    /// Construct an `OnboardOutput` from the required
    /// fields. R2-ARCH-6: this is the supported external
    /// construction API — the struct is `#[non_exhaustive]`
    /// so external code cannot use a struct expression
    /// (`OnboardOutput { .. }`). Use this constructor
    /// instead, which pins the current field set; new
    /// fields added in a future release will get a
    /// `Default` (or `None` for `Option`s) so the
    /// constructor keeps working.
    pub fn new(
        mode: OnboardMode,
        self_id: i64,
        self_username: Option<String>,
        is_bot: bool,
        data_dir: String,
        config_path: String,
        elapsed_ms: u64,
    ) -> Self {
        Self {
            schema_version: Self::SCHEMA_VERSION,
            mode,
            self_id,
            self_username,
            is_bot,
            data_dir,
            config_path,
            elapsed_ms,
        }
    }

    /// Serialize to pretty-printed JSON. The CLI writes this
    /// verbatim to the output path or stdout.
    pub fn to_json_pretty(&self) -> Result<String, serde_json::Error> {
        serde_json::to_string_pretty(self)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip_preserves_fields() {
        let out = OnboardOutput {
            schema_version: OnboardOutput::SCHEMA_VERSION,
            mode: OnboardMode::BotToken,
            self_id: 12345,
            self_username: Some("test_bot".to_string()),
            is_bot: true,
            data_dir: "/tmp/x".to_string(),
            config_path: "/tmp/x/config.json".to_string(),
            elapsed_ms: 100,
        };
        let j = out.to_json_pretty().unwrap();
        let parsed: OnboardOutput = serde_json::from_str(&j).unwrap();
        assert_eq!(parsed, out);
    }

    #[test]
    fn mode_serializes_as_snake_case() {
        let j = serde_json::to_string(&OnboardMode::QrLogin).unwrap();
        assert_eq!(j, "\"qr_login\"");
        let j = serde_json::to_string(&OnboardMode::UserCode).unwrap();
        assert_eq!(j, "\"user_code\"");
    }

    #[test]
    fn schema_version_is_stable() {
        assert_eq!(OnboardOutput::SCHEMA_VERSION, 1);
    }
}
