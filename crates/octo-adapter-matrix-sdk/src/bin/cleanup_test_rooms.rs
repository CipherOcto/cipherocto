//! Standalone cleanup utility for Matrix live-test artifacts (mission
//! 0850h-b §Live-Test Cleanup Infrastructure).
//!
//! The live integration suite (`mx01`–`mx08`) creates short-lived
//! test rooms whose names follow the prefix `octo-test-mx-*`. When
//! a test panics before its cleanup block runs, the room it created
//! is left orphaned on the homeserver, and the next test run picks
//! up the stale `room_id` from `~/.config/octo/matrix.json`'s
//! `rooms[]` array. The adapter then fails with
//! `Room <id> not found in joined rooms`. This binary prunes those
//! stale rooms the same way `cleanup_test_groups.rs` does for
//! WhatsApp and `cleanup_test_artifacts.rs` does for Telegram.
//!
//! ## Usage
//!
//! ```text
//! # Scan only (no state change)
//! cargo run -p octo-adapter-matrix-sdk \
//!     --bin cleanup_test_rooms -- --dry-run
//!
//! # Leave all stale rooms and (optionally) rewrite the session file
//! cargo run -p octo-adapter-matrix-sdk \
//!     --bin cleanup_test_rooms -- --update-config
//! ```
//!
//! ## What it cleans
//!
//! 1. **Rooms we're still in with name prefix `octo-test-mx-`** —
//!    calls `room.leave()` (idempotent on already-left rooms;
//!    silently swallows the SDK's `WrongRoomState` error).
//! 2. **`room_id`s in the session file's `rooms[]` array that the
//!    SDK no longer resolves via `client.get_room(&&rid)`** — these
//!    are the exact failure mode of `mx04_05_06_envelope_round_trip`.
//!    Reported in the summary; only `--update-config` actually
//!    rewrites the session file.
//!
//! ## Env vars / flags
//!
//! - `--config <path>` — override session file location (default:
//!   `~/.config/octo/matrix.json` on Unix, `%APPDATA%\octo\matrix.json`
//!   on Windows, via `dirs`)
//! - `--dry-run` — scan only, no leaves, no writes
//! - `--update-config` — rewrite the session file with the pruned
//!   `rooms[]` array (off by default; off is safer)
//!
//! ## SDK logging
//!
//! Set `RUST_LOG=matrix_sdk=info` (or `debug`) in the environment to
//! see SDK-level logs. The binary itself only emits structured
//! stdout summaries via `println!` — no tracing-subscriber init
//! needed (kept out of `[dependencies]` to keep the binary's build
//! cost minimal).

use std::collections::HashSet;
use std::path::PathBuf;
use std::time::Duration;

use matrix_sdk::authentication::matrix::MatrixSession;
use matrix_sdk::config::SyncSettings;
use matrix_sdk::ruma::{OwnedDeviceId, OwnedRoomId, OwnedUserId, RoomId};
use matrix_sdk::{Client, SessionMeta, SessionTokens};
use serde_json::Value;

/// Name prefix the cipherocto live tests use for their rooms.
const TEST_ROOM_PREFIX: &str = "octo-test-mx-";

/// Default session path (Unix). Windows path is computed at runtime.
fn default_session_path() -> PathBuf {
    dirs::config_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("octo")
        .join("matrix.json")
}

/// Parse the `--config <path>` flag if present.
fn parse_config_path(args: &[String]) -> Option<PathBuf> {
    let mut iter = args.iter();
    while let Some(a) = iter.next() {
        if a == "--config" {
            return iter.next().map(PathBuf::from);
        }
        if let Some(rest) = a.strip_prefix("--config=") {
            return Some(PathBuf::from(rest));
        }
    }
    None
}

fn flag_present(args: &[String], flag: &str) -> bool {
    args.iter().any(|a| a == flag)
}

/// Load the session JSON from `path` and return the parsed `Value`.
///
/// Panics with a clear message if the file is missing — that's the
/// expected behaviour for a live-test-only utility.
fn load_session(path: &PathBuf) -> Value {
    let bytes = std::fs::read(path).unwrap_or_else(|e| {
        panic!(
            "could not read session file at {}: {}\n\
             run `octo-matrix-onboard login oidc --homeserver https://matrix.org` first.",
            path.display(),
            e,
        )
    });
    serde_json::from_slice::<Value>(&bytes).unwrap_or_else(|e| {
        panic!("could not parse session JSON at {}: {}", path.display(), e);
    })
}

/// Build a matrix-sdk Client and restore the session.
async fn build_session_client(session: &Value) -> Client {
    let user_id = OwnedUserId::try_from(
        session["user_id"]
            .as_str()
            .expect("session.user_id is required"),
    )
    .expect("session.user_id is a valid MXID");
    let device_id = OwnedDeviceId::from(
        session["device_id"]
            .as_str()
            .expect("session.device_id is required"),
    );

    let client = Client::builder()
        .homeserver_url(
            session["homeserver_url"]
                .as_str()
                .expect("session.homeserver_url is required"),
        )
        .build()
        .await
        .expect("Client::builder().build() failed");

    client
        .restore_session(MatrixSession {
            meta: SessionMeta { user_id, device_id },
            tokens: SessionTokens {
                access_token: session["access_token"]
                    .as_str()
                    .expect("session.access_token is required")
                    .to_string(),
                refresh_token: session["refresh_token"].as_str().map(|s| s.to_string()),
            },
        })
        .await
        .expect("client.restore_session failed");
    client
}

/// Sync once with a generous timeout. The 60 s window is enough for
/// E2EE bootstrap (one-time key upload + crypto-store init) on a
/// fresh session — the 5 s timeout used in the live tests themselves
/// is too tight for first sync, see the mx01 follow-up.
///
/// Note: `Client::sync_once` returns `Result<SyncResponse, Error>`;
/// we discard the response (the binary only needs the post-sync
/// state, not the since-token).
async fn sync_with_grace(
    client: &Client,
) -> Result<matrix_sdk::sync::SyncResponse, matrix_sdk::Error> {
    client
        .sync_once(SyncSettings::default().timeout(Duration::from_secs(60)))
        .await
}

/// Inspect a room's name. Returns `None` for unnamed rooms.
fn room_name(room: &matrix_sdk::Room) -> Option<String> {
    room.name()
}

/// Pretty-print a `RoomId` for terminal output.
fn rid_label(rid: &RoomId) -> &str {
    rid.as_str()
}

#[tokio::main]
async fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let dry_run = flag_present(&args, "--dry-run");
    let update_config = flag_present(&args, "--update-config");

    let session_path = parse_config_path(&args).unwrap_or_else(default_session_path);

    println!("Loading session from {}", session_path.display());
    let session = load_session(&session_path);

    // Snapshot the session file's rooms[] array so we can cross-ref
    // it against the SDK's actual joined-room set after sync.
    let session_room_ids: Vec<String> = session
        .get("rooms")
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|v| v.as_str().map(String::from))
                .collect()
        })
        .unwrap_or_default();

    println!(
        "Session reports {} room(s) in the rooms[] array:",
        session_room_ids.len()
    );
    for rid in &session_room_ids {
        println!("  {}", rid);
    }

    println!("\nBuilding matrix-sdk Client and restoring session...");
    let client = build_session_client(&session).await;

    println!("Syncing (60 s budget for E2EE bootstrap on first sync)...");
    if let Err(e) = sync_with_grace(&client).await {
        eprintln!("first sync failed: {e}");
        eprintln!("Cannot enumerate rooms without a successful sync. Aborting.");
        std::process::exit(2);
    }
    println!("First sync OK. Syncing once more to settle room state...");
    // A second sync is required because `joined_rooms()` reads from
    // the in-memory `BaseClient` room store, which only updates on a
    // sync. The first sync bootstrap the E2EE crypto store; the
    // second picks up any state transitions (e.g., rooms we just
    // left in a previous run of this binary) so that
    // `joined_rooms()` accurately reflects "rooms we are still in".
    if let Err(e) = sync_with_grace(&client).await {
        eprintln!("second sync failed: {e}");
        eprintln!("Continuing with possibly-stale state — Phase 1 may list rooms we already left.");
    }
    println!("Second sync OK.\n");

    // Phase 1: rooms we're currently in whose name starts with the
    // test prefix. Use `joined_rooms()` (not `rooms()`) so we don't
    // re-attempt to leave rooms we already left in a prior run —
    // `rooms()` includes joined + invited + left, and the in-memory
    // `BaseClient` state retains left rooms until a re-sync.
    let prefix_targets: Vec<_> = client
        .joined_rooms()
        .into_iter()
        .filter_map(|room| {
            let name = room_name(&room)?;
            if name.starts_with(TEST_ROOM_PREFIX) {
                Some((room.room_id().to_owned(), name))
            } else {
                None
            }
        })
        .collect();

    // Phase 2: rooms whose IDs appear in the session file's rooms[]
    // array but the SDK no longer resolves them.
    let mut orphaned_session_rooms: Vec<String> = Vec::new();
    for rid_str in &session_room_ids {
        if let Ok(parsed) = OwnedRoomId::try_from(rid_str.as_str()) {
            if client.get_room(&parsed).is_none() {
                orphaned_session_rooms.push(rid_str.clone());
            }
        } else {
            // Malformed room ID in the file — also an orphan.
            orphaned_session_rooms.push(rid_str.clone());
        }
    }

    println!(
        "=== Phase 1: rooms we're in matching prefix `{}` ===",
        TEST_ROOM_PREFIX
    );
    if prefix_targets.is_empty() {
        println!("  (none)");
    } else {
        for (rid, name) in &prefix_targets {
            println!("  {}  name={:?}", rid_label(rid.as_ref()), name);
        }
    }

    println!("\n=== Phase 2: rooms in session file's rooms[] but not in joined rooms ===");
    if orphaned_session_rooms.is_empty() {
        println!("  (none)");
    } else {
        for rid in &orphaned_session_rooms {
            println!("  {}", rid);
        }
    }

    if prefix_targets.is_empty() && orphaned_session_rooms.is_empty() && !update_config {
        println!("\nNothing to clean. Done.");
        return;
    }

    if dry_run {
        println!(
            "\n[dry-run] Would leave {} prefixed room(s).",
            prefix_targets.len()
        );
        println!(
            "[dry-run] Would report {} orphaned session-file room(s){}.",
            orphaned_session_rooms.len(),
            if update_config {
                " and rewrite the session file"
            } else {
                ""
            },
        );
        return;
    }

    // Phase 3: leave the prefixed rooms.
    let mut left_ok = 0u32;
    let mut left_failed = 0u32;
    for (rid, name) in &prefix_targets {
        // Re-look up in case state changed between scan and now.
        let Some(room) = client.get_room(rid.as_ref()) else {
            println!(
                "  [skip] {} — no longer in joined rooms",
                rid_label(rid.as_ref())
            );
            continue;
        };
        match room.leave().await {
            Ok(()) => {
                left_ok += 1;
                println!("  left: {}  name={:?}", rid_label(rid.as_ref()), name);
            }
            Err(e) => {
                left_failed += 1;
                eprintln!(
                    "  leave FAILED for {}  name={:?}: {e}",
                    rid_label(rid.as_ref()),
                    name,
                );
            }
        }
    }

    // Phase 4: prune the session file's rooms[] array if requested.
    let mut pruned_session_rooms = 0u32;
    if update_config {
        let joined_ids: HashSet<String> = client
            .joined_rooms()
            .into_iter()
            .map(|r| r.room_id().to_string())
            .collect();
        let new_rooms: Vec<String> = session_room_ids
            .iter()
            .filter(|rid| joined_ids.contains(rid.as_str()))
            .cloned()
            .collect();
        pruned_session_rooms = (session_room_ids.len() - new_rooms.len()) as u32;

        let mut updated_session = session.clone();
        updated_session["rooms"] = serde_json::json!(new_rooms);
        let pretty =
            serde_json::to_string_pretty(&updated_session).expect("re-serialize session JSON");
        std::fs::write(&session_path, pretty).unwrap_or_else(|e| {
            panic!(
                "could not write updated session file to {}: {}",
                session_path.display(),
                e
            );
        });
        println!(
            "\nRewrote {} (pruned {} orphan(s) from rooms[]).",
            session_path.display(),
            pruned_session_rooms
        );
    } else if !orphaned_session_rooms.is_empty() {
        println!(
            "\nNote: {} session-file room(s) are orphaned. Pass --update-config to rewrite the session file.",
            orphaned_session_rooms.len()
        );
    }

    // Summary
    println!("\n=== Summary ===");
    println!("Prefixed rooms left OK:    {}", left_ok);
    println!("Prefixed rooms failed:     {}", left_failed);
    println!(
        "Orphaned session rooms:    {}",
        orphaned_session_rooms.len()
    );
    println!("Session rooms pruned:      {}", pruned_session_rooms);
}
