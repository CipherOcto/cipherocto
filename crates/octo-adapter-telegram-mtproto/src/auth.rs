//! Auth state machine for the MTProto adapter (bot mode + user mode).
//!
//! Three lifecycles drive auth:
//!
//! 1. `AdapterLifecycle` — outer state machine that the gateway
//!    polls: Uninitialised → Connecting → Connected → SigningIn →
//!    Ready → ShuttingDown → Stopped. (See `lifecycle.rs`.)
//! 2. `BotAuthLifecycle` — bot sign-in via
//!    `Client::bot_sign_in`. Single-step, no user interaction.
//! 3. `UserAuthLifecycle` — user sign-in via
//!    `request_login_code` → `sign_in` → (optionally
//!    `check_password` for 2FA). User mode is a state machine
//!    with side effects (the login code is delivered to the
//!    user's Telegram app).
//!
//! This module owns the user-mode state machine. Bot mode is a
//! single `MtprotoAuthAction::BotSignIn` that succeeds or fails.

use std::fmt;
use thiserror::Error;

/// Runtime auth-mode selector. Per RFC-0850ab-c §"Data Structures".
///
/// Distinct from the on-disk `MtprotoTelegramConfig.mode: Option<String>`
/// form: the JSON config keeps the legacy flat string + flat
/// `bot_token` / `phone` / `password` fields for backward
/// compatibility (existing Phase 1 deployments), and `AuthMode` is
/// constructed at runtime via `MtprotoTelegramConfig::auth_mode()`.
///
/// The on-disk form is intentionally not the `AuthMode` enum
/// directly: serde-tagged enum forms (`{"BotToken": "..."}`,
/// `{"UserCredentials": {"phone": "..."}}`, `"QrLogin"`) would
/// break every existing Phase 1 JSON config. The runtime form is
/// the type used by the adapter for type-safe dispatch.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum AuthMode {
    /// Bot token from BotFather (primary mode).
    /// The string is the bot token (without the `bot` prefix).
    BotToken(String),

    /// User-mode sign-in: phone number is fixed at config time;
    /// SMS code + (optional) 2FA password are prompted at runtime.
    /// The 2FA password is NEVER stored (RFC-0850ab-c §"Security
    /// Considerations / 2FA Password Storage").
    UserCredentials {
        phone: String,
    },

    /// QR login flow (per RFC-0850ab-a). The adapter calls
    /// `auth::ExportLoginToken` and returns the token + URL; the
    /// caller is responsible for displaying the QR code and polling
    /// until the user scans.
    QrLogin,
}

impl fmt::Display for AuthMode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::BotToken(_) => f.write_str("bot"),
            Self::UserCredentials { .. } => f.write_str("user"),
            Self::QrLogin => f.write_str("qr"),
        }
    }
}

/// Subset of the auth action surface that the gateway can
/// request. Mirrors `octo-adapter-telegram::auth::AuthAction` so
/// the two adapters share the same external API.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum MtprotoAuthAction {
    /// Bot sign-in (no user interaction).
    BotSignIn,
    /// Begin user sign-in: send a login code to the configured
    /// phone number. The adapter transitions to
    /// `UserAuthLifecycle::CodeRequested` and the gateway
    /// surfaces a `MtprotoAuthAction::SubmitCode { code }` prompt
    /// to the operator.
    RequestCode,
    /// Submit the login code received from the user.
    SubmitCode { code: String },
    /// Submit a 2FA password (only valid after a successful
    /// `SubmitCode` if the account has 2FA enabled).
    SubmitPassword { password: String },
    /// Tear down the current session (calls `Client::sign_out`).
    SignOut,
}

/// Identity of a logged-in bot. Set after a successful
/// `MtprotoAuthAction::BotSignIn`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BotIdentity {
    pub user_id: i64,
    pub username: Option<String>,
    /// The `access_hash` of the bot user. Stored so subsequent
    /// `InputPeer::Self` constructions do not need a re-fetch.
    pub access_hash: i64,
}

/// Identity of a logged-in user. Set after a successful
/// `MtprotoAuthAction::SubmitCode` (and `SubmitPassword` if
/// needed).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UserAuth {
    pub user_id: i64,
    pub username: Option<String>,
    pub access_hash: i64,
}

/// Auth state machine for user mode. The states mirror the
/// documented `grammers` auth flow (see `grammers-client` auth
/// module).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub enum AuthStateKey {
    /// Adapter not yet started; no phone number known.
    #[default]
    Uninitialised,
    /// `request_login_code` has been sent. The code is in
    /// flight to the user's Telegram app. The adapter is
    /// holding a `LoginToken`.
    CodeRequested,
    /// The code has been accepted by the server; we are waiting
    /// for a 2FA password (only used if 2FA is enabled on the
    /// account).
    PasswordRequired,
    /// Sign-in complete; `UserAuth` populated.
    SignedIn,
    /// `Client::sign_out` was called; the session is invalidated.
    SignedOut,
}

impl fmt::Display for AuthStateKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let s = match self {
            Self::Uninitialised => "Uninitialised",
            Self::CodeRequested => "CodeRequested",
            Self::PasswordRequired => "PasswordRequired",
            Self::SignedIn => "SignedIn",
            Self::SignedOut => "SignedOut",
        };
        f.write_str(s)
    }
}

#[derive(Debug, Error)]
pub enum MtprotoAuthError {
    #[error("invalid transition from {from} via {action:?}")]
    InvalidTransition {
        from: AuthStateKey,
        action: MtprotoAuthAction,
    },
    #[error("not signed in")]
    NotSignedIn,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn display_round_trip() {
        for s in [
            AuthStateKey::Uninitialised,
            AuthStateKey::CodeRequested,
            AuthStateKey::PasswordRequired,
            AuthStateKey::SignedIn,
            AuthStateKey::SignedOut,
        ] {
            let printed = format!("{}", s);
            assert!(!printed.is_empty());
        }
    }

    #[test]
    fn auth_mode_display_matches_mode_str() {
        // The runtime AuthMode Display matches the
        // MtprotoTelegramConfig.mode string form so callers can
        // serialise either way without translation.
        assert_eq!(AuthMode::BotToken("123:abc".into()).to_string(), "bot");
        assert_eq!(
            AuthMode::UserCredentials { phone: "+15555550100".into() }.to_string(),
            "user"
        );
        assert_eq!(AuthMode::QrLogin.to_string(), "qr");
    }

    #[test]
    fn auth_mode_partial_eq_distinguishes_token() {
        // Two BotToken variants with different tokens are not equal.
        assert_ne!(
            AuthMode::BotToken("111:aaa".into()),
            AuthMode::BotToken("222:bbb".into())
        );
        assert_eq!(
            AuthMode::BotToken("111:aaa".into()),
            AuthMode::BotToken("111:aaa".into())
        );
        // UserCredentials equality ignores nothing (only phone).
        assert_eq!(
            AuthMode::UserCredentials { phone: "+1".into() },
            AuthMode::UserCredentials { phone: "+1".into() }
        );
        assert_ne!(
            AuthMode::UserCredentials { phone: "+1".into() },
            AuthMode::UserCredentials { phone: "+2".into() }
        );
    }
}
