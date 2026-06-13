//! QR-code-based linking.
//!
//! R5-H1: use the shared `wait_for_connected` helper (do not
//! inline-poll). R5-M2: sidecar-first ordering. R1-C2: the
//! `pair_code` is never written to the on-disk config or sidecar.
//!
//! R2-H1: CLI-side pre-flight check `session_path` parent dir
//! creatable, `groups` non-empty strings, `ws_url` starts with
//! `ws://` or `wss://`.

use std::path::Path;

use octo_adapter_whatsapp::{WhatsAppConfig, WhatsAppWebAdapter};

use crate::error::{CoreError, Result};
use crate::output::WhatsAppSession;
use crate::output::QrLinkArgs;
use crate::sidecar::{write_sidecar, SidecarMode};

/// Run the qr-link flow: build adapter, start bot, wait for
/// `Event::Connected`, write sidecar + session (the binary writes
/// the config separately).
pub async fn run(args: QrLinkArgs) -> Result<WhatsAppSession> {
    validate_qr_link_args(&args)?;

    let config = WhatsAppConfig {
        session_path: args.session_path.to_string_lossy().into_owned(),
        pair_phone: None,
        pair_code: None,
        ws_url: args.ws_url.clone(),
        groups: args.groups.clone(),
    };
    config.validate().map_err(|e| CoreError::InvalidSessionPath {
        path: args.session_path.clone(),
        reason: e,
    })?;

    let adapter = WhatsAppWebAdapter::new(config);
    adapter
        .start_bot()
        .await
        .map_err(|e| CoreError::Adapter(anyhow::anyhow!("start_bot: {e}")))?;

    let phone = crate::session::wait_for_connected(
        &adapter,
        std::time::Duration::from_secs(args.timeout_secs),
    )
    .await?;

    let session = WhatsAppSession {
        self_phone: Some(phone),
        session_path: args.session_path.clone(),
        groups: args.groups.clone(),
        pair_phone: None,
    };

    // R5-M2: sidecar first, before any config write.
    write_sidecar(&args.session_path, &session, SidecarMode::QrLink)?;

    Ok(session)
}

fn validate_qr_link_args(args: &QrLinkArgs) -> Result<()> {
    if let Some(parent) = args.session_path.parent() {
        if !parent.as_os_str().is_empty() && !parent.exists() {
            // Try to create it.
            std::fs::create_dir_all(parent).map_err(|e| CoreError::InvalidSessionPath {
                path: parent.to_path_buf(),
                reason: format!("cannot create parent directory: {e}"),
            })?;
        }
    }
    for group in &args.groups {
        if group.is_empty() {
            return Err(CoreError::InvalidSessionPath {
                path: args.session_path.clone(),
                reason: "groups contains an empty string".to_string(),
            });
        }
    }
    if let Some(ref ws_url) = args.ws_url {
        if !(ws_url.starts_with("ws://") || ws_url.starts_with("wss://")) {
            return Err(CoreError::InvalidSessionPath {
                path: args.session_path.clone(),
                reason: format!("ws_url {ws_url:?} must start with ws:// or wss://"),
            });
        }
    }
    Ok(())
}

/// R7-L1: get the session path (used by the binary's `output::write`
/// to know where to put the config file).
pub fn session_path(args: &QrLinkArgs) -> &Path {
    &args.session_path
}
