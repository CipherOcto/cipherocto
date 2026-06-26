/// Standalone cleanup utility for MTProto live test artifacts.
///
/// Usage:
///   cargo run -p octo-adapter-telegram-mtproto --features real-network --bin cleanup_test_artifacts -- --dry-run
///   cargo run -p octo-adapter-telegram-mtproto --features real-network --bin cleanup_test_artifacts
///   cargo run -p octo-adapter-telegram-mtproto --features real-network --bin cleanup_test_artifacts -- --all  (delete ALL messages in Saved Messages)
///
/// Cleans:
///   1. Messages in Saved Messages (full history via messages.getHistory)
///   2. Groups with title prefix "octo_test_" (test groups)
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use grammers_tl_types as tl;
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
    let clear_all = std::env::args().any(|a| a == "--all");

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
    // Phase 1: Clean up messages in Saved Messages via getHistory
    // =========================================================================
    if clear_all {
        println!("\n=== Phase 1: Clearing ALL messages in Saved Messages ===");
    } else {
        println!("\n=== Phase 1: Cleaning test messages in Saved Messages ===");
    }
    let mut deleted_count = 0u32;
    let mut failed_count = 0u32;

    let test_prefixes = ["OCTO_LIVE_", "LT-", "LT_", "octo_test_", "DOT/1/", "test ", "lt4", "lt5"];

    // Saved Messages = InputPeerSelf. Use raw TL getHistory.
    let self_peer = tl::enums::InputPeer::PeerSelf;

    let mut offset_id = 0i32;
    let mut total_scanned = 0u32;
    let mut consecutive_empty = 0u32;
    let limit = 100i32;

    loop {
        let req = tl::functions::messages::GetHistory {
            peer: self_peer.clone(),
            offset_id,
            offset_date: 0,
            add_offset: 0,
            limit,
            max_id: 0,
            min_id: 0,
            hash: 0,
        };

        let response = match client.grammers_client().invoke(&req).await {
            Ok(r) => r,
            Err(e) => {
                eprintln!("getHistory failed: {e}");
                break;
            }
        };

        // Extract messages from the response.
        let messages = match &response {
            tl::enums::messages::Messages::Messages(msgs) => &msgs.messages,
            tl::enums::messages::Messages::Slice(slice) => &slice.messages,
            tl::enums::messages::Messages::ChannelMessages(cm) => &cm.messages,
            tl::enums::messages::Messages::NotModified(_) => {
                eprintln!("getHistory returned NotModified, stopping");
                break;
            }
        };

        if messages.is_empty() {
            consecutive_empty += 1;
            if consecutive_empty >= 2 {
                break;
            }
            continue;
        }
        consecutive_empty = 0;

        let mut batch_ids = Vec::new();
        for msg in messages {
            total_scanned += 1;
            // Update offset_id for next page.
            if let tl::enums::Message::Message(m) = msg {
                offset_id = offset_id.max(m.id);
                let text = m.message.as_str();
                let is_test = clear_all || test_prefixes.iter().any(|p| text.starts_with(p));
                if is_test {
                    batch_ids.push(m.id);
                    if batch_ids.len() <= 20 {
                        eprintln!(
                            "  [found] msg_id={} msg={}",
                            m.id,
                            &text[..text.len().min(60)]
                        );
                    }
                }
            }
        }

        if !batch_ids.is_empty() {
            if dry_run {
                println!("[dry-run] Would delete {} messages", batch_ids.len(),);
            } else {
                match client.delete_messages(user_id, &batch_ids, true).await {
                    Ok(()) => {
                        deleted_count += batch_ids.len() as u32;
                        println!("Deleted {} messages", batch_ids.len());
                    }
                    Err(e) => {
                        failed_count += batch_ids.len() as u32;
                        eprintln!("delete_messages batch failed: {e}");
                    }
                }
            }
            tokio::time::sleep(Duration::from_millis(500)).await;
        }

        // If we got fewer than `limit` messages, we've reached the end.
        if (messages.len() as i32) < limit {
            break;
        }

        // offset_id is already set to the last message ID we saw.
        // getHistory returns messages BEFORE offset_id, so we're good.
    }

    eprintln!("Phase 1: scanned {} messages total", total_scanned);

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
                        match delete_with_flood_wait(&client, chat_id, title).await {
                            Ok(action) => {
                                groups_deleted += 1;
                                println!("{} group: {}", action, title);
                            }
                            Err(e) => {
                                eprintln!("Failed to delete/leave {}: {e}", title);
                                groups_failed += 1;
                            }
                        }
                    }
                }
            }
            Err(e) => {
                eprintln!("get_chat({}) failed (skipping): {e}", chat_id);
            }
        }
        tokio::time::sleep(Duration::from_secs(5)).await;
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

/// Maximum FLOOD_WAIT seconds we'll honor before giving up.
const FLOOD_WAIT_CAP_SECS: u64 = 120;

/// Maximum retries for any FLOOD_WAIT-triggering operation.
const FLOOD_WAIT_MAX_RETRIES: u32 = 3;

/// Extract FLOOD_WAIT seconds from an error message.
/// Handles both `(value: N)` and bare `FLOOD_WAIT N` patterns.
/// Returns a conservative default (30s) if FLOOD_WAIT is detected
/// but the value cannot be parsed.
fn parse_flood_wait(err: &str) -> Option<u64> {
    if !err.contains("FLOOD_WAIT") {
        return None;
    }
    // Pattern 1: "(value: N)" — standard Telegram format.
    let marker = "(value: ";
    if let Some(start) = err.find(marker) {
        let start = start + marker.len();
        if let Some(end) = err[start..].find(')') {
            if let Ok(n) = err[start..start + end].trim().parse::<u64>() {
                if n > 0 {
                    return Some(n);
                }
            }
        }
    }
    // Pattern 2: bare "FLOOD_WAIT N" — fallback.
    if let Some(idx) = err.find("FLOOD_WAIT") {
        let after = &err[idx + "FLOOD_WAIT".len()..];
        let trimmed = after.trim_start_matches(|c: char| !c.is_ascii_digit());
        if let Some(end_idx) = trimmed.find(|c: char| !c.is_ascii_digit()) {
            if let Ok(n) = trimmed[..end_idx].parse::<u64>() {
                if n > 0 {
                    return Some(n);
                }
            }
        }
    }
    // FLOOD_WAIT detected but value unparseable — use conservative default.
    eprintln!("FLOOD_WAIT detected but value unparseable from: {err}");
    Some(30)
}

/// Compute capped sleep duration for a FLOOD_WAIT value.
fn flood_wait_sleep_secs(wait_secs: u64) -> u64 {
    wait_secs.min(FLOOD_WAIT_CAP_SECS) + 5
}

/// Try delete_chat with FLOOD_WAIT retry (up to 3 times, capped).
/// Falls back to leave_chat with same retry policy.
async fn delete_with_flood_wait(
    client: &Arc<RealTelegramMtprotoClient>,
    chat_id: i64,
    title: &str,
) -> Result<String, String> {
    // Attempt delete_chat with retries.
    let mut last_delete_err: Option<String> = None;
    for attempt in 0..=FLOOD_WAIT_MAX_RETRIES {
        match client.delete_chat(chat_id).await {
            Ok(()) => return Ok(if attempt == 0 { "Deleted" } else { "Deleted (after wait)" }.into()),
            Err(e) => {
                let err_str = e.to_string();
                if let Some(wait_secs) = parse_flood_wait(&err_str) {
                    let sleep_secs = flood_wait_sleep_secs(wait_secs);
                    eprintln!(
                        "FLOOD_WAIT on delete_chat for {}: attempt {}/{}, sleeping {}s (requested {}s)",
                        title, attempt + 1, FLOOD_WAIT_MAX_RETRIES + 1, sleep_secs, wait_secs
                    );
                    tokio::time::sleep(Duration::from_secs(sleep_secs)).await;
                    last_delete_err = Some(err_str);
                } else {
                    // Not a FLOOD_WAIT — record and break to fallback.
                    last_delete_err = Some(err_str);
                    break;
                }
            }
        }
    }

    // Fallback: leave_chat with retries.
    let mut last_leave_err: Option<String> = None;
    for attempt in 0..=FLOOD_WAIT_MAX_RETRIES {
        match client.leave_chat(chat_id).await {
            Ok(()) => return Ok(if attempt == 0 { "Left" } else { "Left (after wait)" }.into()),
            Err(e2) => {
                let err2_str = e2.to_string();
                if let Some(wait_secs) = parse_flood_wait(&err2_str) {
                    let sleep_secs = flood_wait_sleep_secs(wait_secs);
                    eprintln!(
                        "FLOOD_WAIT on leave_chat for {}: attempt {}/{}, sleeping {}s",
                        title, attempt + 1, FLOOD_WAIT_MAX_RETRIES + 1, sleep_secs
                    );
                    tokio::time::sleep(Duration::from_secs(sleep_secs)).await;
                    last_leave_err = Some(err2_str);
                } else {
                    last_leave_err = Some(err2_str);
                    break;
                }
            }
        }
    }

    Err(format!(
        "delete_chat: {}; leave_chat: {}",
        last_delete_err.unwrap_or_default(),
        last_leave_err.unwrap_or_default()
    ))
}
