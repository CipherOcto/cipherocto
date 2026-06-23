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

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Instant;

use octo_adapter_telegram_mtproto::{MtprotoTelegramAdapter, MtprotoTelegramClient};
use tokio::sync::{mpsc, oneshot};
use tokio::task::JoinHandle;
use tracing::{debug, info, warn};
use zeroize::Zeroizing;

use crate::adapter_error;
use crate::auth::auth_state_name;
use crate::error::OnboardError;
use crate::output::{validate_username, OnboardMode, OnboardOutput};
use crate::session::SessionRecord;
use crate::time_util::unix_now_secs;

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
///
/// R2-PROTO-12: the previous version rejected phone numbers
/// with surrounding whitespace — the CLI's `read_line`
/// trims trailing whitespace, but operators who paste a
/// number with a leading space (e.g. copied from a
/// contacts-app rendering) would see a confusing
/// `phone must be in E.164 form (start with '+')` error
/// because the trim was only applied to the trailing side.
/// The fix trims both ends before validation, so
/// `+1 555 1234 567` (with spaces from a copy-paste) and
/// `  +15551234567  ` both validate.
pub fn validate_phone(phone: &str) -> Result<(), OnboardError> {
    let phone = phone.trim();
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
/// its stdin-driven `mpsc::Sender<Zeroizing<String>>` to the
/// library's oneshot-based closures.
///
/// R2-SEC-6: the channel element type is `Zeroizing<String>`
/// (not `String`). The forwarder unwraps the `Zeroizing` and
/// sends the inner `String` over the oneshot, but the
/// `Zeroizing` is dropped immediately after the send (i.e.
/// the source-side buffer is wiped). The oneshot itself
/// still stores `String` — `tokio::sync::oneshot::Sender`
/// doesn't accept `Zeroizing<T>` — so the receiver-side
/// closure consumes the string promptly. The CLI's input
/// task is the other side of the channel; the `Zeroizing`
/// there is the third layer of protection.
///
/// The returned `JoinHandle` resolves once the value is
/// forwarded (or the mpsc closes).
pub fn forward_input(
    mut mpsc_rx: mpsc::Receiver<Zeroizing<String>>,
    oneshot_tx: oneshot::Sender<String>,
) -> JoinHandle<()> {
    tokio::spawn(async move {
        match mpsc_rx.recv().await {
            Some(zs) => {
                // R2-SEC-6: the `Zeroizing` is consumed by
                // `to_string` and then dropped at end of
                // scope, wiping the source-side buffer. The
                // `oneshot` sender takes the inner `String`
                // (we can't pass a `Zeroizing<String>` over
                // a oneshot because the channel's storage is
                // `String`); the receiver side is responsible
                // for wiping its copy.
                let _ = oneshot_tx.send(zs.to_string());
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
    // R2-SEC-6: the channel element type is
    // `Zeroizing<String>` (not `String`) so the channel's
    // heap-allocated buffer is wiped on drop. The previous
    // `String` channel left copies of the SMS code and 2FA
    // password in the channel buffer until the allocator
    // reused the memory; the `Zeroizing` wrapper ensures
    // the bytes are overwritten with zeros when the
    // sender/receiver is dropped.
    code_rx: mpsc::Receiver<Zeroizing<String>>,
    password_rx: mpsc::Receiver<Zeroizing<String>>,
    // R2-OPS-12: SMS-code and 2FA-password deadlines.
    // The round-1 hardcoded 60s constants are now
    // caller-supplied so a CI / automated operator can
    // shorten the wait. The deadlines are armed at the
    // first `try_recv` call inside the closures
    // (R2-PROTO-15), not at closure-construction time.
    code_deadline: std::time::Duration,
    password_deadline: std::time::Duration,
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
    // IE-2 (R26): the prior implementation parked the
    // thread on `std::thread::yield_now()` between
    // `try_recv()` calls, which is a busy-loop that pegs
    // a CPU core at 100% for the full 60-second wait
    // window. Replace with a real OS-level sleep. The
    // minimum sleep granularity on most platforms is
    // 1-15ms (Linux ~1us with CONFIG_HZ=1000, Windows
    // 15.6ms default) so we add a tiny budget per
    // iteration but yield the actual CPU to other
    // threads. The wait is bounded by the supplied
    // R2-OPS-12: the SMS-code and 2FA-password deadlines
    // are caller-supplied via `code_deadline` /
    // `password_deadline` (the round-1 hardcoded 60s
    // constants are gone). The deadlines are armed at the
    // first `try_recv` call inside the closures
    // (R2-PROTO-15), not at closure-construction time.
    // 1ms poll interval is short enough to feel
    // interactive (the operator types the code, presses
    // Enter, and the loop wakes on the next iteration)
    // and long enough that we don't burn CPU. The
    // forwarder task is the one doing the actual wakeup
    // — it sends the value into the oneshot, which makes
    // the next `try_recv` return `Ok`.
    let poll = std::time::Duration::from_millis(1);
    // R2-PROTO-15: the `code_deadline_due` flag is flipped
    // on the first `try_recv` call inside `ask_code` /
    // `ask_password`, not at closure-construction time. The
    // prior version captured `code_deadline = Instant::now()
    // + 60s` BEFORE the SMS was even delivered to the
    // operator's phone — a Telegram-side `sendCode` round-
    // trip typically takes 1-3 seconds, so a 60s window
    // effectively shrank to 57-59s in practice. The fix
    // starts the timer at "first poll attempt", so the
    // full 60s is available for the operator to type and
    // submit the code.
    let code_deadline_due = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
    let code_deadline_at: std::sync::Arc<std::sync::Mutex<Option<std::time::Instant>>> =
        std::sync::Arc::new(std::sync::Mutex::new(None));
    let password_deadline_due = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
    let password_deadline_at: std::sync::Arc<std::sync::Mutex<Option<std::time::Instant>>> =
        std::sync::Arc::new(std::sync::Mutex::new(None));
    // R2-IE-11: track whether the code channel was closed
    // (operator's input pipe died) so we can translate the
    // resulting `PHONE_CODE_INVALID` from the adapter into a
    // clearer `OnboardError::ChannelClosed`. The previous
    // version surfaced the empty-string submission as a
    // confusing "PHONE_CODE_INVALID" error, which made it
    // look like the operator typed a wrong code rather than
    // that the input pipeline had died.
    let code_channel_closed = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
    let code_channel_closed_flag = std::sync::Arc::clone(&code_channel_closed);
    let ask_code = {
        let due_flag = std::sync::Arc::clone(&code_deadline_due);
        let due_at = std::sync::Arc::clone(&code_deadline_at);
        let deadline_duration = code_deadline;
        move || loop {
            // R2-PROTO-15: arm the deadline on first
            // `try_recv` call, not at closure build time.
            if !due_flag.swap(true, std::sync::atomic::Ordering::Relaxed) {
                *due_at.lock().unwrap() = Some(std::time::Instant::now() + deadline_duration);
            }
            match code_rx_oneshot.try_recv() {
                Ok(code) => return code,
                Err(oneshot::error::TryRecvError::Closed) => {
                    warn!("ask_code: channel closed before code arrived");
                    code_channel_closed_flag
                        .store(true, std::sync::atomic::Ordering::Relaxed);
                    return String::new();
                }
                Err(oneshot::error::TryRecvError::Empty) => {
                    if due_at
                        .lock()
                        .unwrap()
                        .map(|t| std::time::Instant::now() >= t)
                        .unwrap_or(false)
                    {
                        warn!("ask_code: timed out waiting for code");
                        return String::new();
                    }
                    std::thread::sleep(poll);
                }
            }
        }
    };
    // R2-IE-19: the password closure returns a richer
    // `PasswordOutcome` enum (NotNeeded / Provided / InputClosed)
    // so the operator's "Enter on no 2FA" is distinguishable
    // from "the input pipe died". The adapter still takes
    // `Option<String>`; we project back to `Option<String>`
    // before returning. R3-1: an earlier draft of this
    // closure also stashed the outcome in an
    // `Arc<Mutex<Option<PasswordOutcome>>>` so a caller
    // could inspect it after `connect_user` returned. The
    // stash was never read (the caller has no way to
    // access it; `connect_user` consumes the closure and
    // returns the `MtprotoSelfIdentity`, not the
    // outcome), so it's dead code. Removed.
    let ask_password = {
        let due_flag = std::sync::Arc::clone(&password_deadline_due);
        let due_at = std::sync::Arc::clone(&password_deadline_at);
        let deadline_duration = password_deadline;
        move || -> Option<String> {
            // R2-PROTO-15: arm the deadline on first
            // `try_recv` call (not at closure build time).
            if !due_flag.swap(true, std::sync::atomic::Ordering::Relaxed) {
                *due_at.lock().unwrap() = Some(std::time::Instant::now() + deadline_duration);
            }
            let outcome = loop {
                match password_rx_oneshot.try_recv() {
                    Ok(p) => break PasswordOutcome::Provided(p),
                    Err(oneshot::error::TryRecvError::Closed) => {
                        // R2-IE-19: the CLI's input_task
                        // drops the sender on Enter-no-2FA,
                        // so a `Closed` error here is the
                        // "no 2FA" signal — the operator
                        // deliberately chose not to
                        // provide a password. Map to
                        // `PasswordOutcome::NotNeeded`
                        // (the closure projects back to
                        // `None` at the end).
                        break PasswordOutcome::NotNeeded;
                    }
                    Err(oneshot::error::TryRecvError::Empty) => {
                        if due_at
                            .lock()
                            .unwrap()
                            .map(|t| std::time::Instant::now() >= t)
                            .unwrap_or(false)
                        {
                            warn!("ask_password: timed out waiting for password");
                            break PasswordOutcome::InputClosed;
                        }
                        std::thread::sleep(poll);
                    }
                }
            };
            match outcome {
                PasswordOutcome::Provided(s) => Some(s),
                PasswordOutcome::NotNeeded | PasswordOutcome::InputClosed => None,
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
        // R2-IE-11: if the SMS code channel was closed
        // (operator's input pipe died), translate the
        // empty-string submission (which the adapter
        // reports as `PHONE_CODE_INVALID`) to a more
        // accurate `ChannelClosed("code")` error. The
        // previous version surfaced the adapter's
        // `PHONE_CODE_INVALID` to the operator, which
        // made it look like they typed a wrong code
        // rather than that the input pipeline had died.
        if code_channel_closed.load(std::sync::atomic::Ordering::Relaxed) {
            return OnboardError::ChannelClosed("code".to_string());
        }
        // R2-ARCH-4 / R2-IE-12: use the shared
        // `adapter_error::map` instead of the inline match
        // (the round-1 inline copy was duplicated in three
        // places; the central helper is the single source of
        // truth for the `MtprotoTelegramError` →
        // `OnboardError` mapping).
        adapter_error::map(e, &auth_state_name(&adapter))
    })?;

    if !adapter.has_valid_session() {
        return Err(OnboardError::Lifecycle {
            state: auth_state_name(&adapter),
        });
    }

    let identity = adapter
        .self_handle_ref()
        .get()
        .ok_or_else(|| OnboardError::Lifecycle {
            state: auth_state_name(&adapter),
        })?;
    let elapsed = start.elapsed();

    let record = SessionRecord::from_identity(&identity, "user_code", unix_now_secs());
    let _session_path = record.write_to(data_dir)?;
    let config_path = data_dir.join("config.json");

    let output = OnboardOutput {
        schema_version: OnboardOutput::SCHEMA_VERSION,
        mode: OnboardMode::UserCode,
        self_id: identity.user_id,
        // R2-PROTO-14: strip control chars and look-alike
        // unicode codepoints from the username before
        // embedding it in the JSON output.
        self_username: validate_username(identity.username.clone()),
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

/// Outcome of the password-closure interactive prompt.
/// R2-IE-19: the previous version collapsed three
/// semantically distinct outcomes ("no 2FA required",
/// "operator typed a password", "input pipe died") into
/// a single `Option<String>`. The richer enum lets the
/// CLI surface distinct log messages and (eventually)
/// distinct exit codes for "the input pipeline crashed"
/// vs. "the operator deliberately skipped 2FA".
///
/// The adapter's `connect_user` API still takes
/// `Option<String>` (backward-compat); the closure
/// projects the enum back to `Option<String>` before
/// returning.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum PasswordOutcome {
    /// The password channel closed without a value
    /// arriving. The CLI uses this as the "no 2FA"
    /// signal (Enter on empty input drops the sender).
    NotNeeded,
    /// The operator typed a non-empty password and it
    /// was delivered to the adapter.
    Provided(String),
    /// The password channel closed because the
    /// `--password-timeout-secs` deadline elapsed
    /// without input. Distinct from `NotNeeded` because
    /// the operator might still have intended to provide
    /// a password — they just didn't get to it in time.
    InputClosed,
}

/// Mask all but the last 4 digits of a phone number for
/// log lines. R2-SEC-8: the previous version showed the
/// first 4 digits (country code + area code) and last 2
/// digits, leaking the area code and exposing a chunk
/// big enough to re-identify the line. NIST SP 800-122
/// (`Guide to Protecting the Confidentiality of PII`)
///
/// says masked log entries should keep only the minimum
/// context needed for debugging — and for a phone number,
/// that's the last 4 digits. The full E.164 number
/// (15 digits max) is operator-PII; leaking any prefix
/// substantially narrows the search space. The fix
/// shows the last 4 digits only, prefixed with the
/// country-code hint `+` for shape consistency.
fn mask_phone(phone: &str) -> String {
    let digits: String = phone.chars().filter(|c| c.is_ascii_digit()).collect();
    if digits.len() <= 4 {
        return "+***".to_string();
    }
    let tail = &digits[digits.len() - 4..];
    format!("+***{}", tail)
}

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

    /// R2-PROTO-12: surrounding whitespace is trimmed
    /// before validation. The CLI's `read_line` only
    /// trims trailing whitespace, so an operator who
    /// pastes a number with a leading space (e.g. from a
    /// contacts-app rendering) would otherwise see a
    /// confusing "phone must be in E.164 form" error.
    #[test]
    fn validate_phone_accepts_surrounding_whitespace() {
        validate_phone("  +15551234567  ").unwrap();
        validate_phone("\t+15551234567\n").unwrap();
        // Whitespace is fine; the digits inside must
        // still be in 8..=15 range.
        let e = validate_phone("  +1 ").unwrap_err();
        assert_eq!(e.kind(), "invalid_input");
    }

    /// R2-SEC-8: only the last 4 digits of the phone are
    /// visible in logs. The previous version leaked the
    /// first 4 (country + area code) and the last 2.
    #[test]
    fn mask_phone_hides_everything_but_last_four() {
        // US number: +1 555 123 4567 → +***4567.
        assert_eq!(mask_phone("+15551234567"), "+***4567");
        // UK number: +44 7700 900 1234 → +***1234.
        assert_eq!(mask_phone("+4477009001234"), "+***1234");
    }

    /// R2-SEC-8: a phone with 4 or fewer digits collapses
    /// to the generic `+***` token — we never expose any
    /// digit at all if the number is too short for the
    /// last-4 rule to be safe.
    #[test]
    fn mask_phone_handles_short_input() {
        assert_eq!(mask_phone("+123"), "+***");
        assert_eq!(mask_phone("+12"), "+***");
        assert_eq!(mask_phone(""), "+***");
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn run_succeeds_against_mock() {
        // The mock client accepts any phone + code, so
        // the flow should reach Ready without a real
        // Telegram server. This exercises the
        // mpsc → oneshot → FnOnce plumbing end-to-end.
        let tmp = tempdir().unwrap();
        let adapter = mock_adapter_for_test(tmp.path());
        // R2-SEC-6: channel element type is
        // `Zeroizing<String>`.
        let (code_tx, code_rx) = mpsc::channel::<Zeroizing<String>>(1);
        let (password_tx, password_rx) = mpsc::channel::<Zeroizing<String>>(1);
        let creds = UserCodeCredentials {
            phone: "+15551234567".to_string(),
        };

        // Drive the channels from a sibling task.
        let input_task = tokio::spawn(async move {
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
            code_tx
                .send(Zeroizing::new("12345".to_string()))
                .await
                .unwrap();
            // 2FA isn't required by the mock by default, so
            // the password sender is just dropped (which
            // causes the closure to see `None`).
            drop(password_tx);
        });

        // R2-OPS-12: pass 60s deadlines (the round-1
        // hardcoded values).
        let (out, _cfg_path) = run(
            adapter,
            creds,
            code_rx,
            password_rx,
            std::time::Duration::from_secs(60),
            std::time::Duration::from_secs(60),
            tmp.path(),
        )
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
        // R2-SEC-6: channel element type is
        // `Zeroizing<String>`.
        let (mpsc_tx, mpsc_rx) = mpsc::channel::<Zeroizing<String>>(1);
        let (oneshot_tx, oneshot_rx) = oneshot::channel::<String>();
        let fwd = forward_input(mpsc_rx, oneshot_tx);
        mpsc_tx
            .send(Zeroizing::new("hello".to_string()))
            .await
            .unwrap();
        drop(mpsc_tx);
        fwd.await.unwrap();
        assert_eq!(oneshot_rx.await.unwrap(), "hello");
    }
}
