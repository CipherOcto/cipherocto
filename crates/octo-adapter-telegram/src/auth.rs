//! Authentication handling for bot and user modes.
//!
//! Mission Architecture: "phone/api_id/api_hash load + 2FA prompt stub"
//!
//! Bot mode: Uses bot_token (no user interaction needed).
//! User mode: Uses phone + api_id + api_hash with interactive 2FA prompt.
//!
//! ## Bot Mode
//! Bot tokens are validated via the Bot API `getMe` endpoint (HTTP).
//!
//! ## User Mode
//! Full Telegram auth flow: set_tdlib_parameters → set_authentication_phone_number
//! → check_authentication_code → (optional 2FA) check_authentication_password.
//!
//! Auth state persists to `data_dir/database` and `data_dir/files` via TDLib.

use std::path::Path;

#[cfg(feature = "real-tdlib")]
use tdlib_rs::enums::AuthorizationState;

#[cfg(feature = "real-tdlib")]
use tdlib_rs::functions;

/// Authentication mode: bot (token) or user (phone + api credentials).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AuthMode {
    /// Bot authentication via bot_token.
    Bot { token: String },
    /// User authentication via phone + api_id + api_hash.
    User {
        phone: String,
        api_id: i32,
        api_hash: String,
        password: Option<String>,
    },
}

/// Bot token validation result.
#[derive(Debug, Clone)]
pub struct BotIdentity {
    pub user_id: i64,
    pub username: String,
    pub first_name: String,
    pub last_name: Option<String>,
}

/// Auth error types.
#[derive(Debug, Clone, thiserror::Error, PartialEq, Eq)]
pub enum AuthError {
    #[error("invalid bot token: {0}")]
    InvalidBotToken(String),

    #[error("authentication failed: {0}")]
    AuthenticationFailed(String),

    #[error("2FA password required")]
    TwoFactorRequired,

    #[error("phone number not registered")]
    PhoneNotRegistered,

    #[error("invalid verification code")]
    InvalidCode,

    #[error("session expired, re-authenticate")]
    SessionExpired,

    #[cfg(feature = "real-tdlib")]
    #[error("TDLib error: {message}")]
    Tdlib { message: String },
}

/// Result of authentication attempt.
pub type AuthResult<T> = std::result::Result<T, AuthError>;

/// Discriminator for the high-level auth state decisions a `UserAuth` can
/// reach. The receive loop needs to map TDLib's full `AuthorizationState`
/// (which carries varying inner payloads per variant) onto a small set of
/// actions, and a few of those actions — most importantly `AwaitCode` — are
/// not currently reachable because `handle_authorization_state` short-circuits
/// to `Err(AuthenticationFailed(...))` on `WaitCode`. Routing the decision
/// through this enum lets the receive loop do its own I/O (drain the
/// verification-code channel, call `check_authentication_code`) and keeps the
/// pure decision logic in this module testable without a real TDLib client.
///
/// `AuthStateKey` is the testable surface (no TDLib type to construct), and
/// `decide_key` maps a key to an `AuthAction`. The receive loop uses
/// `decide(&AuthorizationState)` (the TDLib-aware wrapper) at runtime.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AuthStateKey {
    WaitTdlibParameters,
    WaitPhoneNumber,
    WaitCode,
    WaitPassword,
    Ready,
    Closed,
    /// Catch-all for `WaitEncryptionKey`, `WaitRegistration`, etc. — states
    /// that need no action from us.
    Other,
}

/// Action the receive loop should take for a given auth state.
/// The receive loop uses this to drive the side-effecting calls
/// (`set_tdlib_parameters`, `check_authentication_code`, etc.).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AuthAction {
    /// Caller should call `set_tdlib_parameters` with the configured dirs.
    SetParameters,
    /// Caller should call `set_authentication_phone_number`.
    SendPhone,
    /// Caller should drain `code_rx` and call `check_authentication_code`
    /// with the most recent code (if any). If `code_rx` is empty, return
    /// `Ok(())` and wait for the next TDLib update tick.
    AwaitCode,
    /// Caller should call `check_authentication_password` with this password.
    UsePassword(String),
    /// Auth completed successfully.
    Ready,
    /// TDLib session closed (e.g. user logged out elsewhere).
    SessionExpired,
    /// No action required (e.g. `WaitEncryptionKey`).
    Ignore,
    /// A non-recoverable error — the receive loop should surface it to
    /// the constructor. `WaitPassword` with no password configured maps
    /// to `Error(TwoFactorRequired)` so the gateway can prompt the user.
    Error(AuthError),
}

// =============================================================================
// Bot Mode Authentication
// =============================================================================

/// Validate a bot token and return bot identity.
/// Bot mode uses the Bot API's getMe endpoint for validation.
#[cfg(feature = "real-tdlib")]
pub async fn validate_bot_token(token: &str) -> AuthResult<BotIdentity> {
    // Bot API getMe endpoint: https://api.telegram.org/bot<token>/getMe
    let url = format!("https://api.telegram.org/bot{}/getMe", token);

    let response = reqwest::get(url)
        .await
        .map_err(|e| AuthError::InvalidBotToken(format!("network error: {}", e)))?;

    #[derive(serde::Deserialize)]
    struct GetMeResponse {
        ok: bool,
        result: Option<BotInfo>,
        description: Option<String>,
    }

    #[derive(serde::Deserialize)]
    struct BotInfo {
        id: i64,
        username: String,
        first_name: String,
        last_name: Option<String>,
    }

    let response: GetMeResponse = response
        .json()
        .await
        .map_err(|e| AuthError::InvalidBotToken(format!("parse error: {}", e)))?;

    if !response.ok {
        return Err(AuthError::InvalidBotToken(
            response
                .description
                .unwrap_or_else(|| "unknown error".into()),
        ));
    }

    let result = response
        .result
        .ok_or_else(|| AuthError::InvalidBotToken("empty response".into()))?;

    Ok(BotIdentity {
        user_id: result.id,
        username: result.username,
        first_name: result.first_name,
        last_name: result.last_name,
    })
}

// =============================================================================
// User Mode Authentication (TDLib)
// =============================================================================

/// User mode authentication state machine.
/// Follows the TDLib authorization flow:
/// 1. WaitTdlibParameters → set_tdlib_parameters
/// 2. WaitPhoneNumber → set_authentication_phone_number
/// 3. WaitCode → check_authentication_code
/// 4. WaitPassword (if 2FA enabled) → check_authentication_password
/// 5. Ready
#[derive(Clone)]
pub struct UserAuth {
    pub phone: String,
    pub api_id: i32,
    pub api_hash: String,
    pub password: Option<String>,
}

impl UserAuth {
    /// Create a new user auth context.
    pub fn new(phone: String, api_id: i32, api_hash: String, password: Option<String>) -> Self {
        Self {
            phone,
            api_id,
            api_hash,
            password,
        }
    }

    /// Map an [`AuthStateKey`] to the [`AuthAction`] the receive loop should
    /// take. This is the pure decision function — it does no I/O and does not
    /// touch TDLib — so it can be unit-tested without a real client.
    ///
    /// The receive loop uses this through `decide(&AuthorizationState)` (the
    /// feature-gated TDLib wrapper) at runtime.
    pub fn decide_key(&self, key: AuthStateKey) -> AuthAction {
        match key {
            AuthStateKey::WaitTdlibParameters => AuthAction::SetParameters,
            AuthStateKey::WaitPhoneNumber => AuthAction::SendPhone,
            AuthStateKey::WaitCode => AuthAction::AwaitCode,
            AuthStateKey::WaitPassword => match &self.password {
                Some(p) => AuthAction::UsePassword(p.clone()),
                None => AuthAction::Error(AuthError::TwoFactorRequired),
            },
            AuthStateKey::Ready => AuthAction::Ready,
            AuthStateKey::Closed => AuthAction::SessionExpired,
            AuthStateKey::Other => AuthAction::Ignore,
        }
    }
}

#[cfg(feature = "real-tdlib")]
impl UserAuth {
    /// Map a TDLib [`AuthorizationState`] to the high-level [`AuthAction`]
    /// the receive loop should take. This is the TDLib-aware wrapper around
    /// `decide_key` — it strips the per-state payloads (`AuthenticationCodeInfo`,
    /// `AuthenticationPasswordInfo`, etc.) down to the [`AuthStateKey`] the
    /// pure decision function expects.
    pub fn decide(&self, state: &AuthorizationState) -> AuthAction {
        let key = match state {
            AuthorizationState::WaitTdlibParameters => AuthStateKey::WaitTdlibParameters,
            AuthorizationState::WaitPhoneNumber => AuthStateKey::WaitPhoneNumber,
            AuthorizationState::WaitCode(_) => AuthStateKey::WaitCode,
            AuthorizationState::WaitPassword(_) => AuthStateKey::WaitPassword,
            AuthorizationState::Ready => AuthStateKey::Ready,
            AuthorizationState::Closed => AuthStateKey::Closed,
            _ => AuthStateKey::Other,
        };
        self.decide_key(key)
    }

    /// Handle TDLib authorization state and return next action.
    /// Returns the appropriate TDLib function call based on current state.
    ///
    /// `data_dir` is the TDLib base data directory. TDLib's `database_directory`
    /// and `files_directory` will be placed in `<data_dir>/database` and
    /// `<data_dir>/files` respectively, so the two are not contended on the
    /// same path (mission AC line 141).
    pub async fn handle_authorization_state(
        &self,
        state: AuthorizationState,
        client_id: i32,
        data_dir: Option<&Path>,
    ) -> AuthResult<()> {
        match state {
            AuthorizationState::WaitTdlibParameters => {
                let base = data_dir.unwrap_or_else(|| Path::new("octo_telegram_user"));
                let db_dir = base.join("database");
                let files_dir = base.join("files");
                // Ensure directories exist before TDLib is told about them.
                let _ = std::fs::create_dir_all(&db_dir);
                let _ = std::fs::create_dir_all(&files_dir);
                let response = functions::set_tdlib_parameters(
                    false, // use_test_dc
                    db_dir.to_string_lossy().into_owned(),
                    files_dir.to_string_lossy().into_owned(),
                    String::new(), // database_encryption_key
                    true,          // use_file_database
                    true,          // use_chat_info_database
                    true,          // use_message_database
                    false,         // use_secret_chats
                    self.api_id,
                    self.api_hash.clone(),
                    "en".into(),                      // language
                    "CipherOcto".into(),              // device_model
                    String::new(),                    // system_version
                    env!("CARGO_PKG_VERSION").into(), // app_version
                    client_id,
                )
                .await;

                if let Err(e) = response {
                    return Err(AuthError::Tdlib { message: e.message });
                }
                Ok(())
            }
            AuthorizationState::WaitPhoneNumber => {
                // Send phone number for authentication
                let response = functions::set_authentication_phone_number(
                    self.phone.clone(),
                    None, // settings (default)
                    client_id,
                )
                .await;

                if let Err(e) = response {
                    // 401 = phone not registered on this DC
                    if e.code == 401 {
                        return Err(AuthError::PhoneNotRegistered);
                    }
                    return Err(AuthError::AuthenticationFailed(e.message));
                }
                Ok(())
            }
            AuthorizationState::WaitCode(_) => {
                // The receive loop forwards the interactive code request via
                // RealTelegramClient::submit_verification_code; if no code is
                // submitted the loop returns this error which the constructor
                // surfaces verbatim.
                Err(AuthError::AuthenticationFailed(
                    "verification code required - call submit_verification_code()".into(),
                ))
            }
            AuthorizationState::WaitPassword(_) => {
                if let Some(ref password) = self.password {
                    let response =
                        functions::check_authentication_password(password.clone(), client_id).await;

                    if let Err(e) = response {
                        return Err(AuthError::AuthenticationFailed(e.message));
                    }
                    Ok(())
                } else {
                    Err(AuthError::TwoFactorRequired)
                }
            }
            AuthorizationState::Ready => {
                // Authentication successful
                Ok(())
            }
            AuthorizationState::Closed => Err(AuthError::SessionExpired),
            _ => {
                // Other states (WaitEncryption, WaitRegistration, etc.) - skip
                Ok(())
            }
        }
    }
}

// =============================================================================
// Auth Persistence
// =============================================================================

/// Auth key persistence directory.
/// TDLib stores its auth_key in `data_dir/tdlib/<identifier>/database`.
/// We don't add our own `octo_auth_meta` table — TDLib manages its own SQLite
/// database for the auth_key, and the `data_dir/database` directory created
/// above is the TDLib-managed location.
pub fn auth_data_dir(base_dir: &std::path::Path, identifier: &str) -> std::path::PathBuf {
    base_dir.join("tdlib").join(identifier)
}

/// Best-effort creation of the TDLib auth directories.
/// `data_dir` is the user-supplied base directory; TDLib's `database_directory`
/// and `files_directory` will be created as `<data_dir>/database` and
/// `<data_dir>/files` respectively. This is called by `RealTelegramClient`
/// before `set_tdlib_parameters` is invoked, so TDLib's directory check passes.
pub fn create_auth_dirs(data_dir: &Path) -> std::io::Result<()> {
    std::fs::create_dir_all(data_dir)?;
    std::fs::create_dir_all(data_dir.join("database"))?;
    std::fs::create_dir_all(data_dir.join("files"))?;
    Ok(())
}
