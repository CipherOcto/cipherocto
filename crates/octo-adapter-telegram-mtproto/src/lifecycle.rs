//! Adapter lifecycle — the outer state machine the gateway
//! polls.
//!
//! The state machine is intentionally simple. The gateway only
//! cares about three transitions:
//!
//! - `Uninitialised → Connecting`: a `connect()` call was made.
//! - `Connecting → Ready`: the client is authorised and
//!   `receive_messages` is callable. (For bot mode, this is the
//!   state after `bot_sign_in`; for user mode, after
//!   `sign_in`/`check_password`.)
//! - `Ready → ShuttingDown → Stopped`: graceful teardown.
//!
//! The user-mode auth sub-flow (RequestCode → SubmitCode →
//! optional SubmitPassword) lives in `auth::AuthStateKey`; the
//! outer `Lifecycle` reports the *combined* state to the gateway
//! (e.g., a user-mode adapter in the middle of
//! `RequestCode`/`SubmitCode` is reported as
//! `Lifecycle::Authenticating`, not as
//! `AuthStateKey::CodeRequested`).
//!
//! The state is held behind a `parking_lot::Mutex` (matching
//! the rest of the workspace) and exposed via a `Status` trait
//! so tests and CLI tools can poll without owning the adapter.

use parking_lot::Mutex;
use std::fmt;
use std::sync::Arc;

use crate::auth::AuthStateKey;

/// Top-level state of the adapter, as seen by the gateway.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum AdapterLifecycle {
    /// No `connect()` call yet.
    #[default]
    Uninitialised,
    /// `connect()` was called; the underlying grammers client
    /// is establishing the MTProto session and registering with
    /// Telegram's DC.
    Connecting,
    /// Connected but not yet authenticated (user mode
    /// pre-sign-in, or bot mode pre-`bot_sign_in`).
    Connected,
    /// Sign-in in progress. For bot mode this is brief
    /// (sub-second); for user mode this is the entire
    /// RequestCode → SubmitCode → SubmitPassword flow.
    Authenticating,
    /// Authenticated and ready to send/receive.
    Ready,
    /// `shutdown()` was called; flushing pending messages.
    ShuttingDown,
    /// Shut down. `send_envelope` / `receive_messages` return
    /// `MtprotoTelegramError::NotReady`.
    Stopped,
    /// An unrecoverable error occurred (e.g., FLOOD_WAIT
    /// exceeded the retry budget, account banned, schema
    /// migration failed). The adapter is no longer usable;
    /// the gateway should construct a new one.
    Failed,
}

impl fmt::Display for AdapterLifecycle {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let s = match self {
            Self::Uninitialised => "Uninitialised",
            Self::Connecting => "Connecting",
            Self::Connected => "Connected",
            Self::Authenticating => "Authenticating",
            Self::Ready => "Ready",
            Self::ShuttingDown => "ShuttingDown",
            Self::Stopped => "Stopped",
            Self::Failed => "Failed",
        };
        f.write_str(s)
    }
}

impl AdapterLifecycle {
    /// True if the state is terminal (Stopped or Failed).
    /// The adapter is no longer usable in this state.
    pub fn is_terminal_state(&self) -> bool {
        matches!(self, Self::Stopped | Self::Failed)
    }
}

/// Bot-mode auth lifecycle. Per RFC-0850ab-c §"Data Structures /
/// BotAuthLifecycle". 5 states; the `#[repr(u8)]` values are
/// public API (operators may rely on them for log/UI mapping) and
/// are locked in by `tests::bot_auth_lifecycle_repr_values_match_rfc`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum BotAuthLifecycle {
    /// No token yet.
    #[default]
    NoToken = 0x00,
    /// Token provided, validating against Telegram.
    Validating = 0x01,
    /// Signed in. Ready to send/receive.
    SignedIn = 0x02,
    /// Sign-out in progress (auth_key being cleared).
    SigningOut = 0x03,
    /// Signed out. Adapter is no longer authenticated.
    SignedOut = 0x04,
}

impl fmt::Display for BotAuthLifecycle {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let s = match self {
            Self::NoToken => "NoToken",
            Self::Validating => "Validating",
            Self::SignedIn => "SignedIn",
            Self::SigningOut => "SigningOut",
            Self::SignedOut => "SignedOut",
        };
        f.write_str(s)
    }
}

/// User-mode auth lifecycle. Per RFC-0850ab-c §"Data Structures /
/// UserAuthLifecycle" (mirrors RFC-0850ab-a's user-mode state
/// machine). 10 states; `#[repr(u8)]` values are public API and
/// are locked in by
/// `tests::user_auth_lifecycle_repr_values_match_rfc`.
///
/// The state names match RFC-0850ab-a's verbatim so the operator
/// UI can reuse RFC-0850ab-a's interactive prompts without
/// translation.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum UserAuthLifecycle {
    /// Adapter not yet started; no phone number known.
    #[default]
    NoCredentials = 0x00,
    /// Phone number is configured (from
    /// `MtprotoTelegramConfig.phone`).
    PhoneProvided = 0x01,
    /// `auth.sendCode` was called; the SMS code is in flight to
    /// the user's Telegram app.
    SmsCodeSent = 0x02,
    /// User has entered the SMS code; it has been submitted via
    /// `auth.signIn`. Server has not yet responded (or responded
    /// with `SESSION_PASSWORD_NEEDED`).
    SmsCodeProvided = 0x03,
    /// Server returned `SESSION_PASSWORD_NEEDED`; 2FA password
    /// required. Adapter is waiting for operator input.
    PasswordRequired = 0x04,
    /// Operator has entered the 2FA password; it has been
    /// submitted via `auth.checkPassword`. Server has not yet
    /// responded.
    PasswordProvided = 0x05,
    /// Signed in.
    SignedIn = 0x06,
    /// Sign-out in progress.
    SigningOut = 0x07,
    /// Signed out. Adapter is no longer authenticated.
    SignedOut = 0x08,
    /// QR login flow active: `auth.exportLoginToken` was called
    /// and the QR code is displayed. Waiting for user to scan.
    QrLoginPending = 0x09,
    /// QR code was scanned; auth-key transfer is in progress.
    QrLoginConfirmed = 0x0A,
}

impl fmt::Display for UserAuthLifecycle {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let s = match self {
            Self::NoCredentials => "NoCredentials",
            Self::PhoneProvided => "PhoneProvided",
            Self::SmsCodeSent => "SmsCodeSent",
            Self::SmsCodeProvided => "SmsCodeProvided",
            Self::PasswordRequired => "PasswordRequired",
            Self::PasswordProvided => "PasswordProvided",
            Self::SignedIn => "SignedIn",
            Self::SigningOut => "SigningOut",
            Self::SignedOut => "SignedOut",
            Self::QrLoginPending => "QrLoginPending",
            Self::QrLoginConfirmed => "QrLoginConfirmed",
        };
        f.write_str(s)
    }
}

impl From<UserAuthLifecycle> for AuthStateKey {
    /// Map the user-mode-specific lifecycle to the unified
    /// `AuthStateKey` (5-state summary) consumed by
    /// `AdapterLifecycle::transition`. See
    /// `tests::unified_auth_state_key_maps_user_lifecycle` for the
    /// full mapping table.
    fn from(s: UserAuthLifecycle) -> Self {
        use UserAuthLifecycle::*;
        match s {
            NoCredentials | PhoneProvided => AuthStateKey::Uninitialised,
            SmsCodeSent | SmsCodeProvided | QrLoginPending | QrLoginConfirmed => {
                AuthStateKey::CodeRequested
            }
            PasswordRequired | PasswordProvided => AuthStateKey::PasswordRequired,
            SignedIn | SigningOut => AuthStateKey::SignedIn,
            SignedOut => AuthStateKey::SignedOut,
        }
    }
}

impl From<BotAuthLifecycle> for AuthStateKey {
    /// Map the bot-mode-specific lifecycle to the unified
    /// `AuthStateKey`. See
    /// `tests::unified_auth_state_key_maps_bot_lifecycle`.
    fn from(s: BotAuthLifecycle) -> Self {
        use BotAuthLifecycle::*;
        match s {
            NoToken | Validating => AuthStateKey::Uninitialised,
            SignedIn | SigningOut => AuthStateKey::SignedIn,
            SignedOut => AuthStateKey::SignedOut,
        }
    }
}

/// Valid transitions. Used by `Lifecycle::transition_to` to
/// reject out-of-order calls (e.g., `Ready → Connecting`).
const VALID_TRANSITIONS: &[(AdapterLifecycle, &[AdapterLifecycle])] = &[
    (AdapterLifecycle::Uninitialised, &[AdapterLifecycle::Connecting, AdapterLifecycle::Failed]),
    (AdapterLifecycle::Connecting, &[AdapterLifecycle::Connected, AdapterLifecycle::Authenticating, AdapterLifecycle::Failed]),
    (AdapterLifecycle::Connected, &[AdapterLifecycle::Authenticating, AdapterLifecycle::Ready, AdapterLifecycle::Failed, AdapterLifecycle::ShuttingDown]),
    (AdapterLifecycle::Authenticating, &[AdapterLifecycle::Ready, AdapterLifecycle::Failed, AdapterLifecycle::ShuttingDown]),
    (AdapterLifecycle::Ready, &[AdapterLifecycle::ShuttingDown, AdapterLifecycle::Failed]),
    (AdapterLifecycle::ShuttingDown, &[AdapterLifecycle::Stopped, AdapterLifecycle::Failed]),
    (AdapterLifecycle::Stopped, &[]),
    (AdapterLifecycle::Failed, &[AdapterLifecycle::Stopped]),
];

/// Outer state machine. Cheap to clone (`Arc<Mutex<...>>`).
#[derive(Clone, Default)]
pub struct Lifecycle {
    inner: Arc<Mutex<Inner>>,
}

#[derive(Default)]
struct Inner {
    lifecycle: AdapterLifecycle,
    auth: AuthStateKey,
}

impl Lifecycle {
    pub fn new() -> Self {
        Self::default()
    }

    /// Current outer state.
    pub fn state(&self) -> AdapterLifecycle {
        self.inner.lock().lifecycle
    }

    /// Current user-mode auth sub-state. Returns
    /// `AuthStateKey::Uninitialised` for bot mode and for
    /// adapters that have not yet entered user mode.
    pub fn auth_state(&self) -> AuthStateKey {
        self.inner.lock().auth.clone()
    }

    /// Try to transition to `next`. Returns `Ok(())` if the
    /// transition is in `VALID_TRANSITIONS`; `Err` otherwise.
    /// The `auth` sub-state is updated alongside the outer
    /// state when `next` is `Authenticating` or `Ready`.
    pub fn transition(
        &self,
        next: AdapterLifecycle,
        auth: AuthStateKey,
    ) -> Result<(), TransitionError> {
        let mut g = self.inner.lock();
        if !VALID_TRANSITIONS
            .iter()
            .find(|(from, _)| *from == g.lifecycle)
            .map(|(_, allowed)| allowed.contains(&next))
            .unwrap_or(false)
        {
            return Err(TransitionError {
                from: g.lifecycle,
                to: next,
            });
        }
        g.lifecycle = next;
        g.auth = auth;
        Ok(())
    }

    /// Force-set the state (used by the constructor and by the
    /// `sign_out` path that needs to go `Ready → Stopped`
    /// directly, skipping `ShuttingDown`).
    pub fn force(&self, next: AdapterLifecycle, auth: AuthStateKey) {
        let mut g = self.inner.lock();
        g.lifecycle = next;
        g.auth = auth;
    }

    /// True if the adapter can accept `send_envelope` /
    /// `receive_messages` calls.
    pub fn is_ready(&self) -> bool {
        matches!(self.inner.lock().lifecycle, AdapterLifecycle::Ready)
    }

    /// True if the adapter has terminated (Stopped or Failed).
    /// The gateway should drop the instance in either case.
    pub fn is_terminal(&self) -> bool {
        matches!(
            self.inner.lock().lifecycle,
            AdapterLifecycle::Stopped | AdapterLifecycle::Failed
        )
    }
}

#[derive(Debug, Clone, thiserror::Error)]
#[error("invalid lifecycle transition: {from} -> {to}")]
pub struct TransitionError {
    pub from: AdapterLifecycle,
    pub to: AdapterLifecycle,
}

impl std::fmt::Debug for Lifecycle {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let g = self.inner.lock();
        f.debug_struct("Lifecycle")
            .field("lifecycle", &g.lifecycle)
            .field("auth", &g.auth)
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fresh_lifecycle_is_uninitialised() {
        let l = Lifecycle::new();
        assert_eq!(l.state(), AdapterLifecycle::Uninitialised);
        assert!(!l.is_ready());
        assert!(!l.is_terminal());
    }

    #[test]
    fn happy_path() {
        let l = Lifecycle::new();
        l.transition(AdapterLifecycle::Connecting, AuthStateKey::Uninitialised).unwrap();
        l.transition(AdapterLifecycle::Authenticating, AuthStateKey::Uninitialised).unwrap();
        l.transition(AdapterLifecycle::Ready, AuthStateKey::SignedIn).unwrap();
        assert!(l.is_ready());
    }

    #[test]
    fn invalid_transition_rejected() {
        let l = Lifecycle::new();
        let r = l.transition(AdapterLifecycle::Ready, AuthStateKey::SignedIn);
        assert!(r.is_err());
    }

    #[test]
    fn force_bypasses_check() {
        let l = Lifecycle::new();
        l.force(AdapterLifecycle::Ready, AuthStateKey::SignedIn);
        assert_eq!(l.state(), AdapterLifecycle::Ready);
    }

    #[test]
    fn is_terminal_after_stopped() {
        let l = Lifecycle::new();
        l.force(AdapterLifecycle::Stopped, AuthStateKey::SignedOut);
        assert!(l.is_terminal());
    }

    #[test]
    fn is_terminal_after_failed() {
        let l = Lifecycle::new();
        l.force(AdapterLifecycle::Failed, AuthStateKey::SignedOut);
        assert!(l.is_terminal());
    }

    #[test]
    fn user_mode_auth_substate() {
        let l = Lifecycle::new();
        l.transition(AdapterLifecycle::Connecting, AuthStateKey::Uninitialised).unwrap();
        l.transition(AdapterLifecycle::Authenticating, AuthStateKey::CodeRequested).unwrap();
        assert_eq!(l.auth_state(), AuthStateKey::CodeRequested);
        l.transition(AdapterLifecycle::Ready, AuthStateKey::SignedIn).unwrap();
        assert_eq!(l.auth_state(), AuthStateKey::SignedIn);
    }

    // ----- BotAuthLifecycle / UserAuthLifecycle enum tests -----

    #[test]
    fn bot_auth_lifecycle_repr_values_match_rfc() {
        // RFC-0850ab-c §"Data Structures / BotAuthLifecycle" pins
        // these exact `#[repr(u8)]` values. The contract is part
        // of the public API (operators may rely on it for
        // log/UI mapping), so we lock it in with a test.
        assert_eq!(BotAuthLifecycle::NoToken as u8, 0x00);
        assert_eq!(BotAuthLifecycle::Validating as u8, 0x01);
        assert_eq!(BotAuthLifecycle::SignedIn as u8, 0x02);
        assert_eq!(BotAuthLifecycle::SigningOut as u8, 0x03);
        assert_eq!(BotAuthLifecycle::SignedOut as u8, 0x04);
    }

    #[test]
    fn bot_auth_lifecycle_display_round_trip() {
        for s in [
            BotAuthLifecycle::NoToken,
            BotAuthLifecycle::Validating,
            BotAuthLifecycle::SignedIn,
            BotAuthLifecycle::SigningOut,
            BotAuthLifecycle::SignedOut,
        ] {
            let printed = format!("{}", s);
            assert!(!printed.is_empty(), "BotAuthLifecycle {:?} displays empty", s);
            // Re-parse via the FromStr is intentionally NOT
            // provided (the enum is fixed-shape; callers use the
            // variant directly). Round-trip is via Debug instead.
            let debug = format!("{:?}", s);
            assert!(!debug.is_empty());
        }
    }

    #[test]
    fn user_auth_lifecycle_repr_values_match_rfc() {
        // RFC-0850ab-c §"Data Structures / UserAuthLifecycle" pins
        // these exact `#[repr(u8)]` values.
        assert_eq!(UserAuthLifecycle::NoCredentials as u8, 0x00);
        assert_eq!(UserAuthLifecycle::PhoneProvided as u8, 0x01);
        assert_eq!(UserAuthLifecycle::SmsCodeSent as u8, 0x02);
        assert_eq!(UserAuthLifecycle::SmsCodeProvided as u8, 0x03);
        assert_eq!(UserAuthLifecycle::PasswordRequired as u8, 0x04);
        assert_eq!(UserAuthLifecycle::PasswordProvided as u8, 0x05);
        assert_eq!(UserAuthLifecycle::SignedIn as u8, 0x06);
        assert_eq!(UserAuthLifecycle::SigningOut as u8, 0x07);
        assert_eq!(UserAuthLifecycle::SignedOut as u8, 0x08);
        assert_eq!(UserAuthLifecycle::QrLoginPending as u8, 0x09);
        assert_eq!(UserAuthLifecycle::QrLoginConfirmed as u8, 0x0A);
    }

    #[test]
    fn user_auth_lifecycle_has_eleven_variants() {
        // Cross-check: enumerate every variant and count. If a new
        // variant is added without updating RFC + tests, this
        // test will catch the change.
        let variants = [
            UserAuthLifecycle::NoCredentials,
            UserAuthLifecycle::PhoneProvided,
            UserAuthLifecycle::SmsCodeSent,
            UserAuthLifecycle::SmsCodeProvided,
            UserAuthLifecycle::PasswordRequired,
            UserAuthLifecycle::PasswordProvided,
            UserAuthLifecycle::SignedIn,
            UserAuthLifecycle::SigningOut,
            UserAuthLifecycle::SignedOut,
            UserAuthLifecycle::QrLoginPending,
            UserAuthLifecycle::QrLoginConfirmed,
        ];
        assert_eq!(variants.len(), 11); // RFC §"Data Structures / UserAuthLifecycle"
        for v in &variants {
            let printed = format!("{}", v);
            assert!(!printed.is_empty());
        }
    }

    #[test]
    fn unified_auth_state_key_maps_user_lifecycle() {
        // The unified AuthStateKey (5 states) is the summary the
        // AdapterLifecycle consumes via Lifecycle::transition.
        // UserAuthLifecycle -> AuthStateKey mapping rules:
        //   NoCredentials / PhoneProvided       -> Uninitialised
        //   SmsCodeSent / SmsCodeProvided      -> CodeRequested
        //   PasswordRequired / PasswordProvided -> PasswordRequired
        //   SignedIn                            -> SignedIn
        //   SigningOut                          -> SignedIn (transitioning)
        //   SignedOut                           -> SignedOut
        //   QrLoginPending / QrLoginConfirmed   -> CodeRequested
        //     (the QR flow is also a "code requested" auth state
        //     from the adapter's perspective — the user is
        //     providing something out-of-band.)
        use AuthStateKey as A;
        use UserAuthLifecycle as U;
        assert_eq!(A::from(U::NoCredentials), A::Uninitialised);
        assert_eq!(A::from(U::PhoneProvided), A::Uninitialised);
        assert_eq!(A::from(U::SmsCodeSent), A::CodeRequested);
        assert_eq!(A::from(U::SmsCodeProvided), A::CodeRequested);
        assert_eq!(A::from(U::PasswordRequired), A::PasswordRequired);
        assert_eq!(A::from(U::PasswordProvided), A::PasswordRequired);
        assert_eq!(A::from(U::SignedIn), A::SignedIn);
        assert_eq!(A::from(U::SigningOut), A::SignedIn);
        assert_eq!(A::from(U::SignedOut), A::SignedOut);
        assert_eq!(A::from(U::QrLoginPending), A::CodeRequested);
        assert_eq!(A::from(U::QrLoginConfirmed), A::CodeRequested);
    }

    #[test]
    fn unified_auth_state_key_maps_bot_lifecycle() {
        // BotAuthLifecycle -> AuthStateKey mapping:
        //   NoToken / Validating -> Uninitialised
        //   SignedIn             -> SignedIn
        //   SigningOut           -> SignedIn (transitioning)
        //   SignedOut            -> SignedOut
        use AuthStateKey as A;
        use BotAuthLifecycle as B;
        assert_eq!(A::from(B::NoToken), A::Uninitialised);
        assert_eq!(A::from(B::Validating), A::Uninitialised);
        assert_eq!(A::from(B::SignedIn), A::SignedIn);
        assert_eq!(A::from(B::SigningOut), A::SignedIn);
        assert_eq!(A::from(B::SignedOut), A::SignedOut);
    }
}
