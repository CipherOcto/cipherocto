//! QR-code-based linking.
//!
//! R5-H1: use the shared `wait_for_connected` helper (do not
//! inline-poll). R5-M2: sidecar-first ordering. R1-C2: the
//! `pair_code` is never written to the on-disk config or sidecar.
//!
//! R2-H1: CLI-side pre-flight check `session_path` parent dir
//! creatable, `groups` non-empty strings, `ws_url` starts with
//! `ws://` or `wss://`.

use octo_adapter_whatsapp::{WhatsAppConfig, WhatsAppWebAdapter};

use crate::error::{CoreError, Result};
use crate::output::WhatsAppSession;
use crate::output::QrLinkArgs;
use crate::sidecar::{write_sidecar, SidecarMode};

/// Run the qr-link flow: build adapter, start bot, wait for
/// `Event::Connected`, write sidecar + session (the binary writes
/// the config separately).
///
/// R1-M4: takes `&QrLinkArgs` (by reference) so the binary can
/// pass the args struct directly without `clone()`-ing the
/// `OutputArgs` field. This matches the matrix-onboard pattern.
pub async fn run(args: &QrLinkArgs) -> Result<WhatsAppSession> {
    validate_qr_link_args(args)?;

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
    crate::validate_session_args(&args.session_path)
}
