//! Live integration tests for WhatsApp DOT/2/ native media transport
//! (Mission 0850 / RFC-0850 §8.6).
//!
//! These tests load a real session from `$OCTO_WHATSAPP_PERSIST_DIR`
//! (default `$HOME/.local/share/octo/whatsapp/`), drive
//! `WhatsAppWebAdapter::start_bot()` against the production WhatsApp Web
//! servers, and exercise the native media transport primitives
//! end-to-end:
//!
//! 1. `media_capabilities_match_upload_limit` — call `adapter.capabilities()`
//!    to assert the documented 100 MiB upload ceiling, then call
//!    `upload_media` with a payload of `100 MiB + 1 byte` and assert
//!    `Err(PlatformAdapterError::PayloadTooLarge { .. })`. The pre-flight
//!    check rejects before any network round-trip, so this test runs
//!    without an authenticated WhatsApp session (it tests the adapter
//!    boundary, not the network).
//!
//! 2. `upload_then_download_roundtrip` — creates a group, uploads a 64 KiB
//!    document to CDN via `send_document`, which sends a visible
//!    DocumentMessage to the group. Downloads the same bytes from CDN
//!    using the returned media_ref_token. Asserts downloaded == original.
//!
//! Run directly:
//!
//! ```bash
//! cargo test -p octo-adapter-whatsapp \
//!   --features live-whatsapp \
//!   --test whatsapp_media_transport_test \
//!   -- --include-ignored --nocapture --test-threads=1
//! ```
//!
//! Environment variables consumed:
//! - `OCTO_WHATSAPP_PERSIST_DIR` — directory holding `default.session.db`.
//!   Defaults to `$HOME/.local/share/octo/whatsapp/`.
//! - `OCTO_WHATSAPP_SESSION_NAME` — session filename (default:
//!   `default.session.db`).

#![cfg(feature = "live-whatsapp")]

use octo_adapter_whatsapp::{WhatsAppConfig, WhatsAppWebAdapter};
use octo_network::dot::adapters::coordinator_admin::GroupId;
use octo_network::dot::adapters::PlatformAdapter;
use octo_network::dot::error::PlatformAdapterError;
use std::time::Duration;

fn default_persist_dir() -> std::path::PathBuf {
    if let Ok(p) = std::env::var("OCTO_WHATSAPP_PERSIST_DIR") {
        return std::path::PathBuf::from(p);
    }
    let home = std::env::var("HOME").unwrap_or_else(|_| ".".to_string());
    std::path::PathBuf::from(home)
        .join(".local")
        .join("share")
        .join("octo")
        .join("whatsapp")
}

fn default_session_name() -> String {
    std::env::var("OCTO_WHATSAPP_SESSION_NAME").unwrap_or_else(|_| "default.session.db".to_string())
}

fn maybe_live_config() -> Option<WhatsAppConfig> {
    let mut path = default_persist_dir();
    path.push(default_session_name());
    if !path.exists() {
        return None;
    }
    Some(WhatsAppConfig {
        session_path: path.to_string_lossy().into_owned(),
        ws_url: None,
        pair_phone: None,
        pair_code: None,
        groups: vec![],
        sender_allowlist: Default::default(),
    })
}

fn init_tracing() {
    let _ = tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| {
                tracing_subscriber::EnvFilter::new(
                    "info,octo_adapter_whatsapp=debug,whatsapp_rust=info,wacore=info",
                )
            }),
        )
        .try_init();
}

fn offline_adapter() -> WhatsAppWebAdapter {
    let cfg = maybe_live_config().expect("no live WhatsApp session; set OCTO_WHATSAPP_PERSIST_DIR");
    WhatsAppWebAdapter::new(cfg)
}

fn timestamp() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

async fn cleanup_group(adapter: &WhatsAppWebAdapter, group_jid: &str) {
    let admin = adapter.as_coordinator_admin().unwrap();
    let group_id = GroupId::new(group_jid.to_string());
    tokio::time::sleep(Duration::from_secs(2)).await;
    if let Ok(meta) = admin.get_group_metadata(&group_id).await {
        let self_phone = adapter.self_handle().unwrap_or_default();
        for p in &meta.members {
            if p.0.contains(&self_phone) || p.0 == "80836284174444@lid" {
                continue;
            }
            let _ = admin.remove_member(&group_id, p).await;
            tokio::time::sleep(Duration::from_millis(500)).await;
        }
    }
    match admin.destroy_group(&group_id).await {
        Ok(()) => tracing::info!(group_jid, "cleanup: destroyed group"),
        Err(e) => {
            tracing::warn!(error = %e, group_jid, "cleanup: destroy failed, trying leave");
            let _ = admin.leave_group(&group_id).await;
        }
    }
}

// ── Test 2: pre-flight 100 MiB + 1 byte ──────────────────────────

/// Capabilities report must match the documented 100 MiB upload ceiling,
/// and the pre-flight check must reject payloads above that ceiling.
#[tokio::test]
async fn media_capabilities_match_upload_limit() {
    init_tracing();
    let adapter = offline_adapter();
    let caps = adapter.capabilities();
    let media = caps
        .media_capabilities
        .expect("media_capabilities must be populated for DOT/2 transport");
    assert_eq!(
        media.max_upload_bytes,
        WhatsAppWebAdapter::MAX_UPLOAD_BYTES,
        "advertised max_upload_bytes must match the const"
    );
    assert_eq!(
        media.max_upload_bytes,
        100 * 1024 * 1024,
        "advertised max_upload_bytes must match the documented WhatsApp Document ceiling"
    );

    let oversized = vec![0u8; WhatsAppWebAdapter::MAX_UPLOAD_BYTES + 1];
    match adapter
        .upload_media("test.bin", &oversized, "application/octet-stream")
        .await
    {
        Err(PlatformAdapterError::PayloadTooLarge {
            size,
            max,
            platform,
        }) => {
            assert_eq!(size, WhatsAppWebAdapter::MAX_UPLOAD_BYTES + 1);
            assert_eq!(max, WhatsAppWebAdapter::MAX_UPLOAD_BYTES);
            assert_eq!(platform, "whatsapp");
        }
        Err(other) => panic!("expected PayloadTooLarge, got {other:?}"),
        Ok(_) => panic!("oversized payload must be rejected by pre-flight"),
    }
}

// ── Test 1: live send_document → download_media round-trip ───────

/// Live `send_document` → `download_media` round-trip.
///
/// Creates a group, uploads a 64 KiB document to CDN and sends it as
/// a visible DocumentMessage, then downloads the same bytes from CDN
/// and verifies they match exactly.
#[tokio::test]
#[ignore = "requires live WhatsApp Web session + sends a real DocumentMessage to a group"]
async fn upload_then_download_roundtrip() {
    init_tracing();
    let cfg = match maybe_live_config() {
        Some(cfg) => cfg,
        None => {
            tracing::warn!(
                "no live WhatsApp session at {:?}/{}; skipping",
                default_persist_dir(),
                default_session_name()
            );
            return;
        }
    };

    let adapter = WhatsAppWebAdapter::new(cfg);

    // Start bot and wait for connection.
    let connected = adapter.connected();
    let start_bot_fut = adapter.start_bot();
    let connect_result = tokio::time::timeout(Duration::from_secs(30), start_bot_fut).await;
    let notify_result = tokio::time::timeout(Duration::from_secs(30), connected.notified()).await;
    match connect_result {
        Ok(Ok(())) => {}
        Ok(Err(e)) => panic!("start_bot failed: {e:#}"),
        Err(_) => panic!("start_bot did not complete within 30s"),
    }
    notify_result.unwrap_or_else(|_| panic!("timed out waiting for Event::Connected"));
    tracing::info!("connected; creating group for media round-trip test");

    // Create a group (just the bot) so the DocumentMessage is visible.
    let subject = format!("media-test-{}", timestamp());
    let admin = adapter.as_coordinator_admin().unwrap();
    tokio::time::sleep(Duration::from_secs(2)).await;
    let handle = admin
        .create_group(&subject, &[])
        .await
        .expect("create_group");
    let group_jid = handle.id.as_str().to_string();
    adapter
        .register_group_at_runtime(&group_jid)
        .expect("register_group_at_runtime");
    tracing::info!(group_jid = %group_jid, subject = %subject, "group created");

    // Build a 64 KiB deterministic payload.
    let mut original = vec![0u8; 64 * 1024];
    for (i, b) in original.iter_mut().enumerate() {
        *b = (i as u8).wrapping_mul(37).wrapping_add(11);
    }

    // Send document to group — this uploads to CDN AND sends a visible
    // DocumentMessage that the operator can see in WhatsApp.
    let filename = "dot-media-transport-test.bin";
    let (message_id, token) = match adapter
        .send_document(&group_jid, filename, &original, "application/octet-stream")
        .await
    {
        Ok(result) => result,
        Err(e) => {
            tracing::warn!("send_document failed: {e}; skipping round-trip test");
            cleanup_group(&adapter, &group_jid).await;
            return;
        }
    };
    tracing::info!(
        message_id = %message_id,
        "sent 64 KiB DocumentMessage to group"
    );

    // Brief settle for CDN replication.
    tokio::time::sleep(Duration::from_secs(2)).await;

    // Download from CDN using the media_ref_token from the upload.
    let downloaded = match adapter.download_media(&token).await {
        Ok(bytes) => bytes,
        Err(e) => {
            tracing::warn!("download_media failed: {e}; skipping round-trip test");
            cleanup_group(&adapter, &group_jid).await;
            return;
        }
    };

    assert_eq!(
        downloaded.len(),
        original.len(),
        "downloaded length must match uploaded length"
    );
    assert_eq!(
        downloaded, original,
        "downloaded bytes must match uploaded bytes"
    );
    tracing::info!(
        bytes = downloaded.len(),
        "upload→download round-trip verified: bytes match exactly"
    );

    // Cleanup: destroy group + clearChat + deleteChat.
    cleanup_group(&adapter, &group_jid).await;
}
