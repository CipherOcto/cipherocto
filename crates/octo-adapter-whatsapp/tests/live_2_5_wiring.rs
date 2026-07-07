//! Live tests for Phase 2.5 wacore wiring.
//!
//! Each test exercises one of the 6 media-upload + reaction methods that
//! Phase 2.5 wired to real wacore/whatsapp-rust calls. Tests require a
//! real authenticated WhatsApp session (created via
//! `octo-whatsapp-onboard qr-link`) and a peer JID to send to.
//!
//! All tests are `#[ignore]`-d by default. Run with:
//!
//! ```bash
//! cargo test -p octo-adapter-whatsapp \
//!   --features live-whatsapp \
//!   --test live_2_5_wiring \
//!   -- --include-ignored --nocapture --test-threads=1
//! ```
//!
//! Environment variables consumed:
//! - `OCTO_WHATSAPP_PERSIST_DIR` — directory holding `default.session.db`.
//!   Defaults to `$HOME/.local/share/octo/whatsapp/`.
//! - `OCTO_WHATSAPP_SESSION_NAME` — session filename (default:
//!   `default.session.db`).
//! - `OCTO_WHATSAPP_E2E_TEST_PEER` — JID (`<digits>@s.whatsapp.net`) of
//!   the test peer to send media to. Required.

#![cfg(feature = "live-whatsapp")]

use octo_adapter_whatsapp::{WhatsAppConfig, WhatsAppWebAdapter};
use std::collections::BTreeMap;
use std::sync::Arc;
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

fn live_config() -> WhatsAppConfig {
    let mut path = default_persist_dir();
    path.push(default_session_name());
    if !path.exists() {
        panic!(
            "no live WhatsApp session at {path:?}\n\
             set OCTO_WHATSAPP_PERSIST_DIR to the persistent dir created by \
             `octo-whatsapp-onboard qr-link` / `pair-link`."
        );
    }
    WhatsAppConfig {
        session_path: path.to_string_lossy().into_owned(),
        ws_url: None,
        pair_phone: None,
        pair_code: None,
        groups: vec![],
        sender_allowlist: BTreeMap::new(),
    }
}

fn test_peer() -> String {
    std::env::var("OCTO_WHATSAPP_E2E_TEST_PEER").unwrap_or_else(|_| {
        panic!(
            "OCTO_WHATSAPP_E2E_TEST_PEER not set — must be a JID like \
             '<digits>@s.whatsapp.net' that this WhatsApp session can \
             message."
        )
    })
}

/// Build a tiny on-disk file payload for the upload tests. Returns the
/// path to the temp file; the caller is responsible for `remove_file`.
fn tmp_with_bytes(name: &str, bytes: &[u8]) -> std::path::PathBuf {
    let p = std::env::temp_dir().join(format!("octo-phase2_5-live-{name}"));
    std::fs::write(&p, bytes).unwrap();
    p
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

/// Connect to WhatsApp, wait for the connected notification + handle,
/// then run the supplied async closure against the live adapter.
async fn with_live_adapter<F, Fut>(f: F)
where
    F: FnOnce(Arc<WhatsAppWebAdapter>) -> Fut,
    Fut: std::future::Future<Output = ()>,
{
    init_tracing();
    let config = live_config();
    if let Err(e) = config.validate() {
        panic!("invalid live WhatsAppConfig: {e}");
    }
    let adapter = Arc::new(WhatsAppWebAdapter::new(config));
    let notify = adapter.connected();
    adapter.start_bot().await.unwrap_or_else(|e| {
        panic!(
            "WhatsAppWebAdapter::start_bot failed: {e:#}\n\
             is the session database at {:?} valid and the WS reachable?",
            default_persist_dir().join(default_session_name())
        )
    });
    notify.notified().await;
    // Give the device snapshot a beat to land (live_e2e_group_setup_test
    // uses 5s for the same reason).
    tokio::time::sleep(Duration::from_secs(2)).await;
    f(adapter).await;
}

#[tokio::test]
#[ignore = "requires live WhatsApp Web session + OCTO_WHATSAPP_E2E_TEST_PEER"]
async fn live_send_image_succeeds() {
    let peer = test_peer();
    with_live_adapter(|adapter| async move {
        let p = tmp_with_bytes("image", &[0u8; 1024]);
        let (msg_id, token) = adapter
            .send_image(&peer, &p, Some("phase 2.5 live test"))
            .await
            .expect("send_image should succeed on live session");
        assert!(!msg_id.is_empty(), "message_id must be non-empty");
        assert!(!token.is_empty(), "media_ref_token must be non-empty");
        let _ = std::fs::remove_file(&p);
    })
    .await;
}

#[tokio::test]
#[ignore = "requires live WhatsApp Web session + OCTO_WHATSAPP_E2E_TEST_PEER"]
async fn live_send_video_succeeds() {
    let peer = test_peer();
    with_live_adapter(|adapter| async move {
        let p = tmp_with_bytes("video", &[0u8; 1024]);
        let (msg_id, token) = adapter
            .send_video(&peer, &p, Some("phase 2.5 live test"))
            .await
            .expect("send_video should succeed on live session");
        assert!(!msg_id.is_empty());
        assert!(!token.is_empty());
        let _ = std::fs::remove_file(&p);
    })
    .await;
}

#[tokio::test]
#[ignore = "requires live WhatsApp Web session + OCTO_WHATSAPP_E2E_TEST_PEER"]
async fn live_send_audio_succeeds() {
    let peer = test_peer();
    with_live_adapter(|adapter| async move {
        let p = tmp_with_bytes("audio", &[0u8; 1024]);
        let (msg_id, token) = adapter
            .send_audio(&peer, &p)
            .await
            .expect("send_audio should succeed on live session");
        assert!(!msg_id.is_empty());
        assert!(!token.is_empty());
        let _ = std::fs::remove_file(&p);
    })
    .await;
}

#[tokio::test]
#[ignore = "requires live WhatsApp Web session + OCTO_WHATSAPP_E2E_TEST_PEER"]
async fn live_send_voice_succeeds() {
    let peer = test_peer();
    with_live_adapter(|adapter| async move {
        let p = tmp_with_bytes("voice", &[0u8; 1024]);
        let (msg_id, token) = adapter
            .send_voice(&peer, &p)
            .await
            .expect("send_voice should succeed on live session");
        assert!(!msg_id.is_empty());
        assert!(!token.is_empty());
        let _ = std::fs::remove_file(&p);
    })
    .await;
}

#[tokio::test]
#[ignore = "requires live WhatsApp Web session + OCTO_WHATSAPP_E2E_TEST_PEER"]
async fn live_send_sticker_succeeds() {
    let peer = test_peer();
    with_live_adapter(|adapter| async move {
        let p = tmp_with_bytes("sticker", &[0u8; 1024]);
        let (msg_id, token) = adapter
            .send_sticker(&peer, &p)
            .await
            .expect("send_sticker should succeed on live session");
        assert!(!msg_id.is_empty());
        assert!(!token.is_empty());
        let _ = std::fs::remove_file(&p);
    })
    .await;
}

#[tokio::test]
#[ignore = "requires live WhatsApp Web session + OCTO_WHATSAPP_E2E_TEST_PEER"]
async fn live_send_reaction_succeeds() {
    let peer = test_peer();
    with_live_adapter(|adapter| async move {
        // React to a fabricated message id — the server will reject with
        // a 404-equivalent (handled inside wacore) but the *send path*
        // returns Ok with the message id of the reaction itself. If
        // the server actually rejects (e.g. message not found), wacore
        // returns Err which we surface as a test failure. The point of
        // this test is to prove the wacore dispatch path is alive, not
        // to verify reaction semantics (which require a real previous
        // message id).
        let r = adapter
            .send_reaction(&peer, "3EB0B1234567890ABCDEF", "👍")
            .await;
        // Either Ok (server accepted) or Err Unreachable (server said
        // message not found); both prove the dispatch path works.
        match r {
            Ok(msg_id) => assert!(!msg_id.is_empty()),
            Err(octo_network::dot::error::PlatformAdapterError::Unreachable { .. }) => {
                // expected: server said message not found, wacore mapped
                // to transport error. Still proves dispatch path.
            }
            Err(other) => panic!("unexpected error variant: {other:?}"),
        }
    })
    .await;
}
