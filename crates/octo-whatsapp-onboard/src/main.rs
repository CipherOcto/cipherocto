//! `octo-whatsapp-onboard` — CLI binary for WhatsApp Web onboarding.
//!
//! Mission 0850p-a. See `docs/plans/...` for the full design.

mod cli;
mod error;
mod logging;
mod output;

use std::time::Duration;

use clap::Parser;
use cli::{
    Cli, Command, PairLinkArgs, QrLinkArgs, SessionAction, SessionListArgs, SessionRemoveArgs,
    SessionVerifyArgs, WhoamiArgs,
};
use error::OnboardError;
use octo_network::dot::adapters::PlatformAdapter; // brings self_handle + health_check into scope
use octo_whatsapp_onboard_core::{
    wait_for_connected, wait_for_health, CoreError, PairLinkArgs as CorePairLinkArgs,
    QrLinkArgs as CoreQrLinkArgs, SessionInfo, WhatsAppConfig,
    SESSION_LIST_HEALTH_TIMEOUT_SECS, WHOAMI_TIMEOUT_SECS,
};
use std::process::ExitCode;

#[tokio::main]
async fn main() -> ExitCode {
    let cli = Cli::parse();
    logging::init(&cli);

    let result: std::result::Result<(), OnboardError> = async {
        match cli.command {
            Command::QrLink(args) => run_qr_link(args).await,
            Command::PairLink(args) => run_pair_link(args).await,
            Command::Whoami(args) => run_whoami(args).await,
            Command::Session { action } => match action {
                SessionAction::List(args) => run_session_list(args).await,
                SessionAction::Verify(args) => run_session_verify(args).await,
                SessionAction::Remove(args) => run_session_remove(args).await,
            },
            Command::Version => {
                println!("octo-whatsapp-onboard {}", env!("CARGO_PKG_VERSION"));
                Ok(())
            }
        }
    }
    .await;

    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            // R1-M6: Display impl shows ONLY the kind label, but
            // the inner message is preserved for log enrichment.
            if let Some(inner) = e.inner() {
                tracing::error!("{inner}");
            } else {
                tracing::error!("{:#}", e);
            }
            e.as_exit_code()
        }
    }
}

async fn run_qr_link(args: QrLinkArgs) -> std::result::Result<(), OnboardError> {
    let output = args.output.clone();
    let session = octo_whatsapp_onboard_core::qr_link::run(to_core_qr(args)).await?;
    output::write(&output, &session)?;
    println!("Authenticated as +{} (session: {})", session.self_phone.as_deref().unwrap_or("?"), session.session_path.display());
    Ok(())
}

async fn run_pair_link(args: PairLinkArgs) -> std::result::Result<(), OnboardError> {
    let output = args.output.clone();
    let session = octo_whatsapp_onboard_core::pair_link::run(to_core_pair(args)).await?;
    output::write(&output, &session)?;
    println!("Authenticated as +{} (session: {})", session.self_phone.as_deref().unwrap_or("?"), session.session_path.display());
    Ok(())
}

async fn run_whoami(args: WhoamiArgs) -> std::result::Result<(), OnboardError> {
    let _cfg = load_config(&args.config)?;
    // Build a WhatsAppWebAdapter against the session_path
    let session_path = cfg_session_path(&args.config)?;
    let adapter = build_adapter(&session_path, &[])?;

    match wait_for_connected(
        &adapter,
        Duration::from_secs(octo_whatsapp_onboard_core::WHOAMI_TIMEOUT_SECS),
    )
    .await
    {
        Ok(phone) => {
            println!("+{phone}");
            Ok(())
        }
        Err(CoreError::SessionExpired) => Err(OnboardError::SessionExpired(
            "Session expired or invalid".into(),
        )),
        Err(CoreError::Timeout { secs }) => Err(OnboardError::Cancelled(format!(
            "Timeout after {secs}s"
        ))),
        Err(e) => Err(e.into()),
    }
}

async fn run_session_list(args: SessionListArgs) -> std::result::Result<(), OnboardError> {
    let base_dir = args
        .base_dir
        .clone()
        .unwrap_or_else(default_session_base_dir);
    let infos = list_sessions(&base_dir).await?;
    println!(
        "{:<60}  {:<15}  {:<22}  VALID",
        "SESSION_PATH", "SELF_PHONE", "LINKED_AT"
    );
    for info in infos {
        let path_str = info.session_path.to_string_lossy().to_string();
        let phone = info.self_phone.as_deref().unwrap_or("<unknown>");
        let linked = info.last_linked_at.as_deref().unwrap_or("<unknown>");
        let valid = if info.is_valid { "yes" } else { "no" };
        println!("{path_str:<60}  {phone:<15}  {linked:<22}  {valid}");
    }
    Ok(())
}

async fn run_session_verify(args: SessionVerifyArgs) -> std::result::Result<(), OnboardError> {
    let adapter = build_adapter(&args.db_path, &[])?;
    match wait_for_connected(
        &adapter,
        Duration::from_secs(octo_whatsapp_onboard_core::WHOAMI_TIMEOUT_SECS),
    )
    .await
    {
        Ok(_) => {
            println!("valid");
            Ok(())
        }
        Err(CoreError::SessionExpired) => {
            println!("expired");
            Err(OnboardError::SessionExpired(
                "Session expired or invalid".into(),
            ))
        }
        Err(e) => Err(e.into()),
    }
}

async fn run_session_remove(args: SessionRemoveArgs) -> std::result::Result<(), OnboardError> {
    use dialoguer::Confirm;

    if !args.yes {
        let prompt = format!("Remove session at {:?}?", args.db_path);
        let confirmed = Confirm::new()
            .with_prompt(prompt)
            .default(false)
            .interact()
            .map_err(|e| {
                if e.to_string().contains("not a terminal") {
                    OnboardError::BadConfig(
                        "session remove requires a TTY (pass --yes to skip the interactive prompt)"
                            .into(),
                    )
                } else {
                    OnboardError::BadConfig(format!("prompt failed: {e}"))
                }
            })?;
        if !confirmed {
            println!("aborted");
            return Ok(());
        }
    }

    std::fs::remove_file(&args.db_path).map_err(|e| {
        OnboardError::BadConfig(format!("remove {}: {}", args.db_path.display(), e))
    })?;
    println!("removed");
    Ok(())
}

// ── Helpers ────────────────────────────────────────────────────────

fn to_core_qr(args: QrLinkArgs) -> CoreQrLinkArgs {
    CoreQrLinkArgs {
        session_path: args.session_path,
        groups: args.groups,
        ws_url: args.ws_url,
        timeout_secs: args.timeout,
    }
}

fn to_core_pair(args: PairLinkArgs) -> CorePairLinkArgs {
    CorePairLinkArgs {
        session_path: args.session_path,
        phone: args.phone,
        custom_code: args.pair_code,
        groups: args.groups,
        ws_url: args.ws_url,
        timeout_secs: args.timeout,
    }
}

fn default_session_base_dir() -> std::path::PathBuf {
    let mut base = dirs::data_dir().unwrap_or_else(|| std::path::PathBuf::from("."));
    base.push("octo");
    base.push("whatsapp");
    base
}

fn load_config(path: &std::path::Path) -> std::result::Result<WhatsAppConfig, OnboardError> {
    let bytes = std::fs::read(path)
        .map_err(|e| OnboardError::BadConfig(format!("read {path:?}: {e}")))?;
    serde_json::from_slice(&bytes)
        .map_err(|e| OnboardError::BadConfig(format!("parse {path:?}: {e}")))
}

fn cfg_session_path(path: &std::path::Path) -> std::result::Result<std::path::PathBuf, OnboardError> {
    let cfg = load_config(path)?;
    Ok(std::path::PathBuf::from(cfg.session_path))
}

fn build_adapter(
    session_path: &std::path::Path,
    groups: &[String],
) -> std::result::Result<octo_whatsapp_onboard_core::WhatsAppWebAdapter, OnboardError> {
    let cfg = WhatsAppConfig {
        session_path: session_path.to_string_lossy().into_owned(),
        pair_phone: None,
        pair_code: None,
        ws_url: None,
        groups: groups.to_vec(),
    };
    Ok(octo_whatsapp_onboard_core::WhatsAppWebAdapter::new(cfg))
}

/// List sessions under `base_dir`. Reads `*.meta.json` sidecar if
/// present, else falls back to `wait_for_health` on the DB.
async fn list_sessions(base_dir: &std::path::Path) -> std::result::Result<Vec<SessionInfo>, OnboardError> {
    if !base_dir.exists() {
        return Ok(vec![]);
    }
    let mut out = Vec::new();
    let entries = std::fs::read_dir(base_dir).map_err(|e| {
        OnboardError::BadConfig(format!("read_dir {base_dir:?}: {e}"))
    })?;
    for entry in entries {
        let entry = entry.map_err(|e| OnboardError::BadConfig(format!("read_dir entry: {e}")))?;
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) != Some("db") {
            continue;
        }
        let sidecar = path.with_extension("db.meta.json");
        let (self_phone, last_linked_at) = if sidecar.exists() {
            read_sidecar(&sidecar).unwrap_or_else(|_| (None, None))
        } else {
            (None, None)
        };
        let adapter = build_adapter(&path, &[])?;
        let is_valid = wait_for_health(
            &adapter,
            Duration::from_secs(SESSION_LIST_HEALTH_TIMEOUT_SECS),
        )
        .await
        .is_ok();
        out.push(SessionInfo {
            session_path: path,
            self_phone: self_phone.or_else(|| adapter.self_handle()),
            is_valid,
            last_linked_at,
        });
    }
    Ok(out)
}

fn read_sidecar(path: &std::path::Path) -> std::result::Result<(Option<String>, Option<String>), OnboardError> {
    let bytes = std::fs::read(path)
        .map_err(|e| OnboardError::BadConfig(format!("read sidecar {path:?}: {e}")))?;
    let v: serde_json::Value = serde_json::from_slice(&bytes)
        .map_err(|e| OnboardError::BadConfig(format!("parse sidecar {path:?}: {e}")))?;
    let phone = v
        .get("self_phone")
        .and_then(|x| x.as_str().map(String::from));
    let linked = v
        .get("linked_at")
        .and_then(|x| x.as_str().map(String::from));
    Ok((phone, linked))
}
