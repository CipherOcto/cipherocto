//! QR login onboarding flow (Phase 2.5).
//!
//! Drives `MtprotoTelegramAdapter::connect_qr_login` and then
//! loops on `poll_qr_login` until the operator has scanned the
//! QR code from another already-logged-in Telegram device and
//! the import finalized.
//!
//! The CLI's job is to:
//!
//! 1. Render the returned `tg://login?token=...` URL as a QR
//!    code (via `qr2term` or a `qrcode` PNG renderer).
//! 2. Display a "press Ctrl-C to abort" hint.
//! 3. Call [`run`]; the function polls in a loop with a small
//!    backoff and re-renders the QR on each refresh.
//!
//! On success, the adapter's self-handle is populated and we
//! write a `SessionRecord` to `data_dir`.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use octo_adapter_telegram_mtproto::{MtprotoTelegramAdapter, MtprotoTelegramClient};
use tokio::time::sleep;
use tracing::{debug, info, warn};

use crate::auth::auth_state_name;
use crate::error::OnboardError;
use crate::output::{OnboardMode, OnboardOutput};
use crate::session::SessionRecord;

/// How long to wait between `poll_qr_login` calls. Telegram
/// rotates the QR token every ~30 seconds; we poll twice per
/// rotation to keep the UI responsive.
pub const DEFAULT_POLL_INTERVAL: Duration = Duration::from_secs(2);

/// How long the QR login flow is allowed to wait for the
/// operator to scan. After this, the function returns
/// `OnboardError::Timeout`.
pub const DEFAULT_TIMEOUT: Duration = Duration::from_secs(300);

/// Run the QR login onboarding flow to completion.
///
/// `adapter` is the (shared) `Arc<MtprotoTelegramAdapter<C>>`.
/// `data_dir` is the on-disk location where the session and
/// config files will be written.
/// `on_handle` is a callback invoked on each QR handle refresh
/// so the CLI can re-render the QR code (the token changes
/// every ~30 seconds).
///
/// The function returns once either:
/// * `poll_qr_login` returns `Ok(SelfUserInfo)` (success), or
/// * `timeout` elapses without a successful scan
///   (`OnboardError::Timeout`), or
/// * the adapter reports a non-QR error
///   (`OnboardError::TelegramApi` etc.).
///
/// Generic over the client impl so the same code path drives
/// production (real Telegram) and tests (mock).
pub async fn run<C, F>(
    adapter: Arc<MtprotoTelegramAdapter<C>>,
    data_dir: &Path,
    timeout: Duration,
    poll_interval: Duration,
    mut on_handle: F,
) -> Result<(OnboardOutput, PathBuf), OnboardError>
where
    C: MtprotoTelegramClient + 'static,
    F: FnMut(&QrLoginPrompt),
{
    let start = std::time::Instant::now();
    info!(path = "qr_login", "starting QR login onboarding");
    debug!(data_dir = %data_dir.display(), "using data dir");

    // Step 1: ask the adapter for a QR handle.
    let handle_result = adapter.connect_qr_login().await;
    let handle = match handle_result {
        Ok(h) => h,
        Err(e) => {
            // IE-7 (R26): the adapter returns
            // `Internal("qr_login: already authorized (session
            // was valid; no QR needed)")` when the underlying
            // session is already authorised (the user
            // re-scanned while signed in, or the session was
            // restored from disk and the DC is already
            // connected). The adapter has already driven the
            // lifecycle to `Ready` and populated the
            // self-handle. Surface this as a successful flow
            // so the operator doesn't have to re-onboard.
            let s = e.to_string();
            if s.contains("already authorized") {
                if !adapter.has_valid_session() {
                    return Err(OnboardError::Adapter(format!(
                        "qr_login: already authorized but session is invalid: {}",
                        s
                    )));
                }
                let identity = adapter
                    .self_handle_ref()
                    .get()
                    .ok_or_else(|| OnboardError::Lifecycle {
                        state: auth_state_name(&adapter),
                    })?;
                let elapsed = start.elapsed();
                let record =
                    SessionRecord::from_identity(&identity, "qr_login", unix_now_secs());
                let _session_path = record.write_to(data_dir)?;
                let config_path = data_dir.join("config.json");
                let output = OnboardOutput {
                    schema_version: OnboardOutput::SCHEMA_VERSION,
                    mode: OnboardMode::QrLogin,
                    self_id: identity.user_id,
                    self_username: identity.username.clone(),
                    is_bot: false,
                    data_dir: data_dir.display().to_string(),
                    config_path: config_path.display().to_string(),
                    elapsed_ms: elapsed.as_millis() as u64,
                };
                info!(
                    user_id = identity.user_id,
                    elapsed_ms = output.elapsed_ms,
                    "qr-login already authorised; reusing existing session"
                );
                return Ok((output, config_path));
            }
            // Otherwise: map the error to the appropriate
            // OnboardError variant.
            use octo_adapter_telegram_mtproto::MtprotoTelegramError as E;
            return Err(match e {
                E::Config(_) => OnboardError::Config(s),
                E::Auth(_) => OnboardError::TelegramApi(s),
                E::Rpc { .. } => OnboardError::TelegramApi(s),
                E::RateLimited { .. } => OnboardError::TelegramApi(s),
                E::Network(_) => OnboardError::Network(s),
                E::Internal(_) => OnboardError::Adapter(s),
                E::QrLoginHandle { .. } => {
                    // Defensive: connect_qr_login's signature
                    // says it returns QrLoginHandle on success
                    // and only QrLoginHandle-as-error in the
                    // err variant. The from_error extraction is
                    // the adapter's job; if we get an error of
                    // this shape here it's a contract violation
                    // and we surface it as a generic adapter
                    // error.
                    OnboardError::Adapter(s)
                }
                other => OnboardError::Adapter(other.to_string()),
            });
        }
    };

    let mut first_handle = QrLoginPrompt::from_handle(&handle);
    on_handle(&first_handle);

    // Step 2: poll until success or timeout.
    loop {
        if start.elapsed() > timeout {
            return Err(OnboardError::Timeout(format!(
                "qr login did not finalize within {}s",
                timeout.as_secs()
            )));
        }

        match adapter.poll_qr_login().await {
            Ok(info) => {
                // Populate the self-handle is done by the
                // adapter (see poll_qr_login). Verify and
                // write the session record.
                if !adapter.has_valid_session() {
                    return Err(OnboardError::Lifecycle {
                        state: auth_state_name(&adapter),
                    });
                }
                let identity =
                    adapter
                        .self_handle_ref()
                        .get()
                        .ok_or_else(|| OnboardError::Lifecycle {
                            state: auth_state_name(&adapter),
                        })?;
                let elapsed = start.elapsed();

                let record = SessionRecord::from_identity(&identity, "qr_login", unix_now_secs());
                let _session_path = record.write_to(data_dir)?;
                let config_path = data_dir.join("config.json");

                let output = OnboardOutput {
                    schema_version: OnboardOutput::SCHEMA_VERSION,
                    mode: OnboardMode::QrLogin,
                    self_id: identity.user_id,
                    self_username: identity.username.clone(),
                    is_bot: false,
                    data_dir: data_dir.display().to_string(),
                    config_path: config_path.display().to_string(),
                    elapsed_ms: elapsed.as_millis() as u64,
                };
                info!(
                    user_id = identity.user_id,
                    elapsed_ms = output.elapsed_ms,
                    "qr-login onboarding complete"
                );
                let _ = info; // adapter already stored it
                return Ok((output, config_path));
            }
            Err(octo_adapter_telegram_mtproto::MtprotoTelegramError::QrLoginHandle {
                token,
                url,
            }) => {
                // Still pending. Refresh the QR if it
                // changed.
                let refreshed = QrLoginPrompt { token, url };
                if refreshed.url != first_handle.url {
                    debug!("QR login token rotated; re-displaying");
                    first_handle = refreshed.clone();
                    on_handle(&refreshed);
                }
                sleep(poll_interval).await;
            }
            Err(octo_adapter_telegram_mtproto::MtprotoTelegramError::Auth(msg))
                if msg == "2FA_REQUIRED" =>
            {
                // The primary device has 2FA enabled. The
                // adapter does not auto-handle this; the
                // CLI must prompt for the password and call
                // `adapter.client().submit_password(...)`.
                // That step is owned by the user_code flow
                // and is out of scope for Phase B; surface
                // a clear error.
                warn!("QR login: primary device has 2FA enabled; not yet supported in Phase B");
                return Err(OnboardError::Adapter(
                    "QR login on a 2FA-enabled primary device is not \
                     supported in Phase B; use the user_code flow instead"
                        .to_string(),
                ));
            }
            Err(other) => {
                return Err(match other {
                    octo_adapter_telegram_mtproto::MtprotoTelegramError::Config(m) => {
                        OnboardError::Config(m)
                    }
                    octo_adapter_telegram_mtproto::MtprotoTelegramError::Network(m) => {
                        OnboardError::Network(m)
                    }
                    octo_adapter_telegram_mtproto::MtprotoTelegramError::Rpc { code, message } => {
                        OnboardError::TelegramApi(format!("rpc: code={} message={}", code, message))
                    }
                    other => OnboardError::Adapter(other.to_string()),
                });
            }
        }
    }
}

/// Callback payload — a single QR handle. The CLI renders
/// `url` (the `tg://login?token=...` form) as a QR code. The
/// raw `token` bytes are also available for callers that want
/// to base64-encode them themselves.
#[derive(Debug, Clone)]
pub struct QrLoginPrompt {
    /// Raw token bytes (NOT base64-encoded). The CLI can
    /// pass these through `base64::encode` if it wants the
    /// token-only form.
    pub token: Vec<u8>,
    /// `tg://login?token=<base64>` URL. This is the QR
    /// payload (the URL is the public form; the token is
    /// the credential).
    pub url: String,
}

impl QrLoginPrompt {
    fn from_handle(h: &octo_adapter_telegram_mtproto::QrLoginHandle) -> Self {
        Self {
            token: h.token.clone(),
            url: h.url.clone(),
        }
    }
}

fn unix_now_secs() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_helpers::mock_adapter_for_test;
    use tempfile::tempdir;

    #[test]
    fn default_poll_interval_is_2_seconds() {
        // Sanity check: keep the constant stable.
        assert_eq!(DEFAULT_POLL_INTERVAL, Duration::from_secs(2));
    }

    #[test]
    fn default_timeout_is_5_minutes() {
        assert_eq!(DEFAULT_TIMEOUT, Duration::from_secs(300));
    }

    #[tokio::test(flavor = "current_thread")]
    async fn run_succeeds_against_mock_immediately() {
        // Happy path: the default mock accepts any token on
        // the very first poll. `run` should therefore return
        // a successful `OnboardOutput` rather than time out.
        let tmp = tempdir().unwrap();
        let adapter = mock_adapter_for_test(tmp.path());
        let mut seen: Vec<String> = Vec::new();
        let (out, _cfg_path) = run(
            adapter,
            tmp.path(),
            Duration::from_millis(500), // generous timeout
            Duration::from_millis(20),  // 20ms poll
            |prompt| seen.push(prompt.url.clone()),
        )
        .await
        .expect("run should succeed against the default mock");
        // The on_handle callback should have fired at least
        // once (the initial handle, before the first poll
        // succeeded).
        assert!(!seen.is_empty(), "on_handle should have been called");
        assert_eq!(out.mode, OnboardMode::QrLogin);
        assert!(!out.is_bot);
        // For a stricter timeout test, see the adapter-level
        // tests in `octo-adapter-telegram-mtproto` (`adapter.rs
        // ::connect_qr_login_loop_*`); the library-level
        // timeout path is hard to drive from the CLI surface
        // because the mock resets its poll counter on every
        // `qr_login` call.
    }
}
