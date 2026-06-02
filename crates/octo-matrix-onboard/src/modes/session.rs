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
        _ => default_store_path(),
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
    eprintln!(
        "{:<4} {:<32} {:<14} {:<32} {:<10} {:<8} LAST_USED",
        "POS", "USER_ID", "DEVICE_ID", "HOMESERVER", "TYPE", "AGE"
    );
    for s in &sessions {
        eprintln!(
            "{:<4} {:<32} {:<14} {:<32} {:<10} {:<8} {}",
            s.position,
            s.user_id,
            s.device_id,
            s.homeserver_url,
            s.login_type.as_str(),
            format!("{}s", now_epoch().saturating_sub(s.login_timestamp)),
            epoch_to_iso(s.last_used),
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

    // `login_type` is a 0850h-d concept; legacy 0850h-a / 0850h-c
    // configs don't carry it. Default to `password` (the most
    // common path) — operators who onboarded via OIDC / SSO / QR
    // can re-import by hand if they want the type captured
    // accurately.
    let row = SessionRow {
        user_id: user_id.clone(),
        device_id: device_id.clone(),
        homeserver_url,
        access_token,
        refresh_token,
        login_type: LoginType::Password,
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
