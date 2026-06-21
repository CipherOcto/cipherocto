//! ## Status: Phase 2 in progress
//!
//! Bot-mode `sign_in_bot`, `sign_out`, and the user-mode auth
//! flow (`request_login_code` / `submit_code` /
//! `submit_password`) are all wired to the real `grammers`
//! client. The user-mode flow drives the `UserAuthLifecycle`
//! state machine (`NoCredentials → PhoneProvided → SmsCodeSent
//! → SmsCodeProvided → SignedIn`, or via `PasswordRequired →
//! PasswordProvided → SignedIn` if the account has 2FA
//! enabled). Phase 2.5 (QR login) and Phase 2.7 (session
//! persistence integration) are still pending.
//!
//! ## Storage
//!
//! The `StoolapSession` is the canonical session store. The
//! `SenderPool` reads/writes via the `grammers_session::Session`
//! trait which our `StoolapSession` impls. The
//! `RealTelegramMtprotoClient` additionally holds a typed
//! `Arc<StoolapSession>` so `sign_out` can call
//! `StoolapSession::reset()` to wipe the on-disk store.
//!
//! ## User-mode state
//!
//! Across the multi-step user-mode flow, the real client holds:
//! - `user_auth_state: Mutex<UserAuthLifecycle>` — the
//!   state-machine cursor. Every action goes through
//!   `next_user_auth_state` (client-side) and
//!   `next_user_auth_state_server` (server-side) so the
//!   adapter can audit transitions.
//! - `pending_login: Mutex<Option<grammers_client::LoginToken>>` —
//!   returned by `Client::request_login_code` and consumed by
//!   `Client::sign_in`. Lives only between `request_login_code`
//!   and `submit_code`.
//! - `pending_password: Mutex<Option<grammers_client::PasswordToken>>` —
//!   returned by `Client::sign_in` on `SignInError::PasswordRequired`
//!   and consumed by `Client::check_password`. Lives only between
//!   `submit_code` (when it returns `2FA_REQUIRED`) and
//!   `submit_password`.
//!
//! All three are reset on `sign_out` so a fresh sign-in
//! attempt starts from `NoCredentials`.

#![cfg(feature = "real-network")]

use std::sync::Arc;

use async_trait::async_trait;
use grammers_client::client::{LoginToken, PasswordToken};
use grammers_client::sender::SenderPool;
use grammers_client::Client as GrammersClient;
use grammers_client::SignInError;
use grammers_tl_types as tl;
use tokio::task::JoinHandle;
use tracing::{error, warn};

use crate::auth::{
    next_user_auth_state, next_user_auth_state_server, MtprotoAuthError, UserAuthAction,
    UserAuthServerEvent,
};
use crate::client::{
    build_qr_url, MtprotoSentMessage, MtprotoTelegramClient, MtprotoTelegramUpdate, SelfUserInfo,
};
use crate::error::MtprotoTelegramError;
use crate::lifecycle::UserAuthLifecycle;
use crate::self_handle::MtprotoSelfHandle;
use crate::session::StoolapSession;

/// Extract a `SelfUserInfo` from a `tl::enums::auth::Authorization`.
///
/// `LoginTokenSuccess.authorization` is `tl::enums::auth::Authorization`
/// (the enum). Its only payload variant carries the
/// `tl::types::auth::Authorization` struct, which itself
/// holds `user: tl::enums::User` (the user enum: `Empty`
/// or `User`). For the `SignUpRequired` variant we fall
/// back to zeros — same behaviour as the legacy Phase 2.4
/// code.
fn extract_self_user_info(authorization: tl::enums::auth::Authorization) -> SelfUserInfo {
    match authorization {
        tl::enums::auth::Authorization::Authorization(inner) => {
            // `tl::enums::User::id()` collapses both
            // `Empty(UserEmpty)` and `User(User)` to the
            // inner i64.
            let user_id = inner.user.id();
            // Username lives on the inner `User` struct
            // only (the `UserEmpty` variant has no
            // username). Filter out empty strings so the
            // optional is well-defined.
            let username = match &inner.user {
                tl::enums::User::User(u) => u.username.clone(),
                tl::enums::User::Empty(_) => None,
            }
            .filter(|s| !s.is_empty());
            SelfUserInfo {
                user_id,
                username,
                access_hash: 0,
            }
        }
        _ => SelfUserInfo {
            user_id: 0,
            username: None,
            access_hash: 0,
        },
    }
}

/// Wrapper around `grammers_client::Client` that implements
/// `MtprotoTelegramClient`. Constructed via
/// `RealTelegramMtprotoClient::connect`.
pub struct RealTelegramMtprotoClient {
    #[allow(dead_code)]
    client: Arc<GrammersClient>,
    /// Join handle for the SenderPool runner task. Dropped
    /// (and aborted) on `shutdown`.
    #[allow(dead_code)]
    runner: parking_lot::Mutex<Option<JoinHandle<()>>>,
    /// Typed handle to the session so `sign_out` can call
    /// `StoolapSession::reset()`. The same `Arc<StoolapSession>`
    /// is also held by the SenderPool, so a single reset wipes
    /// the on-disk store from both sides.
    session: Arc<StoolapSession>,
    /// Shared self-handle. Populated by the `sign_in_*`
    /// methods after a successful `get_me()`.
    self_handle: MtprotoSelfHandle,
    /// User-mode lifecycle cursor. Always present; starts at
    /// `NoCredentials` after `connect` and is reset to
    /// `NoCredentials` on `sign_out`.
    user_auth_state: parking_lot::Mutex<UserAuthLifecycle>,
    /// `LoginToken` returned by `Client::request_login_code`.
    /// Set by `request_login_code`, consumed by
    /// `submit_code`. `None` outside the request_login_code
    /// → submit_code window.
    pending_login: parking_lot::Mutex<Option<LoginToken>>,
    /// `PasswordToken` returned by `Client::sign_in` on
    /// `SignInError::PasswordRequired`. Set when
    /// `submit_code` returns `2FA_REQUIRED`, consumed by
    /// `submit_password`. `None` outside the
    /// submit_code(2FA) → submit_password window.
    pending_password: parking_lot::Mutex<Option<PasswordToken>>,
    /// Phase 2.5: api_id used for the current QR login
    /// attempt. Set by `qr_login`, used by `poll_qr_login`
    /// and `import_login_token` to re-invoke the same TL
    /// functions.
    qr_api_id: parking_lot::Mutex<Option<i32>>,
    /// Phase 2.5: api_hash used for the current QR login
    /// attempt. Set by `qr_login`, used by `poll_qr_login`.
    qr_api_hash: parking_lot::Mutex<Option<String>>,
    /// Phase 2.5: token bytes returned by the most recent
    /// successful `auth.exportLoginToken` call. Used by
    /// `poll_qr_login` to detect when the token changes
    /// (the user scanned) and by `import_login_token`
    /// to finalize the import.
    qr_token: parking_lot::Mutex<Option<Vec<u8>>>,
}

impl RealTelegramMtprotoClient {
    /// Connect to Telegram and prepare a client. Does NOT
    /// sign in; the caller chooses the auth mode (bot or
    /// user) and calls `sign_in_bot` /
    /// `request_login_code` accordingly.
    ///
    /// `api_id` and `api_hash` are required (from
    /// my.telegram.org). `session` is the persistence
    /// handle; pass a `StoolapSession::open(path)` or
    /// `StoolapSession::open_in_memory()`.
    pub async fn connect(
        api_id: i32,
        _api_hash: &str,
        session: Arc<StoolapSession>,
        self_handle: MtprotoSelfHandle,
    ) -> Result<Arc<Self>, MtprotoTelegramError> {
        // The `SenderPool::new<S: Session + 'static>(session: Arc<S>, api_id: i32)`
        // signature requires a concrete `Arc<S>` (not `Arc<dyn Session>`).
        // `StoolapSession` implements `Session`, so the clone here is
        // straightforward.
        let SenderPool {
            runner,
            handle: _handle,
            ..
        } = SenderPool::new(session.clone(), api_id);
        let client = Arc::new(GrammersClient::new(_handle));
        let runner_task = tokio::spawn(runner.run());
        Ok(Arc::new(Self {
            client,
            runner: parking_lot::Mutex::new(Some(runner_task)),
            session,
            self_handle,
            user_auth_state: parking_lot::Mutex::new(UserAuthLifecycle::NoCredentials),
            pending_login: parking_lot::Mutex::new(None),
            pending_password: parking_lot::Mutex::new(None),
            qr_api_id: parking_lot::Mutex::new(None),
            qr_api_hash: parking_lot::Mutex::new(None),
            qr_token: parking_lot::Mutex::new(None),
        }))
    }

    /// Read-only accessor for the underlying grammers
    /// client. Used by the `MtprotoTelegramAdapter` when it
    /// needs access to RPCs that are not modelled on the
    /// `MtprotoTelegramClient` trait (e.g., `iter_dialogs`
    /// for group discovery).
    #[allow(dead_code)]
    pub fn grammers_client(&self) -> &GrammersClient {
        &self.client
    }

    /// Helper: drive the user-mode state machine through the
    /// two SignOut transitions (`SignedIn → SigningOut →
    /// SignedOut`). Called from `sign_out` and from
    /// any other place that tears down user-mode state.
    /// Errors are deliberately swallowed: sign-out is a
    /// best-effort cleanup and we don't want a state-machine
    /// mismatch to block the session reset.
    fn maybe_transition_user_signout(&self) -> Result<(), MtprotoAuthError> {
        use UserAuthLifecycle::*;
        match *self.user_auth_state.lock() {
            SignedIn => {
                let s = next_user_auth_state(UserAuthAction::SignOut, SignedIn)?;
                *self.user_auth_state.lock() = s;
                let s = next_user_auth_state(UserAuthAction::SignOut, SigningOut)?;
                *self.user_auth_state.lock() = s;
            }
            SigningOut => {
                let s = next_user_auth_state(UserAuthAction::SignOut, SigningOut)?;
                *self.user_auth_state.lock() = s;
            }
            _ => {
                // NoCredentials, PhoneProvided, SmsCodeSent,
                // SmsCodeProvided, PasswordRequired,
                // PasswordProvided, QrLoginPending,
                // QrLoginConfirmed, SignedOut: no transition
                // to perform.
            }
        }
        Ok(())
    }
}

#[async_trait]
impl MtprotoTelegramClient for RealTelegramMtprotoClient {
    async fn send_message(
        &self,
        _chat_id: i64,
        _text: &str,
    ) -> Result<MtprotoSentMessage, MtprotoTelegramError> {
        // Phase 1 stub. Real impl needs to resolve the chat to
        // an `InputPeer` (carrying `access_hash`) before calling
        // `Client::send_message`.
        Err(MtprotoTelegramError::NotReady(
            "RealTelegramMtprotoClient::send_message: peer resolution not yet implemented (Phase 1 stub)".into(),
        ))
    }

    async fn send_document(
        &self,
        _chat_id: i64,
        _caption: &str,
        _filename: &str,
        _data: &[u8],
    ) -> Result<MtprotoSentMessage, MtprotoTelegramError> {
        Err(MtprotoTelegramError::NotReady(
            "RealTelegramMtprotoClient::send_document: peer resolution not yet implemented (Phase 1 stub)".into(),
        ))
    }

    async fn download_file(&self, _file_id: &str) -> Result<Vec<u8>, MtprotoTelegramError> {
        Err(MtprotoTelegramError::NotReady(
            "RealTelegramMtprotoClient::download_file: not yet implemented".into(),
        ))
    }

    async fn receive_updates(&self) -> Result<Vec<MtprotoTelegramUpdate>, MtprotoTelegramError> {
        // Phase 1 stub: real impl drains the SenderPool's
        // update channel and converts via `convert_update`.
        Ok(Vec::new())
    }

    async fn sign_in_bot(
        &self,
        bot_token: &str,
        _api_id: i32,
        api_hash: &str,
    ) -> Result<SelfUserInfo, MtprotoTelegramError> {
        match self.client.bot_sign_in(bot_token, api_hash).await {
            Ok(user) => {
                let info = SelfUserInfo {
                    user_id: user.id().bare_id(),
                    username: user.username().map(String::from),
                    // grammers' `User` does not expose `access_hash`
                    // directly; the cache_peer call inside
                    // `bot_sign_in` stores it for us, so we
                    // don't need it here.
                    access_hash: 0,
                };
                self.self_handle
                    .set_identity(info.user_id, info.username.clone());
                Ok(info)
            }
            Err(e) => {
                error!(error = %e, "bot_sign_in failed");
                Err(MtprotoTelegramError::Auth(format!(
                    "bot_sign_in: {}",
                    crate::error::redact_credentials(&e.to_string())
                )))
            }
        }
    }

    async fn request_login_code(
        &self,
        _api_id: i32,
        api_hash: &str,
        phone: &str,
    ) -> Result<(), MtprotoTelegramError> {
        // 1. Drive the state machine (client-side):
        //    `NoCredentials → PhoneProvided` on `RequestCode`.
        let new_state = {
            let current = *self.user_auth_state.lock();
            next_user_auth_state(
                UserAuthAction::RequestCode {
                    phone: phone.to_string(),
                },
                current,
            )?
        };
        *self.user_auth_state.lock() = new_state;

        // 2. Call grammers' `Client::request_login_code`. On
        //    success, stash the `LoginToken` and advance
        //    `PhoneProvided → SmsCodeSent` (server-side).
        match self.client.request_login_code(phone, api_hash).await {
            Ok(login_token) => {
                let new_state = {
                    let current = *self.user_auth_state.lock();
                    next_user_auth_state_server(UserAuthServerEvent::RequestCodeSucceeded, current)?
                };
                *self.user_auth_state.lock() = new_state;
                *self.pending_login.lock() = Some(login_token);
                Ok(())
            }
            Err(e) => {
                // Server didn't accept the phone. Roll the
                // state machine back to NoCredentials so the
                // operator can retry with a corrected phone.
                *self.user_auth_state.lock() = UserAuthLifecycle::NoCredentials;
                error!(error = %e, "Client::request_login_code failed");
                Err(MtprotoTelegramError::Auth(format!(
                    "request_login_code: {}",
                    crate::error::redact_credentials(&e.to_string())
                )))
            }
        }
    }

    async fn submit_code(&self, code: &str) -> Result<SelfUserInfo, MtprotoTelegramError> {
        // 1. Pull the stashed LoginToken. If missing, the
        //    caller skipped `request_login_code` — that's a
        //    state-machine violation.
        let token = self.pending_login.lock().take().ok_or_else(|| {
            MtprotoTelegramError::Auth(
                "submit_code called without a prior request_login_code".into(),
            )
        })?;

        // 2. Drive the state machine (client-side):
        //    `SmsCodeSent → SmsCodeProvided` on `SubmitCode`.
        let new_state = {
            let current = *self.user_auth_state.lock();
            next_user_auth_state(
                UserAuthAction::SubmitCode {
                    code: code.to_string(),
                },
                current,
            )?
        };
        *self.user_auth_state.lock() = new_state;

        // 3. Call grammers' `Client::sign_in`.
        match self.client.sign_in(&token, code).await {
            Ok(user) => {
                // 4a. Server succeeded. Advance
                //     `SmsCodeProvided → SignedIn` and
                //     populate the self-handle.
                let new_state = {
                    let current = *self.user_auth_state.lock();
                    next_user_auth_state_server(UserAuthServerEvent::SignInSucceeded, current)?
                };
                *self.user_auth_state.lock() = new_state;
                let info = SelfUserInfo {
                    user_id: user.id().bare_id(),
                    username: user.username().map(String::from),
                    access_hash: 0,
                };
                self.self_handle
                    .set_identity(info.user_id, info.username.clone());
                Ok(info)
            }
            Err(SignInError::PasswordRequired(password_token)) => {
                // 4b. Server returned SESSION_PASSWORD_NEEDED.
                //     Stash the password token, advance
                //     `SmsCodeProvided → PasswordRequired`, and
                //     signal the caller via the trait-level
                //     sentinel `MtprotoTelegramError::Auth("2FA_REQUIRED")`.
                let new_state = {
                    let current = *self.user_auth_state.lock();
                    next_user_auth_state_server(UserAuthServerEvent::PasswordRequired, current)?
                };
                *self.user_auth_state.lock() = new_state;
                *self.pending_password.lock() = Some(password_token);
                Err(MtprotoTelegramError::Auth("2FA_REQUIRED".into()))
            }
            Err(SignInError::InvalidCode) => {
                // Roll the state back to SmsCodeSent so the
                // operator can retry with a corrected code.
                // The next `submit_code` call is then valid.
                *self.user_auth_state.lock() = UserAuthLifecycle::SmsCodeSent;
                Err(MtprotoTelegramError::Auth("invalid code".into()))
            }
            Err(SignInError::Other(e)) => {
                // Generic failure — roll back to SmsCodeSent
                // so the operator can retry.
                *self.user_auth_state.lock() = UserAuthLifecycle::SmsCodeSent;
                error!(error = %e, "Client::sign_in failed");
                Err(MtprotoTelegramError::Auth(format!(
                    "sign_in: {}",
                    crate::error::redact_credentials(&e.to_string())
                )))
            }
            Err(SignInError::SignUpRequired) => {
                // grammers does not support third-party sign-up.
                // Reset state to NoCredentials; the user must
                // create their account on an official client
                // first.
                *self.user_auth_state.lock() = UserAuthLifecycle::NoCredentials;
                Err(MtprotoTelegramError::Auth(
                    "sign-up required (use an official Telegram client first)".into(),
                ))
            }
            Err(SignInError::InvalidPassword(_)) => {
                // Not expected from `sign_in` — `sign_in`
                // returns `InvalidPassword` only from
                // `check_password`. Treat as a generic
                // failure.
                *self.user_auth_state.lock() = UserAuthLifecycle::SmsCodeSent;
                Err(MtprotoTelegramError::Auth(
                    "unexpected invalid-password from sign_in".into(),
                ))
            }
        }
    }

    async fn submit_password(&self, password: &str) -> Result<SelfUserInfo, MtprotoTelegramError> {
        // 1. Pull the stashed PasswordToken. If missing, the
        //    caller skipped `submit_code` (or `submit_code`
        //    did not return `2FA_REQUIRED`).
        let password_token = self.pending_password.lock().take().ok_or_else(|| {
            MtprotoTelegramError::Auth(
                "submit_password called without a 2FA_REQUIRED from submit_code".into(),
            )
        })?;

        // 2. Drive the state machine (client-side):
        //    `PasswordRequired → PasswordProvided` on `SubmitPassword`.
        let new_state = {
            let current = *self.user_auth_state.lock();
            next_user_auth_state(
                UserAuthAction::SubmitPassword {
                    password: password.to_string(),
                },
                current,
            )?
        };
        *self.user_auth_state.lock() = new_state;

        // 3. Call grammers' `Client::check_password`.
        match self
            .client
            .check_password(password_token, password.as_bytes())
            .await
        {
            Ok(user) => {
                // 4a. Server accepted the password. Advance
                //     `PasswordProvided → SignedIn` and
                //     populate the self-handle.
                let new_state = {
                    let current = *self.user_auth_state.lock();
                    next_user_auth_state_server(
                        UserAuthServerEvent::CheckPasswordSucceeded,
                        current,
                    )?
                };
                *self.user_auth_state.lock() = new_state;
                let info = SelfUserInfo {
                    user_id: user.id().bare_id(),
                    username: user.username().map(String::from),
                    access_hash: 0,
                };
                self.self_handle
                    .set_identity(info.user_id, info.username.clone());
                Ok(info)
            }
            Err(SignInError::InvalidPassword(_)) => {
                // 4b. Wrong password. Roll back to
                //     `PasswordRequired` so the operator can
                //     retry.
                *self.user_auth_state.lock() = UserAuthLifecycle::PasswordRequired;
                Err(MtprotoTelegramError::Auth("invalid password".into()))
            }
            Err(SignInError::Other(e)) => {
                *self.user_auth_state.lock() = UserAuthLifecycle::PasswordRequired;
                error!(error = %e, "Client::check_password failed");
                Err(MtprotoTelegramError::Auth(format!(
                    "check_password: {}",
                    crate::error::redact_credentials(&e.to_string())
                )))
            }
            // The remaining SignInError variants
            // (SignUpRequired, InvalidCode) are not produced
            // by `check_password`. Treat them as
            // programmer-error / internal failures.
            Err(other) => {
                *self.user_auth_state.lock() = UserAuthLifecycle::PasswordRequired;
                error!(error = %other, "unexpected SignInError from check_password");
                Err(MtprotoTelegramError::Internal(format!(
                    "check_password: unexpected {}",
                    other
                )))
            }
        }
    }

    async fn sign_out(&self) -> Result<(), MtprotoTelegramError> {
        // 1. Drive the state machine: if currently
        //    `SignedIn` or `SigningOut`, advance to `SignedOut`
        //    so a fresh sign-in attempt can start from
        //    `NoCredentials`. Errors here are non-fatal: the
        //    user might be in `NoCredentials` (never signed
        //    in) and the rest of the sign-out still needs to
        //    run.
        let _ = self.maybe_transition_user_signout();

        // 2. Call Telegram's auth.logOut to invalidate the
        //    server-side session.
        if let Err(e) = self.client.sign_out().await {
            warn!(error = %e, "auth.logOut RPC failed; continuing to wipe local state");
        }
        // 3. Wipe the local session store (DD6:
        //    mtproto_dc_option rows including auth_key;
        //    mtproto_peer_info including self_user).
        if let Err(e) = self.session.reset() {
            error!(error = %e, "StoolapSession::reset failed; signing out left on-disk artifacts");
            return Err(MtprotoTelegramError::Session(format!(
                "session reset: {}",
                e
            )));
        }
        // 4. Clear the cached self-handle.
        self.self_handle.clear();
        // 5. Reset the user-mode state machine cursor and
        //    drop any stashed login/password tokens so a
        //    fresh sign-in attempt starts clean.
        *self.user_auth_state.lock() = UserAuthLifecycle::NoCredentials;
        *self.pending_login.lock() = None;
        *self.pending_password.lock() = None;
        *self.qr_api_id.lock() = None;
        *self.qr_api_hash.lock() = None;
        *self.qr_token.lock() = None;
        Ok(())
    }

    // ----- Phase 2.5: QR login -----

    async fn qr_login(&self, api_id: i32, api_hash: &str) -> Result<(), MtprotoTelegramError> {
        // 1. Drive the state machine: NoCredentials →
        //    QrLoginPending (client).
        let new_state = {
            let current = *self.user_auth_state.lock();
            next_user_auth_state(UserAuthAction::QrLoginStart, current)?
        };
        *self.user_auth_state.lock() = new_state;

        // 2. Invoke `auth.exportLoginToken` and parse the
        //    response. The response is one of:
        //    - `LoginToken::Token { token, expires }` — emit
        //      the handle for the caller to display as a QR
        //      code. Stash the token + api_id/api_hash for
        //      the subsequent `poll_qr_login` and
        //      `import_login_token` calls.
        //    - `LoginToken::Success(Authorization)` — we're
        //      already authorized (this is a no-op QR flow).
        //      Return Ok(SelfUserInfo) and drive the state
        //      machine to SignedIn.
        //    - `LoginToken::MigrateTo { dc_id, token }` —
        //      not implemented in Phase 2.5; treat as an
        //      internal error.
        let request = tl::functions::auth::ExportLoginToken {
            api_id,
            api_hash: api_hash.to_string(),
            except_ids: Vec::new(),
        };
        let response: tl::enums::auth::LoginToken =
            self.client.invoke(&request).await.map_err(|e| {
                MtprotoTelegramError::Auth(format!(
                    "auth.exportLoginToken: {}",
                    crate::error::redact_credentials(&e.to_string())
                ))
            })?;
        match response {
            tl::enums::auth::LoginToken::Token(t) => {
                // Stash the api_id / api_hash / token for
                // the subsequent poll and import calls.
                *self.qr_api_id.lock() = Some(api_id);
                *self.qr_api_hash.lock() = Some(api_hash.to_string());
                *self.qr_token.lock() = Some(t.token.clone());
                let url = build_qr_url(&t.token);
                Err(MtprotoTelegramError::QrLoginHandle {
                    token: t.token,
                    url,
                })
            }
            tl::enums::auth::LoginToken::Success(login_token_success) => {
                // Already authorized: drive the state
                // machine QrLoginPending → SignedIn.
                let new_state = {
                    let current = *self.user_auth_state.lock();
                    next_user_auth_state_server(UserAuthServerEvent::SignInSucceeded, current)?
                };
                *self.user_auth_state.lock() = new_state;
                // Pull user_id / username via the inner
                // `Authorization::Authorization` variant,
                // which carries `tl::enums::User` (itself
                // an enum: `Empty(UserEmpty)` or `User(User)`).
                // Note: `qr_login` returns `Result<(), _>`
                // (the user_id/username is exposed via the
                // `self_handle` for the adapter to read);
                // a successful `LoginToken::Success` here
                // is unusual (the session is already
                // authorised) but we still populate the
                // self-handle so the adapter can detect
                // it.
                let info = extract_self_user_info(login_token_success.authorization);
                self.self_handle
                    .set_identity(info.user_id, info.username.clone());
                Ok(())
            }
            tl::enums::auth::LoginToken::MigrateTo(_) => {
                // Not implemented in Phase 2.5. Roll back
                // to NoCredentials.
                *self.user_auth_state.lock() = UserAuthLifecycle::NoCredentials;
                Err(MtprotoTelegramError::Internal(
                    "auth.exportLoginToken returned MigrateTo; not implemented in Phase 2.5".into(),
                ))
            }
        }
    }

    async fn poll_qr_login(&self) -> Result<SelfUserInfo, MtprotoTelegramError> {
        // 1. Re-invoke `auth.exportLoginToken` with the
        //    same api_id / api_hash as the initial
        //    `qr_login` call.
        let (api_id, api_hash) = {
            let id = self.qr_api_id.lock();
            let hash = self.qr_api_hash.lock();
            match (id.as_ref(), hash.as_ref()) {
                (Some(id), Some(hash)) => (*id, hash.clone()),
                _ => {
                    return Err(MtprotoTelegramError::Auth(
                        "poll_qr_login called without a prior qr_login".into(),
                    ));
                }
            }
        };
        let request = tl::functions::auth::ExportLoginToken {
            api_id,
            api_hash: api_hash.clone(),
            except_ids: Vec::new(),
        };
        let response: tl::enums::auth::LoginToken =
            self.client.invoke(&request).await.map_err(|e| {
                MtprotoTelegramError::Auth(format!(
                    "auth.exportLoginToken: {}",
                    crate::error::redact_credentials(&e.to_string())
                ))
            })?;
        match response {
            tl::enums::auth::LoginToken::Token(t) => {
                // No scan yet (token unchanged), or scan
                // happened but import not ready (new
                // token). Either way, return the handle
                // for the caller to re-display.
                *self.qr_token.lock() = Some(t.token.clone());
                let url = build_qr_url(&t.token);
                Err(MtprotoTelegramError::QrLoginHandle {
                    token: t.token,
                    url,
                })
            }
            tl::enums::auth::LoginToken::Success(login_token_success) => {
                // Final import succeeded. Drive the state
                // machine: QrLoginPending → QrLoginConfirmed
                // (client) then → SignedIn (server).
                let new_state = {
                    let current = *self.user_auth_state.lock();
                    next_user_auth_state(UserAuthAction::QrLoginConfirm, current)?
                };
                *self.user_auth_state.lock() = new_state;
                let new_state = {
                    let current = *self.user_auth_state.lock();
                    next_user_auth_state_server(UserAuthServerEvent::SignInSucceeded, current)?
                };
                *self.user_auth_state.lock() = new_state;
                let info = extract_self_user_info(login_token_success.authorization);
                self.self_handle
                    .set_identity(info.user_id, info.username.clone());
                Ok(info)
            }
            tl::enums::auth::LoginToken::MigrateTo(_) => Err(MtprotoTelegramError::Internal(
                "auth.exportLoginToken returned MigrateTo; not implemented in Phase 2.5".into(),
            )),
        }
    }

    async fn import_login_token(&self, token: &[u8]) -> Result<SelfUserInfo, MtprotoTelegramError> {
        // Drive the state machine: QrLoginPending →
        // QrLoginConfirmed (client) via QrLoginConfirm.
        // (After a successful poll, the state is
        // QrLoginPending; this drives the transition to
        // QrLoginConfirmed so the import call can advance
        // to SignedIn.)
        let new_state = {
            let current = *self.user_auth_state.lock();
            next_user_auth_state(UserAuthAction::QrLoginConfirm, current)?
        };
        *self.user_auth_state.lock() = new_state;

        // Invoke `auth.importLoginToken` with the token
        // bytes. The response is `LoginToken::Success`
        // (signed in) or `LoginToken::Token` (a new token
        // to be re-imported — not expected in normal
        // flow) or error variants.
        let request = tl::functions::auth::ImportLoginToken {
            token: token.to_vec(),
        };
        let response: tl::enums::auth::LoginToken =
            self.client.invoke(&request).await.map_err(|e| {
                MtprotoTelegramError::Auth(format!(
                    "auth.importLoginToken: {}",
                    crate::error::redact_credentials(&e.to_string())
                ))
            })?;
        match response {
            tl::enums::auth::LoginToken::Success(login_token_success) => {
                let new_state = {
                    let current = *self.user_auth_state.lock();
                    next_user_auth_state_server(UserAuthServerEvent::SignInSucceeded, current)?
                };
                *self.user_auth_state.lock() = new_state;
                let info = extract_self_user_info(login_token_success.authorization);
                self.self_handle
                    .set_identity(info.user_id, info.username.clone());
                Ok(info)
            }
            tl::enums::auth::LoginToken::Token(_) => {
                // Unexpected: the import returned a new
                // token. Roll back to QrLoginPending and
                // tell the caller to re-poll.
                *self.user_auth_state.lock() = UserAuthLifecycle::QrLoginPending;
                Err(MtprotoTelegramError::Auth(
                    "auth.importLoginToken returned a new token; re-poll required".into(),
                ))
            }
            tl::enums::auth::LoginToken::MigrateTo(_) => {
                *self.user_auth_state.lock() = UserAuthLifecycle::QrLoginPending;
                Err(MtprotoTelegramError::Internal(
                    "auth.importLoginToken returned MigrateTo; not implemented in Phase 2.5".into(),
                ))
            }
        }
    }

    async fn get_file_id_for_message(
        &self,
        _chat_id: i64,
        _message_id: i64,
    ) -> Result<String, MtprotoTelegramError> {
        // Real impl: use grammers' get_messages_by_id to
        // resolve the message, then return its
        // document.file_id. Stubbed in Phase 1.
        Err(MtprotoTelegramError::NotReady(
            "get_file_id_for_message: not yet implemented (Phase 1 stub)".into(),
        ))
    }
}
