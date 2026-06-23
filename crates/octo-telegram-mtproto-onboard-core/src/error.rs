//! Error types for the MTProto Telegram onboarding flow.
//!
//! All errors bubble up to the CLI as `OnboardError`. Variants are
//! domain-tagged so the CLI can render actionable, mode-specific
//! remediation hints (e.g. "did you forget the SMS code?").
//!
//! Sticking to `thiserror` (no `anyhow` in the public surface) so
//! downstream code can match on a specific cause.

use thiserror::Error;

/// All errors that can arise during MTProto Telegram onboarding.
///
/// Conforming to the project's "no stubs, no mocks in production
/// code" rule: every variant maps to a real, observable failure
/// mode reported by `octo-adapter-telegram-mtproto` or by the
/// `tokio::sync::mpsc` channel used to feed SMS codes / 2FA
/// passwords to the adapter.
///
/// R2-ARCH-8: marked `#[non_exhaustive]` so adding a new
/// variant is a backward-compatible change for downstream
/// crates. The `kind()` and `exit_code()` methods are the
/// supported external surface — downstream code should
/// switch on those, not on the enum variant.
#[derive(Debug, Error)]
#[non_exhaustive]
pub enum OnboardError {
    /// I/O error reading/writing the on-disk session JSON or the
    /// data directory.
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),

    /// JSON (de)serialization error for the on-disk session /
    /// config file.
    #[error("json error: {0}")]
    Json(#[from] serde_json::Error),

    /// Required input missing or malformed (e.g. empty phone, empty
    /// bot token, malformed QR login token).
    #[error("invalid input: {0}")]
    InvalidInput(String),

    /// Configuration lookup failed (e.g. could not determine
    /// `data_dir`, missing `api_id` / `api_hash`).
    #[error("config error: {0}")]
    Config(String),

    /// `connect_*` returned before the adapter reached `Ready`.
    /// Carries the latest observed lifecycle state for diagnostics.
    ///
    /// ARCH-1 (R26): the prior `NotReady` variant was reused
    /// for both "adapter lifecycle not yet Ready" AND "no
    /// session file" (the `whoami` reader's missing-file
    /// case). The two are different operator-facing
    /// conditions: the lifecycle error means the auth flow
    /// is in progress and the operator should retry; the
    /// no-session-file error means the operator has never
    /// run onboarding. Split into two variants so the CLI
    /// can render distinct remediation hints.
    #[error("adapter did not reach Ready (last state: {state})")]
    Lifecycle {
        /// `auth_state_name(last_observed)` so the operator can
        /// tell at a glance whether they need a code / password /
        /// QR scan.
        state: String,
    },

    /// `SessionRecord::read_from` could not find a session file
    /// at `<data_dir>/session.json` (or its schema is too old).
    /// Distinct from `Lifecycle` because the operator has
    /// never completed onboarding (vs. the lifecycle case
    /// where onboarding is in flight).
    #[error("no session file: {0}")]
    NoSessionFile(String),

    /// SMS code / 2FA password channel closed before the
    /// `ask_code` / `ask_password` callback consumed the
    /// request.
    #[error("interactive channel closed unexpectedly: {0}")]
    ChannelClosed(String),

    /// The interactive user did not provide input within the
    /// deadline (SMS code window or 2FA password window in the
    /// user-code flow, or the 5-minute QR-scan window in the
    /// QR-login flow). R2-ARCH-13: this variant IS wired —
    /// `qr_login::run` returns it on a 5-minute poll timeout,
    /// and `user_code::run` could return it if the
    /// `code_timeout` / `password_timeout` deadlines elapse
    /// without input. The doc-comment previously said
    /// "currently unused but reserved for a future timeout
    /// variant", which was incorrect.
    #[error("interactive timeout: {0}")]
    Timeout(String),

    /// The adapter reported a Telegram-side error (FLOOD_WAIT,
    /// AUTH_KEY_UNREGISTERED, etc.). The wrapped string is the
    /// `ApiError` display, which the CLI surfaces verbatim.
    #[error("telegram api error: {0}")]
    TelegramApi(String),

    /// Catch-all for unexpected wrapping of the adapter's own
    /// error type. Kept as a string so we don't pin this crate
    /// to a specific adapter version.
    #[error("adapter error: {0}")]
    Adapter(String),

    /// Catch-all for `octo-network` IO errors (TCP connect
    /// failures, TLS handshake failures) during the initial
    /// `connect_*` call.
    #[error("network error: {0}")]
    Network(String),

    /// `tokio::task::JoinError` from a spawned onboarding task.
    #[error("join error: {0}")]
    Join(#[from] tokio::task::JoinError),
}

impl OnboardError {
    /// Stable string discriminator for log lines and CLI exit
    /// codes. **Do not localize** — operators grep for these.
    pub fn kind(&self) -> &'static str {
        match self {
            OnboardError::Io(_) => "io",
            OnboardError::Json(_) => "json",
            OnboardError::InvalidInput(_) => "invalid_input",
            OnboardError::Config(_) => "config",
            OnboardError::Lifecycle { .. } => "lifecycle",
            OnboardError::NoSessionFile(_) => "no_session_file",
            OnboardError::ChannelClosed(_) => "channel_closed",
            OnboardError::Timeout(_) => "timeout",
            OnboardError::TelegramApi(_) => "telegram_api",
            OnboardError::Adapter(_) => "adapter",
            OnboardError::Network(_) => "network",
            OnboardError::Join(_) => "join",
        }
    }

    /// Map an `OnboardError` to a process exit code. Stable
    /// across releases (operators script against it). Lives in
    /// the core crate (not the CLI) so the orphan rule is
    /// satisfied — the CLI just calls `e.exit_code()`.
    pub fn exit_code(&self) -> u8 {
        match self {
            OnboardError::InvalidInput(_) => 2,
            OnboardError::Config(_) => 3,
            OnboardError::Lifecycle { .. } => 4,
            OnboardError::NoSessionFile(_) => 4,
            OnboardError::ChannelClosed(_) => 5,
            OnboardError::Timeout(_) => 6,
            OnboardError::TelegramApi(_) => 7,
            OnboardError::Network(_) => 8,
            OnboardError::Io(_) => 9,
            OnboardError::Json(_) => 10,
            OnboardError::Adapter(_) => 11,
            OnboardError::Join(_) => 12,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn kind_returns_stable_discriminators() {
        assert_eq!(
            OnboardError::InvalidInput("x".into()).kind(),
            "invalid_input"
        );
        assert_eq!(
            OnboardError::Lifecycle {
                state: "WaitCode".into()
            }
            .kind(),
            "lifecycle"
        );
        assert_eq!(
            OnboardError::NoSessionFile("missing".into()).kind(),
            "no_session_file"
        );
        assert_eq!(OnboardError::Config("x".into()).kind(), "config");
    }

    #[test]
    fn io_error_converts_via_from() {
        let e: OnboardError = std::io::Error::new(std::io::ErrorKind::NotFound, "nope").into();
        assert_eq!(e.kind(), "io");
    }

    #[test]
    fn json_error_converts_via_from() {
        // Bind to the json::Error type directly so the
        // From<serde_json::Error> impl on OnboardError
        // applies (serde_json::Value is unrelated to
        // serde_json::Error even though both are in the
        // same crate).
        let bad: serde_json::Error = serde_json::from_str::<serde_json::Value>("{").unwrap_err();
        let e: OnboardError = bad.into();
        assert_eq!(e.kind(), "json");
    }

    /// ARCH-1 (R26): the `Lifecycle` and `NoSessionFile`
    /// variants are distinct so the CLI can render
    /// mode-specific remediation hints. The shared exit
    /// code (4) is intentional — both are "not yet
    /// onboarded" conditions from the operator's POV, just
    /// from different causes.
    #[test]
    fn lifecycle_and_no_session_share_exit_code_4() {
        assert_eq!(
            OnboardError::Lifecycle {
                state: "x".into()
            }
            .exit_code(),
            4
        );
        assert_eq!(
            OnboardError::NoSessionFile("x".into()).exit_code(),
            4
        );
    }
}
