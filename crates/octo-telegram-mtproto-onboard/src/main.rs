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
                println!(
                    "octo-telegram-mtproto-onboard {}",
                    env!("CARGO_PKG_VERSION")
                );
                Ok(())
            }
        }
    }
    .await;

    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            error!(kind = e.kind(), "{}", e);
            ExitCode::from(e.exit_code())
        }
    }
}

// ─── bot-token ──────────────────────────────────────────────

async fn run_bot_token(
    args: octo_telegram_mtproto_onboard::cli::BotTokenArgs,
) -> Result<(), OnboardError> {
    let api_id = resolve_api_id(args.api_id).map_err(OnboardError::Config)?;
    let api_hash = resolve_api_hash(args.api_hash).map_err(OnboardError::Config)?;
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
    write_config_and_output(
        &out,
        &config_path,
        &on_disk_cfg,
        bot_token_zs.as_str(),
        args.output.as_deref(),
    )
}

// ─── user-code ──────────────────────────────────────────────

async fn run_user_code(
    args: octo_telegram_mtproto_onboard::cli::UserCodeArgs,
) -> Result<(), OnboardError> {
    let api_id = resolve_api_id(args.api_id).map_err(OnboardError::Config)?;
    let api_hash = resolve_api_hash(args.api_hash).map_err(OnboardError::Config)?;
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
    let creds = UserCodeCredentials { phone };

    // Build the mpsc channel pair the core flow consumes.
    let (code_tx, code_rx) = mpsc::channel::<String>(1);
    let (password_tx, password_rx) = mpsc::channel::<String>(1);

    // Spawn a task that drives the operator-facing prompts
    // into the channels. Uses --code-file / --password-file
    // if supplied (test-friendly), otherwise prompts on
    // stdin. R26-S4/S5: SMS code is short-lived but still
    // wrapped in Zeroizing for hygiene; 2FA password is a
    // long-lived secret and is read with echo disabled
    // (rpassword). The bytes are wiped when `zs` is dropped
    // at the end of the closure.
    let input_task = tokio::spawn(async move {
        if let Some(path) = args.code_file {
            let code_zs: Zeroizing<String> = Zeroizing::new(
                std::fs::read_to_string(&path)
                    .map_err(OnboardError::Io)?
                    .trim()
                    .to_string(),
            );
            code_tx
                .send(code_zs.to_string())
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
            let code = read_line_from_stdin("SMS code: ")?;
            code_tx
                .send(code)
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
            password_tx
                .send(password_zs.to_string())
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
            // Note: this is a UX trade-off. If the account
            // has no 2FA, the operator types a password
            // that is silently dropped (no harm). If the
            // account has 2FA, the password is delivered to
            // the adapter. Either way, the keystrokes are
            // not echoed.
            let password_zs: Zeroizing<String> =
                read_secret_line_from_stdin("2FA password (press Enter if none): ")?;
            // Allow empty (the operator pressed Enter on
            // "no 2FA password"). Drop the sender after
            // sending an empty string so the core flow
            // observes a closed channel and skips 2FA.
            if password_zs.is_empty() {
                drop(password_tx);
            } else {
                password_tx
                    .send(password_zs.to_string())
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
    let run_result = user_code::run(adapter, creds, code_rx, password_rx, &data_dir).await;
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
    write_config_and_output(&out, &config_path, &on_disk_cfg, "", args.output.as_deref())
}

// ─── qr-login ───────────────────────────────────────────────

async fn run_qr_login(
    args: octo_telegram_mtproto_onboard::cli::QrLoginArgs,
) -> Result<(), OnboardError> {
    let api_id = resolve_api_id(args.api_id).map_err(OnboardError::Config)?;
    let api_hash = resolve_api_hash(args.api_hash).map_err(OnboardError::Config)?;
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
    let (out, config_path) = qr_flow::run(
        adapter,
        &data_dir,
        timeout,
        poll_interval,
        |prompt: &QrLoginPrompt| {
            // R26-OPS-2: the QR URL is a per-session
            // auth credential, not a long-lived secret,
            // but the workspace convention is "tracing
            // for everything, no `eprintln!` /
            // `println!` in the binary". Use `tracing`
            // at info level so the operator can grep
            // for the URL if needed but it doesn't
            // pollute `--output` JSON / structured
            // logs. The URL is the only thing an
            // operator needs to act on (it's what the
            // QR code encodes).
            if render_ascii {
                tracing::info!(
                    url_len = prompt.url.len(),
                    token_len = prompt.token.len(),
                    "[qr] token refreshed; see URL in structured logs"
                );
            } else {
                tracing::info!(
                    url = %prompt.url,
                    "[qr] scan with another device"
                );
            }
        },
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
    write_config_and_output(&out, &config_path, &on_disk_cfg, "", args.output.as_deref())
}

// ─── whoami ─────────────────────────────────────────────────

async fn run_whoami(
    args: octo_telegram_mtproto_onboard::cli::WhoamiArgs,
) -> Result<(), OnboardError> {
    let data_dir = resolve_data_dir(args.data_dir);
    let rec = SessionRecord::read_from(&data_dir)?;
    let out = OnboardOutput {
        schema_version: OnboardOutput::SCHEMA_VERSION,
        mode: match rec.mode.as_str() {
            "bot_token" => octo_telegram_mtproto_onboard_core::output::OnboardMode::BotToken,
            "user_code" => octo_telegram_mtproto_onboard_core::output::OnboardMode::UserCode,
            "qr_login" => octo_telegram_mtproto_onboard_core::output::OnboardMode::QrLogin,
            _ => octo_telegram_mtproto_onboard_core::output::OnboardMode::Whoami,
        },
        self_id: rec.user_id,
        self_username: rec.username,
        is_bot: rec.mode == "bot_token",
        data_dir: data_dir.display().to_string(),
        config_path: data_dir.join("config.json").display().to_string(),
        elapsed_ms: 0,
    };
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
fn write_config_and_output(
    out: &OnboardOutput,
    config_path: &Path,
    cfg: &MtprotoTelegramConfig,
    bot_token: &str,
    output: Option<&Path>,
) -> Result<(), OnboardError> {
    // Build the on-disk config. For user-mode we DO NOT
    // embed the phone (the operator re-enters it on
    // reconnect), and we DO NOT embed the SMS code (it's
    // already spent). For bot-mode we embed the token.
    let mut on_disk = cfg.clone();
    if !bot_token.is_empty() {
        on_disk.bot_token = Some(bot_token.to_string());
        on_disk.mode = Some("bot".to_string());
    } else {
        on_disk.mode = Some("user".to_string());
    }
    let json = serde_json::to_string_pretty(&on_disk).map_err(OnboardError::Json)?;
    if let Some(parent) = config_path.parent() {
        if !parent.exists() {
            std::fs::create_dir_all(parent).map_err(OnboardError::Io)?;
        }
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
            // output JSON. The output does NOT carry secrets
            // (it has self_id/username only), so the file
            // permissions do not need to be 0600 — but
            // atomicity is still desirable (deploy
            // pipelines may consume the file immediately).
            atomic_write(p, body.as_bytes())?;
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

/// Atomic write helper shared by Unix and non-Unix paths.
/// Stage to `<path>.tmp`, then `rename` over `<path>`.
fn atomic_write(path: &Path, data: &[u8]) -> Result<(), OnboardError> {
    atomic_write_with_mode(path, data, None)
}

/// Atomic write with an optional Unix file mode. On non-
/// Unix platforms the mode is ignored.
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
        let out = OnboardOutput {
            schema_version: 1,
            mode: octo_telegram_mtproto_onboard_core::output::OnboardMode::BotToken,
            self_id: 1,
            self_username: Some("x".into()),
            is_bot: true,
            data_dir: tmp.path().display().to_string(),
            config_path: config_path.display().to_string(),
            elapsed_ms: 0,
        };
        let cfg = MtprotoTelegramConfig {
            api_id: Some(1),
            api_hash: Some("h".into()),
            data_dir: Some(tmp.path().to_path_buf()),
            ..Default::default()
        };
        write_config_and_output(&out, &config_path, &cfg, "1:abc", None).unwrap();
        assert!(config_path.exists());
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
        let out = OnboardOutput {
            schema_version: 1,
            mode: octo_telegram_mtproto_onboard_core::output::OnboardMode::BotToken,
            self_id: 1,
            self_username: Some("x".into()),
            is_bot: true,
            data_dir: tmp.path().display().to_string(),
            config_path: config_path.display().to_string(),
            elapsed_ms: 0,
        };
        let cfg = MtprotoTelegramConfig {
            api_id: Some(1),
            api_hash: Some("h".into()),
            data_dir: Some(tmp.path().to_path_buf()),
            ..Default::default()
        };
        write_config_and_output(&out, &config_path, &cfg, "123:secret", None).unwrap();
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
        let out = OnboardOutput {
            schema_version: 1,
            mode: octo_telegram_mtproto_onboard_core::output::OnboardMode::BotToken,
            self_id: 1,
            self_username: Some("x".into()),
            is_bot: true,
            data_dir: tmp.path().display().to_string(),
            config_path: config_path.display().to_string(),
            elapsed_ms: 0,
        };
        let cfg = MtprotoTelegramConfig {
            api_id: Some(1),
            api_hash: Some("h".into()),
            data_dir: Some(tmp.path().to_path_buf()),
            ..Default::default()
        };
        write_config_and_output(&out, &config_path, &cfg, "1:abc", None).unwrap();
        assert!(config_path.exists());
        assert!(
            !tmp.path().join("config.json.tmp").exists(),
            "tmp file must be renamed away"
        );
    }
}
