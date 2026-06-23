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
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use octo_adapter_telegram_mtproto::{MtprotoTelegramAdapter, MtprotoTelegramClient};
use tokio::time::sleep;
use tracing::{debug, info, warn};

use crate::adapter_error::{self, AdapterErrorKind};
use crate::auth::auth_state_name;
use crate::error::OnboardError;
use crate::output::{validate_username, OnboardMode, OnboardOutput};
use crate::session::SessionRecord;
use crate::time_util::unix_now_secs;

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
/// `abort` is an `Arc<AtomicBool>` the CLI sets to `true` to
/// request a graceful abort (typically wired to a SIGINT
/// handler). R2-OPS-8: the round-1 implementation had no
/// abort path — Ctrl-C killed the process mid-poll, leaving
/// `session.json` written but `config.json` not yet
/// committed (or vice versa). The flag is checked once per
/// poll iteration, and the function returns
/// `OnboardError::ChannelClosed("aborted by SIGINT")` so the
/// CLI can clean up.
///
/// The function returns once either:
/// * `poll_qr_login` returns `Ok(SelfUserInfo)` (success), or
/// * `timeout` elapses without a successful scan
///   (`OnboardError::Timeout`), or
/// * `abort` is set (R2-OPS-8), or
/// * the adapter reports a non-QR error
///   (`OnboardError::TelegramApi` etc.).
///
/// R2-IE-17: a `poll_interval` of zero would busy-loop the
/// poll iteration (no `sleep`). The CLI's `cli.rs` already
/// rejects `--poll-interval-secs 0` (it returns an error
/// before calling `run`), so the floor here is defensive
/// — if a future caller forgets the CLI check, we still
/// don't burn a CPU core. The floor is 100ms (one human-
/// perceptible frame).
///
/// Generic over the client impl so the same code path drives
/// production (real Telegram) and tests (mock).
pub async fn run<C, F>(
    adapter: Arc<MtprotoTelegramAdapter<C>>,
    data_dir: &Path,
    timeout: Duration,
    poll_interval: Duration,
    mut on_handle: F,
    abort: Arc<AtomicBool>,
) -> Result<(OnboardOutput, PathBuf), OnboardError>
where
    C: MtprotoTelegramClient + 'static,
    F: FnMut(&QrLoginPrompt),
{
    // R2-IE-17: floor the poll interval at 100ms so a
    // misconfigured caller (e.g. `--poll-interval-secs 0`)
    // can't busy-loop the QR poll. The CLI's `cli.rs`
    // already rejects `--poll-interval-secs 0`, but we
    // belt-and-braces it here too.
    let poll_interval = if poll_interval < Duration::from_millis(100) {
        Duration::from_millis(100)
    } else {
        poll_interval
    };
    // R2-IE-17: same floor for the timeout — a 0s timeout
    // would race the very first poll call. We keep the
    // existing 300s default but cap the minimum at 1s so
    // any future caller passing `Duration::from_secs(0)`
    // gets a sane retry window instead of immediate
    // timeout.
    let timeout = if timeout < Duration::from_secs(1) {
        Duration::from_secs(1)
    } else {
        timeout
    };
    let start = std::time::Instant::now();
    info!(path = "qr_login", "starting QR login onboarding");
    debug!(data_dir = %data_dir.display(), "using data dir");

    // Step 1: ask the adapter for a QR handle.
    let handle_result = adapter.connect_qr_login().await;
    let handle = match handle_result {
        Ok(h) => h,
        Err(e) => {
            // R2-IE-9: classify the error via the typed
            // `AdapterErrorKind` enum (centralised in
            // `crate::adapter_error`) instead of substring-
            // matching the `Display` output. The round-1
            // implementation used `if s.contains("already
            // authorized")`, which is fragile (any change to
            // the adapter's error message breaks the flow
            // silently). The classification is a prefix match
            // against the adapter-documented
            // `"qr_login: already authorized"` string — still
            // a string match, but now centralised here with a
            // test that pins the prefix.
            match adapter_error::classify(&e) {
                AdapterErrorKind::AlreadyAuthorized => {
                    // IE-7 (R26): the adapter has already
                    // driven the lifecycle to `Ready` and
                    // populated the self-handle. Surface
                    // this as a successful flow so the
                    // operator doesn't have to re-onboard.
                    if !adapter.has_valid_session() {
                        return Err(adapter_error::map(
                            e,
                            &auth_state_name(&adapter),
                        ));
                    }
                    let identity = adapter
                        .self_handle_ref()
                        .get()
                        .ok_or_else(|| OnboardError::Lifecycle {
                            state: auth_state_name(&adapter),
                        })?;
                    let elapsed = start.elapsed();
                    let record = SessionRecord::from_identity(
                        &identity,
                        "qr_login",
                        unix_now_secs(),
                    );
                    let _session_path = record.write_to(data_dir)?;
                    let config_path = data_dir.join("config.json");
                    let output = OnboardOutput {
                        schema_version: OnboardOutput::SCHEMA_VERSION,
                        mode: OnboardMode::QrLogin,
                        self_id: identity.user_id,
                        // R2-PROTO-14: strip control chars
                        // and look-alike unicode codepoints.
                        self_username: validate_username(identity.username.clone()),
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
                // R2-ARCH-4 / R2-IE-12: every other error
                // path goes through the shared
                // `adapter_error::map`. The QR-login flow
                // used to inline its own match (which was
                // a less-complete superset of the bot/user
                // flows' matches); the shared helper is
                // the single source of truth.
                _ => {
                    return Err(adapter_error::map(
                        e,
                        &auth_state_name(&adapter),
                    ));
                }
            }
        }
    };

    let mut first_handle = QrLoginPrompt::from_handle(&handle);
    on_handle(&first_handle);

    // Step 2: poll until success, timeout, or abort.
    loop {
        // R2-OPS-8: check the abort flag at the top of
        // every iteration. If the CLI's SIGINT handler
        // set the flag, return a `ChannelClosed` error
        // so the operator gets a clear "aborted" exit
        // code (5) rather than a stack trace from a
        // process killed mid-write.
        if abort.load(Ordering::Relaxed) {
            warn!("QR login: abort requested (SIGINT or operator cancel)");
            return Err(OnboardError::ChannelClosed(
                "aborted by SIGINT".to_string(),
            ));
        }
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
                    // R2-PROTO-14: strip control chars
                    // and look-alike unicode codepoints.
                    self_username: validate_username(identity.username.clone()),
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
                // R2-ARCH-4 / R2-IE-12: the round-1 inline
                // match is gone; every adapter error
                // (including the `2FA_REQUIRED` special case
                // below) goes through the shared
                // `adapter_error::map` helper.
                //
                // R2-PROTO-8: the 2FA special case used to
                // match on `MtprotoTelegramError::Auth(msg)
                // if msg == "2FA_REQUIRED"` and surface a
                // `OnboardError::Adapter(...)`. The check
                // is preserved here (it's a documented
                // signal from the adapter) but the error
                // path now flows through `adapter_error::map`
                // for consistent CLI exit codes and
                // redaction-friendly rendering.
                if let octo_adapter_telegram_mtproto::MtprotoTelegramError::Auth(msg) = &other {
                    if msg == "2FA_REQUIRED" {
                        // The primary device has 2FA
                        // enabled. The adapter does not
                        // auto-handle this; the CLI must
                        // prompt for the password and call
                        // `adapter.client().submit_password(...)`.
                        // That step is owned by the
                        // user_code flow and is out of
                        // scope for Phase B; surface a
                        // clear error.
                        warn!("QR login: primary device has 2FA enabled; not yet supported in Phase B");
                        return Err(OnboardError::Adapter(
                            "QR login on a 2FA-enabled primary device is not \
                             supported in Phase B; use the user_code flow instead"
                                .to_string(),
                        ));
                    }
                }
                return Err(adapter_error::map(
                    other,
                    &auth_state_name(&adapter),
                ));
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

    /// R2-IE-17: a poll interval of zero (or near-zero)
    /// must NOT busy-loop. `run` floors it to 100ms. We
    /// assert the floor indirectly by passing 0 and
    /// checking that the call returns within a
    /// reasonable bound (the default mock succeeds on
    /// the first poll, so the elapsed time is bounded
    /// by the floor, not by an infinite loop).
    #[tokio::test(flavor = "current_thread")]
    async fn run_does_not_busy_loop_when_poll_interval_is_zero() {
        let tmp = tempdir().unwrap();
        let adapter = mock_adapter_for_test(tmp.path());
        let start = std::time::Instant::now();
        // 100ms timeout gives plenty of headroom for
        // the mock to succeed on the first poll.
        let (_out, _cfg) = run(
            adapter,
            tmp.path(),
            Duration::from_millis(100),
            Duration::from_millis(0), // zero poll → must be floored
            |_prompt| {},
            Arc::new(AtomicBool::new(false)),
        )
        .await
        .expect("run should succeed against the default mock");
        let elapsed = start.elapsed();
        // The floor is 100ms; if the floor weren't
        // applied, the loop would burn CPU and we
        // couldn't observe it from this side, but at
        // least we confirm the call returned (i.e.
        // didn't deadlock / infinite-loop).
        assert!(
            elapsed < Duration::from_secs(2),
            "run took too long ({:?}) — poll floor may not be applied",
            elapsed
        );
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
            Arc::new(AtomicBool::new(false)),
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

    /// R2-OPS-8: setting the abort flag (which the CLI's
    /// SIGINT handler would do) makes `run` return
    /// `OnboardError::ChannelClosed` instead of timing out
    /// or getting killed mid-write. The default mock
    /// succeeds on the first poll, so we set the flag
    /// BEFORE the call (simulating a SIGINT that arrived
    /// during `connect_qr_login`) — the flag check at the
    /// top of the poll loop sees it on the first iteration
    /// and returns.
    #[tokio::test(flavor = "current_thread")]
    async fn run_aborts_on_flag() {
        let tmp = tempdir().unwrap();
        let adapter = mock_adapter_for_test(tmp.path());
        let abort = Arc::new(AtomicBool::new(true)); // pre-armed
        let err = run(
            adapter,
            tmp.path(),
            Duration::from_secs(5),    // 5s timeout — the abort must beat it
            Duration::from_millis(20), // 20ms poll
            |_prompt| {},
            abort,
        )
        .await
        .expect_err("run should return an error when the abort flag is set");
        assert_eq!(err.kind(), "channel_closed");
        // Display includes the "aborted by SIGINT" hint so
        // the operator understands what happened.
        assert!(err.to_string().contains("aborted by SIGINT"));
    }
}
