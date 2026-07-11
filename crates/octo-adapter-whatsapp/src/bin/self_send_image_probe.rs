//! Direct minimal self-send **image** probe.
//!
//! Companion to `self_send_probe.rs` (which only sends text). Boots the
//! `whatsapp_rust` SDK against the standard session, waits for
//! Connected, then uploads the PNG file given as the first CLI
//! argument (or `/tmp/1px.png` by default) and dispatches it to the
//! resolved self-JID via `client.send_message`.
//!
//! **Why this exists.** The text probe (`self_send_probe`) confirmed
//! `client.send_text(self_jid, ...)` renders on the operator's linked
//! WA client. The image path uses `client.send_message(jid, image_msg)`
//! through `Client::upload`. We need a clean test that this dispatch
//! reaches the linked client without any daemon / adapter /
//! `accept_message` filter / synthetic emit in the middle, so we can
//! isolate whether the bubble-render gap on the linked phone is a
//! wacore self-media issue or a daemon issue.
//!
//! Lives in the adapter crate because `octo-adapter-whatsapp` already
//! declares `whatsapp-rust` with the right feature set and the
//! `StoolapStore -> Backend` bound is known-good here.
//!
//! Usage:
//!   cargo run -p octo-adapter-whatsapp --bin self_send_image_probe -- /tmp/1px.png
//!
//! Env vars (same as text probe):
//!   OCTO_WHATSAPP_PERSIST_DIR  — session directory (default
//!                                `~/.local/share/octo/whatsapp`)
//!   OCTO_WHATSAPP_SESSION_NAME — session filename (default
//!                                `default.session.db`)

use std::time::Duration;

use octo_adapter_whatsapp::StoolapStore;
use whatsapp_rust::download::MediaType;
use whatsapp_rust::prelude::*;
use whatsapp_rust::upload::UploadOptions;
use whatsapp_rust::waproto::whatsapp as wa;

fn resolve_session_path() -> std::path::PathBuf {
    let persist_dir = std::env::var("OCTO_WHATSAPP_PERSIST_DIR").unwrap_or_else(|_| {
        let home = std::env::var("HOME").unwrap_or_else(|_| ".".into());
        format!("{home}/.local/share/octo/whatsapp")
    });
    let session_name = std::env::var("OCTO_WHATSAPP_SESSION_NAME")
        .unwrap_or_else(|_| "default.session.db".into());
    std::path::PathBuf::from(&persist_dir).join(&session_name)
}

#[tokio::main]
async fn main() {
    let session_path = resolve_session_path();
    eprintln!("[img-probe] session: {session_path:?}");

    let store = match StoolapStore::new(&session_path) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("[img-probe] FATAL: StoolapStore::new({session_path:?}) failed: {e:#}");
            std::process::exit(1);
        }
    };

    let bot = match Bot::builder().with_backend(store).build().await {
        Ok(b) => b,
        Err(e) => {
            eprintln!("[img-probe] FATAL: Bot::builder().build() failed: {e}");
            std::process::exit(1);
        }
    };

    let client = bot.client();
    let _run = tokio::spawn(bot.run());

    let deadline = std::time::Instant::now() + Duration::from_secs(60);
    let mut self_jid_str: Option<String> = None;
    while std::time::Instant::now() < deadline {
        let snap = client.persistence_manager().get_device_snapshot();
        if let Some(ref pn) = snap.pn {
            self_jid_str = Some(pn.to_string());
            break;
        }
        tokio::time::sleep(Duration::from_millis(250)).await;
    }
    let self_jid_str = match self_jid_str {
        Some(s) => s,
        None => {
            eprintln!("[img-probe] FATAL: never reached Connected within 60s (pn is None)");
            std::process::exit(1);
        }
    };
    eprintln!("[img-probe] connected; self_jid = {self_jid_str}");

    // The pn being Some does NOT mean the WA socket is fully wired —
    // `client.upload` requires the media-conn subsystem to be ready,
    // which only happens after the noise/handshake completes. Wait
    // for `is_connected()` before issuing uploads, or we get a
    // `client is not connected` error from the request layer.
    let connected_deadline = std::time::Instant::now() + Duration::from_secs(60);
    while std::time::Instant::now() < connected_deadline {
        if client.is_connected() {
            break;
        }
        tokio::time::sleep(Duration::from_millis(250)).await;
    }
    if !client.is_connected() {
        eprintln!("[img-probe] FATAL: client.is_connected() never went true within 60s");
        std::process::exit(1);
    }
    eprintln!("[img-probe] is_connected=true; ready to upload");

    // Resolve image path from argv or default to /tmp/1px.png.
    let image_path = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "/tmp/1px.png".to_string());
    let image_path = std::path::PathBuf::from(&image_path);
    if !image_path.exists() {
        eprintln!("[img-probe] FATAL: image file {image_path:?} does not exist");
        eprintln!("[img-probe] hint: generate one with `printf '\\x89PNG\\r\\n...' > /tmp/1px.png`");
        std::process::exit(1);
    }
    let bytes = match tokio::fs::read(&image_path).await {
        Ok(b) => b,
        Err(e) => {
            eprintln!("[img-probe] FATAL: read {image_path:?} failed: {e}");
            std::process::exit(1);
        }
    };
    eprintln!(
        "[img-probe] read {} bytes from {image_path:?}",
        bytes.len()
    );

    // Upload to WA CDN.
    let upload = match client
        .upload(bytes.clone(), MediaType::Image, UploadOptions::new())
        .await
    {
        Ok(u) => u,
        Err(e) => {
            eprintln!("[img-probe] FATAL: client.upload failed: {e:#}");
            std::process::exit(2);
        }
    };
    eprintln!("[img-probe] upload OK; url={}", upload.url);

    let jid: Jid = match self_jid_str.parse() {
        Ok(j) => j,
        Err(e) => {
            eprintln!("[img-probe] FATAL: parse self_jid {self_jid_str:?} -> Jid: {e}");
            std::process::exit(1);
        }
    };

    let marker_unix_ms = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis())
        .unwrap_or(0);
    let caption = format!("probe img {marker_unix_ms}");

    let img_msg = wa::message::ImageMessage {
        url: Some(upload.url.clone()),
        direct_path: Some(upload.direct_path.clone()),
        media_key: Some(upload.media_key.to_vec()),
        file_sha256: Some(upload.file_sha256.to_vec()),
        file_enc_sha256: Some(upload.file_enc_sha256.to_vec()),
        file_length: Some(bytes.len() as u64),
        mimetype: Some("image/png".to_string()),
        caption: Some(caption.clone()),
        ..Default::default()
    };
    let outgoing = wa::Message {
        image_message: whatsapp_rust::buffa::MessageField::some(img_msg),
        ..Default::default()
    };

    match client.send_message(jid, outgoing).await {
        Ok(sr) => {
            eprintln!("[img-probe] send_message OK");
            eprintln!("[img-probe] message_id = {}", sr.message_id);
            eprintln!("[img-probe] to         = {}", sr.to);
            eprintln!("[img-probe] >>> PLEASE CHECK THE LINKED WA CLIENT OF {self_jid_str} <<<");
            eprintln!("[img-probe] >>> expected bubble caption: {caption:?}");
            eprintln!(
                "[img-probe] >>> if the bubble does NOT appear, the round-trip is broken at the \
                 WA / network / session-shape / multi-device-echo layer, not in our daemon."
            );
        }
        Err(e) => {
            eprintln!("[img-probe] send_message ERROR: {e:#}");
            std::process::exit(2);
        }
    }

    tokio::time::sleep(Duration::from_secs(2)).await;
}