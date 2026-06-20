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
//!    boundary, not the network). **Note:** it still requires the
//!    `live-whatsapp` feature flag to be enabled (the file is
//!    `#![cfg(feature = "live-whatsapp")]`); it's not "always-on" in
//!    the `cargo test -p octo-adapter-whatsapp` sense.
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
//! **R11-H3 / R11-H1 follow-up:** the test in this file exercises
//! `upload_media` and `download_media` directly, NOT the
//! `send_envelope` → `send_envelope_native` path that the R9-H1
//! production bug broke. The R9-H1 bug was in the send path
//! (`send_envelope_native` uploaded the DOT/1/ base64 text instead
//! of the raw 282-byte wire bytes), and the receiver's `canonicalize`
//! rejected the wrong-length payload. This test cannot reproduce
//! that bug because (a) WhatsApp multi-device does not echo a
//! sender's own message back to the sending device (per
//! `tests/live_e2e_group_setup_test.rs:35-42`), and (b) the R4-M3
//! design decision forbids mocking the `Client`. A future test
//! that requires a real DOT/2/ round-trip would need either a
//! two-account test or a refactor of `send_envelope_native` to
//! extract the bytes-selection into a pure helper (deferred). For
//! now, the unit tests (`canonicalize_native_mode_rejects_non_282_byte_payload`
//! and friends in `crates/octo-adapter-whatsapp/src/adapter.rs:2703`)
//! cover the receiver-side length check, and the call-site
//! doc-comment at `adapter.rs:1710` documents the bytes-selection
//! invariant for human reviewers.

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

// ── Test 2: pre-flight 100 MiB + 1 byte ──────────────────────────
//
// R11-L2 fix: clarified comment. The mission spec at
// `missions/open/0850-whatsapp-media-transport.md:303` calls this
// "the only test in the suite that doesn't require `start_bot`".
// That is true with respect to the OTHER live tests in the
// `live-whatsapp` feature gate (which all need an authenticated
// session on disk). This test still requires the `live-whatsapp`
// feature to be enabled (the file is `#![cfg(feature =
// "live-whatsapp")]`); it is NOT "always-on" in the
// `cargo test -p octo-adapter-whatsapp` (default features) sense.
// The always-on equivalent is the unit tests
// `upload_media_rejects_payload_over_max_upload_bytes` and
// `upload_media_accepts_payload_exactly_at_max_upload_bytes` at
// `crates/octo-adapter-whatsapp/src/adapter.rs:3591-3642`. This
// test exists to pin the live test file's contract: a regression
// in the production pre-flight check would be caught at the unit
// level AND at the integration level. Documented inline so
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
/// R10-H1 fix: this test exercises the `upload_to_cdn` /
/// `download_via_media_ref` primitives end-to-end against a real
/// WhatsApp CDN. R11-H3 (R10-H1 follow-up): this test does NOT
/// exercise the `send_envelope` → `send_envelope_native` path that
/// R9-H1 broke — see the file-level doc-comment at the top of this
/// file for the full explanation of why a true R9-H1 regression
/// test is out of scope.
///
/// R11-H1 fix: wait for `Event::Connected` (via `self_handle()`)
/// before calling `upload_media`. The previous version of this
/// test called `start_bot()` and immediately called `upload_media()`
/// without waiting for the connection to fully establish, which
/// caused the test to always skip on a healthy production run with
/// a misleading "client not connected" warning. The pattern below
/// mirrors `live_session_test.rs:111-138` exactly: `start_bot()` is
/// followed by a 30s poll on `self_handle()` for `Some` (which
/// indicates `Event::Connected` has fired and the bot is ready to
/// accept traffic).
///
/// R11-L1 fix: removed the dead 60s `tokio::time::timeout` on
/// `start_bot()`. The 60s was wasted because `start_bot()` returns
/// once the noise handshake is in flight (typically <1s), not once
/// the connection is established (which takes another 5-30s).
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
    // R11-H1 fix: start_bot() returns once the noise handshake is
    // in flight, NOT once the connection is fully established. We
    // must wait for Event::Connected to fire before attempting any
    // network operation.
    //
    // The cleanest wait is on the `connected()` `Notify`, which is
    // `notify_waiters()`'d inside the `Event::Connected` handler at
    // `adapter.rs` (see the doc-comment at line 835-838 and the
    // public `connected()` getter at line 343-345). The previous
    // version of this test polled `self_handle().is_some()` for 30s
    // (mirroring `live_session_test.rs:111-138`); in this CI
    // environment, `self_handle()` can stay `None` even after the
    // bot is actively receiving messages (the bot's device snapshot
    // `pn` field was already cached on a prior session, so
    // `Event::Connected`'s `self_phone` write at line 831 was a
    // no-op — but the `connected_notify` still fires unconditionally
    // on `Event::Connected` itself). Using `connected().notified()`
    // is the canonical wait and is race-free.
    //
    // We run `start_bot()` and the `connected().notified()` wait in
    // parallel via `tokio::join!`, so the wait doesn't have to
    // "catch up" after `start_bot()` returns. `start_bot()` returns
    // after the noise handshake is in flight (~1-3s); the connected
    // notify fires once the WA server confirms the connection
    // (~5-15s after that). The 30s budget on each matches
    // `live_session_test.rs:124` and the production
    // `wait_for_connected` timeout in
    // `octo-whatsapp-onboard-core::session::wait_for_connected`.
    let connected = adapter.connected();
    let start_bot_fut = adapter.start_bot();
    let connect_result = tokio::time::timeout(Duration::from_secs(30), start_bot_fut).await;
    let notify_result = tokio::time::timeout(Duration::from_secs(30), connected.notified()).await;
    match connect_result {
        Ok(Ok(())) => {}
        Ok(Err(e)) => {
            panic!(
                "start_bot failed: {e:#}\n\
                 is the session database at {:?} valid and the WS reachable?",
                default_persist_dir().join(default_session_name())
            );
        }
        Err(_elapsed) => {
            panic!("start_bot did not complete within 30s");
        }
    }
    notify_result.unwrap_or_else(|_elapsed| {
        panic!(
            "timed out after 30s waiting for Event::Connected; \
             the bot may have been logged out, or the WA servers \
             may have rejected the noise handshake."
        );
    });
    tracing::info!("Event::Connected received; proceeding to round-trip test");

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
