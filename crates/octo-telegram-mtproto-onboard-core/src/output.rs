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
    ///
    /// R2-ARCH-19: this is currently a thin wrapper around
    /// `serde_json::to_string_pretty`. The wrapper is kept
    /// (rather than inlining the call at every site) so the
    /// CLI's render path is one place to add centralised error
    /// reporting if `serde_json` ever introduces a version
    /// pin or breaking change. Removing it would force every
    /// caller to update when the underlying API shifts. The
    /// doc-comment previously claimed "centralised error
    /// reporting if serde ever adds version-pinning" — that's
    /// the rationale; this comment makes it explicit.
    pub fn to_json_pretty(&self) -> Result<String, serde_json::Error> {
        serde_json::to_string_pretty(self)
    }
}

/// Strip control characters and other non-validating bytes
/// from a Telegram username. R2-PROTO-14: Telegram enforces
/// ASCII server-side, but the `User::username` field
/// technically carries `String` and could carry any UTF-8 if
/// the upstream server (or a malicious replay attack)
/// returns non-ASCII bytes. For safety, strip any control
/// characters and return `None` if the cleaned result is
/// empty. Whitespace and unicode-look-alike characters
/// (zero-width joiners, RTL marks, etc.) are also
/// filtered out — a username with embedded `\u{200B}` (zero-
/// width space) would silently break a downstream consumer
/// that pattern-matches on the username.
///
/// Applied in `user_code::run`, `bot_token::run`, and
/// `qr_login::run` before constructing `OnboardOutput`.
pub fn validate_username(raw: Option<String>) -> Option<String> {
    let s = raw?;
    // Strip ASCII control chars (0x00–0x1F, 0x7F) and
    // common Unicode "invisible" codepoints (zero-width
    // spaces, RTL marks, etc.).
    let cleaned: String = s
        .chars()
        .filter(|c| !c.is_control() & !is_invisible_unicode(*c))
        .collect();
    if cleaned.is_empty() {
        None
    } else {
        Some(cleaned)
    }
}

/// True for Unicode "invisible" codepoints that look
/// ASCII but are actually look-alikes (zero-width spaces,
/// bidi marks, BOM). Used by `validate_username`.
fn is_invisible_unicode(c: char) -> bool {
    matches!(
        c,
        '\u{200B}' // ZERO WIDTH SPACE
            | '\u{200C}' // ZERO WIDTH NON-JOINER
            | '\u{200D}' // ZERO WIDTH JOINER
            | '\u{200E}' // LEFT-TO-RIGHT MARK
            | '\u{200F}' // RIGHT-TO-LEFT MARK
            | '\u{202A}' // LEFT-TO-RIGHT EMBEDDING
            | '\u{202B}' // RIGHT-TO-LEFT EMBEDDING
            | '\u{202C}' // POP DIRECTIONAL FORMATTING
            | '\u{202D}' // LEFT-TO-RIGHT OVERRIDE
            | '\u{202E}' // RIGHT-TO-LEFT OVERRIDE
            | '\u{2066}' // LEFT-TO-RIGHT ISOLATE
            | '\u{2067}' // RIGHT-TO-LEFT ISOLATE
            | '\u{2068}' // FIRST STRONG ISOLATE
            | '\u{2069}' // POP DIRECTIONAL ISOLATE
            | '\u{FEFF}' // ZERO WIDTH NO-BREAK SPACE (BOM)
    )
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

    /// R2-PROTO-14: a "normal" username passes through
    /// unchanged. This is the common case.
    #[test]
    fn validate_username_passes_through_ascii() {
        assert_eq!(
            validate_username(Some("alice_bot".into())),
            Some("alice_bot".into())
        );
    }

    /// R2-PROTO-14: `None` stays `None` (most user
    /// accounts don't have a public username).
    #[test]
    fn validate_username_preserves_none() {
        assert_eq!(validate_username(None), None);
    }

    /// R2-PROTO-14: ASCII control characters are
    /// stripped. A username with a trailing `\n` would
    /// otherwise leak into `OnboardOutput` and break a
    /// downstream consumer that compares the username
    /// to a configured allowlist.
    #[test]
    fn validate_username_strips_control_chars() {
        assert_eq!(
            validate_username(Some("alice\u{0000}bot".into())),
            Some("alicebot".into())
        );
        // Tabs, newlines, BEL — all stripped.
        assert_eq!(
            validate_username(Some("a\tli\nce\u{07}bot".into())),
            Some("alicebot".into())
        );
    }

    /// R2-PROTO-14: zero-width spaces and RTL marks are
    /// filtered — these are the "look-alike" attack
    /// vectors (a username that visually matches
    /// `alice_bot` but is actually `alice\u{200B}_bot`).
    #[test]
    fn validate_username_strips_zero_width_and_rtl() {
        // Zero-width space embedded in the middle.
        assert_eq!(
            validate_username(Some("alice\u{200B}_bot".into())),
            Some("alice_bot".into())
        );
        // Right-to-Left Override — used to render
        // usernames backwards in terminals that honour
        // bidi. Always stripped.
        assert_eq!(
            validate_username(Some("alice\u{202E}_bot".into())),
            Some("alice_bot".into())
        );
        // BOM at the start.
        assert_eq!(
            validate_username(Some("\u{FEFF}alice".into())),
            Some("alice".into())
        );
    }

    /// R2-PROTO-14: a username that is *only* control
    /// characters returns `None` (not the empty string).
    /// An empty-string `self_username` would serialise
    /// as `""` in the JSON output, which a downstream
    /// consumer would treat as "the user has an empty
    /// username" — semantically wrong.
    #[test]
    fn validate_username_returns_none_when_only_control_chars() {
        assert_eq!(
            validate_username(Some("\u{0000}\u{0001}\u{200B}".into())),
            None
        );
    }
}
