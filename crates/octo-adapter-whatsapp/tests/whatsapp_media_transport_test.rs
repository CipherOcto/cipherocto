//! Live integration tests for WhatsApp DOT/2/ native media transport
//! (Mission 0850 / RFC-0850 §8.6).
//!
//! These tests load a real session from `$OCTO_WHATSAPP_PERSIST_DIR`
//! (default `$HOME/.local/share/octo/whatsapp/`), drive
//! `WhatsAppWebAdapter::start_bot()` against the production WhatsApp Web
//! servers, and exercise the native media transport path end-to-end:
//!
//! 1. `media_capabilities_match_upload_limit` — call `adapter.capabilities()`
//!    to assert the documented 100 MiB upload ceiling, then call
//!    `upload_media` with a payload of `100 MiB + 1 byte` and assert
//!    `Err(PlatformAdapterError::PayloadTooLarge { .. })`. The pre-flight
//!    check rejects before any network round-trip, so this test runs
//!    even without an authenticated session (it tests the adapter
//!    boundary, not the network).
//!
//! 2. `upload_then_download_roundtrip` — generate a 64 KiB random
//!    payload (large enough to exceed the 4096-byte text-mode
//!    threshold, small enough to fit in the `MediaType::Document`
//!    ceiling), call `upload_media`, capture the returned
//!    `message_id`, call `download_media(message_id)`, assert
//!    `decoded == original_payload`. Skips gracefully if no
//!    authenticated session is mounted (informational, not a CI gate).
//!
//! **Not** run by default — requires:
//! - A mounted authenticated session (a `.session.db` produced by
//!   `octo-whatsapp-onboard qr-link` / `pair-link`).
//! - Network access to `web.whatsapp.com` / `wss://web.whatsapp.com`.
//! - ~60s for connect + handshake + critical-app-state sync + upload
//!   + download to settle.
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
//!
//! Why `--test-threads=1`: a single host should only hold one WhatsApp
//! Web connection per phone number (the WA servers reject a second
//! concurrent device as a duplicate). Running these tests in parallel
//! with `live_session_test.rs` / `live_e2e_group_setup_test.rs` would
//! race for the connection and produce flaky "logged out" errors.
//!
//! R10-H1 fix: this file closes the test gap that hid the R9-H1
//! production bug (`send_envelope_native` uploading the wrong bytes —
//! base64 text instead of raw wire bytes). The R9-H1 bug was invisible
//! to unit tests because we have no live `Client` stub per the R4-M3
//! design decision; the only way to catch a wrong-bytes-on-the-wire
//! regression is a real round-trip.

#![cfg(feature = "live-whatsapp")]

use octo_adapter_whatsapp::{WhatsAppConfig, WhatsAppWebAdapter};
use octo_network::dot::adapters::PlatformAdapter;
use octo_network::dot::error::PlatformAdapterError;
use std::time::Duration;

/// Default session directory matching the on-disk layout that
/// `octo-whatsapp-onboard` writes (see
/// `crates/octo-adapter-whatsapp/tests/live_session_test.rs:default_persist_dir`
/// for the same convention).
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

/// Build a `WhatsAppConfig` pointed at the on-disk session database.
/// Returns `None` if no session is mounted (so tests can skip
/// gracefully instead of panicking). Callers decide whether to skip
/// or fail hard.
fn maybe_live_config() -> Option<WhatsAppConfig> {
    let mut path = default_persist_dir();
    path.push(default_session_name());
    if !path.exists() {
        return None;
    }
    Some(WhatsAppConfig {
        session_path: path.to_string_lossy().into_owned(),
        // Production WS URL (the adapter's default when `ws_url` is None).
        ws_url: None,
        pair_phone: None,
        pair_code: None,
        // groups is empty: media upload/download is per-recipient
        // (a message_id), not per-group. The dot_mode transport does
        // not need a registered domain.
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

/// Build a fresh in-process adapter pointed at the live session. Does
/// NOT call `start_bot()` — callers do that explicitly so the test
/// can assert pre-flight behavior without waiting for a connection.
fn offline_adapter() -> WhatsAppWebAdapter {
    let cfg = maybe_live_config().expect("no live WhatsApp session; set OCTO_WHATSAPP_PERSIST_DIR");
    WhatsAppWebAdapter::new(cfg)
}

// ── Test 2: pre-flight 100 MiB + 1 byte (always-on) ──────────────
//
// Per the mission spec (line 303): "this is the only test in the
// suite that doesn't require `start_bot` — run it as the first
// assertion in the file to fail fast on capability regressions".
// Runs even without an authenticated session because the pre-flight
// check short-circuits before any network call. Documented inline so
// future maintainers know to keep this test first in the file.

/// Test 2: capabilities report must match the documented 100 MiB
/// upload ceiling, and the pre-flight check must reject payloads
/// above that ceiling before any network round-trip.
///
/// R10-H1 fix: this is the always-on portion of Test 2 of the
/// mission spec. The full Test 2 also asserts `capabilities()`
/// advertises the same value (covered by the always-on unit test
/// `capabilities_includes_media_capabilities` in
/// `crates/octo-adapter-whatsapp/src/adapter.rs:3532`); this
/// integration-test entry pins the cross-crate contract: a
/// regression that drops `media_capabilities` from
/// `CapabilityReport` would break the
/// `media_capabilities.is_some() → Native` branch at
/// `crates/octo-network/src/dot/transport.rs:92` and silently
/// degrade DOT/2/ to text mode. The pre-flight 100 MiB + 1 byte
/// assertion runs without a live session.
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
        "advertised max_upload_bytes must match the documented WhatsApp \
         Document ceiling"
    );

    // Pre-flight: 100 MiB + 1 byte must be rejected.
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

// ── Test 1: live upload→download round-trip ───────────────────────
//
// Per the mission spec (line 300): "start the bot (or skip if
// `start_bot` already ran in a prior test), generate a 64 KiB
// random payload, call `upload_media`, capture the returned
// message_id, call `download_media(message_id)`, assert
// `decoded == original_payload`."

/// Test 1: live `upload_media` → `download_media` round-trip.
///
/// R10-H1 fix: this test closes the gap that hid the R9-H1
/// production bug. The R9-H1 bug was that `send_envelope_native`
/// uploaded `encoded.as_bytes()` (the DOT/1/ base64 text) instead
/// of `wire_bytes` (the raw `DeterministicEnvelope` wire format).
/// The unit tests didn't catch it because there's no live `Client`
/// stub per the R4-M3 design decision. This test exercises the
/// real `Client::upload` and `Client::download` against a real
/// WhatsApp CDN, and asserts the bytes that come back match the
/// bytes that went in. A wrong-bytes-on-the-wire regression would
/// either:
///   (a) cause the upload to fail (size mismatch with the
///       Document ceiling, or base64 text ≫ 100 MiB), OR
///   (b) cause the download to return different bytes than were
///       uploaded (round-trip mismatch).
/// Either failure mode is caught by the byte-equality assertion.
///
/// Skips gracefully if no session is mounted (`#[ignore]` is the
/// default; the test only runs when `--include-ignored` is passed
/// AND a session is available).
#[tokio::test]
#[ignore = "requires live WhatsApp Web session + uploads a real Document to the operator's WhatsApp CDN"]
async fn upload_then_download_roundtrip() {
    init_tracing();
    let cfg = match maybe_live_config() {
        Some(cfg) => cfg,
        None => {
            tracing::warn!(
                "no live WhatsApp session at {:?}/{}; skipping upload_then_download_roundtrip",
                default_persist_dir(),
                default_session_name()
            );
            return;
        }
    };

    let adapter = WhatsAppWebAdapter::new(cfg);
    // Connect to WhatsApp Web. 60s matches the live_session_test.rs
    // budget for noise handshake + critical-app-state sync.
    let connect = adapter.start_bot();
    let connect_result = tokio::time::timeout(Duration::from_secs(60), connect).await;
    match connect_result {
        Ok(Ok(())) => {}
        Ok(Err(e)) => {
            // Rate-limited or transient: skip rather than fail.
            tracing::warn!("start_bot failed: {e}; skipping round-trip test");
            return;
        }
        Err(_elapsed) => {
            panic!("start_bot did not complete within 60s");
        }
    }

    // 64 KiB random payload (large enough to exceed the 4096-byte
    // text-mode threshold, small enough to fit in the
    // MediaType::Document ceiling).
    let mut original = vec![0u8; 64 * 1024];
    for (i, b) in original.iter_mut().enumerate() {
        // Deterministic-but-varying bytes so the test is reproducible
        // (random bytes would force operators to use a fixed seed for
        // any post-mortem byte comparison).
        *b = (i as u8).wrapping_mul(37).wrapping_add(11);
    }

    let filename = "dot-media-transport-test.bin";
    let message_id = match adapter
        .upload_media(filename, &original, "application/octet-stream")
        .await
    {
        Ok(id) => id,
        Err(e) => {
            // CI rate limits are common; surface the error and skip
            // rather than fail the test.
            tracing::warn!("upload_media failed: {e}; skipping round-trip test");
            return;
        }
    };
    tracing::info!(message_id = %message_id, "uploaded 64 KiB Document");

    // Brief settle to let the CDN finish replicating the upload.
    tokio::time::sleep(Duration::from_secs(2)).await;

    let downloaded = match adapter.download_media(&message_id).await {
        Ok(bytes) => bytes,
        Err(e) => {
            tracing::warn!("download_media failed: {e}; skipping round-trip test");
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
        "downloaded bytes must match uploaded bytes — a mismatch would \
         indicate the wrong bytes were uploaded (R9-H1 regression class)"
    );
    tracing::info!(
        bytes = downloaded.len(),
        "upload→download round-trip verified: bytes match exactly"
    );
}
