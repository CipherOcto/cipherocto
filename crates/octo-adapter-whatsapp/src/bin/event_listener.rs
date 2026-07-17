/// Event listener that creates a group, then monitors all incoming events.
/// Purpose: capture what happens when you manually delete a group/chat
/// in the official WhatsApp app (Android or Web).
///
/// Usage:
///   cargo run -p octo-adapter-whatsapp --features live-whatsapp --bin event_listener
///
/// Then manually delete the group in the official WhatsApp app and watch
/// what events fire in the terminal.
use std::collections::BTreeMap;
use std::sync::Arc;
use std::time::Duration;

use octo_adapter_whatsapp::{WhatsAppConfig, WhatsAppWebAdapter};
use octo_network::dot::PlatformAdapter;

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
    WhatsAppConfig {
        session_path: path.to_string_lossy().into_owned(),
        ws_url: None,
        pair_phone: None,
        pair_code: None,
        groups: vec![],
        sender_allowlist: BTreeMap::new(),
        passkey_authenticator: None,
    }
}

#[tokio::main]
async fn main() {
    let config = live_config();
    let adapter = Arc::new(WhatsAppWebAdapter::new(config));

    // Subscribe to raw events BEFORE starting (to avoid missing early events).
    let mut raw_rx = adapter.subscribe_raw_events();

    // Register notification futures BEFORE start_bot.
    let connected_notify = adapter.connected();
    let synced_notify = adapter.synced();
    let connected_fut = connected_notify.notified();
    let synced_fut = synced_notify.notified();

    println!("Starting WhatsApp Web bot...");
    adapter.start_bot().await.expect("start_bot failed");

    // Wait for connected.
    tokio::time::timeout(Duration::from_secs(60), connected_fut)
        .await
        .expect("timed out waiting for connected");
    println!("Connected to WhatsApp Web.");

    // Wait for synced.
    tokio::time::timeout(Duration::from_secs(120), synced_fut)
        .await
        .expect("timed out waiting for synced");
    println!("Synced. HistorySync complete.");

    // Wait for self_handle.
    let deadline = std::time::Instant::now() + Duration::from_secs(30);
    while std::time::Instant::now() < deadline {
        if adapter.self_handle().is_some() {
            break;
        }
        tokio::time::sleep(Duration::from_millis(250)).await;
    }
    let self_phone = adapter.self_handle().unwrap_or_default();
    println!("Bot identity: +{self_phone}");

    // Create a test group.
    println!("\nCreating test group...");
    let admin = adapter.as_coordinator_admin().unwrap();
    let timestamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    let subject = format!("event_listener_test_{timestamp}");

    tokio::time::sleep(Duration::from_secs(3)).await;
    let handle = admin
        .create_group(&subject, &[])
        .await
        .expect("create_group failed");
    let group_jid = handle.id.as_str().to_string();
    println!("Created group: {} (subject: {})", group_jid, subject);
    println!("\n╔══════════════════════════════════════════════════════════════╗");
    println!("║  Now manually delete this group/chat in the official app.   ║");
    println!("║  Watch below for events that fire when you do.              ║");
    println!("╚══════════════════════════════════════════════════════════════╝\n");

    // Listen for raw events indefinitely.
    let mut event_count = 0u64;
    loop {
        match raw_rx.recv().await {
            Ok(desc) => {
                event_count += 1;
                println!("[EVENT #{event_count}] {desc}");
            }
            Err(tokio::sync::broadcast::error::RecvError::Lagged(n)) => {
                println!("[LAGGED] missed {n} events");
            }
            Err(e) => {
                eprintln!("[ERROR receiving event: {e}]");
                break;
            }
        }
    }
}
