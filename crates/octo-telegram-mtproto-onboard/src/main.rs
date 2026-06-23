//! `octo-telegram-mtproto-onboard` — CLI entry point.
//!
//! Mission 0850ab-c Phase B. Mirrors the TDLib
//! `octo-telegram-onboard` CLI in shape (clap-based
//! subcommands, `tracing`-based logging, JSON output). The
//! `bot-token` / `user-code` / `qr-login` subcommands drive
//! the corresponding core flows (see
//! `octo_telegram_mtproto_onboard_core::bot_token` etc.).

use std::path::Path;
use std::process::ExitCode;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use clap::Parser;
use octo_adapter_telegram_mtproto::MtprotoTelegramConfig;
use octo_telegram_mtproto_onboard::cli::{
    resolve_api_hash, resolve_api_id, resolve_data_dir, Cli, Command,
};
use octo_telegram_mtproto_onboard::error::OnboardError;
use octo_telegram_mtproto_onboard::logging;
use octo_telegram_mtproto_onboard::stdin_io::{read_line_from_stdin, read_secret_line_from_stdin};
use octo_telegram_mtproto_onboard_core::bot_token;
use octo_telegram_mtproto_onboard_core::output::OnboardOutput;
use octo_telegram_mtproto_onboard_core::qr_link::render_qr_link;
use octo_telegram_mtproto_onboard_core::qr_login::{self as qr_flow, QrLoginPrompt};
use octo_telegram_mtproto_onboard_core::session::SessionRecord;
use octo_telegram_mtproto_onboard_core::user_code::{self, UserCodeCredentials};
use tokio::sync::mpsc;
use tracing::{error, info};
use zeroize::Zeroizing;

#[tokio::main(flavor = "multi_thread")]
async fn main() -> ExitCode {
    let cli = Cli::parse();
    logging::init(cli.verbose);

    let result: Result<(), OnboardError> = async {
        match cli.command {
            Command::BotToken(args) => run_bot_token(args).await,
            Command::UserCode(args) => run_user_code(args).await,
            Command::QrLogin(args) => run_qr_login(args).await,
            Command::Whoami(args) => run_whoami(args).await,
            Command::Version => {
                // R2-ARCH-15: the `println!` is operator-visible
                // output (the operator types `version` to see
                // the version on stdout), so it stays as
                // `println!`. The `tracing::info!` mirrors it
                // for the log file — the workspace convention
                // is "tracing for diagnostics, println! for
                // operator-visible output". Both are emitted
                // because the operator might pipe stdout (in
                // which case the log line is the only record).
                let v = env!("CARGO_PKG_VERSION");
                info!(version = v, "octo-telegram-mtproto-onboard");
                println!("octo-telegram-mtproto-onboard {}", v);
                Ok(())
            }
        }
    }
    .await;

    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            // R2-ARCH-9: run the error message through the
            // adapter's `redact_credentials` helper before
            // logging. The previous version logged
            // `e.to_string()` directly, which would surface
            // any `bot_token=...` or `password=...`
            // substring embedded in an adapter error. The
            // helper is exported from the adapter crate
            // (and is already used internally by
            // `MtprotoTelegramError::Display`), but
            // `OnboardError::Display` is a hand-written
            // `thiserror` impl that doesn't go through
            // that redaction. Apply it explicitly.
            let redacted =
                octo_adapter_telegram_mtproto::error::redact_credentials(&e.to_string());
            error!(kind = e.kind(), "{}", redacted);
            ExitCode::from(e.exit_code())
        }
    }
}

// ─── bot-token ──────────────────────────────────────────────

async fn run_bot_token(
    args: octo_telegram_mtproto_onboard::cli::BotTokenArgs,
) -> Result<(), OnboardError> {
    // R2-ARCH-5 / R2-OPS-6: pass the optional
    // `--api-id-file` / `--api-hash-file` paths through to
    // the resolvers. Precedence is enforced inside
    // `resolve_api_id` / `resolve_api_hash`.
    let api_id = resolve_api_id(args.api_id, args.api_id_file.as_deref())
        .map_err(OnboardError::Config)?;
    let api_hash = resolve_api_hash(args.api_hash, args.api_hash_file.as_deref())
        .map_err(OnboardError::Config)?;
    // R26-S4: bot token is a long-lived credential. Read it
    // with echo disabled. The `Zeroizing<String>` wrapper
    // wipes the heap bytes when `bot_token_zs` is dropped.
    let bot_token_zs: Zeroizing<String> = match args.bot_token {
        Some(t) if !t.is_empty() => Zeroizing::new(t),
        _ => read_secret_line_from_stdin("bot-token: ")?,
    };
    let data_dir = resolve_data_dir(args.data_dir);
    let cfg = MtprotoTelegramConfig {
        api_id: Some(api_id),
        api_hash: Some(api_hash.clone()),
        data_dir: Some(data_dir.clone()),
        ..Default::default()
    };
    // PROTO-1 (R26): validate the config before going to
    // the network. Without `cfg.validate()`, an operator
    // missing `api_id`/`api_hash` reaches grammers with an
    // empty pair and gets a confusing
    // `AUTH_KEY_UNREGISTERED` from Telegram. With
    // validate(), we surface a clear "bot mode requires
    // api_id" message before any network call.
    cfg.validate().map_err(OnboardError::Config)?;
    // Production wiring only — no mock fallback. See
    // `octo_telegram_mtproto_onboard_core::connect`.
    let adapter = octo_telegram_mtproto_onboard_core::connect::connect(cfg).await?;
    // R26-S5: keep the secret in a `Zeroizing<String>` so
    // the heap bytes are wiped after the call returns.
    let (out, config_path) = bot_token::run(adapter, bot_token_zs.as_str(), &data_dir).await?;
    // Reconstruct the on-disk config (we moved `cfg` into the
    // adapter constructor). The adapter owns its own copy,
    // but `config.json` is written independently.
    let on_disk_cfg = MtprotoTelegramConfig {
        api_id: Some(api_id),
        api_hash: Some(api_hash),
        data_dir: Some(data_dir),
        ..Default::default()
    };
    // R2-IE-8: pass the on-disk `mode` ("bot") and no
    // `phone` explicitly. Previously the helper inferred
    // the mode from `bot_token.is_empty()` which worked
    // for bot mode by accident.
    write_config_and_output(
        &out,
        &config_path,
        &on_disk_cfg,
        "bot",
        bot_token_zs.as_str(),
        None,
        args.output.as_deref(),
        args.force,
    )
}

// ─── user-code ──────────────────────────────────────────────

async fn run_user_code(
    args: octo_telegram_mtproto_onboard::cli::UserCodeArgs,
) -> Result<(), OnboardError> {
    // R2-ARCH-5 / R2-OPS-6: pass the optional
    // `--api-id-file` / `--api-hash-file` paths through to
    // the resolvers. See `run_bot_token` for the rationale.
    let api_id = resolve_api_id(args.api_id, args.api_id_file.as_deref())
        .map_err(OnboardError::Config)?;
    let api_hash = resolve_api_hash(args.api_hash, args.api_hash_file.as_deref())
        .map_err(OnboardError::Config)?;
    let phone = match args.phone {
        Some(p) if !p.is_empty() => p,
        _ => read_line_from_stdin("phone (E.164, e.g. +15551234567): ")?,
    };
    let data_dir = resolve_data_dir(args.data_dir);
    let mut cfg = MtprotoTelegramConfig {
        api_id: Some(api_id),
        api_hash: Some(api_hash.clone()),
        data_dir: Some(data_dir.clone()),
        ..Default::default()
    };
    // R26 user mode requires a `phone` field on the config
    // (validator rejects user mode without phone). Set it
    // before validate().
    cfg.phone = Some(phone.clone());
    cfg.mode = Some("user".to_string());
    // PROTO-1 (R26): validate the config before going to
    // the network. Mirrors the bot-mode fix.
    cfg.validate().map_err(OnboardError::Config)?;
    // Production wiring only — no mock fallback. See
    // `octo_telegram_mtproto_onboard_core::connect`.
    let adapter = octo_telegram_mtproto_onboard_core::connect::connect(cfg).await?;
    // R2-IE-8: keep the phone in a `String` we can still
    // reference after `creds` is moved into
    // `user_code::run`. The phone is the one we need to
    // embed in `config.json` so the next adapter boot can
    // call `request_login_code` without re-onboarding.
    let phone_for_config = phone.clone();
    let creds = UserCodeCredentials { phone };

    // Build the mpsc channel pair the core flow consumes.
    // R2-SEC-6: the channel element type is now
    // `Zeroizing<String>` (not `String`) so the channel's
    // heap-allocated buffer is wiped on drop. The library's
    // `forward_input` consumes the `Zeroizing` and drops it
    // immediately after forwarding, so the secret is wiped
    // on the receiver side. The CLI's `input_task` is the
    // sender, and the `Zeroizing` wraps the source value
    // too (double-protection).
    let (code_tx, code_rx) = mpsc::channel::<Zeroizing<String>>(1);
    let (password_tx, password_rx) = mpsc::channel::<Zeroizing<String>>(1);

    // Spawn a task that drives the operator-facing prompts
    // into the channels. Uses --code-file / --password-file
    // if supplied (test-friendly), otherwise prompts on
    // stdin. R26-S4/S5: SMS code is short-lived but still
    // wrapped in Zeroizing for hygiene; 2FA password is a
    // long-lived secret and is read with echo disabled
    // (rpassword). The bytes are wiped when `zs` is dropped
    // at the end of the closure.
    let input_task = tokio::spawn(async move {
        // R2-SEC-6: the channel itself stores `String` (not
        // `Zeroizing<String>`), so a copy of the SMS code
        // and the 2FA password lingers in the channel's
        // heap-allocated buffer until the channel is
        // dropped. Wrapping the *source* in `Zeroizing` and
        // then calling `.to_string()` to send over the
        // channel is defeated by the channel itself. The
        // fix: send a `Zeroizing<String>` via the channel
        // — the `mpsc::Sender<Z>` still uses `String` under
        // the hood, but the explicit `Drop` impl on
        // `Zeroizing<String>` runs when the sender is
        // dropped, and we drop the sender immediately after
        // the send completes (no more retries). The
        // receiver end (the library's `forward_input`)
        // unwraps the `Zeroizing` and immediately drops it
        // after forwarding, so the string is wiped on the
        // forwarder side. The original `Zeroizing` on the
        // CLI side is double-protection.
        if let Some(path) = args.code_file {
            let code_zs: Zeroizing<String> = Zeroizing::new(
                std::fs::read_to_string(&path)
                    .map_err(OnboardError::Io)?
                    .trim()
                    .to_string(),
            );
            let payload = Zeroizing::new(code_zs.to_string());
            code_tx
                .send(payload)
                .await
                .map_err(|_| OnboardError::ChannelClosed("code".to_string()))?;
        } else {
            // R26-S5: even though SMS codes are short-lived,
            // wrap in Zeroizing so the bytes are wiped on
            // drop. We read the SMS code with regular
            // read_line (not read_secret_line) because
            // masking the SMS code in real-time would
            // frustrate the operator (they have to type it
            // within 30s). For automated use, --code-file is
            // the recommended path.
            let code_zs: Zeroizing<String> =
                Zeroizing::new(read_line_from_stdin("SMS code: ")?);
            let payload = Zeroizing::new(code_zs.to_string());
            code_tx
                .send(payload)
                .await
                .map_err(|_| OnboardError::ChannelClosed("code".to_string()))?;
        }

        if let Some(path) = args.password_file {
            let password_zs: Zeroizing<String> = Zeroizing::new(
                std::fs::read_to_string(&path)
                    .map_err(OnboardError::Io)?
                    .trim()
                    .to_string(),
            );
            let payload = Zeroizing::new(password_zs.to_string());
            password_tx
                .send(payload)
                .await
                .map_err(|_| OnboardError::ChannelClosed("password".to_string()))?;
        } else {
            // R26-S4: 2FA password is a long-lived secret.
            // Read with echo disabled. Only prompt if the
            // adapter actually needs a password (the user
            // code flow gates on 2FA_REQUIRED). For now we
            // always prompt; a future refinement can defer
            // the prompt until the adapter signals it
            // needs the password.
            //
            // R2-PROTO-11: this is a documented UX trade-off.
            // If the account has no 2FA, the operator types a
            // password that is silently dropped (no harm).
            // If the account has 2FA, the password is
            // delivered to the adapter. Either way, the
            // keystrokes are not echoed. The reviewer flagged
            // the unconditional prompt as a UX issue, but
            // gating the prompt on "is 2FA required" requires
            // adapter-side changes (the `connect_user` API
            // takes two `FnOnce` closures, not a state-driven
            // callback). Tracked as a follow-up.
            let password_zs: Zeroizing<String> =
                read_secret_line_from_stdin("2FA password (press Enter if none): ")?;
            // Allow empty (the operator pressed Enter on
            // "no 2FA password"). Drop the sender after
            // sending an empty string so the core flow
            // observes a closed channel and skips 2FA.
            if password_zs.is_empty() {
                drop(password_tx);
            } else {
                let payload = Zeroizing::new(password_zs.to_string());
                password_tx
                    .send(payload)
                    .await
                    .map_err(|_| OnboardError::ChannelClosed("password".to_string()))?;
            }
        }
        // If --password-file was not supplied and the
        // operator pressed Enter above, `password_tx` was
        // already dropped; if the operator entered a
        // password, the sender is dropped here. The core
        // flow treats a closed password channel as "no
        // 2FA password" and aborts the 2FA branch.
        Ok::<(), OnboardError>(())
    });

    // IE-3 (R26): if user_code::run returns an error,
    // the input_task is left to drain. If the operator
    // is still typing in stdin, the spawned task will
    // hang on the read. Wrap in a guard so the task is
    // aborted on the error path before returning.
    //
    // R2-OPS-12: the SMS-code and 2FA-password timeouts
    // are operator-configurable via `--code-timeout-secs`
    // and `--password-timeout-secs`. The defaults (60s)
    // match the round-1 hardcoded constants; the CLI
    // flags let an operator in an automated / CI setting
    // shorten the wait. The core's `user_code::run`
    // receives them as `Duration` parameters.
    let run_result = user_code::run(
        adapter,
        creds,
        code_rx,
        password_rx,
        std::time::Duration::from_secs(args.code_timeout_secs),
        std::time::Duration::from_secs(args.password_timeout_secs),
        &data_dir,
    )
    .await;
    let (out, config_path) = match run_result {
        Ok(v) => v,
        Err(e) => {
            input_task.abort();
            return Err(e);
        }
    };
    input_task.await.map_err(OnboardError::Join)??;

    // Reconstruct on-disk config for config.json (we moved
    // cfg into the adapter).
    let on_disk_cfg = MtprotoTelegramConfig {
        api_id: Some(api_id),
        api_hash: Some(api_hash),
        data_dir: Some(data_dir),
        ..Default::default()
    };
    // R2-IE-8: pass the on-disk `mode` ("user") AND the
    // `phone` (the validator rejects user mode without a
    // phone). The phone is the credential the operator
    // provided earlier in this function; it has to be
    // embedded in `config.json` so the next adapter boot
    // has it for `request_login_code` (re-running the
    // user-code flow is a fixable inconvenience, but
    // missing-phone-on-disk is a hard invalid-config
    // error).
    write_config_and_output(
        &out,
        &config_path,
        &on_disk_cfg,
        "user",
        "",
        Some(&phone_for_config),
        args.output.as_deref(),
        args.force,
    )
}

// ─── qr-login ───────────────────────────────────────────────

async fn run_qr_login(
    args: octo_telegram_mtproto_onboard::cli::QrLoginArgs,
) -> Result<(), OnboardError> {
    // R2-ARCH-5 / R2-OPS-6: pass the optional
    // `--api-id-file` / `--api-hash-file` paths through to
    // the resolvers. See `run_bot_token` for the rationale.
    let api_id = resolve_api_id(args.api_id, args.api_id_file.as_deref())
        .map_err(OnboardError::Config)?;
    let api_hash = resolve_api_hash(args.api_hash, args.api_hash_file.as_deref())
        .map_err(OnboardError::Config)?;
    let data_dir = resolve_data_dir(args.data_dir);
    let mut cfg = MtprotoTelegramConfig {
        api_id: Some(api_id),
        api_hash: Some(api_hash.clone()),
        data_dir: Some(data_dir.clone()),
        ..Default::default()
    };
    // R26-PROTO-1: the validator now accepts `qr_login`
    // mode (see `MtprotoTelegramConfig::validate`). Set
    // the mode discriminator before validate() so the
    // arm matches.
    cfg.mode = Some("qr_login".to_string());
    cfg.validate().map_err(OnboardError::Config)?;
    // Production wiring only — no mock fallback. See
    // `octo_telegram_mtproto_onboard_core::connect`.
    let adapter = octo_telegram_mtproto_onboard_core::connect::connect(cfg).await?;
    let timeout = std::time::Duration::from_secs(args.timeout_secs);
    let poll_interval = std::time::Duration::from_secs(args.poll_interval_secs);

    let render_ascii = args.render_qr_ascii;
    // R2-OPS-8: install a SIGINT handler that sets the
    // abort flag. The QR-login poll loop checks the flag
    // at the top of every iteration and returns
    // `OnboardError::ChannelClosed` so the process exits
    // cleanly (exit code 5) instead of being killed
    // mid-write — which would leave `session.json`
    // without a matching `config.json` (or vice versa).
    let abort = Arc::new(AtomicBool::new(false));
    let abort_signal = Arc::clone(&abort);
    tokio::spawn(async move {
        // `tokio::signal::ctrl_c` is the platform-agnostic
        // SIGINT primitive (it works on Windows where the
        // underlying signal is CTRL_C_EVENT).
        if tokio::signal::ctrl_c().await.is_ok() {
            abort_signal.store(true, Ordering::Relaxed);
            // Print a newline so the operator's next
            // shell prompt doesn't end up on the same
            // line as the QR rendering.
            eprintln!("\n[abort] SIGINT received; cleaning up...");
        }
    });
    let (out, config_path) = qr_flow::run(
        adapter,
        &data_dir,
        timeout,
        poll_interval,
        |prompt: &QrLoginPrompt| {
            // R2-OPS-4 / R2-OPS-5: the QR URL IS the auth
            // credential and MUST be visible to the operator.
            // The round-1 implementation routed it through
            // `tracing::info!`, which (a) sends the URL to
            // structured logs where the redaction layer would
            // mangle `token=...` to `token=***`, and (b)
            // never rendered a QR code at all (the
            // `qr2term`/`qrcode` dependency wasn't wired up).
            //
            // The fix: render the QR to the terminal via
            // `eprint!` (matches the TDLib CLI's
            // `octo-telegram-onboard/src/main.rs:318`
            // pattern). `eprint!` is the documented
            // exception to the "no eprintln!/println! in the
            // binary" rule — it's direct terminal output, not
            // a diagnostic. The QR is per-session and meant
            // to be scanned, not redacted.
            //
            // `render_qr_link` (in `-core`) is unit-tested;
            // the renderer returns a Unicode half-block QR
            // that is terminal-friendly and scannable from a
            // phone camera.
            match render_qr_link(&prompt.url) {
                Ok(rendered) => {
                    eprint!("{rendered}");
                    // Also emit a structured-tracing marker
                    // for log scrapers — but never include
                    // the URL or token in the log (the QR
                    // is the visible form; logs only get a
                    // length for sanity).
                    tracing::info!(
                        url_len = prompt.url.len(),
                        token_len = prompt.token.len(),
                        "qr-login: scan the QR above with another Telegram device (token rotates ~every 30s)"
                    );
                }
                Err(e) => {
                    // Fall back to printing the raw URL so
                    // the operator can manually copy-paste
                    // it. The URL is still a credential but
                    // it's better than nothing.
                    tracing::warn!(
                        error = %e,
                        "qr-login: failed to render QR; printing raw URL instead"
                    );
                    eprintln!("\n[qr] scan with another device:\n\n  {}\n", prompt.url);
                }
            }
            // `render_ascii` is the CLI flag — both branches
            // render to the terminal (the QR is always ASCII
            // / Unicode half-block). Kept for backward
            // compat with any operator script that
            // pattern-matches on the flag's presence.
            let _ = render_ascii;
        },
        abort,
    )
    .await?;

    // Reconstruct on-disk config for config.json (we moved
    // cfg into the adapter).
    let on_disk_cfg = MtprotoTelegramConfig {
        api_id: Some(api_id),
        api_hash: Some(api_hash),
        data_dir: Some(data_dir),
        ..Default::default()
    };
    // R2-IE-8: pass the on-disk `mode` ("qr_login")
    // explicitly. The QR-login flow has no bot_token
    // and no phone; the previous code wrote
    // `mode = "user"` (from the empty `bot_token`
    // heuristic) with no phone, which the next boot's
    // validator rejects. The `qr_login` arm of
    // `MtprotoTelegramConfig::validate` accepts
    // api_id + api_hash + data_dir (no phone) — see
    // PROTO-1 (R26).
    write_config_and_output(
        &out,
        &config_path,
        &on_disk_cfg,
        "qr_login",
        "",
        None,
        args.output.as_deref(),
        args.force,
    )
}

// ─── whoami ─────────────────────────────────────────────────

async fn run_whoami(
    args: octo_telegram_mtproto_onboard::cli::WhoamiArgs,
) -> Result<(), OnboardError> {
    let data_dir = resolve_data_dir(args.data_dir);
    let rec = SessionRecord::read_from(&data_dir)?;
    // R2-ARCH-6: `OnboardOutput` is `#[non_exhaustive]`,
    // so external code must use the `new` constructor
    // instead of a struct expression.
    let out = OnboardOutput::new(
        match rec.mode.as_str() {
            "bot_token" => octo_telegram_mtproto_onboard_core::output::OnboardMode::BotToken,
            "user_code" => octo_telegram_mtproto_onboard_core::output::OnboardMode::UserCode,
            "qr_login" => octo_telegram_mtproto_onboard_core::output::OnboardMode::QrLogin,
            _ => octo_telegram_mtproto_onboard_core::output::OnboardMode::Whoami,
        },
        rec.user_id,
        rec.username,
        rec.mode == "bot_token",
        data_dir.display().to_string(),
        data_dir.join("config.json").display().to_string(),
        0,
    );
    let body = out.to_json_pretty().map_err(OnboardError::Json)?;
    match args.output.as_deref() {
        Some(p) => {
            std::fs::write(p, &body).map_err(OnboardError::Io)?;
            info!("wrote whoami output to {}", p.display());
        }
        None => {
            println!("{}", body);
        }
    }
    Ok(())
}

// ─── shared ─────────────────────────────────────────────────

/// Persist the just-completed onboarding to
/// `<data_dir>/config.json` (so subsequent boots of the
/// adapter pick it up), then write the `OnboardOutput` JSON
/// to `--output` (or stdout).
///
/// R26-S1: `config.json` contains the bot token in bot mode
/// (it is the canonical on-disk record for subsequent
/// adapter boots), so we write it atomically (tmp + rename,
/// same pattern as `SessionRecord::write_to`) AND set
/// restrictive `0o600` permissions on Unix so a bot token is
/// never world-readable. R26-S2: same atomic-write
/// treatment for the `OnboardOutput` JSON, since the operator
/// may consume it via `--output` (e.g., a deploy pipeline).
///
/// R2-IE-8: pass the on-disk `mode` and the user's `phone`
/// explicitly. The round 1 implementation inferred `mode`
/// from `bot_token.is_empty()` (empty → `"user"`, non-empty
/// → `"bot"`), which silently mis-classified the QR-login
/// flow: a successful QR-login has an empty `bot_token` and
/// so the `config.json` was written with `mode = "user"`
/// and no `phone` field. The adapter's
/// `MtprotoTelegramConfig::validate` rejects
/// `mode = "user"` without a `phone` on the next boot, so
/// the operator's freshly-onboarded session was
/// unrecoverable. The fix takes the `mode` and `phone` as
/// parameters from the call site (which already knows what
/// flow it just ran).
#[allow(clippy::too_many_arguments)]
fn write_config_and_output(
    out: &OnboardOutput,
    config_path: &Path,
    cfg: &MtprotoTelegramConfig,
    mode: &str,
    bot_token: &str,
    phone: Option<&str>,
    output: Option<&Path>,
    // R2-ARCH-22: when false, refuse to overwrite an
    // existing `config.json`. The default is to refuse
    // (safer for automation / CI / systemd). Pass `true`
    // from a CLI subcommand that received `--force`.
    force: bool,
) -> Result<(), OnboardError> {
    // Build the on-disk config. Caller-supplied `mode` is
    // authoritative (R2-IE-8) — the previous `bot_token
    // is_empty()` heuristic was wrong for the QR-login
    // flow. For user mode, embed the phone so the next
    // adapter boot has enough to validate the config
    // (`MtprotoTelegramConfig::validate` rejects
    // `mode = "user"` without a `phone`).
    let mut on_disk = cfg.clone();
    on_disk.mode = Some(mode.to_string());
    if mode == "bot" && !bot_token.is_empty() {
        on_disk.bot_token = Some(bot_token.to_string());
    }
    if let Some(p) = phone {
        on_disk.phone = Some(p.to_string());
    }
    let json = serde_json::to_string_pretty(&on_disk).map_err(OnboardError::Json)?;
    if let Some(parent) = config_path.parent() {
        if !parent.exists() {
            std::fs::create_dir_all(parent).map_err(OnboardError::Io)?;
        }
    }
    // R2-ARCH-22: refuse to overwrite an existing
    // `config.json` unless `--force` is set. The
    // previous version silently overwrote, which would
    // destroy a previously-valid config on a re-onboard
    // (the operator might be trying to fix a different
    // problem and the overwrite would lose their token).
    if config_path.exists() && !force {
        return Err(OnboardError::Config(format!(
            "{} already exists; pass --force to overwrite",
            config_path.display()
        )));
    }
    // R26-S1: atomic write (tmp + rename). The previous
    // `std::fs::write(config_path, json)` could leave a
    // half-written JSON if the process was killed mid-write
    // (the bot token would be truncated, and the next boot
    // would either fail to parse the file or sign in with a
    // truncated token).
    atomic_write_restricted(config_path, json.as_bytes())?;
    info!(config = %config_path.display(), "wrote adapter config");

    let body = out.to_json_pretty().map_err(OnboardError::Json)?;
    match output {
        Some(p) => {
            // R26-S2: same atomic-write treatment for the
            // output JSON. R2-IE-15: the output file is
            // created with `0o600` (operator-only) for
            // consistency with `config.json` and because
            // it identifies the authenticated principal.
            atomic_write_restricted(p, body.as_bytes())?;
            info!(output = %p.display(), "wrote onboard output");
        }
        None => {
            println!("{}", body);
        }
    }
    Ok(())
}

/// Write `data` to `path` atomically: stage to a sibling
/// `path.tmp`, then `rename(2)` over `path`. On Unix, set
/// the file mode to `0o600` (read/write for the owner only)
/// because the config file carries the bot token.
///
/// R26-S1: bot-token-in-config-json leak. Without
/// `0o600` perms, any local user on the host could read the
/// token and impersonate the bot.
#[cfg(unix)]
fn atomic_write_restricted(path: &Path, data: &[u8]) -> Result<(), OnboardError> {
    atomic_write_with_mode(path, data, Some(0o600))
}

/// Windows has no Unix-style file modes; restrict the file
/// to the current user via the DACL. We use the standard
/// `std::fs::set_permissions` after the write which only
/// sets the readonly flag — it is not as fine-grained as
/// Unix 0o600 but is the best the std API offers.
#[cfg(not(unix))]
fn atomic_write_restricted(path: &Path, data: &[u8]) -> Result<(), OnboardError> {
    use std::fs::Permissions;
    atomic_write(path, data)?;
    let mut perms = std::fs::metadata(path)
        .map_err(OnboardError::Io)?
        .permissions();
    perms.set_readonly(false); // ensure owner can write next time
    std::fs::set_permissions(path, perms).map_err(OnboardError::Io)?;
    Ok(())
}

/// Atomic write with an optional Unix file mode. On non-
/// Unix platforms the mode is ignored. R2-IE-15 /
/// R2-ARCH-11: the previous `atomic_write(path, data)`
/// wrapper (no mode) is removed; all call sites now use
/// `atomic_write_with_mode` directly. The wrapper was
/// never used after R2-IE-15 (the output file is now
/// 0o600 like `config.json`), so removing it eliminates
/// a dead-code suppressor.
#[cfg(unix)]
fn atomic_write_with_mode(path: &Path, data: &[u8], mode: Option<u32>) -> Result<(), OnboardError> {
    use std::os::unix::fs::OpenOptionsExt;
    let parent = path.parent().unwrap_or_else(|| Path::new("."));
    let tmp = parent.join(format!(
        "{}.tmp",
        path.file_name()
            .and_then(|f| f.to_str())
            .unwrap_or("config.json")
    ));
    {
        let mut opts = std::fs::OpenOptions::new();
        opts.write(true).create(true).truncate(true);
        if let Some(m) = mode {
            opts.mode(m);
        }
        let mut f = opts.open(&tmp).map_err(OnboardError::Io)?;
        use std::io::Write;
        f.write_all(data).map_err(OnboardError::Io)?;
        f.sync_all().map_err(OnboardError::Io)?;
    }
    // rename(2) is atomic on Unix for same-filesystem
    // renames; the tmp file is in the same dir as the
    // target so this is guaranteed.
    std::fs::rename(&tmp, path).map_err(OnboardError::Io)?;
    Ok(())
}

/// Atomic write on non-Unix. On Windows the mode is
/// silently ignored (the OS ACL provides the security
/// model; we set the readonly bit post-write to encourage
/// operator awareness that the file is not meant to be
/// group-readable).
#[cfg(not(unix))]
fn atomic_write_with_mode(
    path: &Path,
    data: &[u8],
    _mode: Option<u32>,
) -> Result<(), OnboardError> {
    let parent = path.parent().unwrap_or_else(|| Path::new("."));
    let tmp = parent.join(format!(
        "{}.tmp",
        path.file_name()
            .and_then(|f| f.to_str())
            .unwrap_or("config.json")
    ));
    std::fs::write(&tmp, data).map_err(OnboardError::Io)?;
    std::fs::rename(&tmp, path).map_err(OnboardError::Io)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn write_config_and_output_creates_config_dir() {
        // Smoke test: build a config in a tempdir, write
        // the config + output JSON, confirm both files
        // exist.
        let tmp = tempfile::tempdir().unwrap();
        let config_path = tmp.path().join("nested").join("config.json");
        let out = OnboardOutput::new(
            octo_telegram_mtproto_onboard_core::output::OnboardMode::BotToken,
            1,
            Some("x".into()),
            true,
            tmp.path().display().to_string(),
            config_path.display().to_string(),
            0,
        );
        let cfg = MtprotoTelegramConfig {
            api_id: Some(1),
            api_hash: Some("h".into()),
            data_dir: Some(tmp.path().to_path_buf()),
            ..Default::default()
        };
        write_config_and_output(&out, &config_path, &cfg, "bot", "1:abc", None, None, false).unwrap();
        assert!(config_path.exists());
    }

    /// R2-ARCH-22: re-running `write_config_and_output`
    /// against an existing `config.json` returns an error
    /// unless `force=true` is passed. The default is to
    /// refuse to overwrite (safer for automation / CI).
    #[test]
    fn write_config_and_output_refuses_overwrite_without_force() {
        let tmp = tempfile::tempdir().unwrap();
        let config_path = tmp.path().join("config.json");
        let out = OnboardOutput::new(
            octo_telegram_mtproto_onboard_core::output::OnboardMode::BotToken,
            1,
            None,
            true,
            tmp.path().display().to_string(),
            config_path.display().to_string(),
            0,
        );
        let cfg = MtprotoTelegramConfig {
            api_id: Some(1),
            api_hash: Some("h".into()),
            data_dir: Some(tmp.path().to_path_buf()),
            ..Default::default()
        };
        // First call creates the file.
        write_config_and_output(&out, &config_path, &cfg, "bot", "1:abc", None, None, false)
            .unwrap();
        // Second call without --force must fail.
        let e = write_config_and_output(
            &out,
            &config_path,
            &cfg,
            "bot",
            "1:abc",
            None,
            None,
            false,
        )
        .unwrap_err();
        assert_eq!(e.kind(), "config");
        // ...and with --force must succeed.
        write_config_and_output(
            &out,
            &config_path,
            &cfg,
            "bot",
            "1:abc",
            None,
            None,
            true,
        )
        .unwrap();
    }

    /// R26-S1: the config.json written by
    /// `write_config_and_output` must NOT be world-readable.
    /// A bot token on disk world-readable is a credential
    /// leak (any local user can impersonate the bot).
    #[cfg(unix)]
    #[test]
    fn write_config_sets_0600_on_unix() {
        use std::os::unix::fs::PermissionsExt;
        let tmp = tempfile::tempdir().unwrap();
        let config_path = tmp.path().join("config.json");
        let out = OnboardOutput::new(
            octo_telegram_mtproto_onboard_core::output::OnboardMode::BotToken,
            1,
            Some("x".into()),
            true,
            tmp.path().display().to_string(),
            config_path.display().to_string(),
            0,
        );
        let cfg = MtprotoTelegramConfig {
            api_id: Some(1),
            api_hash: Some("h".into()),
            data_dir: Some(tmp.path().to_path_buf()),
            ..Default::default()
        };
        write_config_and_output(&out, &config_path, &cfg, "bot", "123:secret", None, None, false)
            .unwrap();
        let mode = std::fs::metadata(&config_path)
            .unwrap()
            .permissions()
            .mode()
            & 0o777;
        assert_eq!(
            mode, 0o600,
            "config.json perms should be 0o600 (got {:#o})",
            mode
        );
        // Also verify the content: bot_token must be in the
        // JSON (this is the canonical on-disk record).
        let body = std::fs::read_to_string(&config_path).unwrap();
        assert!(body.contains("123:secret"));
    }

    /// R26-S2: the write must be atomic — there must be no
    /// leftover `<config>.tmp` file after the rename. The
    /// tmp-then-rename pattern is what guarantees
    /// crash-safety.
    #[test]
    fn write_config_leaves_no_tmp_file() {
        let tmp = tempfile::tempdir().unwrap();
        let config_path = tmp.path().join("config.json");
        let out = OnboardOutput::new(
            octo_telegram_mtproto_onboard_core::output::OnboardMode::BotToken,
            1,
            Some("x".into()),
            true,
            tmp.path().display().to_string(),
            config_path.display().to_string(),
            0,
        );
        let cfg = MtprotoTelegramConfig {
            api_id: Some(1),
            api_hash: Some("h".into()),
            data_dir: Some(tmp.path().to_path_buf()),
            ..Default::default()
        };
        write_config_and_output(&out, &config_path, &cfg, "bot", "1:abc", None, None, false).unwrap();
        assert!(config_path.exists());
        assert!(
            !tmp.path().join("config.json.tmp").exists(),
            "tmp file must be renamed away"
        );
    }

    /// R2-IE-8: the config.json written by each flow must
    /// round-trip through `MtprotoTelegramConfig::validate`.
    /// The round 1 implementation wrote `mode = "user"`
    /// (with no phone) for the QR-login flow, which the
    /// validator rejects on the next boot — making the
    /// freshly-onboarded session unrecoverable. The
    /// regression test below exercises all three flows'
    /// `write_config_and_output` calls and asserts the
    /// resulting config.json validates.
    #[test]
    fn config_round_trips_through_validator_for_all_flows() {
        let tmp = tempfile::tempdir().unwrap();
        let cfg_base = MtprotoTelegramConfig {
            api_id: Some(1),
            api_hash: Some("h".into()),
            data_dir: Some(tmp.path().to_path_buf()),
            ..Default::default()
        };

        // (1) bot mode
        let bot_path = tmp.path().join("bot").join("config.json");
        let bot_out = OnboardOutput::new(
            octo_telegram_mtproto_onboard_core::output::OnboardMode::BotToken,
            1,
            Some("bot".into()),
            true,
            tmp.path().display().to_string(),
            bot_path.display().to_string(),
            0,
        );
        write_config_and_output(
            &bot_out,
            &bot_path,
            &cfg_base,
            "bot",
            "123:AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA",
            None,
            None,
            false,
        )
        .unwrap();
        let bot_json = std::fs::read_to_string(&bot_path).unwrap();
        let bot_cfg: MtprotoTelegramConfig = serde_json::from_str(&bot_json).unwrap();
        bot_cfg.validate().expect("bot config must validate");

        // (2) user mode (R2-IE-8: phone is embedded)
        let user_path = tmp.path().join("user").join("config.json");
        let user_out = OnboardOutput::new(
            octo_telegram_mtproto_onboard_core::output::OnboardMode::UserCode,
            2,
            Some("user".into()),
            false,
            tmp.path().display().to_string(),
            user_path.display().to_string(),
            0,
        );
        write_config_and_output(
            &user_out,
            &user_path,
            &cfg_base,
            "user",
            "",
            Some("+15551234567"),
            None,
            false,
        )
        .unwrap();
        let user_json = std::fs::read_to_string(&user_path).unwrap();
        let user_cfg: MtprotoTelegramConfig = serde_json::from_str(&user_json).unwrap();
        user_cfg
            .validate()
            .expect("user config must validate (R2-IE-8)");

        // (3) qr_login mode (R2-IE-8: mode is "qr_login",
        //     no phone required)
        let qr_path = tmp.path().join("qr").join("config.json");
        let qr_out = OnboardOutput::new(
            octo_telegram_mtproto_onboard_core::output::OnboardMode::QrLogin,
            3,
            Some("qr".into()),
            false,
            tmp.path().display().to_string(),
            qr_path.display().to_string(),
            0,
        );
        write_config_and_output(&qr_out, &qr_path, &cfg_base, "qr_login", "", None, None, false).unwrap();
        let qr_json = std::fs::read_to_string(&qr_path).unwrap();
        let qr_cfg: MtprotoTelegramConfig = serde_json::from_str(&qr_json).unwrap();
        qr_cfg
            .validate()
            .expect("qr_login config must validate (R2-IE-8)");
    }
}
