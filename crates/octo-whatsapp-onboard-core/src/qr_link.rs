//! QR-code-based linking.
//!
//! R5-H1: use the shared `wait_for_connected` helper (do not
//! inline-poll). R5-M2: sidecar-first ordering. R1-C2: the
//! `pair_code` is never written to the on-disk config or sidecar.
//!
//! R2-H1: CLI-side pre-flight check `session_path` parent dir
//! creatable, `groups` non-empty strings, `ws_url` starts with
//! `ws://` or `wss://`.

use crate::error::{CoreError, Result};
use crate::output::QrLinkArgs;
use crate::output::WhatsAppSession;
use crate::sidecar::{write_sidecar, SidecarMode};
use octo_adapter_whatsapp::{WhatsAppConfig, WhatsAppWebAdapter};
use octo_network::dot::adapters::PlatformAdapter;

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
        sender_allowlist: Default::default(),
        passkey_authenticator: None,
    };
    config
        .validate()
        .map_err(|e| CoreError::InvalidSessionPath {
            path: args.session_path.clone(),
            reason: e,
        })?;

    let adapter = WhatsAppWebAdapter::new(config);
    adapter
        .start_bot()
        .await
        .map_err(|e| CoreError::Adapter(anyhow::anyhow!("start_bot: {e}")))?;

    let timeout = std::time::Duration::from_secs(args.timeout_secs);

    if args.wait_sync {
        // --wait-sync mode: wait for the full history sync to complete.
        // This is the most reliable connection signal — Event::Connected
        // sometimes doesn't fire after pairing, but HistorySync and
        // OfflineSyncCompleted always do when the connection is alive.
        eprintln!("Waiting for initial history sync...");
        crate::session::wait_for_synced(&adapter, timeout).await?;
        eprintln!("History sync complete.");

        // The sync proved the connection is alive. Now resolve the
        // phone from the device snapshot (which was populated during
        // the sync and pairing flow).
        let phone = resolve_phone_from_adapter(&adapter)
            .await
            .ok_or(crate::error::CoreError::SessionExpired)?;
        let session = WhatsAppSession {
            self_phone: Some(phone),
            session_path: args.session_path.clone(),
            groups: args.groups.clone(),
            pair_phone: None,
        };
        write_sidecar(&args.session_path, &session, SidecarMode::QrLink)?;
        let _ = adapter.shutdown().await;
        Ok(session)
    } else {
        // Standard mode: wait for Event::Connected (or HistorySync fallback).
        let phone = crate::session::wait_for_connected(&adapter, timeout).await?;
        let session = WhatsAppSession {
            self_phone: Some(phone),
            session_path: args.session_path.clone(),
            groups: args.groups.clone(),
            pair_phone: None,
        };
        write_sidecar(&args.session_path, &session, SidecarMode::QrLink)?;
        let _ = adapter.shutdown().await;
        Ok(session)
    }
}

fn validate_qr_link_args(args: &QrLinkArgs) -> Result<()> {
    crate::validate_session_args(&args.session_path)
}

/// Try to resolve the phone number from the adapter's self_handle
/// or by polling the device snapshot. Returns None if unresolvable.
async fn resolve_phone_from_adapter(adapter: &WhatsAppWebAdapter) -> Option<String> {
    // Fast path: already resolved by the Event::Connected or
    // Event::HistorySync handler.
    if let Some(phone) = adapter.self_handle() {
        return Some(phone);
    }
    // Slow path: poll for a few seconds.
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
    while std::time::Instant::now() < deadline {
        if let Some(phone) = adapter.self_handle() {
            return Some(phone);
        }
        tokio::time::sleep(std::time::Duration::from_millis(250)).await;
    }
    None
}
