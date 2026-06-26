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
    Cli, Command, OutputArgs, PairLinkArgs, QrLinkArgs, SessionAction, SessionListArgs,
    SessionRemoveArgs, SessionVerifyArgs, WhoamiArgs,
};
use error::OnboardError;
use octo_network::dot::adapters::PlatformAdapter;
use octo_whatsapp_onboard_core::{
    wait_for_connected, CoreError, PairLinkArgs as CorePairLinkArgs, QrLinkArgs as CoreQrLinkArgs,
    SessionInfo, WhatsAppConfig, WHOAMI_TIMEOUT_SECS,
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
    // Mission 0850p-a-ws-url-release-guard: refuse --ws-url in
    // release builds without OCTO_WHATSAPP_ALLOW_WS_URL=1.
    check_ws_url_allowed(args.ws_url.as_ref())?;
    // R1-M4: pass args by reference; no clone of OutputArgs needed.
    let core_args = to_core_qr(&args);
    let session = octo_whatsapp_onboard_core::qr_link::run(&core_args).await?;
    run_link(&args.output, session).await
}

async fn run_pair_link(args: PairLinkArgs) -> std::result::Result<(), OnboardError> {
    // Mission 0850p-a-ws-url-release-guard: refuse --ws-url in
    // release builds without OCTO_WHATSAPP_ALLOW_WS_URL=1.
    check_ws_url_allowed(args.ws_url.as_ref())?;
    // Mission 0850p-a-ci-mode-pair-link: --ci bypasses
    // Event::Connected wait and validates the pre-paired session
    // DB. No phone interaction needed.
    if args.ci {
        return run_pair_link_ci(&args);
    }
    // R1-M4: pass args by reference; no clone of OutputArgs needed.
    let core_args = to_core_pair(&args);
    let session = octo_whatsapp_onboard_core::pair_link::run(&core_args).await?;
    run_link(&args.output, session).await
}

/// Mission 0850p-a-ci-mode-pair-link: CI mode. Loads the
/// pre-paired session DB at `--session-path`, validates it via
/// the adapter's `has_valid_session()`, and writes the sidecar
/// (so downstream `whoami` works). No phone interaction.
fn run_pair_link_ci(args: &PairLinkArgs) -> std::result::Result<(), OnboardError> {
    use octo_whatsapp_onboard_core::sidecar::{write_sidecar, SidecarMode};
    use octo_whatsapp_onboard_core::WhatsAppSession;

    // Validate parent dir + symlink check (same as interactive flow).
    octo_whatsapp_onboard_core::validate_session_args(&args.session_path)
        .map_err(|e| OnboardError::BadConfig(format!("session_path invalid: {e}")))?;

    // Build the adapter and check has_valid_session(). This opens
    // the session DB; if the DB is empty or the Signal keys are
    // missing, the check returns false.
    let cfg = WhatsAppConfig {
        session_path: format!("{}", args.session_path.display()),
        pair_phone: Some(args.phone.clone()),
        pair_code: args.pair_code.clone(),
        ws_url: args.ws_url.clone(),
        groups: args.groups.clone(),
        sender_allowlist: Default::default(),
    };
    let adapter = octo_whatsapp_onboard_core::WhatsAppWebAdapter::new(cfg);
    if !adapter.has_valid_session() {
        return Err(OnboardError::BadConfig(format!(
            "session DB at {:?} is empty or invalid; cannot use --ci mode without a pre-paired session",
            args.session_path
        )));
    }

    // Build the sidecar so subsequent `whoami` works. The adapter
    // exposes `self_handle` via the `PlatformAdapter` trait
    // (imported at the top of this file; mission
    // 0850p-a-has-valid-session).
    let phone = adapter.self_handle().unwrap_or_default();
    let session = WhatsAppSession {
        self_phone: Some(phone),
        session_path: args.session_path.clone(),
        groups: args.groups.clone(),
        pair_phone: Some(args.phone.clone()),
    };
    write_sidecar(&args.session_path, &session, SidecarMode::PairLink)
        .map_err(|e| OnboardError::BadConfig(format!("write_sidecar: {e}")))?;

    output::write(&args.output, &session)?;
    println!(
        "CI mode: pre-paired session at {} accepted",
        args.session_path.display()
    );
    Ok(())
}

async fn run_link(
    output: &OutputArgs,
    session: octo_whatsapp_onboard_core::WhatsAppSession,
) -> std::result::Result<(), OnboardError> {
    output::write(output, &session)?;
    println!(
        "Authenticated as +{} (session: {})",
        session.self_phone.as_deref().unwrap_or("?"),
        session.session_path.display()
    );
    Ok(())
}

async fn run_whoami(args: WhoamiArgs) -> std::result::Result<(), OnboardError> {
    // Mission 0850p-a-has-valid-session: a fast pre-check that
    // returns the phone in <2s for an already-paired bot. This
    // replaces the 250ms polling loop pattern.
    if args.detect_replacement {
        // Mission 0850p-a-replaced-state: --detect-replacement
        // exits 8 on Replaced, 7 on SessionExpired, 2 on other
        // LoggedOut. Currently we cannot inspect the cause from
        // the whatsapp-rust Event::LoggedOut (the cause field is
        // not yet exposed in the pinned version), so this branch
        // is a no-op stub that exits 0 on a healthy session and
        // 7 on SessionExpired. The Replaced path will be enabled
        // when whatsapp-rust exposes the cause.
        let cfg = load_config(&args.config)?;
        let session_path = std::path::PathBuf::from(&cfg.session_path);
        let sidecar = session_path.with_extension("db.meta.json");
        if !sidecar.exists() {
            return Err(OnboardError::SessionExpired(
                "no sidecar; session is not paired".into(),
            ));
        }
        if let Ok((Some(p), _)) = read_sidecar(&sidecar) {
            println!("+{p}");
            return Ok(());
        }
        return Err(OnboardError::SessionExpired(
            "sidecar unreadable; session may be invalid".into(),
        ));
    }
    let cfg = load_config(&args.config)?;
    let session_path = std::path::PathBuf::from(&cfg.session_path);
    let sidecar = session_path.with_extension("db.meta.json");
    if sidecar.exists() {
        if let Ok((Some(p), _)) = read_sidecar(&sidecar) {
            println!("+{p}");
            return Ok(());
        }
    }
    let adapter = build_adapter(&session_path, &[])?;
    start_bot(&adapter).await?; // R1-C2
    match wait_for_connected(&adapter, Duration::from_secs(WHOAMI_TIMEOUT_SECS)).await {
        Ok(phone) => {
            println!("+{phone}");
            Ok(())
        }
        Err(CoreError::SessionExpired) => Err(OnboardError::SessionExpired(
            "Session expired or invalid".into(),
        )),
        Err(CoreError::Timeout { secs }) => {
            Err(OnboardError::Cancelled(format!("Timeout after {secs}s")))
        }
        Err(e) => Err(e.into()),
    }
}

async fn run_session_list(args: SessionListArgs) -> std::result::Result<(), OnboardError> {
    // R4-L1: no clone needed; args is by-value and args.base_dir
    // is the only consumer of `args`.
    let base_dir = args.base_dir.unwrap_or_else(default_session_base_dir);
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
    start_bot(&adapter).await?; // R1-H2
    match wait_for_connected(&adapter, Duration::from_secs(WHOAMI_TIMEOUT_SECS)).await {
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
                not_tty_to_bad_config(&e)
                    .unwrap_or_else(|| OnboardError::BadConfig(format!("prompt failed: {e}")))
            })?;
        if !confirmed {
            println!("aborted");
            return Ok(());
        }
    }

    std::fs::remove_file(&args.db_path).map_err(|e| {
        OnboardError::BadConfig(format!("remove {}: {}", args.db_path.display(), e))
    })?;
    let sidecar = args.db_path.with_extension("db.meta.json");
    if sidecar.exists() {
        let _ = std::fs::remove_file(&sidecar);
    }
    println!("removed");
    Ok(())
}

// ── Helpers ────────────────────────────────────────────────────────

// R1-M4: take by reference (the core's `run` now takes by ref too).
fn to_core_qr(args: &QrLinkArgs) -> CoreQrLinkArgs {
    // R1-H1: env var fallback for --ws-url. CLI arg wins if both
    // are set (the operator typed it explicitly, so honor that).
    let ws_url = env_or_arg_opt(args.ws_url.as_ref(), "OCTO_WHATSAPP_WS_URL");
    CoreQrLinkArgs {
        session_path: args.session_path.clone(),
        groups: args.groups.clone(),
        ws_url,
        timeout_secs: args.timeout,
        wait_sync: args.wait_sync,
    }
}

fn to_core_pair(args: &PairLinkArgs) -> CorePairLinkArgs {
    // R1-H1: env var fallback for --phone and --pair-code. CLI
    // arg wins if both are set.
    let phone = env_or_arg(&args.phone, "OCTO_WHATSAPP_PHONE");
    let custom_code = env_or_arg_opt(args.pair_code.as_ref(), "OCTO_WHATSAPP_PAIR_CODE");
    let ws_url = env_or_arg_opt(args.ws_url.as_ref(), "OCTO_WHATSAPP_WS_URL");
    CorePairLinkArgs {
        session_path: args.session_path.clone(),
        phone,
        custom_code,
        groups: args.groups.clone(),
        ws_url,
        timeout_secs: args.timeout,
    }
}

/// R1-H1: env var fallback. Returns the CLI arg if non-empty,
/// else the env var. Returns empty string if both are empty
/// (the core lib's `validate_phone` will reject empty).
fn env_or_arg(arg: &str, env_var: &str) -> String {
    if !arg.is_empty() {
        return arg.to_string();
    }
    std::env::var(env_var).unwrap_or_default()
}

/// R1-H1: env var fallback for `Option<String>` args. Returns
/// the CLI arg if non-empty, else the env var. Returns None if
/// both are absent or empty.
fn env_or_arg_opt(arg: Option<&String>, env_var: &str) -> Option<String> {
    if let Some(s) = arg {
        if !s.is_empty() {
            return Some(s.clone());
        }
    }
    std::env::var(env_var).ok().filter(|s| !s.is_empty())
}

/// Mission 0850p-a-ws-url-release-guard: refuse `--ws-url` in
/// release builds unless `OCTO_WHATSAPP_ALLOW_WS_URL=1` is set in
/// the environment. The flag is a debug-only escape hatch for
/// testing against a mock WhatsApp Web server; in production it
/// is an unnecessary attack surface (an attacker who controls the
/// operator's config could redirect the noise handshake to their
/// own server and harvest all future messages — D-WA-5).
///
/// Debug builds (`cfg!(debug_assertions) == true`) skip the check
/// entirely. Release builds require the env-var override.
fn check_ws_url_allowed(ws_url: Option<&String>) -> std::result::Result<(), OnboardError> {
    if ws_url.is_none() {
        return Ok(());
    }
    if cfg!(debug_assertions) {
        return Ok(());
    }
    if std::env::var("OCTO_WHATSAPP_ALLOW_WS_URL").ok().as_deref() == Some("1") {
        return Ok(());
    }
    Err(OnboardError::WsUrlReleaseForbidden)
}

fn default_session_base_dir() -> std::path::PathBuf {
    let mut base = dirs::data_dir().unwrap_or_else(|| std::path::PathBuf::from("."));
    base.push("octo");
    base.push("whatsapp");
    base
}

fn load_config(path: &std::path::Path) -> std::result::Result<WhatsAppConfig, OnboardError> {
    let bytes =
        std::fs::read(path).map_err(|e| OnboardError::BadConfig(format!("read {path:?}: {e}")))?;
    serde_json::from_slice(&bytes)
        .map_err(|e| OnboardError::BadConfig(format!("parse {path:?}: {e}")))
}

// R2-H1: check if a `dialoguer::Error` is a "stdin is not a TTY"
// condition. The previous string-match `e.to_string().contains(
// "not a terminal")` was fragile (depends on dialoguer's exact
// error wording). Now we check the structured `std::io::ErrorKind`
// inside the `dialoguer::Error::IO` variant. Note: dialoguer 0.11's
// `Error` is a single-variant enum (just `IO(IoError)`), so the
// destructure is irrefutable.
fn not_tty_to_bad_config(e: &dialoguer::Error) -> Option<OnboardError> {
    let dialoguer::Error::IO(io_err) = e;
    if io_err.kind() == std::io::ErrorKind::NotConnected {
        Some(OnboardError::BadConfig(
            "session remove requires a TTY (pass --yes to skip the interactive prompt)".into(),
        ))
    } else {
        None
    }
}

// R1-L1: `build_adapter` doesn't error (WhatsAppWebAdapter::new
// never fails). Changed to return the adapter directly.
// R2-L2: also call cfg.validate() for defense-in-depth (catches
// config corruption from `whoami` / `session verify` paths that
// load a pre-existing config from disk).
fn build_adapter(
    session_path: &std::path::Path,
    groups: &[String],
) -> Result<octo_whatsapp_onboard_core::WhatsAppWebAdapter, OnboardError> {
    let cfg = WhatsAppConfig {
        session_path: format!("{}", session_path.display()),
        pair_phone: None,
        pair_code: None,
        ws_url: None,
        groups: groups.to_vec(),
        sender_allowlist: Default::default(),
    };
    // Validate field shape (R1-H1 + R2-L2). For link flows, the
    // binary has already validated inputs via clap; this is
    // belt-and-suspenders for the whoami / verify / list paths
    // that load configs from disk.
    cfg.validate()
        .map_err(|e| OnboardError::BadConfig(format!("invalid config: {e}")))?;
    Ok(octo_whatsapp_onboard_core::WhatsAppWebAdapter::new(cfg))
}

// R1-C2 / R1-H2: whoami and session verify must start the bot
// before polling self_handle() / health_check(). Without this,
// self_handle() is always None (the bot's on_event handler
// populates it after Event::Connected) and the call always
// times out.
async fn start_bot(
    adapter: &octo_whatsapp_onboard_core::WhatsAppWebAdapter,
) -> std::result::Result<(), OnboardError> {
    adapter
        .start_bot()
        .await
        .map_err(|e| OnboardError::Generic(anyhow::anyhow!("start_bot: {e}")))
}

/// List sessions under `base_dir`. Reads `*.meta.json` sidecar if
/// present, else falls back to `wait_for_health` on the DB.
///
/// R1-M1: `is_valid` is reported from the sidecar presence (the
/// sidecar is only written after `Event::Connected`, so its
/// presence IS a validity signal). This avoids a 5s bot-startup
/// per DB in the common case. The fallback is documented as a
/// hint, not a live health check.
async fn list_sessions(
    base_dir: &std::path::Path,
) -> std::result::Result<Vec<SessionInfo>, OnboardError> {
    if !base_dir.exists() {
        return Ok(vec![]);
    }
    let entries = std::fs::read_dir(base_dir)
        .map_err(|e| OnboardError::BadConfig(format!("read_dir {base_dir:?}: {e}")))?;
    let db_paths: Vec<_> = entries
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| p.extension().and_then(|e| e.to_str()) == Some("db"))
        .collect();
    let mut out = Vec::with_capacity(db_paths.len());
    for path in db_paths {
        let sidecar = path.with_extension("db.meta.json");
        let (self_phone, last_linked_at) = if sidecar.exists() {
            read_sidecar(&sidecar).unwrap_or((None, None))
        } else {
            (None, None)
        };
        // R1-M1: sidecar presence indicates successful link at last
        // write. Not a live health check — for that, use
        // `session verify <db-path>`.
        let is_valid = sidecar.exists();
        out.push(SessionInfo {
            session_path: path,
            self_phone,
            is_valid,
            last_linked_at,
        });
    }
    Ok(out)
}

fn read_sidecar(
    path: &std::path::Path,
) -> std::result::Result<(Option<String>, Option<String>), OnboardError> {
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
