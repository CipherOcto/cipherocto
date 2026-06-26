/// Standalone cleanup utility for MTProto live test artifacts.
///
/// Usage:
///   cargo run -p octo-adapter-telegram-mtproto --features real-network --bin cleanup_test_artifacts -- --dry-run
///   cargo run -p octo-adapter-telegram-mtproto --features real-network --bin cleanup_test_artifacts
///
/// Cleans:
///   1. Messages in Saved Messages matching OCTO_LIVE_* test markers
///   2. Groups with title prefix "octo_test_" (test groups)
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use octo_adapter_telegram_mtproto::client::MtprotoTelegramClient;
use octo_adapter_telegram_mtproto::config::MtprotoTelegramConfig;
use octo_adapter_telegram_mtproto::real_client::RealTelegramMtprotoClient;
use octo_adapter_telegram_mtproto::self_handle::MtprotoSelfHandle;
use octo_adapter_telegram_mtproto::session::StoolapSession;

fn live_config() -> MtprotoTelegramConfig {
    let data_dir = std::env::var("TELEGRAM_DATA_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|_| {
            let base = std::env::var("XDG_DATA_HOME")
                .map(PathBuf::from)
                .unwrap_or_else(|_| {
                    let home = std::env::var("HOME").unwrap_or_else(|_| "/tmp".to_string());
                    PathBuf::from(home).join(".local").join("share")
                });
            base.join("octo").join("telegram-mtproto")
        });

    let config_path = data_dir.join("config.json");
    MtprotoTelegramConfig::from_file_or_env(&config_path)
        .unwrap_or_else(|e| panic!("could not load config from {}: {e}", config_path.display()))
}

#[tokio::main]
async fn main() {
    let dry_run = std::env::args().any(|a| a == "--dry-run");

    let config = live_config();
    let api_id = config.api_id.expect("api_id required");
    let api_hash = config.api_hash.as_deref().expect("api_hash required");
    let data_dir = config.data_dir.as_ref().expect("data_dir required");

    let session = StoolapSession::open(&data_dir.join("session.db"))
        .unwrap_or_else(|e| panic!("failed to open session: {e}"));

    let self_handle = MtprotoSelfHandle::new();
    let client = RealTelegramMtprotoClient::connect(api_id, api_hash, session, self_handle.clone())
        .await
        .expect("connect failed -- is the session valid?");

    match client.grammers_client().get_me().await {
        Ok(me) => {
            let user_id = me.id().bare_id();
            let username = me.username().map(String::from);
            self_handle.set_identity(user_id, username);
        }
        Err(e) => {
            eprintln!("get_me() failed: {e}");
            eprintln!("Re-run: rm -rf ~/.local/share/octo/telegram-mtproto/session.db*");
            eprintln!("./scripts/mtproto-onboard-qr.sh");
            std::process::exit(1);
        }
    }

    let client = Arc::new(client);
    let identity = self_handle.get().expect("Not logged in");
    let user_id = identity.user_id;
    println!(
        "Logged in as: {} (user_id: {})",
        identity.username.as_deref().unwrap_or("?"),
        user_id
    );

    // =========================================================================
    // Phase 1: Clean up test messages in Saved Messages
    // =========================================================================
    println!("\n=== Phase 1: Cleaning test messages in Saved Messages ===");
    let mut deleted_count = 0u32;
    let mut failed_count = 0u32;

    let test_prefixes = ["OCTO_LIVE_", "LT-", "LT_", "octo_test_", "DOT/1/"];

    for pass in 0..10 {
        eprintln!("[pass {}] draining updates...", pass);
        let updates = match client.receive_updates().await {
            Ok(u) => u,
            Err(e) => {
                eprintln!("receive_updates failed: {e}");
                break;
            }
        };

        if updates.is_empty() {
            eprintln!("[pass {}] no more updates", pass);
            break;
        }

        let mut msg_ids_to_delete = Vec::new();
        for u in &updates {
            if let octo_adapter_telegram_mtproto::client::MtprotoTelegramUpdate::NewMessage(nm) = u
            {
                let is_test = test_prefixes.iter().any(|p| nm.message.starts_with(p));
                if is_test && nm.chat_id == user_id {
                    msg_ids_to_delete.push(nm.message_id as i32);
                    eprintln!(
                        "  [found] msg_id={} msg={}",
                        nm.message_id,
                        &nm.message[..nm.message.len().min(60)]
                    );
                }
            }
        }

        if !msg_ids_to_delete.is_empty() {
            if dry_run {
                println!(
                    "[dry-run] Would delete {} messages: {:?}",
                    msg_ids_to_delete.len(),
                    msg_ids_to_delete
                );
            } else {
                match client
                    .delete_messages(user_id, &msg_ids_to_delete, true)
                    .await
                {
                    Ok(()) => {
                        deleted_count += msg_ids_to_delete.len() as u32;
                        println!("Deleted {} messages", msg_ids_to_delete.len());
                    }
                    Err(e) => {
                        failed_count += msg_ids_to_delete.len() as u32;
                        eprintln!("delete_messages failed: {e}");
                    }
                }
            }
        }

        tokio::time::sleep(Duration::from_millis(500)).await;
    }

    // =========================================================================
    // Phase 2: Clean up test groups
    // =========================================================================
    println!("\n=== Phase 2: Cleaning test groups (prefix: octo_test_) ===");

    let dialogs = client.list_dialog_ids().await.unwrap_or_default();
    let test_title_prefix = "octo_test_";
    let mut groups_deleted = 0u32;
    let mut groups_failed = 0u32;

    for &chat_id in &dialogs {
        if chat_id >= 0 {
            continue; // skip user chats
        }
        match client.get_chat(chat_id).await {
            Ok(info) => {
                let title = info.title.as_str();
                if title.starts_with(test_title_prefix) {
                    println!("Found test group: {} (chat_id: {})", title, chat_id);
                    if dry_run {
                        println!("[dry-run] Would delete group: {}", title);
                    } else {
                        // Try delete_chat first (owner), then leave_chat (fallback)
                        if let Err(e) = client.delete_chat(chat_id).await {
                            eprintln!("delete_chat failed for {}: {e}, trying leave_chat", title);
                            if let Err(e2) = client.leave_chat(chat_id).await {
                                eprintln!("leave_chat also failed for {}: {e2}", title);
                                groups_failed += 1;
                            } else {
                                groups_deleted += 1;
                                println!("Left group: {}", title);
                            }
                        } else {
                            groups_deleted += 1;
                            println!("Deleted group: {}", title);
                        }
                    }
                }
            }
            Err(e) => {
                eprintln!("get_chat({}) failed (skipping): {e}", chat_id);
            }
        }
        tokio::time::sleep(Duration::from_millis(300)).await;
    }

    // =========================================================================
    // Summary
    // =========================================================================
    println!("\n=== Summary ===");
    println!("Messages deleted: {}", deleted_count);
    println!("Messages failed:  {}", failed_count);
    println!("Groups deleted:   {}", groups_deleted);
    println!("Groups failed:    {}", groups_failed);
    if dry_run {
        println!("\n(dry-run mode, nothing was actually deleted)");
    }
}
