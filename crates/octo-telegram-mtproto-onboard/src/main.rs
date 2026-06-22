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
use octo_telegram_mtproto_onboard::stdin_io::read_line_from_stdin;
use octo_telegram_mtproto_onboard_core::bot_token;
use octo_telegram_mtproto_onboard_core::output::OnboardOutput;
use octo_telegram_mtproto_onboard_core::qr_login::{self as qr_flow, QrLoginPrompt};
use octo_telegram_mtproto_onboard_core::session::SessionRecord;
use octo_telegram_mtproto_onboard_core::user_code::{self, UserCodeCredentials};
use tokio::sync::mpsc;
use tracing::{error, info};

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
    let bot_token_str = match args.bot_token {
        Some(t) if !t.is_empty() => t,
        _ => read_line_from_stdin("bot-token: ")?,
    };
    let data_dir = resolve_data_dir(args.data_dir);
    let cfg = MtprotoTelegramConfig {
        api_id: Some(api_id),
        api_hash: Some(api_hash.clone()),
        data_dir: Some(data_dir.clone()),
        ..Default::default()
    };
    // Production wiring only — no mock fallback. See
    // `octo_telegram_mtproto_onboard_core::connect`.
    let adapter = octo_telegram_mtproto_onboard_core::connect::connect(cfg).await?;
    let (out, config_path) = bot_token::run(adapter, &bot_token_str, &data_dir).await?;
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
        &bot_token_str,
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
    let cfg = MtprotoTelegramConfig {
        api_id: Some(api_id),
        api_hash: Some(api_hash.clone()),
        data_dir: Some(data_dir.clone()),
        ..Default::default()
    };
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
    // stdin.
    let input_task = tokio::spawn(async move {
        if let Some(path) = args.code_file {
            let code = std::fs::read_to_string(&path)
                .map_err(OnboardError::Io)?
                .trim()
                .to_string();
            code_tx
                .send(code)
                .await
                .map_err(|_| OnboardError::ChannelClosed("code".to_string()))?;
        } else {
            let code = read_line_from_stdin("SMS code: ")?;
            code_tx
                .send(code)
                .await
                .map_err(|_| OnboardError::ChannelClosed("code".to_string()))?;
        }

        if let Some(path) = args.password_file {
            let password = std::fs::read_to_string(&path)
                .map_err(OnboardError::Io)?
                .trim()
                .to_string();
            password_tx
                .send(password)
                .await
                .map_err(|_| OnboardError::ChannelClosed("password".to_string()))?;
        }
        // If --password-file was not supplied, we drop the
        // sender here; the core flow treats a closed
        // password channel as "no 2FA password" and aborts
        // the 2FA branch.
        Ok::<(), OnboardError>(())
    });

    let (out, config_path) =
        user_code::run(adapter, creds, code_rx, password_rx, &data_dir).await?;
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
    let cfg = MtprotoTelegramConfig {
        api_id: Some(api_id),
        api_hash: Some(api_hash.clone()),
        data_dir: Some(data_dir.clone()),
        ..Default::default()
    };
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
            if render_ascii {
                eprintln!(
                    "[qr] token refreshed; URL ({} bytes):\n  {}",
                    prompt.token.len(),
                    prompt.url
                );
            } else {
                eprintln!("[qr] scan with another device:");
                eprintln!("     {}", prompt.url);
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
    std::fs::write(config_path, json).map_err(OnboardError::Io)?;
    info!(config = %config_path.display(), "wrote adapter config");

    let body = out.to_json_pretty().map_err(OnboardError::Json)?;
    match output {
        Some(p) => {
            std::fs::write(p, body).map_err(OnboardError::Io)?;
            info!(output = %p.display(), "wrote onboard output");
        }
        None => {
            println!("{}", body);
        }
    }
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
}
