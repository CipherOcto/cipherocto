//! Multi-account session-store subcommands (mission 0850h-d).
//!
//! Four subcommands, all operating on the stoolap-backed store
//! (`octo-matrix-session-store`):
//!
//! - `session list` — print all sessions, ordered by insertion
//!   position. Each row includes user_id, device_id, homeserver,
//!   login type, and a redacted token preview.
//! - `session use <user_id> <device_id>` — mark a session as the
//!   most-recently-used (`set_latest_session`). Updates
//!   `last_used` only; never changes `position`.
//! - `session remove <user_id> <device_id>` — drop a session from
//!   the store. Refuses when the row is missing.
//! - `session import <file>` — read a legacy 0850h-a / 0850h-c JSON
//!   config and insert a row. Refuses to overwrite an existing
//!   `(user_id, device_id)` unless `--force` is set.

use crate::cli::{SessionImportArgs, SessionListArgs, SessionRemoveArgs, SessionUseArgs};
use crate::error::{OnboardError, Result};
use octo_matrix_session_store::{
    default_store_path, LoginType, SessionRow, SessionStore, SessionStoreError, StoolapSessionStore,
};
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

/// Open the store at the operator-specified path, or at the
/// per-platform default when `--store` is not set.
fn open_store(path: Option<&PathBuf>) -> Result<StoolapSessionStore> {
    let resolved = match path {
        Some(p) if !p.as_os_str().is_empty() => p.clone(),
        _ => default_store_path().map_err(|e| {
            OnboardError::BadConfig(format!(
                "{e} — pass --store <path> to specify the location explicitly"
            ))
        })?,
    };
    StoolapSessionStore::new(&resolved)
        .map_err(|e| OnboardError::Generic(anyhow::anyhow!("open store: {}", e)))
}

fn now_epoch() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

/// Redact a token for display: show the first 8 characters and `***`.
fn redact_token(token: &str) -> String {
    if token.len() > 8 {
        format!("{}***", &token[..8])
    } else {
        "***".to_string()
    }
}

fn epoch_to_iso(epoch: i64) -> String {
    // Local ISO-style timestamp without bringing in chrono. We use
    // the standard `DateTime<UNIX_EPOCH>` formatter via
    // `format!("{:?}", ...)` (debug formatter includes the timezone
    // offset as `+00:00`); this is good enough for CLI display.
    let dt = std::time::UNIX_EPOCH + std::time::Duration::from_secs(epoch.max(0) as u64);
    format!("{:?}", dt)
}

pub async fn list(args: SessionListArgs) -> Result<()> {
    let store = open_store(args.store.store.as_ref())?;
    let sessions: Vec<SessionRow> = store
        .get_all_sessions()
        .await
        .map_err(|e: SessionStoreError| OnboardError::Generic(anyhow::anyhow!("list: {}", e)))?;
    if sessions.is_empty() {
        eprintln!("(no sessions in the store)");
        return Ok(());
    }
    // R2-M11: compute column widths from the actual data so a
    // long user_id / homeserver URL doesn't get silently
    // truncated. We pad each value to the max of its column's
    // data width and the header width, then use `eprintln!` with
    // the same widths for alignment. (Adding `comfy-table` would
    // be heavier than this — 4 lines of code, no new dep.)
    let header = [
        "POS",
        "USER_ID",
        "DEVICE_ID",
        "HOMESERVER",
        "TYPE",
        "LOGIN_AGE",
    ];
    let width_user = sessions
        .iter()
        .map(|s| s.user_id.chars().count())
        .max()
        .unwrap_or(0)
        .max(header[1].len());
    let width_device = sessions
        .iter()
        .map(|s| s.device_id.chars().count())
        .max()
        .unwrap_or(0)
        .max(header[2].len());
    let width_homeserver = sessions
        .iter()
        .map(|s| s.homeserver_url.chars().count())
        .max()
        .unwrap_or(0)
        .max(header[3].len());
    eprintln!(
        "{:<4} {:<wu$} {:<wd$} {:<wh$} {:<10} {:<12} LAST_USED",
        header[0],
        header[1],
        header[2],
        header[3],
        header[4],
        header[5],
        wu = width_user,
        wd = width_device,
        wh = width_homeserver,
    );
    for s in &sessions {
        // R1-M16: LOGIN_AGE is "time since the store's recorded
        // `login_timestamp`". For sessions added via `session import`
        // the legacy 0850h-a / 0850h-c config didn't carry a
        // timestamp, so the store overwrites it to `now_epoch()` at
        // import time. The column therefore reports the time since
        // import, not the time since the original login.
        let age_label = if s.login_timestamp == 0 {
            "unknown".to_string()
        } else {
            format!("{}s", now_epoch().saturating_sub(s.login_timestamp))
        };
        eprintln!(
            "{:<4} {:<wu$} {:<wd$} {:<wh$} {:<10} {:<12} {}",
            s.position,
            s.user_id,
            s.device_id,
            s.homeserver_url,
            s.login_type.as_str(),
            age_label,
            epoch_to_iso(s.last_used),
            wu = width_user,
            wd = width_device,
            wh = width_homeserver,
        );
        eprintln!(
            "     access_token: {}  refresh_token: {}",
            redact_token(&s.access_token),
            s.refresh_token
                .as_deref()
                .map(redact_token)
                .unwrap_or_else(|| "<none>".to_string()),
        );
    }
    Ok(())
}

pub async fn use_(args: SessionUseArgs) -> Result<()> {
    let store = open_store(args.store.store.as_ref())?;
    store
        .set_latest_session(&args.user_id, &args.device_id)
        .await
        .map_err(|e: SessionStoreError| {
            OnboardError::Generic(anyhow::anyhow!(
                "set latest {} / {}: {}",
                args.user_id,
                args.device_id,
                e
            ))
        })?;
    eprintln!(
        "Marked {} / {} as the most-recently-used session (last_used updated; position unchanged).",
        args.user_id, args.device_id
    );
    Ok(())
}

pub async fn remove(args: SessionRemoveArgs) -> Result<()> {
    let store = open_store(args.store.store.as_ref())?;
    store
        .remove_session(&args.user_id, &args.device_id)
        .await
        .map_err(|e: SessionStoreError| match e {
            SessionStoreError::NotFound { .. } => OnboardError::Generic(anyhow::anyhow!(
                "no session for {} / {}",
                args.user_id,
                args.device_id
            )),
            other => OnboardError::Generic(anyhow::anyhow!("remove: {}", other)),
        })?;
    eprintln!("Removed session {} / {}.", args.user_id, args.device_id);
    Ok(())
}

pub async fn import(args: SessionImportArgs) -> Result<()> {
    let store = open_store(args.store.store.as_ref())?;

    // Read the on-disk JSON directly. The on-disk shape is
    // `homeserver_url / user_id / device_id / access_token /
    // refresh_token / rooms` (see `octo-adapter-matrix-sdk::config_writer::OnDiskConfig`).
    // We deliberately do NOT try to `restore_session` here — that
    // would dial the homeserver, which has no value for a pure
    // import. The field-level checks below catch malformed JSON
    // and missing fields; the e2ee subcommands do the network
    // validation when they actually need the SDK.
    let bytes = std::fs::read(&args.file)
        .map_err(|e| OnboardError::Generic(anyhow::anyhow!("read {:?}: {}", args.file, e)))?;
    let on_disk: serde_json::Value = serde_json::from_slice(&bytes)
        .map_err(|e| OnboardError::Generic(anyhow::anyhow!("parse {:?}: {}", args.file, e)))?;

    let homeserver_url = on_disk["homeserver_url"]
        .as_str()
        .ok_or_else(|| OnboardError::Generic(anyhow::anyhow!("missing homeserver_url")))?
        .to_string();
    let user_id = on_disk["user_id"]
        .as_str()
        .ok_or_else(|| OnboardError::Generic(anyhow::anyhow!("missing user_id")))?
        .to_string();
    let device_id = on_disk["device_id"]
        .as_str()
        .ok_or_else(|| OnboardError::Generic(anyhow::anyhow!("missing device_id")))?
        .to_string();
    let access_token = on_disk["access_token"]
        .as_str()
        .ok_or_else(|| OnboardError::Generic(anyhow::anyhow!("missing access_token")))?
        .to_string();
    let refresh_token = on_disk["refresh_token"].as_str().map(str::to_string);

    // R2-M10: the operator can now set the login type via
    // `--login-type` (the legacy JSON does not carry one). Default
    // is still `Password` for back-compat, but OIDC / SSO / QR
    // operators should set the flag to avoid a misleading `password`
    // label in `session list`.
    let login_type = match args.login_type {
        crate::cli::LoginTypeArg::Password => LoginType::Password,
        crate::cli::LoginTypeArg::Oidc => LoginType::Oidc,
        crate::cli::LoginTypeArg::Sso => LoginType::Sso,
        crate::cli::LoginTypeArg::Qr => LoginType::Qr,
    };
    let row = SessionRow {
        user_id: user_id.clone(),
        device_id: device_id.clone(),
        homeserver_url,
        access_token,
        refresh_token,
        login_type,
        login_timestamp: 0,
        last_used: 0,
        position: 0,
        display_name: None,
        avatar_url: None,
    };
    store
        .add_session(&row, args.force)
        .await
        .map_err(|e: SessionStoreError| match e {
            SessionStoreError::AlreadyExists { .. } => OnboardError::Generic(anyhow::anyhow!(
                "session already exists for {} / {} (pass --force to overwrite)",
                user_id,
                device_id
            )),
            other => OnboardError::Generic(anyhow::anyhow!("import: {}", other)),
        })?;
    eprintln!(
        "Imported session {} / {} from {:?}.",
        user_id, device_id, args.file
    );
    Ok(())
}
