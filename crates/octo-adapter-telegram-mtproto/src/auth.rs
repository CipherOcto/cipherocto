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

use crate::lifecycle::UserAuthLifecycle;

/// Subset of the user-mode auth action surface that the gateway
/// can request. Per RFC-0850ab-c §"Algorithms / Algorithm 2 & 3".
/// Distinct from `MtprotoAuthAction` (bot-mode flow + the unified
/// RequestCode/SubmitCode/SubmitPassword shape); `UserAuthAction`
/// is the user-mode-specific equivalent that drives the
/// `UserAuthLifecycle` state machine.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum UserAuthAction {
    /// Begin user sign-in: send a login code to the configured
    /// phone number. Transitions `NoCredentials → PhoneProvided`
    /// (then `PhoneProvided → SmsCodeSent` after the server
    /// `request_login_code` succeeds).
    RequestCode { phone: String },

    /// Submit the SMS code received from the user.
    /// Transitions `SmsCodeSent → SmsCodeProvided`.
    SubmitCode { code: String },

    /// Submit a 2FA password (only valid after a successful
    /// `SubmitCode` if the account has 2FA enabled).
    /// Transitions `PasswordRequired → PasswordProvided`.
    SubmitPassword { password: String },

    /// Start the QR login flow. The adapter calls
    /// `auth::ExportLoginToken` and returns a `QrLoginHandle`.
    /// Transitions `NoCredentials → QrLoginPending`.
    QrLoginStart,

    /// Confirm that the user has scanned the QR code (the
    /// caller is responsible for detecting the scan via
    /// `poll_qr_login`). Transitions
    /// `QrLoginPending → QrLoginConfirmed`.
    QrLoginConfirm,

    /// Tear down the current user session (calls
    /// `Client::sign_out` and resets the StoolapSession).
    /// Transitions `SignedIn → SigningOut → SignedOut`.
    SignOut,
}

impl fmt::Display for UserAuthAction {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let s = match self {
            Self::RequestCode { .. } => "RequestCode",
            Self::SubmitCode { .. } => "SubmitCode",
            Self::SubmitPassword { .. } => "SubmitPassword",
            Self::QrLoginStart => "QrLoginStart",
            Self::QrLoginConfirm => "QrLoginConfirm",
            Self::SignOut => "SignOut",
        };
        f.write_str(s)
    }
}

/// The single source of truth for user-mode transitions. Pure
/// function (no I/O); exhaustively unit-tested.
///
/// Mirrors RFC-0850ab-a §"User Auth State Machine" and
/// RFC-0850ab-c §"Lifecycle Requirements / UserAuthLifecycle
/// State Machine" + §"Algorithms / Algorithm 2 & 3".
///
/// Transitions are divided into client-side transitions (driven
/// by `UserAuthAction`) and server-side transitions (driven by
/// the grammers client response). Both are modelled here so the
/// adapter and the test harness can call them uniformly.
///
/// Client-side transitions (driven by `UserAuthAction`):
///   - `NoCredentials → PhoneProvided`         on `RequestCode`
///   - `SmsCodeSent → SmsCodeProvided`        on `SubmitCode`
///   - `PasswordRequired → PasswordProvided`  on `SubmitPassword`
///   - `NoCredentials → QrLoginPending`       on `QrLoginStart`
///   - `QrLoginPending → QrLoginConfirmed`    on `QrLoginConfirm`
///   - `SignedIn → SigningOut`                on `SignOut`
///   - `SigningOut → SignedOut`               on `SignOut`
///
/// Server-side transitions (driven by `next_user_auth_state_server`):
///   - `PhoneProvided → SmsCodeSent`          on `RequestCodeSucceeded`
///   - `SmsCodeProvided → SignedIn`           on `SignInSucceeded`
///   - `SmsCodeProvided → PasswordRequired`   on `PasswordRequired`
///   - `PasswordProvided → SignedIn`          on `CheckPasswordSucceeded`
///   - `QrLoginConfirmed → SignedIn`          on `SignInSucceeded`
///   - `QrLoginConfirmed → PasswordRequired`  on `PasswordRequired`
///
/// Any other (state, action) pair is `MtprotoAuthError::InvalidTransition`.
pub fn next_user_auth_state(
    action: UserAuthAction,
    current: UserAuthLifecycle,
) -> Result<UserAuthLifecycle, MtprotoAuthError> {
    use UserAuthAction::*;
    use UserAuthLifecycle::*;
    match (current, action) {
        // ----- Client-driven transitions -----
        (NoCredentials, RequestCode { .. }) => Ok(PhoneProvided),
        (SmsCodeSent, SubmitCode { .. }) => Ok(SmsCodeProvided),
        (PasswordRequired, SubmitPassword { .. }) => Ok(PasswordProvided),
        (NoCredentials, QrLoginStart) => Ok(QrLoginPending),
        (QrLoginPending, QrLoginConfirm) => Ok(QrLoginConfirmed),
        (SignedIn, SignOut) => Ok(SigningOut),
        (SigningOut, SignOut) => Ok(SignedOut),

        // ----- Invalid combinations (representative ones; the
        // ----- catch-all below returns InvalidTransition for
        // ----- every other pair). We bind both `from` and `action`
        // ----- so neither is moved-out of the tuple. -----
        (from, action) => Err(MtprotoAuthError::InvalidUserTransition { from, action }),
    }
}

/// Server-driven user-mode transitions. Called by the adapter
/// after the grammers client returns from a request:
///   - `RequestCodeSucceeded`     — `auth.sendCode` returned Ok
///     with a `SentCode`; advance `PhoneProvided → SmsCodeSent`.
///   - `SignInSucceeded`          — `auth.signIn` returned
///     `Ok(Authorization)` (no 2FA required); advance
///     `SmsCodeProvided → SignedIn` (or `QrLoginConfirmed →
///     SignedIn`).
///   - `PasswordRequired`         — `auth.signIn` returned
///     `SESSION_PASSWORD_NEEDED`; advance
///     `SmsCodeProvided → PasswordRequired` (or
///     `QrLoginConfirmed → PasswordRequired`).
///   - `CheckPasswordSucceeded`   — `auth.checkPassword` returned
///     `Ok(Authorization)`; advance
///     `PasswordProvided → SignedIn`.
///
/// Any other (state, server_event) pair is
/// `MtprotoAuthError::InvalidUserTransition`.
pub fn next_user_auth_state_server(
    event: UserAuthServerEvent,
    current: UserAuthLifecycle,
) -> Result<UserAuthLifecycle, MtprotoAuthError> {
    use UserAuthLifecycle::*;
    // Note: UserAuthLifecycle and UserAuthServerEvent both have a
    // `PasswordRequired` variant, so we fully-qualify the event
    // variants below to avoid ambiguity.
    match (current, event) {
        (PhoneProvided, UserAuthServerEvent::RequestCodeSucceeded) => Ok(SmsCodeSent),
        (SmsCodeProvided, UserAuthServerEvent::SignInSucceeded) => Ok(SignedIn),
        (SmsCodeProvided, UserAuthServerEvent::PasswordRequired) => Ok(PasswordRequired),
        (PasswordProvided, UserAuthServerEvent::CheckPasswordSucceeded) => Ok(SignedIn),
        (QrLoginConfirmed, UserAuthServerEvent::SignInSucceeded) => Ok(SignedIn),
        (QrLoginConfirmed, UserAuthServerEvent::PasswordRequired) => Ok(PasswordRequired),

        (from, event) => Err(MtprotoAuthError::InvalidUserServerTransition { from, event }),
    }
}

/// Server-side event in the user-mode auth flow. Distinct from
/// `UserAuthAction` because these events are NOT operator-driven;
/// they are emitted by the grammers client as RPC responses.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum UserAuthServerEvent {
    /// `auth.sendCode` returned Ok with a `SentCode`. Adapter
    /// advances `PhoneProvided → SmsCodeSent`.
    RequestCodeSucceeded,
    /// `auth.signIn` returned `Ok(Authorization)` with no 2FA
    /// required. Adapter advances `SmsCodeProvided → SignedIn`
    /// (or `QrLoginConfirmed → SignedIn`).
    SignInSucceeded,
    /// `auth.signIn` returned `SESSION_PASSWORD_NEEDED`. Adapter
    /// advances `SmsCodeProvided → PasswordRequired` (or
    /// `QrLoginConfirmed → PasswordRequired`).
    PasswordRequired,
    /// `auth.checkPassword` returned `Ok(Authorization)`. Adapter
    /// advances `PasswordProvided → SignedIn`.
    CheckPasswordSucceeded,
}

impl fmt::Display for UserAuthServerEvent {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let s = match self {
            Self::RequestCodeSucceeded => "RequestCodeSucceeded",
            Self::SignInSucceeded => "SignInSucceeded",
            Self::PasswordRequired => "PasswordRequired",
            Self::CheckPasswordSucceeded => "CheckPasswordSucceeded",
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
    #[error("invalid user-mode transition from {from} via {action}")]
    InvalidUserTransition {
        from: UserAuthLifecycle,
        action: UserAuthAction,
    },
    #[error("invalid user-mode server transition from {from} via {event}")]
    InvalidUserServerTransition {
        from: UserAuthLifecycle,
        event: UserAuthServerEvent,
    },
    #[error("not signed in")]
    NotSignedIn,
}

/// Map a `MtprotoAuthError` (state-machine error) into the
/// public `MtprotoTelegramError` so the real-network impl
/// (`real_client.rs`) can use `?` to propagate state-machine
/// failures without manually wrapping each one.
///
/// Mapping:
/// - `InvalidTransition` / `InvalidUserTransition` /
///   `InvalidUserServerTransition` → `Auth(...)` (operator
///   called an action out of order; this is a programmer error,
///   not a transient failure).
/// - `NotSignedIn` → `Auth(...)` (auth-related).
impl From<MtprotoAuthError> for crate::error::MtprotoTelegramError {
    fn from(e: MtprotoAuthError) -> Self {
        use MtprotoAuthError::*;
        match e {
            InvalidTransition { from, action } => {
                crate::error::MtprotoTelegramError::Auth(format!(
                    "invalid transition from {} via {:?}",
                    from, action
                ))
            }
            InvalidUserTransition { from, action } => {
                crate::error::MtprotoTelegramError::Auth(format!(
                    "invalid user-mode transition from {} via {}",
                    from, action
                ))
            }
            InvalidUserServerTransition { from, event } => {
                crate::error::MtprotoTelegramError::Auth(format!(
                    "invalid user-mode server transition from {} via {}",
                    from, event
                ))
            }
            NotSignedIn => {
                crate::error::MtprotoTelegramError::Auth("not signed in".into())
            }
        }
    }
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

    // ----- User-mode state machine tests -----

    #[test]
    fn user_auth_action_display_round_trip() {
        use UserAuthAction::*;
        for a in [
            RequestCode { phone: "+1".into() },
            SubmitCode { code: "12345".into() },
            SubmitPassword { password: "secret".into() },
            QrLoginStart,
            QrLoginConfirm,
            SignOut,
        ] {
            let printed = format!("{}", a);
            assert!(!printed.is_empty());
        }
    }

    #[test]
    fn user_auth_server_event_display_round_trip() {
        use UserAuthServerEvent::*;
        for e in [RequestCodeSucceeded, SignInSucceeded, PasswordRequired, CheckPasswordSucceeded] {
            let printed = format!("{}", e);
            assert!(!printed.is_empty());
        }
    }

    #[test]
    fn user_auth_happy_path_no_2fa() {
        use UserAuthAction::*;
        use UserAuthLifecycle::*;

        // 1. Operator provides phone → NoCredentials → PhoneProvided.
        let s = next_user_auth_state(
            RequestCode { phone: "+15555550100".into() },
            NoCredentials,
        )
        .unwrap();
        assert_eq!(s, PhoneProvided);

        // 2. Server request_login_code succeeds → PhoneProvided → SmsCodeSent.
        let s = next_user_auth_state_server(UserAuthServerEvent::RequestCodeSucceeded, s).unwrap();
        assert_eq!(s, SmsCodeSent);

        // 3. Operator submits SMS code → SmsCodeSent → SmsCodeProvided.
        let s = next_user_auth_state(SubmitCode { code: "12345".into() }, s).unwrap();
        assert_eq!(s, SmsCodeProvided);

        // 4. Server sign_in succeeds (no 2FA) → SmsCodeProvided → SignedIn.
        let s = next_user_auth_state_server(UserAuthServerEvent::SignInSucceeded, s).unwrap();
        assert_eq!(s, SignedIn);

        // 5. Operator signs out → SignedIn → SigningOut → SignedOut.
        let s = next_user_auth_state(SignOut, s).unwrap();
        assert_eq!(s, SigningOut);
        let s = next_user_auth_state(SignOut, s).unwrap();
        assert_eq!(s, SignedOut);
    }

    #[test]
    fn user_auth_happy_path_with_2fa() {
        use UserAuthAction::*;
        use UserAuthLifecycle::*;

        // Steps 1-3 identical to the no-2FA happy path.
        let s = next_user_auth_state(
            RequestCode { phone: "+15555550100".into() },
            NoCredentials,
        )
        .unwrap();
        let s = next_user_auth_state_server(UserAuthServerEvent::RequestCodeSucceeded, s).unwrap();
        let s = next_user_auth_state(SubmitCode { code: "12345".into() }, s).unwrap();

        // 4. Server returns SESSION_PASSWORD_NEEDED → PasswordRequired.
        let s = next_user_auth_state_server(UserAuthServerEvent::PasswordRequired, s).unwrap();
        assert_eq!(s, PasswordRequired);

        // 5. Operator submits 2FA password → PasswordRequired → PasswordProvided.
        let s =
            next_user_auth_state(SubmitPassword { password: "secret".into() }, s).unwrap();
        assert_eq!(s, PasswordProvided);

        // 6. Server check_password succeeds → PasswordProvided → SignedIn.
        let s =
            next_user_auth_state_server(UserAuthServerEvent::CheckPasswordSucceeded, s).unwrap();
        assert_eq!(s, SignedIn);
    }

    #[test]
    fn user_auth_happy_path_qr_login() {
        use UserAuthAction::*;
        use UserAuthLifecycle::*;

        // QR login flow:
        //   NoCredentials → QrLoginPending → QrLoginConfirmed → SignedIn.
        let s = next_user_auth_state(QrLoginStart, NoCredentials).unwrap();
        assert_eq!(s, QrLoginPending);

        let s = next_user_auth_state(QrLoginConfirm, s).unwrap();
        assert_eq!(s, QrLoginConfirmed);

        let s = next_user_auth_state_server(UserAuthServerEvent::SignInSucceeded, s).unwrap();
        assert_eq!(s, SignedIn);
    }

    #[test]
    fn user_auth_qr_login_with_2fa_on_primary() {
        // Same as qr_login happy path, but the primary device has
        // 2FA enabled. After QrLoginConfirmed, the server returns
        // SESSION_PASSWORD_NEEDED → PasswordRequired → ...
        use UserAuthAction::*;
        use UserAuthLifecycle::*;

        let s = next_user_auth_state(QrLoginStart, NoCredentials).unwrap();
        let s = next_user_auth_state(QrLoginConfirm, s).unwrap();
        let s = next_user_auth_state_server(UserAuthServerEvent::PasswordRequired, s).unwrap();
        assert_eq!(s, PasswordRequired);
        let s =
            next_user_auth_state(SubmitPassword { password: "secret".into() }, s).unwrap();
        let s =
            next_user_auth_state_server(UserAuthServerEvent::CheckPasswordSucceeded, s).unwrap();
        assert_eq!(s, SignedIn);
    }

    #[test]
    fn user_auth_invalid_transitions() {
        use UserAuthAction::*;
        use UserAuthLifecycle::*;

        // SubmitCode from NoCredentials is not valid (must RequestCode first).
        let err = next_user_auth_state(
            SubmitCode { code: "12345".into() },
            NoCredentials,
        )
        .unwrap_err();
        assert!(matches!(
            err,
            MtprotoAuthError::InvalidUserTransition { .. }
        ));

        // SubmitPassword from SmsCodeSent (no 2FA flow yet) is invalid.
        let err = next_user_auth_state(
            SubmitPassword { password: "x".into() },
            SmsCodeSent,
        )
        .unwrap_err();
        assert!(matches!(
            err,
            MtprotoAuthError::InvalidUserTransition { .. }
        ));

        // SignOut from SmsCodeSent (not signed in yet) is invalid.
        let err = next_user_auth_state(SignOut, SmsCodeSent).unwrap_err();
        assert!(matches!(
            err,
            MtprotoAuthError::InvalidUserTransition { .. }
        ));

        // QrLoginConfirm from NoCredentials (no QR pending) is invalid.
        let err = next_user_auth_state(QrLoginConfirm, NoCredentials).unwrap_err();
        assert!(matches!(
            err,
            MtprotoAuthError::InvalidUserTransition { .. }
        ));
    }

    #[test]
    fn user_auth_invalid_server_transitions() {
        // No glob import here on purpose: the variants below are
        // fully-qualified via `UserAuthServerEvent::` so we don't
        // shadow the `UserAuthLifecycle::NoCredentials` /
        // `UserAuthLifecycle::PasswordProvided` / etc. that
        // appear in the same expressions.

        // RequestCodeSucceeded from NoCredentials (must RequestCode first) is invalid.
        let err = next_user_auth_state_server(
            UserAuthServerEvent::RequestCodeSucceeded,
            UserAuthLifecycle::NoCredentials,
        )
        .unwrap_err();
        assert!(matches!(
            err,
            MtprotoAuthError::InvalidUserServerTransition { .. }
        ));

        // SignInSucceeded from PasswordProvided (must check_password first) is invalid.
        let err = next_user_auth_state_server(
            UserAuthServerEvent::SignInSucceeded,
            UserAuthLifecycle::PasswordProvided,
        )
        .unwrap_err();
        assert!(matches!(
            err,
            MtprotoAuthError::InvalidUserServerTransition { .. }
        ));

        // CheckPasswordSucceeded from SmsCodeProvided (must enter 2FA state first) is invalid.
        let err = next_user_auth_state_server(
            UserAuthServerEvent::CheckPasswordSucceeded,
            UserAuthLifecycle::SmsCodeProvided,
        )
        .unwrap_err();
        assert!(matches!(
            err,
            MtprotoAuthError::InvalidUserServerTransition { .. }
        ));
    }

    #[test]
    fn user_auth_sign_out_only_from_signed_in_or_signing_out() {
        use UserAuthAction::*;
        use UserAuthLifecycle::*;

        // SignOut from any other state is invalid.
        for bad in [
            NoCredentials,
            PhoneProvided,
            SmsCodeSent,
            SmsCodeProvided,
            PasswordRequired,
            PasswordProvided,
            QrLoginPending,
            QrLoginConfirmed,
            SignedOut,
        ] {
            let r = next_user_auth_state(SignOut, bad);
            assert!(
                r.is_err(),
                "SignOut from {:?} should be invalid but got {:?}",
                bad,
                r
            );
        }
    }

    // ----- From<MtprotoAuthError> for MtprotoTelegramError -----

    #[test]
    fn auth_error_to_telegram_error_mapping() {
        use crate::error::MtprotoTelegramError;
        use MtprotoAuthError::*;

        let e: MtprotoTelegramError = InvalidUserTransition {
            from: UserAuthLifecycle::NoCredentials,
            action: UserAuthAction::SignOut,
        }
        .into();
        match e {
            MtprotoTelegramError::Auth(msg) => {
                assert!(msg.contains("invalid user-mode transition"), "msg = {}", msg);
                assert!(msg.contains("SignOut"), "msg = {}", msg);
            }
            other => panic!("expected Auth, got {:?}", other),
        }

        let e: MtprotoTelegramError = InvalidUserServerTransition {
            from: UserAuthLifecycle::NoCredentials,
            event: UserAuthServerEvent::SignInSucceeded,
        }
        .into();
        match e {
            MtprotoTelegramError::Auth(msg) => {
                assert!(
                    msg.contains("invalid user-mode server transition"),
                    "msg = {}",
                    msg
                );
                assert!(msg.contains("SignInSucceeded"), "msg = {}", msg);
            }
            other => panic!("expected Auth, got {:?}", other),
        }

        let e: MtprotoTelegramError = NotSignedIn.into();
        match e {
            MtprotoTelegramError::Auth(msg) => {
                assert_eq!(msg, "not signed in");
            }
            other => panic!("expected Auth, got {:?}", other),
        }
    }
}
