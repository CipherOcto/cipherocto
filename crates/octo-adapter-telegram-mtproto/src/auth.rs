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
}
