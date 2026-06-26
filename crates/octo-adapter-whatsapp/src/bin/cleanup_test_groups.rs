/// Standalone cleanup utility for WhatsApp live test artifacts.
///
/// Usage:
///   cargo run -p octo-adapter-whatsapp --features live-whatsapp --bin cleanup_test_groups -- --dry-run
///   cargo run -p octo-adapter-whatsapp --features live-whatsapp --bin cleanup_test_groups
///
/// Cleans:
///   1. Groups we're still in with subject prefix "octo_test_" or "renamed_" —
///      destroys the group (revoke invite link + leave) and deletes the chat entry.
///   2. Chat entries from groups we already left but that still linger in the UI —
///      calls leave_group (idempotent, triggers delete_chat) to remove the chat.
///
/// Env vars:
///   OCTO_WHATSAPP_PERSIST_DIR   - session dir (default: ~/.local/share/octo/whatsapp)
///   OCTO_WHATSAPP_SESSION_NAME  - session file (default: default.session.db)
use std::collections::BTreeMap;
use std::sync::Arc;
use std::time::Duration;

use octo_adapter_whatsapp::{WhatsAppConfig, WhatsAppWebAdapter};
use octo_network::dot::adapters::coordinator_admin::GroupId;
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
    if !path.exists() {
        panic!(
            "no live WhatsApp session at {path:?}\n\
             set OCTO_WHATSAPP_PERSIST_DIR or run \
             `octo-whatsapp-onboard qr-link` first."
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

async fn connect() -> Arc<WhatsAppWebAdapter> {
    let config = live_config();
    if let Err(e) = config.validate() {
        panic!("invalid config: {e}");
    }
    let adapter = Arc::new(WhatsAppWebAdapter::new(config));
    // Register notification futures BEFORE start_bot so we don't miss
    // events that fire between connected and our await.
    let connected_notify = adapter.connected();
    let synced_notify = adapter.synced();
    let connected_fut = connected_notify.notified();
    let synced_fut = synced_notify.notified();
    adapter
        .start_bot()
        .await
        .unwrap_or_else(|e| panic!("start_bot failed: {e:#}"));
    tokio::time::timeout(Duration::from_secs(60), connected_fut)
        .await
        .unwrap_or_else(|_| panic!("timed out waiting for connected"));
    tokio::time::timeout(Duration::from_secs(120), synced_fut)
        .await
        .unwrap_or_else(|_| panic!("timed out waiting for synced (HistorySync)"));
    // Wait for self_handle.
    let deadline = std::time::Instant::now() + Duration::from_secs(30);
    while std::time::Instant::now() < deadline {
        if adapter.self_handle().is_some() {
            return adapter;
        }
        tokio::time::sleep(Duration::from_millis(250)).await;
    }
    panic!("self_handle() still None after 30s");
}

/// Read persisted group conversations directly from the stoolap DB.
/// Must be called before the adapter opens the DB (which locks it).
fn read_persisted_group_conversations(session_path: &std::path::Path) -> Vec<String> {
    let dsn = format!("file://{}", session_path.display());
    let db = match stoolap::Database::open(&dsn) {
        Ok(db) => db,
        Err(e) => {
            eprintln!("Warning: could not open session DB: {e}");
            return Vec::new();
        }
    };
    // Ensure table exists (idempotent).
    let _ = db.execute(
        "CREATE TABLE IF NOT EXISTS conversations (jid TEXT NOT NULL, name TEXT, is_group INTEGER NOT NULL DEFAULT 0, updated_at INTEGER NOT NULL, UNIQUE (jid))",
        (),
    );
    let mut rows = match db.query("SELECT jid FROM conversations WHERE is_group = 1", ()) {
        Ok(rows) => rows,
        Err(e) => {
            eprintln!("Warning: could not query conversations: {e}");
            return Vec::new();
        }
    };
    let mut result = Vec::new();
    while let Some(Ok(row)) = rows.next() {
        if let Ok(jid) = row.get::<String>(0) {
            result.push(jid);
        }
    }
    result
}

#[tokio::main]
async fn main() {
    let dry_run = std::env::args().any(|a| a == "--dry-run");

    // ── Phase 0: Read persisted conversations from DB ───────────
    // Must happen BEFORE connect() which locks the DB.
    let session_path = {
        let mut path = default_persist_dir();
        path.push(default_session_name());
        path
    };
    let persisted_groups = read_persisted_group_conversations(&session_path);
    println!(
        "Read {} persisted group conversations from DB",
        persisted_groups.len()
    );

    println!("Connecting to WhatsApp Web...");
    let adapter = connect().await;
    let admin = adapter.as_coordinator_admin().unwrap();

    println!("Connected and synced.");

    // ── Phase 1: Groups we're currently in ──────────────────────
    println!("\n=== Phase 1: Groups we're currently in ===");
    let groups = admin
        .list_own_groups()
        .await
        .expect("list_own_groups failed");
    println!("Found {} groups:", groups.len());
    for g in &groups {
        println!(
            "  {}  subject={:?}",
            g.id.as_str(),
            g.subject.as_deref().unwrap_or("(none)")
        );
    }

    let test_prefixes = ["octo_test_", "renamed_"];
    let active_orphans: Vec<_> = groups
        .iter()
        .filter(|g| {
            g.subject
                .as_deref()
                .map(|s| test_prefixes.iter().any(|p| s.starts_with(p)))
                .unwrap_or(false)
        })
        .collect();

    println!("\n  Active orphaned groups: {}", active_orphans.len());

    // ── Phase 2: Persisted conversations (including left groups) ──
    println!("\n=== Phase 2: Persisted conversations (stoolap DB) ===");

    // Group JIDs from conversations that we're NOT currently in.
    let active_jids: std::collections::HashSet<String> =
        groups.iter().map(|g| g.id.as_str().to_string()).collect();

    let all_left_groups: Vec<String> = persisted_groups
        .iter()
        .filter(|jid| !active_jids.contains(jid.as_str()))
        .cloned()
        .collect();

    println!(
        "  Left groups (in conversations but not active): {}",
        all_left_groups.len()
    );
    for jid in &all_left_groups {
        println!("    {}", jid);
    }

    if active_orphans.is_empty() && all_left_groups.is_empty() {
        println!("\nNo orphaned groups or chats found. Clean!");
        let _ = adapter.shutdown().await;
        return;
    }

    if dry_run {
        println!(
            "\n[dry-run] Would destroy {} active orphans + delete chat for {} left groups.",
            active_orphans.len(),
            all_left_groups.len()
        );
        let _ = adapter.shutdown().await;
        return;
    }

    // ── Phase 3: Clean up active orphaned groups ────────────────
    println!("\n=== Phase 3: Destroying active orphaned groups ===");
    let mut destroyed = 0u32;
    let mut left = 0u32;
    let mut failed = 0u32;

    for g in &active_orphans {
        let gid = GroupId::new(g.id.as_str().to_string());
        let subject = g.subject.as_deref().unwrap_or("?");
        tokio::time::sleep(Duration::from_secs(2)).await;

        match admin.destroy_group(&gid).await {
            Ok(()) => {
                destroyed += 1;
                println!("  destroyed: {} ({})", g.id.as_str(), subject);
            }
            Err(e) => {
                eprintln!(
                    "  destroy failed for {} ({}): {e}, trying leave_group...",
                    g.id.as_str(),
                    subject
                );
                tokio::time::sleep(Duration::from_secs(2)).await;
                match admin.leave_group(&gid).await {
                    Ok(()) => {
                        left += 1;
                        println!("  left (fallback): {} ({})", g.id.as_str(), subject);
                    }
                    Err(e2) => {
                        failed += 1;
                        eprintln!(
                            "  leave also failed for {} ({}): {e2}",
                            g.id.as_str(),
                            subject
                        );
                    }
                }
            }
        }
    }

    // ── Phase 4: Delete chat entries for left groups ────────────
    println!("\n=== Phase 4: Deleting chat entries for left groups ===");
    let mut chats_deleted = 0u32;
    let mut chats_failed = 0u32;

    for jid in &all_left_groups {
        tokio::time::sleep(Duration::from_secs(2)).await;
        let gid = GroupId::new(jid.clone());
        // leave_group is idempotent on "not a participant" — and now
        // it calls delete_chat after a successful leave (or on the
        // "not a participant" path via the trait impl).
        match admin.leave_group(&gid).await {
            Ok(()) => {
                chats_deleted += 1;
                println!("  chat deleted: {}", jid);
            }
            Err(e) => {
                chats_failed += 1;
                eprintln!("  chat delete failed for {}: {e}", jid);
            }
        }
    }

    // ── Summary ─────────────────────────────────────────────────
    println!("\n=== Summary ===");
    println!("Active groups destroyed:  {}", destroyed);
    println!("Active groups left:       {}", left);
    println!("Active groups failed:     {}", failed);
    println!("Left-group chats deleted: {}", chats_deleted);
    println!("Left-group chats failed:  {}", chats_failed);

    let _ = adapter.shutdown().await;
}
