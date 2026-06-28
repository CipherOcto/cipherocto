use grammers_tl_types as tl;
use octo_adapter_telegram_mtproto::config::MtprotoTelegramConfig;
use octo_adapter_telegram_mtproto::real_client::RealTelegramMtprotoClient;
use octo_adapter_telegram_mtproto::self_handle::MtprotoSelfHandle;
use octo_adapter_telegram_mtproto::session::StoolapSession;
/// Standalone utility to list contacts/users for live test configuration.
///
/// Lists all user dialogs with their user_id, username, display name,
/// phone, and contact status. Output is formatted for easy selection
/// of a test partner.
///
/// Usage:
///   cargo run -p octo-adapter-telegram-mtproto --features real-network --bin list_test_users
use std::path::PathBuf;

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

struct UserInfo {
    user_id: i64,
    first_name: String,
    last_name: String,
    username: String,
    phone: String,
    is_bot: bool,
    is_contact: bool,
    is_mutual_contact: bool,
}

#[tokio::main]
async fn main() {
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

    let me = match client.grammers_client().get_me().await {
        Ok(me) => {
            let user_id = me.id().bare_id();
            let username = me.username().map(String::from);
            self_handle.set_identity(user_id, username);
            me
        }
        Err(e) => {
            eprintln!("get_me() failed: {e}");
            eprintln!("Re-run: rm -rf ~/.local/share/octo/telegram-mtproto/session.db*");
            eprintln!("./scripts/mtproto-onboard-qr.sh");
            std::process::exit(1);
        }
    };

    let self_id = me.id().bare_id();
    println!(
        "Logged in as: {} (user_id: {})\n",
        me.username().unwrap_or("?"),
        self_id
    );

    // Step 1: Collect user peer IDs from dialogs.
    println!("Scanning dialogs...");
    let mut iter = client.grammers_client().iter_dialogs();
    let mut user_peer_ids: Vec<i64> = Vec::new();
    let mut total_dialogs = 0u32;

    loop {
        let dialog = match iter.next().await {
            Ok(Some(d)) => d,
            Ok(None) => break,
            Err(e) => {
                eprintln!("iter_dialogs error: {e}");
                break;
            }
        };
        total_dialogs += 1;

        let peer = dialog.peer();
        let peer_id = peer.id();
        let kind = peer_id.kind();

        use grammers_session::types::PeerKind;
        match kind {
            PeerKind::User | PeerKind::UserSelf => {}
            _ => continue,
        }

        let bare_id = peer_id.bare_id();
        if bare_id == self_id {
            continue;
        }

        user_peer_ids.push(bare_id);
    }

    println!(
        "Found {} user dialogs out of {} total.\n",
        user_peer_ids.len(),
        total_dialogs
    );

    if user_peer_ids.is_empty() {
        println!("No user dialogs found. Start a chat with someone first.");
        return;
    }

    // Step 2: Batch-fetch user details via users.getUsers.
    let input_users: Vec<tl::enums::InputUser> = user_peer_ids
        .iter()
        .map(|&id| {
            tl::enums::InputUser::User(tl::types::InputUser {
                user_id: id,
                access_hash: 0, // resolve_peer will populate from session cache
            })
        })
        .collect();

    let raw_users: Vec<tl::enums::User> = client
        .grammers_client()
        .invoke(&tl::functions::users::GetUsers { id: input_users })
        .await
        .unwrap_or_else(|e| {
            eprintln!("users.getUsers failed: {e}");
            eprintln!("Falling back to resolve_peer (slower)...");
            Vec::new()
        });

    let mut users: Vec<UserInfo> = Vec::new();

    if !raw_users.is_empty() {
        // Parse from batch response.
        for raw in &raw_users {
            if let tl::enums::User::User(u) = raw {
                users.push(UserInfo {
                    user_id: u.id,
                    first_name: u.first_name.clone().unwrap_or_default(),
                    last_name: u.last_name.clone().unwrap_or_default(),
                    username: u.username.clone().unwrap_or_default(),
                    phone: u.phone.clone().unwrap_or_default(),
                    is_bot: u.bot,
                    is_contact: u.contact,
                    is_mutual_contact: u.mutual_contact,
                });
            }
        }
    } else {
        // Fallback: resolve each peer individually.
        for &user_id in &user_peer_ids {
            let input_peer = tl::enums::InputPeer::User(tl::types::InputPeerUser {
                user_id,
                access_hash: 0,
            });
            match client.grammers_client().resolve_peer(input_peer).await {
                Ok(grammers_client::peer::Peer::User(u)) => {
                    users.push(UserInfo {
                        user_id: u.id().bare_id(),
                        first_name: u.first_name().unwrap_or("").to_string(),
                        last_name: u.last_name().unwrap_or("").to_string(),
                        username: u.username().unwrap_or("").to_string(),
                        phone: u.phone().unwrap_or("").to_string(),
                        is_bot: u.is_bot(),
                        is_contact: u.contact(),
                        is_mutual_contact: u.mutual_contact(),
                    });
                }
                Ok(_) => {
                    eprintln!("  user_id {} resolved to non-User peer (skipped)", user_id);
                }
                Err(e) => {
                    eprintln!("  resolve_peer({}) failed: {} (skipped)", user_id, e);
                }
            }
        }
    }

    // Step 3: Sort and display.
    users.sort_by_key(|u| u.user_id);

    println!(
        "{:<4} {:<12} {:<20} {:<20} {:<16} {:<6} {}",
        "#", "user_id", "first_name", "last_name", "username", "phone", "flags"
    );
    println!("{}", "-".repeat(100));

    for (i, u) in users.iter().enumerate() {
        let mut flags = Vec::new();
        if u.is_bot {
            flags.push("bot");
        }
        if u.is_contact {
            flags.push("contact");
        }
        if u.is_mutual_contact {
            flags.push("mutual");
        }

        let first = if u.first_name.is_empty() {
            "-"
        } else {
            &u.first_name
        };
        let last = if u.last_name.is_empty() {
            "-"
        } else {
            &u.last_name
        };
        let uname = if u.username.is_empty() {
            String::from("-")
        } else {
            format!("@{}", u.username)
        };
        let phone = if u.phone.is_empty() { "-" } else { &u.phone };
        let flag_str = if flags.is_empty() {
            String::new()
        } else {
            flags.join(", ")
        };

        println!(
            "{:<4} {:<12} {:<20} {:<20} {:<16} {:<6} {}",
            i + 1,
            u.user_id,
            first,
            last,
            uname,
            phone,
            flag_str,
        );
    }

    println!();
    println!("To use a user for live tests, set:");
    println!("  export OCTO_TEST_USER_ID=<user_id>");
    println!();
    println!("Recommended: pick a mutual contact who is NOT a bot.");
    if let Some(best) = users.iter().find(|u| u.is_mutual_contact && !u.is_bot) {
        println!("  Suggested: export OCTO_TEST_USER_ID={}", best.user_id);
    }
}
