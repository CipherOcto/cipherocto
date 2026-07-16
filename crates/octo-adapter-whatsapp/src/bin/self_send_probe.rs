//! Direct minimal self-send probe.
//!
//! Bypasses the entire `octo-whatsapp` daemon / events-buffer /
//! synthetic-emit pipeline and the adapter's `accept_message` inbound
//! filter. Boots the `whatsapp_rust` SDK against the standard
//! `${OCTO_WHATSAPP_PERSIST_DIR}/default.session.db` (stoolap backend
//! via `StoolapStore`, the same backend the live test fixture uses),
//! waits for the WA handshake to reach `Connected`, then issues a
//! single `client.send_text(self_jid, "probe <marker>")` call.
//! Prints the result and exits.
//!
//! Purpose: prove whether WA itself delivers a self-send message to
//! the operator's linked WA client. If the bubble does NOT appear on
//! the linked phone after a successful dispatch here, the round-trip
//! is broken at the WA / session-shape layer — not in the daemon, not
//! in the adapter's `accept_message` policy, not in the synthetic
//! emit added for the live-test canary.
//!
//! Lives in the adapter crate because `octo-adapter-whatsapp` already
//! declares `whatsapp-rust` with the right feature set and the
//! `StoolapStore` -> `Backend` bound is known-good here. Moving the
//! probe into `octo-whatsapp` would require duplicating the WA dep
//! feature graph and reconciling `Backend` satisfaction across two
//! `whatsapp-rust` instances.
//!
//! Post-buffa migration (wacore 6e0f241): upstream `BotBuilder::with_backend`
//! takes `impl Backend + 'static`, not `Arc<dyn Backend>`. The adapter's
//! `WhatsAppWebAdapter::start_bot` calls `.with_backend(storage)` with the
//! bare `StoolapStore`; we mirror that exactly here.
//!
//! Usage:
//!   cargo run -p octo-adapter-whatsapp --bin self_send_probe
//!
//! Env vars:
//!   OCTO_WHATSAPP_PERSIST_DIR  — session directory (default
//!                                `~/.local/share/octo/whatsapp`)
//!   OCTO_WHATSAPP_SESSION_NAME — session filename (default
//!                                `default.session.db`)

use std::time::Duration;

use octo_adapter_whatsapp::StoolapStore;
use whatsapp_rust::prelude::*;

fn resolve_session_path() -> std::path::PathBuf {
    let persist_dir = std::env::var("OCTO_WHATSAPP_PERSIST_DIR").unwrap_or_else(|_| {
        let home = std::env::var("HOME").unwrap_or_else(|_| ".".into());
        format!("{home}/.local/share/octo/whatsapp")
    });
    let session_name =
        std::env::var("OCTO_WHATSAPP_SESSION_NAME").unwrap_or_else(|_| "default.session.db".into());
    std::path::PathBuf::from(&persist_dir).join(&session_name)
}

#[tokio::main]
async fn main() {
    let session_path = resolve_session_path();
    eprintln!("[probe] session: {session_path:?}");

    let store = match StoolapStore::new(&session_path) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("[probe] FATAL: StoolapStore::new({session_path:?}) failed: {e:#}");
            std::process::exit(1);
        }
    };

    let bot = match Bot::builder().with_backend(store).build().await {
        Ok(b) => b,
        Err(e) => {
            eprintln!("[probe] FATAL: Bot::builder().build() failed: {e}");
            std::process::exit(1);
        }
    };

    let client = bot.client();
    let _run = tokio::spawn(bot.run());

    // Wait up to 60s for the WA handshake to resolve `pn` (our own JID).
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
            eprintln!("[probe] FATAL: never reached Connected within 60s (pn is None)");
            std::process::exit(1);
        }
    };
    eprintln!("[probe] connected; self_jid = {self_jid_str}");

    let marker_unix_ms = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis())
        .unwrap_or(0);
    let marker = format!("ping-test-{marker_unix_ms}");
    let text = format!("probe {marker}");
    eprintln!("[probe] dispatching self-send: text={text:?}");

    let jid: Jid = match self_jid_str.parse() {
        Ok(j) => j,
        Err(e) => {
            eprintln!("[probe] FATAL: parse self_jid {self_jid_str:?} -> Jid: {e}");
            std::process::exit(1);
        }
    };

    match client.send_text(jid, &text).await {
        Ok(sr) => {
            eprintln!("[probe] send_text OK");
            eprintln!("[probe] message_id = {}", sr.message_id);
            eprintln!("[probe] to         = {}", sr.to);
            eprintln!("[probe] >>> PLEASE CHECK THE LINKED WA CLIENT OF {self_jid_str} <<<");
            eprintln!("[probe] >>> expected bubble text: {text:?}");
            eprintln!(
                "[probe] >>> if the bubble does NOT appear, the round-trip is broken at the WA / network / session-shape layer, not in our code."
            );
        }
        Err(e) => {
            eprintln!("[probe] send_text ERROR: {e}");
            std::process::exit(2);
        }
    }

    // Brief tail so any final log lines flush before exit.
    tokio::time::sleep(Duration::from_secs(2)).await;
}
