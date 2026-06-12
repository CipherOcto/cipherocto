//! `octo-telegram-onboard` — CLI binary for Telegram onboarding.
//!
//! Mission 0850ab-a. See RFC-0850ab-a for the full specification.

mod cli;
mod logging;

use clap::Parser;
use cli::{Cli, Command, SessionAction};
use octo_telegram_onboard_core::auth::{
    classify_tdlib_error, close_tdlib_client_with_timeout, drive_bot_auth, drive_user_auth,
    set_tdlib_parameters_raw, try_acquire_receive_lock, validate_api_id, Credentials,
};
use octo_telegram_onboard_core::error::{OnboardError, Result};
use octo_telegram_onboard_core::keys::validate_verifying_key;
use octo_telegram_onboard_core::output::{
    build_config_json, default_config_path_opt, write_config,
};
use octo_telegram_onboard_core::session::{SessionMeta, TelegramSession};
use std::process::ExitCode;
use zeroize::Zeroizing;

#[tokio::main]
async fn main() -> ExitCode {
    let cli = Cli::parse();
    logging::init(cli.verbose);

    let result: Result<()> = async {
        match cli.command {
            Command::BotSetup(args) => run_bot_setup(args).await,
            Command::UserLogin(args) => run_user_login(args).await,
            Command::Whoami(args) => run_whoami(args).await,
            Command::Session { action } => match action {
                SessionAction::List(args) => run_session_list(args).await,
                SessionAction::Verify { dir, config } => {
                    run_session_verify(&dir, config.as_deref()).await
                }
                SessionAction::Remove { dir, yes } => run_session_remove(&dir, yes).await,
            },
            Command::Version => {
                println!("octo-telegram-onboard {}", env!("CARGO_PKG_VERSION"));
                Ok(())
            }
        }
    }
    .await;

    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            match &e {
                OnboardError::Generic(any) => {
                    tracing::error!("{:#}", any);
                }
                _ => {
                    let kind = e.to_string();
                    match e.inner() {
                        Some(detail) if !detail.is_empty() => {
                            tracing::error!("{}: {}", kind, detail);
                        }
                        _ => {
                            tracing::error!("{}", kind);
                        }
                    }
                }
            }
            e.as_exit_code()
        }
    }
}

/// Build a full TelegramConfig-compatible JSON from credentials + session.
fn build_full_config(creds: &Credentials, session: &TelegramSession) -> serde_json::Value {
    let mut json = build_config_json(session);

    if let Some(ref token) = creds.bot_token {
        json["bot_token"] = serde_json::Value::String(token.to_string());
    }
    json["api_id"] = serde_json::Value::Number(creds.api_id.into());
    json["api_hash"] = serde_json::Value::String(creds.api_hash.to_string());
    if let Some(ref phone) = creds.phone {
        json["phone"] = serde_json::Value::String(phone.clone());
    }

    json
}

/// TDLib get_me flow with a timeout — used by whoami and session list fallback.
/// NOTE: `tdlib_rs::receive()` is process-global; this function assumes no other
/// TDLib client is active in this process. Do not call in parallel.
async fn tdlib_get_me_with_timeout(
    data_dir: &std::path::Path,
    api_id: i32,
    api_hash: &str,
    timeout_secs: u64,
) -> std::result::Result<(i64, Option<String>, Option<String>), OnboardError> {
    let _receive_guard = try_acquire_receive_lock()?;
    let client_id = tdlib_rs::create_client();

    let params_err = set_tdlib_parameters_raw(client_id, api_id, api_hash, data_dir).await;
    if let Err(e) = params_err {
        close_tdlib_client_with_timeout(client_id, std::time::Duration::from_secs(2)).await;
        return Err(e);
    }

    let (tx, mut rx) = tokio::sync::mpsc::channel::<bool>(1);
    let timeout = std::time::Duration::from_secs(timeout_secs);

    let receive_handle = match std::thread::Builder::new()
        .name("tdlib-get-me-receive".into())
        .spawn(move || {
            let start = std::time::Instant::now();
            while start.elapsed() < timeout {
                if let Some((tdlib_rs::enums::Update::AuthorizationState(ref auth_update), _cid)) =
                    tdlib_rs::receive()
                {
                    match &auth_update.authorization_state {
                        tdlib_rs::enums::AuthorizationState::Ready => {
                            let _ = tx.blocking_send(true);
                            return;
                        }
                        tdlib_rs::enums::AuthorizationState::Closed
                        | tdlib_rs::enums::AuthorizationState::WaitPhoneNumber => {
                            let _ = tx.blocking_send(false);
                            return;
                        }
                        _ => {}
                    }
                }
                std::thread::sleep(std::time::Duration::from_millis(50));
            }
            let _ = tx.blocking_send(false);
        }) {
        Ok(h) => h,
        Err(e) => {
            close_tdlib_client_with_timeout(client_id, std::time::Duration::from_secs(2)).await;
            return Err(OnboardError::Generic(anyhow::anyhow!(
                "failed to spawn receive thread: {}",
                e
            )));
        }
    };

    let channel_result = tokio::time::timeout(timeout, rx.recv()).await;
    // Ensure the receive thread has exited before proceeding.
    // The thread has already sent its value (or timed out), so join completes immediately.
    let _ = receive_handle.join();
    match channel_result {
        Ok(Some(true)) => {
            let me_enum = match tdlib_rs::functions::get_me(client_id).await {
                Ok(u) => u,
                Err(e) => {
                    close_tdlib_client_with_timeout(client_id, std::time::Duration::from_secs(2))
                        .await;
                    return Err(classify_tdlib_error(e.message));
                }
            };
            #[allow(unreachable_patterns)]
            match me_enum {
                tdlib_rs::enums::User::User(u) => {
                    let username = u
                        .usernames
                        .as_ref()
                        .and_then(|names| names.active_usernames.first().cloned());
                    close_tdlib_client_with_timeout(client_id, std::time::Duration::from_secs(2))
                        .await;
                    Ok((u.id, username, Some(u.first_name)))
                }
                _ => {
                    close_tdlib_client_with_timeout(client_id, std::time::Duration::from_secs(2))
                        .await;
                    Err(OnboardError::Generic(anyhow::anyhow!(
                        "get_me returned unexpected User variant"
                    )))
                }
            }
        }
        Ok(Some(false)) => {
            close_tdlib_client_with_timeout(client_id, std::time::Duration::from_secs(2)).await;
            Err(OnboardError::AuthRejected(
                "session expired or invalid".into(),
            ))
        }
        Ok(None) => {
            close_tdlib_client_with_timeout(client_id, std::time::Duration::from_secs(2)).await;
            Err(OnboardError::Cancelled("whoami channel closed".into()))
        }
        Err(_) => {
            close_tdlib_client_with_timeout(client_id, std::time::Duration::from_secs(2)).await;
            Err(OnboardError::Cancelled("whoami timed out".into()))
        }
    }
}

async fn run_bot_setup(args: cli::BotSetupArgs) -> Result<()> {
    let data_dir = args.resolved_data_dir();
    let out = args.out.clone();
    let stdout = args.stdout;
    let force = args.force;
    let timeout = args.timeout;

    let bot_token = args.bot_token.ok_or_else(|| {
        OnboardError::BadConfig("bot-setup requires --bot-token or $TELEGRAM_BOT_TOKEN".into())
    })?;
    let api_id_raw = args.api_id.ok_or_else(|| {
        OnboardError::BadConfig("bot-setup requires --api-id or $TELEGRAM_API_ID".into())
    })?;
    let api_hash = args.api_hash.ok_or_else(|| {
        OnboardError::BadConfig("bot-setup requires --api-hash or $TELEGRAM_API_HASH".into())
    })?;

    if let Some(ref key) = args.verifying_key {
        validate_verifying_key(key)?;
    }

    let verifying_key = args.verifying_key;

    let creds = Credentials::try_new(
        None,
        api_id_raw,
        Zeroizing::new(api_hash),
        Some(Zeroizing::new(bot_token)),
        verifying_key,
    )?;

    tracing::info!(data_dir = %data_dir.display(), "Starting bot auth...");
    let client_id = tdlib_rs::create_client();
    let session = drive_bot_auth(
        client_id,
        &creds,
        &data_dir,
        std::time::Duration::from_secs(timeout),
    )
    .await?;

    SessionMeta::from_session(&session).write(&session.data_dir)?;

    let json = build_full_config(&creds, &session);

    tracing::info!(
        username = %session.username.as_deref().unwrap_or("(unknown)"),
        user_id = session.user_id,
        "Authenticated successfully"
    );

    write_config(out.as_deref(), stdout, force, &json)
}

async fn run_user_login(args: cli::UserLoginArgs) -> Result<()> {
    let data_dir = args.resolved_data_dir();
    let out = args.out.clone();
    let stdout = args.stdout;
    let force = args.force;
    let timeout = args.timeout;

    let api_id_raw = args.api_id.ok_or_else(|| {
        OnboardError::BadConfig("user-login requires --api-id or $TELEGRAM_API_ID".into())
    })?;
    let api_hash = args.api_hash.ok_or_else(|| {
        OnboardError::BadConfig("user-login requires --api-hash or $TELEGRAM_API_HASH".into())
    })?;
    let phone = args.phone.ok_or_else(|| {
        OnboardError::BadConfig("user-login requires --phone or $TELEGRAM_PHONE".into())
    })?;

    if let Some(ref key) = args.verifying_key {
        validate_verifying_key(key)?;
    }

    let verifying_key = args.verifying_key;

    let creds = Credentials::try_new(
        Some(phone),
        api_id_raw,
        Zeroizing::new(api_hash),
        None,
        verifying_key,
    )?;

    tracing::info!(data_dir = %data_dir.display(), "Starting user auth...");
    let client_id = tdlib_rs::create_client();
    let session = drive_user_auth(
        client_id,
        &creds,
        &data_dir,
        std::time::Duration::from_secs(timeout),
    )
    .await?;

    SessionMeta::from_session(&session).write(&session.data_dir)?;

    let json = build_full_config(&creds, &session);

    tracing::info!(
        username = %session.username.as_deref().unwrap_or("(unknown)"),
        user_id = session.user_id,
        "Authenticated successfully"
    );

    write_config(out.as_deref(), stdout, force, &json)
}

async fn run_whoami(args: cli::WhoamiArgs) -> Result<()> {
    let config_path = args
        .config
        .or_else(default_config_path_opt)
        .ok_or_else(|| {
            OnboardError::BadConfig("could not determine config path (use --config)".into())
        })?;

    let bytes = std::fs::read(&config_path)
        .map_err(|e| OnboardError::BadConfig(format!("read {}: {}", config_path.display(), e)))?;
    let config: serde_json::Value = serde_json::from_slice(&bytes)
        .map_err(|e| OnboardError::BadConfig(format!("parse {}: {}", config_path.display(), e)))?;

    let data_dir = config["data_dir"]
        .as_str()
        .ok_or_else(|| OnboardError::BadConfig("config missing data_dir".into()))?;

    // Try reading sidecar for display, but always validate via get_me()
    let data_path = std::path::Path::new(data_dir);
    let cached = SessionMeta::read(data_path);

    // Always call get_me() to validate the session (AC line 64: exit 2 for expired)
    tracing::info!("Validating session via get_me()...");
    let api_id_raw = config["api_id"]
        .as_i64()
        .ok_or_else(|| OnboardError::BadConfig("config missing api_id".into()))?;
    let api_id = validate_api_id(api_id_raw)?;
    let api_hash = config["api_hash"]
        .as_str()
        .filter(|s| !s.is_empty())
        .ok_or_else(|| OnboardError::BadConfig("config missing or empty api_hash".into()))?
        .to_string();

    let (user_id, username, first_name) =
        tdlib_get_me_with_timeout(data_path, api_id, &api_hash, 10).await?;
    let username = username.unwrap_or_else(|| "(none)".into());
    let first_name = first_name.unwrap_or_else(|| "(none)".into());
    let mode = cached
        .as_ref()
        .map(|m| m.mode.as_str())
        .unwrap_or("unknown");
    println!(
        "User ID: {}\nUsername: {}\nFirst name: {}\nMode: {}",
        user_id, username, first_name, mode
    );
    Ok(())
}

async fn run_session_list(args: cli::SessionListArgs) -> Result<()> {
    let base_dir = args.resolved_base_dir();

    if !base_dir.exists() {
        tracing::info!(dir = %base_dir.display(), "No sessions directory found");
        return Ok(());
    }

    let (h_data, h_mode, h_uid, h_user, h_valid) =
        ("DATA_DIR", "MODE", "USER_ID", "USERNAME", "VALID");
    println!(
        "{:<50} {:<6} {:<15} {:<20} {}",
        h_data, h_mode, h_uid, h_user, h_valid
    );

    let entries = std::fs::read_dir(&base_dir)
        .map_err(|e| OnboardError::BadConfig(format!("read_dir {}: {}", base_dir.display(), e)))?;

    for entry in entries {
        let entry = entry.map_err(|e| OnboardError::Generic(e.into()))?;
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }

        if let Some(meta) = SessionMeta::read(&path) {
            let db_exists = path.join("database").exists();
            let valid = if db_exists { "yes" } else { "no (missing db)" };
            println!(
                "{:<50} {:<6} {:<15} {:<20} {}",
                path.display(),
                meta.mode,
                meta.user_id,
                meta.username.as_deref().unwrap_or("(none)"),
                valid
            );
        } else {
            let db_path = path.join("database");
            let db_file = db_path.join("db.sqlite");
            if db_file.exists() {
                // H3: AC line 65 — fallback to get_me() with 5s timeout
                // for sidecar-less dirs that have a real TDLib database.
                if let Some(meta) = SessionMeta::read(&path) {
                    println!(
                        "{:<50} {:<6} {:<15} {:<20} yes",
                        path.display(),
                        meta.mode,
                        meta.user_id,
                        meta.username.as_deref().unwrap_or("(none)")
                    );
                } else {
                    // Try get_me fallback — read config for api_id/api_hash
                    // Only use config credentials if the config's data_dir matches
                    // this session's path (prevents wrong credentials in multi-account setups).
                    let config_path = args.config.clone().or_else(default_config_path_opt);
                    let fallback = if let Some(cp) = config_path {
                        if let Ok(bytes) = std::fs::read(&cp) {
                            if let Ok(cfg) = serde_json::from_slice::<serde_json::Value>(&bytes) {
                                let config_data_dir = cfg["data_dir"].as_str();
                                if config_data_dir != Some(path.to_string_lossy().as_ref()) {
                                    None // Different session, skip fallback
                                } else {
                                    let api_id = cfg["api_id"]
                                        .as_i64()
                                        .and_then(|v| validate_api_id(v).ok());
                                    let api_hash = cfg["api_hash"]
                                        .as_str()
                                        .filter(|s| !s.is_empty())
                                        .map(String::from);
                                    if let (Some(aid), Some(ahash)) = (api_id, api_hash) {
                                        tdlib_get_me_with_timeout(&path, aid, &ahash, 5).await.ok()
                                    } else {
                                        None
                                    }
                                }
                            } else {
                                None
                            }
                        } else {
                            None
                        }
                    } else {
                        None
                    };

                    if let Some((user_id, username, _first_name)) = fallback {
                        println!(
                            "{:<50} {:<6} {:<15} {:<20} yes (via get_me)",
                            path.display(),
                            "?",
                            user_id,
                            username.as_deref().unwrap_or("(none)")
                        );
                    } else {
                        let (q, no_sidecar, unk) = ("?", "(no sidecar)", "unknown");
                        println!(
                            "{:<50} {:<6} {:<15} {:<20} {}",
                            path.display(),
                            q,
                            q,
                            no_sidecar,
                            unk
                        );
                    }
                }
            } else {
                println!(
                    "{:<50} {:<6} {:<15} {:<20} no",
                    path.display(),
                    "?",
                    "?",
                    "(no db.sqlite)"
                );
            }
        }
    }

    Ok(())
}

async fn run_session_verify(
    dir: &std::path::Path,
    config_path: Option<&std::path::Path>,
) -> Result<()> {
    if !dir.exists() {
        return Err(OnboardError::BadConfig(format!(
            "directory does not exist: {}",
            dir.display()
        )));
    }

    let cached = SessionMeta::read(dir);

    // Try to validate via get_me() using config credentials
    let cp = config_path
        .map(std::path::PathBuf::from)
        .or_else(default_config_path_opt);

    if let Some(config_path) = cp {
        if let Ok(bytes) = std::fs::read(&config_path) {
            if let Ok(cfg) = serde_json::from_slice::<serde_json::Value>(&bytes) {
                let api_id = cfg["api_id"].as_i64().and_then(|v| validate_api_id(v).ok());
                let api_hash = cfg["api_hash"]
                    .as_str()
                    .filter(|s| !s.is_empty())
                    .map(String::from);
                if let (Some(aid), Some(ahash)) = (api_id, api_hash) {
                    match tdlib_get_me_with_timeout(dir, aid, &ahash, 5).await {
                        Ok((user_id, username, _)) => {
                            let username = username.unwrap_or_else(|| "(none)".into());
                            println!(
                                "Valid session: user_id={}, username={}, mode={}",
                                user_id,
                                username,
                                cached
                                    .as_ref()
                                    .map(|m| m.mode.as_str())
                                    .unwrap_or("unknown")
                            );
                            return Ok(());
                        }
                        Err(_) => {
                            println!("Expired or invalid session in {}", dir.display());
                            return Ok(());
                        }
                    }
                }
            }
        }
    }

    // Fallback: read sidecar only (no get_me possible without credentials)
    if let Some(meta) = cached {
        println!(
            "Session metadata (unvalidated): user_id={}, username={}, mode={}",
            meta.user_id,
            meta.username.as_deref().unwrap_or("(none)"),
            meta.mode
        );
    } else {
        println!("No session_meta.json found in {}", dir.display());
    }

    Ok(())
}

async fn run_session_remove(dir: &std::path::Path, yes: bool) -> Result<()> {
    if !dir.exists() {
        return Err(OnboardError::BadConfig(format!(
            "directory does not exist: {}",
            dir.display()
        )));
    }

    // Safety check: ensure the directory looks like a Telegram session
    let is_session = dir.join("session_meta.json").exists() || dir.join("database").is_dir();
    if !is_session {
        return Err(OnboardError::BadConfig(format!(
            "directory does not look like a Telegram session (no session_meta.json or database/): {}",
            dir.display()
        )));
    }

    if !yes {
        let dir_display = dir.display().to_string();
        let confirmed = tokio::task::spawn_blocking(move || {
            use std::io::{BufRead, Write};
            print!("Remove {}? [y/N] ", dir_display);
            std::io::stdout().flush().ok();
            let mut line = String::new();
            std::io::stdin().lock().read_line(&mut line).ok();
            line.trim().eq_ignore_ascii_case("y")
        })
        .await
        .unwrap_or(false);
        if !confirmed {
            tracing::info!("Aborted");
            return Ok(());
        }
    }

    std::fs::remove_dir_all(dir)
        .map_err(|e| OnboardError::BadConfig(format!("remove {}: {}", dir.display(), e)))?;
    tracing::info!(dir = %dir.display(), "Removed session directory");
    Ok(())
}
