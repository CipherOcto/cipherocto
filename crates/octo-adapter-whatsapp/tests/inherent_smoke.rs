//! Hermetic smoke tests for Phase 2.5 wacore wiring of `WhatsAppWebAdapter`.
//!
//! Each of the 18 inherent methods wired in Phase 2.5 (Parts A + B) is
//! invoked on a disconnected adapter and asserted to return
//! `Err(PlatformAdapterError::Unreachable { reason: "client not connected" })`.
//! This proves the wacore wiring compiles and dispatches correctly through
//! the new client-lock / Arc-clone / waproto::Message construction path
//! without requiring a live WhatsApp session.
//!
//! Real wacore behaviour (upload, network round-trip, message-id format)
//! is exercised by the live tests under `--features live-whatsapp` (see
//! `tests/live_2_5_wiring.rs`).

use octo_adapter_whatsapp::{PlatformAdapterError, WhatsAppConfig, WhatsAppWebAdapter};
use std::collections::BTreeMap;
use std::path::PathBuf;

const JID: &str = "1234567890@s.whatsapp.net";
const MSG_ID: &str = "3EB0B1234567890ABCDEF";

/// Construct a `WhatsAppWebAdapter` without calling `start_bot()` so
/// operations return `Unreachable { reason: "client not connected" }`.
/// Replaces the previously gated `new_unconnected_for_tests()` ctor
/// (which required `--features test-helpers`); uses the public
/// `WhatsAppWebAdapter::new(WhatsAppConfig)` with a dummy session path
/// that is never opened since the adapter never boots.
fn adapter() -> WhatsAppWebAdapter {
    WhatsAppWebAdapter::new(WhatsAppConfig {
        session_path: "/tmp/octo-smoke-unused.db".into(),
        pair_phone: None,
        pair_code: None,
        ws_url: None,
        groups: vec![],
        sender_allowlist: BTreeMap::new(),
        passkey_authenticator: None,
    })
}

fn tmp_with_size(name: &str, size: usize) -> PathBuf {
    let p = std::env::temp_dir().join(format!("octo-phase2_5-smoke-{name}"));
    std::fs::write(&p, vec![0u8; size]).unwrap();
    p
}

fn assert_client_not_connected<T: std::fmt::Debug>(r: Result<T, PlatformAdapterError>) {
    match r {
        Err(PlatformAdapterError::Unreachable { reason, .. }) => {
            assert!(
                reason.contains("client not connected"),
                "expected reason containing 'client not connected', got {reason:?}"
            );
        }
        other => panic!("expected Err(Unreachable {{ client not connected }}), got {other:?}"),
    }
}

// ── Part A: text + control methods (Tasks 1-9) ──────────────────────

#[tokio::test]
async fn smoke_send_reaction_unconnected() {
    assert_client_not_connected(adapter().send_reaction(JID, MSG_ID, "👍").await);
}

#[tokio::test]
async fn smoke_send_poll_unconnected() {
    let opts = vec!["A".to_string(), "B".to_string()];
    assert_client_not_connected(adapter().send_poll(JID, "Q?", &opts, false).await);
}

#[tokio::test]
async fn smoke_send_location_unconnected() {
    assert_client_not_connected(
        adapter()
            .send_location(JID, 51.5074, -0.1278, "London")
            .await,
    );
}

#[tokio::test]
async fn smoke_edit_message_unconnected() {
    assert_client_not_connected(adapter().edit_message(JID, MSG_ID, "edited").await);
}

#[tokio::test]
async fn smoke_delete_message_unconnected() {
    assert_client_not_connected(adapter().delete_message(JID, MSG_ID).await);
}

#[tokio::test]
async fn smoke_mark_read_unconnected() {
    assert_client_not_connected(adapter().mark_read(JID, MSG_ID).await);
}

#[tokio::test]
async fn smoke_message_search_unconnected() {
    // message_search returns Ok(empty) when StoolapStore is uninitialised
    // (it doesn't need the wacore client).
    let r = adapter().message_search("query", Some(JID)).await;
    assert!(matches!(r, Ok(ref v) if v.is_empty()));
}

#[tokio::test]
async fn smoke_chat_info_unconnected() {
    // chat_info returns Ok(Some(ChatInfo {..})) even when StoolapStore is
    // uninitialised (it derives kind from the JID suffix).
    let r = adapter().chat_info(JID).await;
    assert!(matches!(r, Ok(Some(_))));
}

#[tokio::test]
async fn smoke_set_chat_pinned_unconnected() {
    // Stub: no wacore 0.6 API.
    let r = adapter().set_chat_pinned(JID, true).await;
    assert!(matches!(r, Err(PlatformAdapterError::Unreachable { .. })));
}

#[tokio::test]
async fn smoke_set_chat_muted_unconnected() {
    let r = adapter().set_chat_muted(JID, 1_700_000_000).await;
    assert!(matches!(r, Err(PlatformAdapterError::Unreachable { .. })));
}

#[tokio::test]
async fn smoke_set_chat_archived_unconnected() {
    let r = adapter().set_chat_archived(JID, true).await;
    assert!(matches!(r, Err(PlatformAdapterError::Unreachable { .. })));
}

#[tokio::test]
async fn smoke_delete_chat_local() {
    // delete_chat is a client-side op, returns Ok(()) regardless of client.
    assert!(adapter().delete_chat(JID).await.is_ok());
}

#[tokio::test]
async fn smoke_send_typing_unconnected() {
    assert_client_not_connected(adapter().send_typing(JID, true).await);
}

// ── Part B: media-upload methods (Tasks 11-16) ─────────────────────

#[tokio::test]
async fn smoke_send_image_unconnected() {
    let p = tmp_with_size("img", 1024);
    assert_client_not_connected(adapter().send_image(JID, &p, None).await);
    let _ = std::fs::remove_file(&p);
}

#[tokio::test]
async fn smoke_send_video_unconnected() {
    let p = tmp_with_size("vid", 1024);
    assert_client_not_connected(adapter().send_video(JID, &p, None).await);
    let _ = std::fs::remove_file(&p);
}

#[tokio::test]
async fn smoke_send_audio_unconnected() {
    let p = tmp_with_size("aud", 1024);
    assert_client_not_connected(adapter().send_audio(JID, &p).await);
    let _ = std::fs::remove_file(&p);
}

#[tokio::test]
async fn smoke_send_voice_unconnected() {
    let p = tmp_with_size("voice", 1024);
    assert_client_not_connected(adapter().send_voice(JID, &p).await);
    let _ = std::fs::remove_file(&p);
}

#[tokio::test]
async fn smoke_send_sticker_unconnected() {
    let p = tmp_with_size("stk", 1024);
    assert_client_not_connected(adapter().send_sticker(JID, &p).await);
    let _ = std::fs::remove_file(&p);
}

#[tokio::test]
async fn smoke_send_contact_unconnected() {
    let p = tmp_with_size("contact", 256);
    // Write valid vCard-ish text so the read_to_string succeeds before
    // the client-not-connected error surfaces.
    std::fs::write(&p, b"FN:Alice\nBEGIN:VCARD\nEND:VCARD\n").unwrap();
    assert_client_not_connected(adapter().send_contact(JID, &p).await);
    let _ = std::fs::remove_file(&p);
}
