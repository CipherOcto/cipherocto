//! Auth driver for the onboard tool.
//!
//! Bot mode: handles TDLib auth state machine directly (no `UserAuth`).
//! User mode: uses adapter's `UserAuth::decide_key` for state decisions.

use crate::error::{OnboardError, Result};
use crate::session::TelegramSession;
use octo_adapter_telegram::auth::{AuthAction, AuthError, AuthStateKey, UserAuth};
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tokio::sync::{mpsc, Notify};
use zeroize::Zeroizing;

/// Process-global lock ensuring only one TDLib receive loop is active at a time.
/// `tdlib_rs::receive()` is process-global; concurrent consumers would steal
/// each other's updates. This flag is checked-and-set by `spawn_receive_loop`
/// and `tdlib_get_me_with_timeout`, and cleared when the loop exits.
static RECEIVE_IN_USE: AtomicBool = AtomicBool::new(false);

/// Guard that clears `RECEIVE_IN_USE` on drop.
pub struct ReceiveLockGuard;

impl Drop for ReceiveLockGuard {
    fn drop(&mut self) {
        RECEIVE_IN_USE.store(false, Ordering::SeqCst);
    }
}

/// Try to acquire the process-global receive lock. Returns `Some(ReceiveLockGuard)`
/// on success, or `Err(OnboardError::Cancelled)` if another consumer is active.
pub fn try_acquire_receive_lock() -> std::result::Result<ReceiveLockGuard, OnboardError> {
    if RECEIVE_IN_USE
        .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
        .is_err()
    {
        return Err(OnboardError::Cancelled(
            "another TDLib operation is already in progress; \
             wait for it to finish or check for a stale process holding the receive lock"
                .into(),
        ));
    }
    Ok(ReceiveLockGuard)
}

/// Send `AuthorizationState::Close` to TDLib and wait for `Closed`.
/// Best-effort: if the close or wait fails, logs and returns (the client
/// handle will leak until process exit, which is the same as not calling this).
pub async fn close_tdlib_client(client_id: i32) {
    close_tdlib_client_with_timeout(client_id, std::time::Duration::from_secs(10)).await;
}

/// Send `AuthorizationState::Close` to TDLib and wait for `Closed` with a
/// configurable timeout.
pub async fn close_tdlib_client_with_timeout(client_id: i32, timeout: std::time::Duration) {
    // Attempt close; on error, still try to drain the receive queue for timeout
    if let Err(e) = tdlib_rs::functions::close(client_id).await {
        tracing::debug!(
            "tdlib close() failed: {} (see TDLib logs for details)",
            e.message.len()
        );
    }
    let start = std::time::Instant::now();
    while start.elapsed() < timeout {
        if let Some((tdlib_rs::enums::Update::AuthorizationState(ref update), cid)) =
            tdlib_rs::receive()
        {
            if cid == client_id
                && matches!(
                    update.authorization_state,
                    tdlib_rs::enums::AuthorizationState::Closed
                )
            {
                return;
            }
        }
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    }
}

/// Credentials provided by the operator (CLI args or env vars).
pub struct Credentials {
    pub phone: Option<String>,
    pub api_id: i32,
    pub api_hash: Zeroizing<String>,
    pub bot_token: Option<Zeroizing<String>>,
    pub verifying_key: Option<String>,
}

impl Credentials {
    /// Create credentials with validation. Returns Err if api_id is invalid,
    /// api_hash is empty, or bot_token is empty (when provided).
    pub fn try_new(
        phone: Option<String>,
        api_id: i32,
        api_hash: Zeroizing<String>,
        bot_token: Option<Zeroizing<String>>,
        verifying_key: Option<String>,
    ) -> Result<Self> {
        if api_id <= 0 {
            return Err(OnboardError::BadConfig(format!(
                "api_id must be positive and fit in i32, got {}",
                api_id
            )));
        }
        if api_hash.is_empty() {
            return Err(OnboardError::BadConfig("api_hash must not be empty".into()));
        }
        if let Some(ref token) = bot_token {
            if token.is_empty() {
                return Err(OnboardError::BadConfig(
                    "bot_token must not be empty".into(),
                ));
            }
        }
        Ok(Self {
            phone,
            api_id,
            api_hash,
            bot_token,
            verifying_key,
        })
    }
}

/// Classify a TDLib error message into the appropriate `OnboardError` variant.
/// Sanitizes the message to strip file paths and other PII before embedding.
pub fn classify_tdlib_error(msg: String) -> OnboardError {
    let sanitized = sanitize_tdlib_message(&msg);
    let lower = msg.to_lowercase();
    if lower.contains("flood_wait_")
        || lower.contains("slowmode_wait_")
        || lower.contains("too many")
    {
        OnboardError::RateLimited(sanitized)
    } else if lower.contains("network")
        || lower.contains("connect")
        || lower.contains("dns")
        || lower.contains("timeout")
    {
        OnboardError::TelegramUnreachable(sanitized)
    } else {
        OnboardError::AuthRejected(sanitized)
    }
}

/// Sanitize a TDLib error message by stripping file paths and other PII.
fn sanitize_tdlib_message(msg: &str) -> String {
    // Replace Unix-style file paths with <path>
    let mut result = msg.to_string();
    // Match common path patterns: /home/..., /tmp/..., /var/..., C:\, etc.
    let path_patterns = [
        "/home/", "/Users/", "/root/", "/tmp/", "/var/", "/usr/", "/opt/", "/etc/", "/srv/",
        "/mnt/", "/run/", "/data/", "C:\\", "D:\\",
    ];
    for pattern in &path_patterns {
        while let Some(start) = result.find(pattern) {
            // Find the end of the path: first character that's NOT a valid path char.
            // Valid path chars: / . _ - a-z A-Z 0-9 (covers Unix and Windows paths).
            let end = result[start..]
                .find(|c: char| {
                    !matches!(
                        c,
                        '/' | '.' | '_' | '-' | 'a'..='z' | 'A'..='Z' | '0'..='9'
                    )
                })
                .map(|e| start + e)
                .unwrap_or(result.len());
            result = format!("{}<path>{}", &result[..start], &result[end..]);
        }
    }
    result
}

/// Validate and cast `api_id` from i64 (JSON) to i32 (TDLib).
pub fn validate_api_id(raw: i64) -> Result<i32> {
    if raw > 0 && raw <= i32::MAX as i64 {
        Ok(raw as i32)
    } else {
        Err(OnboardError::BadConfig(format!(
            "api_id must be positive and fit in i32, got {}",
            raw
        )))
    }
}

/// Create the TDLib auth directories with mode 0700 on Unix.
fn create_auth_dirs(data_dir: &Path) -> Result<()> {
    std::fs::create_dir_all(data_dir)
        .map_err(|e| OnboardError::BadConfig(format!("create data_dir: {}", e)))?;

    // H2: Set mode 0700 immediately after creating data_dir to minimize
    // the TOCTOU window where the directory is world-readable.
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(data_dir, std::fs::Permissions::from_mode(0o700)).map_err(
            |e| OnboardError::BadConfig(format!("set_permissions 0700 on data_dir: {}", e)),
        )?;
    }

    std::fs::create_dir_all(data_dir.join("database"))
        .map_err(|e| OnboardError::BadConfig(format!("create database dir: {}", e)))?;
    std::fs::create_dir_all(data_dir.join("files"))
        .map_err(|e| OnboardError::BadConfig(format!("create files dir: {}", e)))?;
    Ok(())
}

/// Spawn the TDLib receive loop on a blocking thread.
/// Returns a channel receiver for updates, a shutdown flag, and a receive lock guard.
/// NOTE: `tdlib_rs::receive()` blocks for up to ~2s (tdjson timeout). After
/// `shutdown` is set, the thread will exit when the current `receive()` call
/// returns. There is no way to interrupt a synchronous `receive()` call — the
/// thread naturally drains within one timeout cycle.
fn spawn_receive_loop() -> std::result::Result<
    (
        mpsc::Receiver<tdlib_rs::enums::Update>,
        Arc<AtomicBool>,
        ReceiveLockGuard,
    ),
    OnboardError,
> {
    let _guard = try_acquire_receive_lock()?;
    let (tx, rx) = mpsc::channel(256);
    let shutdown = Arc::new(AtomicBool::new(false));
    let shutdown_clone = shutdown.clone();

    match std::thread::Builder::new()
        .name("tdlib-receive".into())
        .spawn(move || {
            loop {
                if shutdown_clone.load(Ordering::Relaxed) {
                    break;
                }
                if let Some((update, _cid)) = tdlib_rs::receive() {
                    if matches!(update, tdlib_rs::enums::Update::AuthorizationState(_))
                        && tx.blocking_send(update).is_err()
                    {
                        break; // receiver dropped
                    }
                }
            }
        }) {
        Ok(_) => {}
        Err(e) => {
            return Err(OnboardError::Generic(anyhow::anyhow!(
                "failed to spawn receive thread: {}",
                e
            )));
        }
    }

    Ok((rx, shutdown, _guard))
}

/// Read 2FA password from stdin with echo disabled (rpassword).
fn read_password_stdin(prompt: &str) -> Result<Zeroizing<String>> {
    let pwd = rpassword::prompt_password(prompt)
        .map_err(|e| OnboardError::Cancelled(format!("read password: {}", e)))?;
    Ok(Zeroizing::new(pwd))
}

/// Set TDLib parameters (shared between bot and user modes).
async fn set_tdlib_parameters(client_id: i32, creds: &Credentials, data_dir: &Path) -> Result<()> {
    set_tdlib_parameters_raw(client_id, creds.api_id, &creds.api_hash, data_dir).await
}

/// Set TDLib parameters with raw values (used by both auth drivers and get_me fallback).
pub async fn set_tdlib_parameters_raw(
    client_id: i32,
    api_id: i32,
    api_hash: &str,
    data_dir: &Path,
) -> Result<()> {
    let db_dir = data_dir.join("database");
    let files_dir = data_dir.join("files");

    let response = tdlib_rs::functions::set_tdlib_parameters(
        false,
        db_dir.to_string_lossy().into_owned(),
        files_dir.to_string_lossy().into_owned(),
        String::new(),
        true,
        true,
        true,
        false,
        api_id,
        api_hash.to_string(),
        "en".into(),
        "CipherOcto-TelegramOnboard".into(),
        String::new(),
        env!("CARGO_PKG_VERSION").into(),
        client_id,
    )
    .await;

    if let Err(e) = response {
        return Err(classify_tdlib_error(e.message));
    }
    Ok(())
}

/// Drive TDLib auth to completion for bot mode.
/// The receive lock guard is held internally through `close_tdlib_client` on
/// error paths. On success, the receive thread has already exited (shutdown
/// was set), so the guard is safely dropped.
pub async fn drive_bot_auth(
    client_id: i32,
    creds: &Credentials,
    data_dir: &Path,
    timeout: std::time::Duration,
) -> Result<TelegramSession> {
    let bot_token = creds
        .bot_token
        .as_deref()
        .ok_or_else(|| OnboardError::BadConfig("bot mode requires bot_token".into()))?;

    if let Err(e) = create_auth_dirs(data_dir) {
        close_tdlib_client(client_id).await;
        return Err(e);
    }
    if let Err(e) = set_tdlib_parameters(client_id, creds, data_dir).await {
        close_tdlib_client(client_id).await;
        return Err(e);
    }

    let (mut rx, shutdown, _receive_guard) = match spawn_receive_loop() {
        Ok(t) => t,
        Err(e) => {
            close_tdlib_client(client_id).await;
            return Err(e);
        }
    };
    let notify = Arc::new(Notify::new());
    let result: Arc<parking_lot::Mutex<Option<std::result::Result<(), String>>>> =
        Arc::new(parking_lot::Mutex::new(None));

    let notify_clone = notify.clone();
    let result_clone = result.clone();
    let token = bot_token.to_string();

    let handle = tokio::task::spawn(async move {
        while let Some(update) = rx.recv().await {
            if let tdlib_rs::enums::Update::AuthorizationState(auth_update) = update {
                match auth_update.authorization_state {
                    tdlib_rs::enums::AuthorizationState::WaitPhoneNumber => {
                        let resp = tdlib_rs::functions::check_authentication_bot_token(
                            token.clone(),
                            client_id,
                        )
                        .await;
                        if let Err(e) = resp {
                            *result_clone.lock() = Some(Err(e.message));
                            notify_clone.notify_one();
                            break;
                        }
                    }
                    tdlib_rs::enums::AuthorizationState::Ready => {
                        *result_clone.lock() = Some(Ok(()));
                        notify_clone.notify_one();
                        break;
                    }
                    tdlib_rs::enums::AuthorizationState::Closed => {
                        *result_clone.lock() = Some(Err("TDLib session closed".into()));
                        notify_clone.notify_one();
                        break;
                    }
                    _ => {}
                }
            }
        }
    });

    let wait_result = tokio::time::timeout(timeout, notify.notified()).await;

    // M1: Signal shutdown before aborting
    shutdown.store(true, Ordering::Relaxed);
    handle.abort();
    // Wait for the auth task to actually exit before proceeding.
    let _ = tokio::time::timeout(std::time::Duration::from_secs(2), handle).await;

    // Keep _receive_guard alive through close_tdlib_client calls on error paths.
    // On success, the receive thread has already exited (shutdown was set).
    let auth_result = result.lock().clone();
    let auth_err = match auth_result {
        Some(Ok(())) => None,
        Some(Err(msg)) => Some(classify_tdlib_error(msg)),
        None if wait_result.is_err() => Some(OnboardError::Cancelled(
            "auth timed out waiting for Ready".into(),
        )),
        None => Some(OnboardError::Cancelled("auth did not complete".into())),
    };

    if let Some(e) = auth_err {
        close_tdlib_client(client_id).await;
        return Err(e);
    }

    let session = match extract_identity(client_id, creds, data_dir).await {
        Ok(s) => s,
        Err(e) => {
            close_tdlib_client(client_id).await;
            return Err(e);
        }
    };
    close_tdlib_client(client_id).await;
    Ok(session)
}

/// Drive TDLib auth to completion for user mode.
pub async fn drive_user_auth(
    client_id: i32,
    creds: &Credentials,
    data_dir: &Path,
    timeout: std::time::Duration,
) -> Result<TelegramSession> {
    let phone = creds
        .phone
        .as_deref()
        .ok_or_else(|| OnboardError::BadConfig("user mode requires phone".into()))?;

    if let Err(e) = create_auth_dirs(data_dir) {
        close_tdlib_client(client_id).await;
        return Err(e);
    }
    if let Err(e) = set_tdlib_parameters(client_id, creds, data_dir).await {
        close_tdlib_client(client_id).await;
        return Err(e);
    }

    let user_auth = UserAuth::new(
        phone.to_string(),
        creds.api_id,
        creds.api_hash.to_string(),
        None, // 2FA password always read from stdin via read_password_stdin
    );

    let (mut rx, shutdown, _receive_guard) = match spawn_receive_loop() {
        Ok(t) => t,
        Err(e) => {
            close_tdlib_client(client_id).await;
            return Err(e);
        }
    };
    let notify = Arc::new(Notify::new());
    let result: Arc<parking_lot::Mutex<Option<std::result::Result<(), String>>>> =
        Arc::new(parking_lot::Mutex::new(None));

    let notify_clone = notify.clone();
    let result_clone = result.clone();
    let phone_owned = phone.to_string();

    let handle = tokio::task::spawn(async move {
        while let Some(update) = rx.recv().await {
            if let tdlib_rs::enums::Update::AuthorizationState(auth_update) = update {
                let key = match &auth_update.authorization_state {
                    tdlib_rs::enums::AuthorizationState::WaitTdlibParameters => {
                        AuthStateKey::WaitTdlibParameters
                    }
                    tdlib_rs::enums::AuthorizationState::WaitPhoneNumber => {
                        AuthStateKey::WaitPhoneNumber
                    }
                    tdlib_rs::enums::AuthorizationState::WaitCode(_) => AuthStateKey::WaitCode,
                    tdlib_rs::enums::AuthorizationState::WaitPassword(_) => {
                        AuthStateKey::WaitPassword
                    }
                    tdlib_rs::enums::AuthorizationState::Ready => AuthStateKey::Ready,
                    tdlib_rs::enums::AuthorizationState::Closed => AuthStateKey::Closed,
                    _ => AuthStateKey::Other,
                };

                let action = user_auth.decide_key(key);

                match action {
                    AuthAction::SetParameters => {}
                    AuthAction::SendPhone => {
                        let resp = tdlib_rs::functions::set_authentication_phone_number(
                            phone_owned.clone(),
                            None,
                            client_id,
                        )
                        .await;
                        if let Err(e) = resp {
                            *result_clone.lock() = Some(Err(e.message));
                            notify_clone.notify_one();
                            break;
                        }
                    }
                    AuthAction::AwaitCode => {
                        let code = tokio::task::spawn_blocking(|| {
                            read_line_from_stdin("Enter verification code: ")
                        })
                        .await
                        .unwrap_or_else(|e| Err(std::io::Error::other(e)));
                        match code {
                            Ok(c) => {
                                let resp = tdlib_rs::functions::check_authentication_code(
                                    c.trim().to_string(),
                                    client_id,
                                )
                                .await;
                                if let Err(e) = resp {
                                    *result_clone.lock() = Some(Err(e.message));
                                    notify_clone.notify_one();
                                    break;
                                }
                            }
                            Err(e) => {
                                *result_clone.lock() = Some(Err(format!("stdin read: {}", e)));
                                notify_clone.notify_one();
                                break;
                            }
                        }
                    }
                    AuthAction::UsePassword(pwd) => {
                        let resp =
                            tdlib_rs::functions::check_authentication_password(pwd, client_id)
                                .await;
                        if let Err(e) = resp {
                            *result_clone.lock() = Some(Err(e.message));
                            notify_clone.notify_one();
                            break;
                        }
                    }
                    AuthAction::Ready => {
                        *result_clone.lock() = Some(Ok(()));
                        notify_clone.notify_one();
                        break;
                    }
                    AuthAction::SessionExpired => {
                        *result_clone.lock() = Some(Err("session expired".into()));
                        notify_clone.notify_one();
                        break;
                    }
                    AuthAction::Error(AuthError::RegistrationRequired) => {
                        *result_clone.lock() =
                            Some(Err("This phone number is not registered with Telegram. \
                             Please register via the Telegram app first."
                                .into()));
                        notify_clone.notify_one();
                        break;
                    }
                    AuthAction::Error(AuthError::TwoFactorRequired) => {
                        // H1: Read 2FA password from stdin with echo disabled
                        // H2: Wrapped in spawn_blocking so abort can interrupt the await
                        let pwd_result = tokio::task::spawn_blocking(|| {
                            read_password_stdin("Enter 2FA password: ")
                        })
                        .await
                        .unwrap_or_else(|e| {
                            Err(OnboardError::Cancelled(format!("spawn_blocking: {}", e)))
                        });
                        match pwd_result {
                            Ok(pwd) => {
                                let resp = tdlib_rs::functions::check_authentication_password(
                                    pwd.to_string(),
                                    client_id,
                                )
                                .await;
                                if let Err(e) = resp {
                                    *result_clone.lock() = Some(Err(e.message));
                                    notify_clone.notify_one();
                                    break;
                                }
                            }
                            Err(e) => {
                                *result_clone.lock() = Some(Err(format!("read password: {}", e)));
                                notify_clone.notify_one();
                                break;
                            }
                        }
                    }
                    AuthAction::Error(e) => {
                        *result_clone.lock() = Some(Err(e.to_string()));
                        notify_clone.notify_one();
                        break;
                    }
                    AuthAction::Ignore => {}
                }
            }
        }
    });

    let wait_result = tokio::time::timeout(timeout, notify.notified()).await;

    // M1: Signal shutdown before aborting
    shutdown.store(true, Ordering::Relaxed);
    handle.abort();
    // Wait for the auth task to actually exit before proceeding.
    let _ = tokio::time::timeout(std::time::Duration::from_secs(2), handle).await;

    let auth_result = result.lock().clone();
    let auth_err = match auth_result {
        Some(Ok(())) => None,
        Some(Err(msg)) => Some(classify_tdlib_error(msg)),
        None if wait_result.is_err() => Some(OnboardError::Cancelled(
            "auth timed out waiting for Ready".into(),
        )),
        None => Some(OnboardError::Cancelled("auth did not complete".into())),
    };

    if let Some(e) = auth_err {
        close_tdlib_client(client_id).await;
        return Err(e);
    }

    let session = match extract_identity(client_id, creds, data_dir).await {
        Ok(s) => s,
        Err(e) => {
            close_tdlib_client(client_id).await;
            return Err(e);
        }
    };
    close_tdlib_client(client_id).await;
    Ok(session)
}

/// Extract identity via `get_me` after auth completes.
async fn extract_identity(
    client_id: i32,
    creds: &Credentials,
    data_dir: &Path,
) -> Result<TelegramSession> {
    let me_enum = tdlib_rs::functions::get_me(client_id)
        .await
        .map_err(|e| classify_tdlib_error(e.message))?;

    // M3: Match instead of unwrap to handle future TDLib variants gracefully
    #[allow(unreachable_patterns)]
    let me = match me_enum {
        tdlib_rs::enums::User::User(u) => u,
        _ => {
            return Err(OnboardError::Generic(anyhow::anyhow!(
                "get_me returned unexpected User variant"
            )))
        }
    };

    let mode = if creds.bot_token.is_some() {
        "bot".to_string()
    } else {
        "user".to_string()
    };

    let username = me.usernames.as_ref().and_then(|u| {
        u.active_usernames.first().cloned().or_else(|| {
            if u.editable_username.is_empty() {
                None
            } else {
                Some(u.editable_username.clone())
            }
        })
    });

    Ok(TelegramSession {
        username,
        user_id: me.id,
        mode: Some(mode),
        data_dir: data_dir.to_path_buf(),
        verifying_key: creds.verifying_key.clone(),
    })
}

/// Read a single line from stdin (verification code, etc.).
/// Returns a plain `String` because TDLib's `check_authentication_code`
/// takes `String` by value — the clone is inevitable. The buffer is
/// microseconds-lived and will be overwritten on the next allocation.
fn read_line_from_stdin(prompt: &str) -> std::result::Result<String, std::io::Error> {
    use std::io::{BufRead, Write};
    let stdin = std::io::stdin();
    let stdout = std::io::stdout();
    let mut stdout = stdout.lock();
    write!(stdout, "{}", prompt)?;
    stdout.flush()?;
    let mut line = String::new();
    stdin.lock().read_line(&mut line)?;
    Ok(line)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classify_flood_wait() {
        let e = classify_tdlib_error("FLOOD_WAIT_60".into());
        assert!(matches!(e, OnboardError::RateLimited(_)));
        assert_eq!(e.exit_code(), 6);
    }

    #[test]
    fn classify_network_error() {
        let e = classify_tdlib_error("network timeout".into());
        assert!(matches!(e, OnboardError::TelegramUnreachable(_)));
        assert_eq!(e.exit_code(), 3);
    }

    #[test]
    fn classify_auth_error() {
        let e = classify_tdlib_error("invalid bot token".into());
        assert!(matches!(e, OnboardError::AuthRejected(_)));
        assert_eq!(e.exit_code(), 2);
    }

    #[test]
    fn classify_wait_registration_is_auth_rejected() {
        let e = classify_tdlib_error("WAIT_REGISTRATION".into());
        assert!(matches!(e, OnboardError::AuthRejected(_)));
        assert_eq!(e.exit_code(), 2);
    }

    #[test]
    fn classify_please_wait_is_auth_rejected() {
        let e = classify_tdlib_error("Please wait for SMS".into());
        assert!(matches!(e, OnboardError::AuthRejected(_)));
        assert_eq!(e.exit_code(), 2);
    }

    #[test]
    fn classify_flood_wait_300() {
        let e = classify_tdlib_error("FLOOD_WAIT_300".into());
        assert!(matches!(e, OnboardError::RateLimited(_)));
        assert_eq!(e.exit_code(), 6);
    }

    #[test]
    fn validate_api_id_positive() {
        assert_eq!(validate_api_id(12345).unwrap(), 12345i32);
    }

    #[test]
    fn validate_api_id_zero_rejected() {
        assert!(validate_api_id(0).is_err());
    }

    #[test]
    fn validate_api_id_negative_rejected() {
        assert!(validate_api_id(-1).is_err());
    }

    #[test]
    fn validate_api_id_overflow_rejected() {
        assert!(validate_api_id(i32::MAX as i64 + 1).is_err());
    }

    #[cfg(unix)]
    #[test]
    fn create_auth_dirs_sets_0700() {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::TempDir::new().unwrap();
        let data_dir = dir.path().join("test-session");
        create_auth_dirs(&data_dir).unwrap();
        let perms = std::fs::metadata(&data_dir).unwrap().permissions().mode() & 0o777;
        assert_eq!(perms, 0o700, "expected 0700, got {:o}", perms);
    }
}
