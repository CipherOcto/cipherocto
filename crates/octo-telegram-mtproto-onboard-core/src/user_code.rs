//! User phone + SMS code onboarding flow.
//!
//! Drives `MtprotoTelegramAdapter::connect_user` end-to-end:
//!
//! 1. `request_login_code` — Telegram sends an SMS to the
//!    supplied phone.
//! 2. `submit_code` — operator pastes the SMS code (delivered
//!    via the `code_rx` receiver).
//! 3. *Optional* `submit_password` — if the account has 2FA,
//!    the operator supplies the cloud password (via the
//!    `password_rx` receiver).
//!
//! ## Channel shape
//!
//! `MtprotoTelegramAdapter::connect_user` takes two `FnOnce`
//! closures (`ask_code`, `ask_password`). Those closures are
//! *synchronous* — they cannot `.await` directly. To bridge
//! from async input (the CLI reading stdin) to a synchronous
//! closure, we use a `tokio::sync::oneshot` for each step.
//! Polling via `try_recv` + `std::thread::yield_now` would
//! suffice, but `oneshot::blocking_recv` is illegal inside
//! any Tokio runtime — so we use the polling approach with a
//! deadline-based timeout to avoid hangs.
//!
//! The CLI is expected to spawn a forwarder task that reads
//! the operator's input and writes it to the `oneshot`. The
//! library exposes [`forward_input`] to make that one-liner.

use std::future::Future;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Instant;

use octo_adapter_telegram_mtproto::{MtprotoTelegramAdapter, MtprotoTelegramClient};
use tokio::sync::{mpsc, oneshot};
use tokio::task::JoinHandle;
use tracing::{debug, info, warn};

use crate::auth::auth_state_name;
use crate::error::OnboardError;
use crate::output::{OnboardMode, OnboardOutput};
use crate::session::SessionRecord;

/// User-mode credentials required to start the flow.
#[derive(Debug, Clone)]
pub struct UserCodeCredentials {
    /// E.164 phone number (e.g. `+15551234567`).
    pub phone: String,
}

/// Validate a phone number shape. Cheap structural check
/// (starts with `+`, 8–15 digits). The adapter's
/// `request_login_code` does the real `auth.sendCode` RPC
/// (which can still fail with `PHONE_NUMBER_INVALID` etc.).
pub fn validate_phone(phone: &str) -> Result<(), OnboardError> {
    if phone.is_empty() {
        return Err(OnboardError::InvalidInput("phone is empty".to_string()));
    }
    if !phone.starts_with('+') {
        return Err(OnboardError::InvalidInput(
            "phone must be in E.164 form (start with '+')".to_string(),
        ));
    }
    let digits = phone.chars().filter(|c| c.is_ascii_digit()).count();
    if !(8..=15).contains(&digits) {
        return Err(OnboardError::InvalidInput(format!(
            "phone has {} digits; E.164 requires 8..=15",
            digits
        )));
    }
    Ok(())
}

/// Spawn a forwarder that reads one value from `mpsc_rx` and
/// sends it down `oneshot_tx`. Used by the CLI to bridge from
/// its stdin-driven `mpsc::Sender<String>` to the library's
/// oneshot-based closures.
///
/// The returned `JoinHandle` resolves once the value is
/// forwarded (or the mpsc closes).
pub fn forward_input(
    mut mpsc_rx: mpsc::Receiver<String>,
    oneshot_tx: oneshot::Sender<String>,
) -> JoinHandle<()> {
    tokio::spawn(async move {
        match mpsc_rx.recv().await {
            Some(value) => {
                let _ = oneshot_tx.send(value);
            }
            None => {
                // mpsc closed; the oneshot sender is dropped,
                // which makes the closure see a closed
                // channel and abort.
                warn!("forward_input: mpsc closed before value arrived");
            }
        }
    })
}

/// Run the user-code onboarding flow to completion.
///
/// The function blocks until the adapter reaches `Ready` or
/// returns an error. The SMS code (and 2FA password, if
/// required) must be sent down the matching `mpsc::Sender`
/// while `run` is awaiting — typically from a sibling
/// `tokio::spawn` task in the CLI.
///
/// The adapter's `connect_user` is run on a
/// `spawn_blocking` thread because its `FnOnce` closures use
/// `oneshot::blocking_recv` (which is illegal on a Tokio
/// worker thread).
///
/// Generic over the client impl so the same code path drives
/// production (real Telegram) and tests (mock).
pub async fn run<C>(
    adapter: Arc<MtprotoTelegramAdapter<C>>,
    credentials: UserCodeCredentials,
    code_rx: mpsc::Receiver<String>,
    password_rx: mpsc::Receiver<String>,
    data_dir: &Path,
) -> Result<(OnboardOutput, PathBuf), OnboardError>
where
    C: MtprotoTelegramClient + 'static,
{
    validate_phone(&credentials.phone)?;
    let start = Instant::now();
    info!(
        path = "user_code",
        phone_prefix = %mask_phone(&credentials.phone),
        "starting user-code onboarding"
    );
    debug!(data_dir = %data_dir.display(), "using data dir");

    // Build the oneshot pairs the closures will pull from,
    // and spawn forwarders to translate mpsc → oneshot.
    let (code_tx, mut code_rx_oneshot) = oneshot::channel::<String>();
    let (password_tx, mut password_rx_oneshot) = oneshot::channel::<String>();

    let forward_code = forward_input(code_rx, code_tx);
    let forward_password = forward_input(password_rx, password_tx);

    // The closures must be `FnOnce` and synchronous (the
    // adapter's `connect_user` API requires it). We bridge
    // from async input via `oneshot` — but we cannot use
    // `oneshot::blocking_recv` because we are running inside
    // a Tokio runtime (`spawn_blocking` reuses the test
    // worker's blocking pool; nesting another `current_thread`
    // runtime would mark a runtime as current, and
    // `blocking_recv` panics if any Tokio runtime is current).
    //
    // Instead we yield the runtime thread until the oneshot
    // resolves. `std::thread::yield_now` parks the current
    // OS thread briefly without depending on Tokio. Combined
    // with `try_recv`, this gives the forwarder task time to
    // deliver the value. The wait is bounded by the supplied
    // `code_timeout` / `password_timeout` (default 60s each)
    // so a non-arriving input cannot deadlock the flow.
    let code_deadline = std::time::Instant::now() + std::time::Duration::from_secs(60);
    let password_deadline = std::time::Instant::now() + std::time::Duration::from_secs(60);
    let ask_code = move || loop {
        match code_rx_oneshot.try_recv() {
            Ok(code) => return code,
            Err(oneshot::error::TryRecvError::Closed) => {
                warn!("ask_code: channel closed before code arrived");
                return String::new();
            }
            Err(oneshot::error::TryRecvError::Empty) => {
                if std::time::Instant::now() >= code_deadline {
                    warn!("ask_code: timed out waiting for code");
                    return String::new();
                }
                std::thread::yield_now();
            }
        }
    };
    let ask_password = move || loop {
        match password_rx_oneshot.try_recv() {
            Ok(p) => return Some(p),
            Err(oneshot::error::TryRecvError::Closed) => {
                // Operator chose not to provide a password.
                return None;
            }
            Err(oneshot::error::TryRecvError::Empty) => {
                if std::time::Instant::now() >= password_deadline {
                    warn!("ask_password: timed out waiting for password");
                    return None;
                }
                std::thread::yield_now();
            }
        }
    };

    // Run the whole `connect_user` call. It is async, so we
    // just `.await` it from the test's current runtime — no
    // `spawn_blocking` or nested runtime needed (the closures
    // above use `yield_now`, not `blocking_recv`).
    let phone = credentials.phone.clone();
    let adapter_for_connect = Arc::clone(&adapter);
    let connect_result = adapter_for_connect
        .connect_user(&phone, ask_code, ask_password)
        .await;

    // The forwarders exit as soon as the oneshot fires
    // (or the mpsc closes), so we just abort them to be
    // safe in the error path.
    forward_code.abort();
    forward_password.abort();

    connect_result.map_err(|e| {
        use octo_adapter_telegram_mtproto::MtprotoTelegramError as E;
        match e {
            E::Config(_) => OnboardError::Config(e.to_string()),
            E::Auth(_) => OnboardError::TelegramApi(e.to_string()),
            E::Rpc { .. } => OnboardError::TelegramApi(e.to_string()),
            E::RateLimited { .. } => OnboardError::TelegramApi(e.to_string()),
            E::Network(_) => OnboardError::Network(e.to_string()),
            E::NotReady(_) => OnboardError::NotReady {
                last_state: auth_state_name(&adapter),
            },
            other => OnboardError::Adapter(other.to_string()),
        }
    })?;

    if !adapter.has_valid_session() {
        return Err(OnboardError::NotReady {
            last_state: auth_state_name(&adapter),
        });
    }

    let identity = adapter
        .self_handle_ref()
        .get()
        .ok_or_else(|| OnboardError::NotReady {
            last_state: auth_state_name(&adapter),
        })?;
    let elapsed = start.elapsed();

    let record = SessionRecord::from_identity(&identity, "user_code", unix_now_secs());
    let _session_path = record.write_to(data_dir)?;
    let config_path = data_dir.join("config.json");

    let output = OnboardOutput {
        schema_version: OnboardOutput::SCHEMA_VERSION,
        mode: OnboardMode::UserCode,
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
        "user-code onboarding complete"
    );
    Ok((output, config_path))
}

fn unix_now_secs() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

/// Mask all but the first 4 and last 2 digits of a phone
/// number for log lines. Logs MUST NOT contain the full
/// phone (it's personally-identifiable information).
fn mask_phone(phone: &str) -> String {
    let digits: String = phone.chars().filter(|c| c.is_ascii_digit()).collect();
    if digits.len() <= 6 {
        return "+***".to_string();
    }
    let head = &digits[..4];
    let tail = &digits[digits.len() - 2..];
    format!("+{}***{}", head, tail)
}

// Suppress the "imported but not used" warning for `Future`
// (kept so callers can `use` it for forwarder futures).
#[allow(dead_code)]
fn _ensure_future_in_scope<F: Future<Output = ()>>(_f: F) {}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_helpers::mock_adapter_for_test;
    use tempfile::tempdir;

    #[test]
    fn validate_phone_rejects_empty() {
        let e = validate_phone("").unwrap_err();
        assert_eq!(e.kind(), "invalid_input");
    }

    #[test]
    fn validate_phone_rejects_no_plus() {
        let e = validate_phone("15551234567").unwrap_err();
        assert_eq!(e.kind(), "invalid_input");
    }

    #[test]
    fn validate_phone_rejects_too_few_digits() {
        let e = validate_phone("+123").unwrap_err();
        assert_eq!(e.kind(), "invalid_input");
    }

    #[test]
    fn validate_phone_rejects_too_many_digits() {
        let e = validate_phone("+1234567890123456").unwrap_err();
        assert_eq!(e.kind(), "invalid_input");
    }

    #[test]
    fn validate_phone_accepts_canonical_e164() {
        validate_phone("+15551234567").unwrap();
    }

    #[test]
    fn mask_phone_hides_middle() {
        assert_eq!(mask_phone("+15551234567"), "+1555***67");
    }

    #[test]
    fn mask_phone_handles_short_input() {
        assert_eq!(mask_phone("+123"), "+***");
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn run_succeeds_against_mock() {
        // The mock client accepts any phone + code, so
        // the flow should reach Ready without a real
        // Telegram server. This exercises the
        // mpsc → oneshot → FnOnce plumbing end-to-end.
        let tmp = tempdir().unwrap();
        let adapter = mock_adapter_for_test(tmp.path());
        let (code_tx, code_rx) = mpsc::channel::<String>(1);
        let (password_tx, password_rx) = mpsc::channel::<String>(1);
        let creds = UserCodeCredentials {
            phone: "+15551234567".to_string(),
        };

        // Drive the channels from a sibling task.
        let input_task = tokio::spawn(async move {
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
            code_tx.send("12345".to_string()).await.unwrap();
            // 2FA isn't required by the mock by default, so
            // the password sender is just dropped (which
            // causes the closure to see `None`).
            drop(password_tx);
        });

        let (out, _cfg_path) = run(adapter, creds, code_rx, password_rx, tmp.path())
            .await
            .expect("user-code run should succeed against mock");
        let _ = input_task.await;
        assert!(!out.is_bot);
        assert!(out.self_id != 0);
        assert_eq!(out.mode, OnboardMode::UserCode);
        assert!(tmp.path().join("session.json").exists());
    }

    #[tokio::test(flavor = "current_thread")]
    async fn forward_input_translates_mpsc_to_oneshot() {
        let (mpsc_tx, mpsc_rx) = mpsc::channel::<String>(1);
        let (oneshot_tx, oneshot_rx) = oneshot::channel::<String>();
        let fwd = forward_input(mpsc_rx, oneshot_tx);
        mpsc_tx.send("hello".to_string()).await.unwrap();
        drop(mpsc_tx);
        fwd.await.unwrap();
        assert_eq!(oneshot_rx.await.unwrap(), "hello");
    }
}
