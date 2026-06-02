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
//! This is the pre-flight assertion the integration test uses to
//! confirm "the config the CLI just wrote is actually valid" before
//! running the real assertions against the adapter.

use crate::cli::WhoamiArgs;
use crate::error::{OnboardError, Result};
use matrix_sdk::authentication::matrix::MatrixSession;
use matrix_sdk::{Client, SessionMeta, SessionTokens};
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

pub async fn run(args: WhoamiArgs) -> Result<()> {
    let bytes = std::fs::read(&args.config)
        .map_err(|e| OnboardError::BadConfig(format!("read {}: {}", args.config.display(), e)))?;
    let cfg: OnboardConfig = serde_json::from_slice(&bytes)
        .map_err(|e| OnboardError::BadConfig(format!("parse {}: {}", args.config.display(), e)))?;

    let user_id = matrix_sdk::ruma::OwnedUserId::try_from(cfg.user_id.as_str()).map_err(|e| {
        OnboardError::BadConfig(format!("invalid user_id '{}': {}", cfg.user_id, e))
    })?;
    let device_id = matrix_sdk::ruma::OwnedDeviceId::from(cfg.device_id.as_str());

    let session = MatrixSession {
        meta: SessionMeta { user_id, device_id },
        tokens: SessionTokens {
            access_token: cfg.access_token.clone(),
            refresh_token: cfg.refresh_token.clone(),
        },
    };

    let client = Client::builder()
        .homeserver_url(&cfg.homeserver_url)
        .handle_refresh_tokens()
        .build()
        .await
        .map_err(|e| {
            let msg = e.to_string();
            if msg.contains("dns") || msg.contains("connect") {
                OnboardError::Unreachable(format!("{}: {}", cfg.homeserver_url, msg))
            } else {
                OnboardError::Generic(anyhow::anyhow!("build client: {}", msg))
            }
        })?;
    client
        .restore_session(session)
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
    if server_device.as_str() != cfg.device_id {
        return Err(OnboardError::BadConfig(format!(
            "device_id mismatch: config says '{}' but server says '{}'",
            cfg.device_id, server_device
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
