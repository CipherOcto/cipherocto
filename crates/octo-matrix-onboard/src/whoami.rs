//! `whoami` subcommand — load a config, call `/whoami` via
//! `matrix-sdk`'s `Client` directly, print the resolved user/device.
//!
//! Mission 0850h-a §Acceptance Criteria:
//! - `octo-matrix-onboard whoami --config <path>` — load config, call
//!   `/whoami` via `matrix-sdk`'s `Client`, print user/device
//!   (the CLI is a standalone binary and does **not** call through the
//!   adapter cdylib; the adapter is loaded by a host process, not by
//!   the CLI).
//!
//! Mission 0850h-d extended this to support a `--store` flag that
//! reads credentials from the multi-account stoolap store when
//! present (the `(user_id, device_id)` in the file selects the row).
//! Without `--store`, whoami reads the file directly (legacy
//! 0850h-a / 0850h-c behavior).
//!
//! This is the pre-flight assertion the integration test uses to
//! confirm "the config the CLI just wrote is actually valid" before
//! running the real assertions against the adapter.

use crate::cli::WhoamiArgs;
use crate::error::{OnboardError, Result};
use matrix_sdk::authentication::matrix::MatrixSession;
use matrix_sdk::{Client, SessionMeta, SessionTokens};
use octo_session_store::{SessionStore, StoolapSessionStore};
use serde::Deserialize;
use tracing::info;

/// On-disk config shape. We deserialize minimally here — only the
/// fields needed to restore a session and call /whoami. The adapter's
/// `MatrixConfig` is the authoritative schema; this struct exists
/// only to avoid a hard dependency from the binary to the adapter
/// crate (the adapter is a cdylib, not a lib the binary should link).
#[derive(Debug, Clone, Deserialize)]
struct OnboardConfig {
    homeserver_url: String,
    user_id: String,
    device_id: String,
    access_token: String,
    #[serde(default)]
    refresh_token: Option<String>,
}

/// Resolved credentials — either from the store (multi-account) or
/// the file (legacy). The two paths produce the same shape so the
/// downstream SDK calls don't care which was used.
struct ResolvedSession {
    homeserver_url: String,
    user_id: String,
    device_id: String,
    access_token: String,
    refresh_token: Option<String>,
}

async fn load_from_store(
    store_path: &std::path::Path,
    file_user_id: &str,
    file_device_id: &str,
) -> Result<ResolvedSession> {
    let store = StoolapSessionStore::new(store_path).map_err(|e| {
        OnboardError::Generic(anyhow::anyhow!("open store {:?}: {}", store_path, e))
    })?;
    let row =
        store
            .get_session(file_user_id, file_device_id)
            .await
            .map_err(|e| {
                OnboardError::Generic(anyhow::anyhow!(
                    "store lookup for {} / {}: {}",
                    file_user_id,
                    file_device_id,
                    e
                ))
            })?
            .ok_or_else(|| {
                OnboardError::BadConfig(format!(
                "no row in store for {} / {} (was this session imported? run `session import {}`)",
                file_user_id, file_device_id, store_path.display()
            ))
            })?;
    Ok(ResolvedSession {
        homeserver_url: row.homeserver_url,
        user_id: row.user_id,
        device_id: row.device_id,
        access_token: row.access_token,
        refresh_token: row.refresh_token,
    })
}

fn load_from_file(path: &std::path::Path) -> Result<ResolvedSession> {
    let bytes = std::fs::read(path)
        .map_err(|e| OnboardError::BadConfig(format!("read {}: {}", path.display(), e)))?;
    let cfg: OnboardConfig = serde_json::from_slice(&bytes)
        .map_err(|e| OnboardError::BadConfig(format!("parse {}: {}", path.display(), e)))?;
    Ok(ResolvedSession {
        homeserver_url: cfg.homeserver_url,
        user_id: cfg.user_id,
        device_id: cfg.device_id,
        access_token: cfg.access_token,
        refresh_token: cfg.refresh_token,
    })
}

pub async fn run(args: WhoamiArgs) -> Result<()> {
    // Always read the file first — we need `user_id` and `device_id`
    // to look up the store row, and we also need the `homeserver_url`
    // in case the store is unreachable. The file is the metadata
    // anchor in both modes.
    let file_session = load_from_file(&args.config)?;

    let session = match args.store.as_ref() {
        Some(p) if !p.as_os_str().is_empty() => {
            load_from_store(p, &file_session.user_id, &file_session.device_id).await?
        }
        _ => file_session,
    };

    let user_id =
        matrix_sdk::ruma::OwnedUserId::try_from(session.user_id.as_str()).map_err(|e| {
            OnboardError::BadConfig(format!("invalid user_id '{}': {}", session.user_id, e))
        })?;
    let device_id = matrix_sdk::ruma::OwnedDeviceId::from(session.device_id.as_str());

    let sdk_session = MatrixSession {
        meta: SessionMeta {
            user_id: user_id.clone(),
            device_id: device_id.clone(),
        },
        tokens: SessionTokens {
            access_token: session.access_token.clone(),
            refresh_token: session.refresh_token.clone(),
        },
    };

    let client = Client::builder()
        .homeserver_url(&session.homeserver_url)
        .handle_refresh_tokens()
        .build()
        .await
        .map_err(|e| {
            let msg = e.to_string();
            if msg.contains("dns") || msg.contains("connect") {
                OnboardError::Unreachable(format!("{}: {}", session.homeserver_url, msg))
            } else {
                OnboardError::Generic(anyhow::anyhow!("build client: {}", msg))
            }
        })?;
    client
        .restore_session(sdk_session)
        .await
        .map_err(|e| OnboardError::Generic(anyhow::anyhow!("restore_session: {}", e)))?;

    let who = client.whoami().await.map_err(|e| {
        let msg = e.to_string();
        if msg.contains("Unauthorized") || msg.contains("401") || msg.contains("M_UNKNOWN_TOKEN") {
            OnboardError::AuthRejected(format!("whoami: {}", msg))
        } else if msg.contains("dns") || msg.contains("connect") {
            OnboardError::Unreachable(msg)
        } else {
            OnboardError::Generic(anyhow::anyhow!("whoami: {}", msg))
        }
    })?;

    println!("user_id: {}", who.user_id);
    let server_device = who
        .device_id
        .as_ref()
        .ok_or_else(|| OnboardError::BadConfig("server returned no device_id in whoami".into()))?;
    println!("device_id: {}", server_device);
    if server_device.as_str() != session.device_id {
        return Err(OnboardError::BadConfig(format!(
            "device_id mismatch: config says '{}' but server says '{}'",
            session.device_id, server_device
        )));
    }
    info!(user_id = %who.user_id, device_id = %server_device, "whoami ok");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_minimal_config() {
        let j = serde_json::json!({
            "homeserver_url": "https://matrix.example.com",
            "user_id": "@bot:matrix.example.com",
            "device_id": "ABCDEFGHIJ",
            "access_token": "syt_abcdefgh"
        });
        let parsed: OnboardConfig = serde_json::from_value(j).unwrap();
        assert_eq!(parsed.user_id, "@bot:matrix.example.com");
        assert_eq!(parsed.device_id, "ABCDEFGHIJ");
        assert!(parsed.refresh_token.is_none());
    }

    #[test]
    fn parse_with_refresh_token() {
        let j = serde_json::json!({
            "homeserver_url": "https://matrix.example.com",
            "user_id": "@bot:matrix.example.com",
            "device_id": "ABCDEFGHIJ",
            "access_token": "syt_abcdefgh",
            "refresh_token": "syr_xyz"
        });
        let parsed: OnboardConfig = serde_json::from_value(j).unwrap();
        assert_eq!(parsed.refresh_token.as_deref(), Some("syr_xyz"));
    }

    #[test]
    fn reject_missing_required_field() {
        let j = serde_json::json!({
            "homeserver_url": "https://matrix.example.com",
            "user_id": "@bot:matrix.example.com",
            "device_id": "ABCDEFGHIJ"
            // access_token missing
        });
        let parsed: std::result::Result<OnboardConfig, _> = serde_json::from_value(j);
        assert!(parsed.is_err());
    }
}
