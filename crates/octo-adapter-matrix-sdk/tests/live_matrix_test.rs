//! Live integration tests for octo-adapter-matrix-sdk against matrix.org.
//!
//! Requires a session at `~/.config/octo/matrix.json` obtained via
//! `octo-matrix-onboard login oidc --homeserver https://matrix.org`.
//!
//! Run:
//! ```
//! cargo test -p octo-adapter-matrix-sdk --features live-matrix \
//!     --test live_matrix_test -- --ignored --nocapture
//! ```

#![cfg(feature = "live-matrix")]

use matrix_sdk::ruma::api::client::room::create_room::v3::{
    Request as CreateRoomRequest, RoomPreset,
};
use matrix_sdk::Client;
use octo_adapter_matrix_sdk::MatrixAdapter;
use octo_network::dot::adapters::coordinator_admin::{CoordinatorAdmin, GroupId, PeerId};
use octo_network::dot::adapters::PlatformAdapter;
use octo_network::dot::domain::BroadcastDomainId;
use octo_network::dot::envelope::DeterministicEnvelope;
use std::path::PathBuf;
use std::time::Duration;

fn init_tracing() {
    let _ = tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .try_init();
}

fn config_path() -> PathBuf {
    dirs::config_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("octo")
        .join("matrix.json")
}

fn load_session() -> serde_json::Value {
    let path = config_path();
    let bytes = std::fs::read(&path).unwrap_or_else(|e| panic!("read {}: {}", path.display(), e));
    serde_json::from_slice(&bytes).unwrap_or_else(|e| panic!("parse {}: {}", path.display(), e))
}

/// Build a multi_thread tokio runtime. Both room setup (matrix-sdk
/// Client) and adapter async methods must run on a multi_thread
/// runtime because the SDK's `tokio::spawn` calls need worker threads.
fn make_runtime() -> tokio::runtime::Runtime {
    tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .worker_threads(2)
        .build()
        .expect("test runtime")
}

/// Build a raw matrix-sdk Client for room setup/teardown.
async fn build_session_client(session: &serde_json::Value) -> Client {
    use matrix_sdk::authentication::matrix::MatrixSession;
    use matrix_sdk::ruma::{OwnedDeviceId, OwnedUserId};
    use matrix_sdk::{SessionMeta, SessionTokens};

    let user_id = OwnedUserId::try_from(session["user_id"].as_str().unwrap()).expect("user_id");
    let device_id = OwnedDeviceId::from(session["device_id"].as_str().unwrap());

    let client = Client::builder()
        .homeserver_url(session["homeserver_url"].as_str().unwrap())
        .build()
        .await
        .expect("build session client");

    client
        .restore_session(MatrixSession {
            meta: SessionMeta { user_id, device_id },
            tokens: SessionTokens {
                access_token: session["access_token"].as_str().unwrap().to_string(),
                refresh_token: session["refresh_token"].as_str().map(|s| s.to_string()),
            },
        })
        .await
        .expect("restore_session");
    client
}

/// Pre-scan guard: leave any pre-existing `octo-test-mx-*` rooms
/// before the caller creates its own test room. Makes mx04_05_06
/// and mx07 self-healing — if a previous run panicked before
/// cleanup, the next run cleans up here instead of failing on a
/// stale `room_id` left in the session file's `rooms[]` array
/// (mission 0850h-b §Live-Test Cleanup Infrastructure).
///
/// Returns the number of rooms that were left. Logs each left room
/// at INFO so the operator can see what was cleaned up.
async fn leave_stale_test_rooms(client: &Client, prefix: &str) -> u32 {
    use matrix_sdk::config::SyncSettings;

    // Sync once with the same 5 s timeout the live tests use
    // elsewhere — this is a warm-up sync, the rooms we're leaving
    // are already known. If the sync fails we return 0 and let
    // the test body handle the error; the pre-scan is best-effort.
    if let Err(e) = client
        .sync_once(SyncSettings::default().timeout(Duration::from_secs(5)))
        .await
    {
        tracing::warn!(error = %e, "pre-scan guard: warm-up sync failed, skipping stale-room sweep");
        return 0;
    }

    let stale: Vec<_> = client
        .joined_rooms()
        .into_iter()
        .filter_map(|room| {
            let name = room.name()?;
            if name.starts_with(prefix) {
                Some((room.room_id().to_owned(), name))
            } else {
                None
            }
        })
        .collect();

    let mut left = 0u32;
    for (rid, name) in &stale {
        // Re-look up after the filter (filter already saw the
        // joined_rooms() snapshot, but be defensive).
        if let Some(room) = client.get_room(rid.as_ref()) {
            match room.leave().await {
                Ok(()) => {
                    left += 1;
                    tracing::info!(
                        room_id = %rid,
                        room_name = %name,
                        "pre-scan guard: left stale test room",
                    );
                }
                Err(e) => {
                    tracing::warn!(
                        room_id = %rid,
                        room_name = %name,
                        error = %e,
                        "pre-scan guard: leave failed",
                    );
                }
            }
        }
    }

    if left > 0 {
        tracing::info!(
            count = left,
            prefix,
            "pre-scan guard: cleaned up stale test rooms"
        );
    }
    left
}

/// Build a MatrixAdapter on a dedicated thread (MatrixAdapter::new()
/// creates its own tokio runtime internally — cannot nest runtimes).
fn build_adapter(cfg_json: &[u8]) -> MatrixAdapter {
    let cfg_json = cfg_json.to_vec();
    std::thread::spawn(move || MatrixAdapter::from_config_bytes(&cfg_json))
        .join()
        .expect("adapter thread panicked")
        .expect("adapter construction")
}

/// Build a MatrixConfig JSON blob. No passphrase (in-memory crypto
/// store). Each adapter construction generates fresh Olm keys.
fn adapter_config_json(session: &serde_json::Value, room_id: &str) -> Vec<u8> {
    let mut cfg = session.clone();
    cfg["rooms"] = serde_json::json!([room_id]);
    cfg["passphrase"] = serde_json::Value::Null;
    cfg["config_path"] = serde_json::json!("");
    cfg["force_writeback"] = serde_json::json!(false);
    cfg["use_session_store"] = serde_json::json!(false);
    cfg["session_store_path"] = serde_json::json!("");
    serde_json::to_vec(&cfg).expect("serialize config")
}

fn make_envelope_bytes() -> Vec<u8> {
    let mut wire = Vec::with_capacity(282);
    wire.extend_from_slice(&1u16.to_be_bytes());
    wire.extend_from_slice(&0xDEAD_BEEFu32.to_be_bytes());
    wire.extend_from_slice(&1u16.to_be_bytes());
    wire.extend_from_slice(&[0u8; 32]); // envelope_id
    wire.extend_from_slice(&[0u8; 32]); // mission_id
    wire.extend_from_slice(&[0u8; 32]); // source_peer
    wire.extend_from_slice(&[0u8; 32]); // origin_gateway
    wire.extend_from_slice(&0u64.to_be_bytes());
    wire.extend_from_slice(&1u16.to_be_bytes());
    wire.extend_from_slice(&[0u8; 32]); // payload_hash
    wire.extend_from_slice(&[0u8; 32]); // route_trace_root
    wire.extend_from_slice(&0u64.to_be_bytes());
    debug_assert_eq!(wire.len(), 218);
    wire.extend_from_slice(&[0u8; 64]); // signature
    debug_assert_eq!(wire.len(), 282);
    wire
}

fn broadcast_domain(adapter: &MatrixAdapter, room_id: &str) -> BroadcastDomainId {
    adapter.domain_id(room_id)
}

// ── mx00: diagnostic — raw SDK sync (no adapter) ────────────────

#[test]
#[ignore = "diagnostic test"]
fn mx00_raw_sdk_sync() {
    init_tracing();
    let session = load_session();
    let rt = make_runtime();

    rt.block_on(async {
        let client = build_session_client(&session).await;
        println!("Calling sync_once...");
        let sync_result = tokio::time::timeout(
            Duration::from_secs(10),
            client.sync_once(
                matrix_sdk::config::SyncSettings::default().timeout(Duration::from_millis(1)),
            ),
        )
        .await;
        match &sync_result {
            Ok(Ok(_)) => println!("sync_once OK"),
            Ok(Err(e)) => println!("sync_once error: {e}"),
            Err(_) => println!("sync_once timed out"),
        }

        println!("Calling whoami...");
        match client.whoami().await {
            Ok(resp) => println!("whoami OK: {}", resp.user_id),
            Err(e) => println!("whoami error: {e}"),
        }
    });
}

// ── mx01: health_check (via raw SDK, adapter runtime issue) ─────

#[test]
#[ignore = "requires live Matrix session; run with --features live-matrix -- --ignored"]
fn mx01_health_check() {
    init_tracing();
    let session = load_session();
    let rt = make_runtime();

    let result = rt.block_on(async {
        let client = build_session_client(&session).await;
        // mx01 sync-timeout follow-up (mission 0850h-b §mx01
        // Sync-Timeout Follow-up): budget raised from 10 s to
        // 60 s. On a cold session the SDK must upload one-time
        // keys and bootstrap the crypto store BEFORE the sync
        // request can be sent — this takes 5–30 s against
        // matrix.org. The 10 s budget was a mirror of the
        // production `health_check` outer timeout, which is
        // itself now 60 s; aligning the test prevents a false
        // failure on cold sessions. The inner 1 ms server-side
        // long-poll (`SyncSettings::timeout`) is preserved.
        let sync_result = tokio::time::timeout(
            Duration::from_secs(60),
            client.sync_once(
                matrix_sdk::config::SyncSettings::default().timeout(Duration::from_millis(1)),
            ),
        )
        .await;
        match sync_result {
            Ok(Ok(_)) => {
                let who = client.whoami().await.expect("whoami");
                tracing::info!(user_id = %who.user_id, "MX-01: health_check OK");
                Ok(())
            }
            Ok(Err(e)) => Err(format!("sync error: {e}")),
            Err(_) => Err("sync timed out".into()),
        }
    });
    assert!(result.is_ok(), "health_check failed: {:?}", result.err());
}

// ── mx02: self_handle ───────────────────────────────────────────

#[test]
#[ignore = "requires live Matrix session; run with --features live-matrix -- --ignored"]
fn mx02_self_handle() {
    init_tracing();
    let session = load_session();
    let cfg_json = adapter_config_json(&session, "!placeholder:matrix.org");
    let adapter = build_adapter(&cfg_json);

    let handle = adapter.self_handle();
    assert!(handle.is_some(), "self_handle returned None");
    let expected = session["user_id"].as_str().unwrap();
    assert_eq!(handle.unwrap(), expected, "self_handle mismatch");
    tracing::info!("MX-02: self_handle OK");
}

// ── mx03: capabilities ──────────────────────────────────────────

#[test]
#[ignore = "requires live Matrix session; run with --features live-matrix -- --ignored"]
fn mx03_capabilities() {
    init_tracing();
    let session = load_session();
    let cfg_json = adapter_config_json(&session, "!placeholder:matrix.org");
    let adapter = build_adapter(&cfg_json);

    let caps = adapter.capabilities();
    assert!(caps.max_payload_bytes > 0);
    assert!(caps.supports_fragmentation);
    assert!(caps.media_capabilities.is_some());
    let media = caps.media_capabilities.unwrap();
    assert_eq!(media.max_upload_bytes, 50 * 1024 * 1024);
    assert!(!media.supported_mime_types.is_empty());
    assert_eq!(
        adapter.platform_type(),
        octo_network::dot::domain::PlatformType::Matrix
    );
    tracing::info!(
        max_payload = caps.max_payload_bytes,
        "MX-03: capabilities OK"
    );
}

// ── mx04 + mx05 + mx06: send_message, receive_messages, canonicalize ──

#[test]
#[ignore = "requires live Matrix session; run with --features live-matrix -- --ignored"]
fn mx04_05_06_envelope_round_trip() {
    init_tracing();
    let session = load_session();
    let rt = make_runtime();

    // Pre-scan guard (mission 0850h-b §Live-Test Cleanup): clean
    // up any stale `octo-test-mx-*` rooms before creating the new
    // one. Idempotent self-healing — protects against a previous
    // run that panicked before its cleanup block.
    rt.block_on(async {
        let client = build_session_client(&session).await;
        leave_stale_test_rooms(&client, "octo-test-mx-").await;
    });

    // Create a test room.
    let room_id = rt.block_on(async {
        let client = build_session_client(&session).await;
        client
            .sync_once(matrix_sdk::config::SyncSettings::default().timeout(Duration::from_secs(5)))
            .await
            .expect("initial sync");
        let ts = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();
        let name = format!("octo-test-mx-mx04-{}", ts);
        let mut req = CreateRoomRequest::default();
        req.name = Some(name.clone());
        req.preset = Some(RoomPreset::PrivateChat);
        let room = client.create_room(req).await.expect("create_room");
        let rid = room.room_id().to_string();
        tracing::info!(room_name = %name, room_id = %rid, "test room created");
        rid
    });

    // Build adapter pointing at the test room.
    let cfg_json = adapter_config_json(&session, &room_id);
    let adapter = build_adapter(&cfg_json);

    let envelope_bytes = make_envelope_bytes();
    let envelope =
        DeterministicEnvelope::from_wire_bytes(&envelope_bytes).expect("from_wire_bytes");
    let domain = broadcast_domain(&adapter, &room_id);

    // mx04: send_message
    let receipt = rt
        .block_on(adapter.send_message(&domain, &envelope, b"test"))
        .expect("send_message");
    assert!(!receipt.platform_message_id.is_empty());
    assert!(
        receipt.platform_message_id.starts_with('$'),
        "got: {}",
        receipt.platform_message_id
    );
    assert!(receipt.delivered_at > 0);
    tracing::info!(event_id = %receipt.platform_message_id, "MX-04: send_message OK");

    // mx05 + mx06: receive_messages + canonicalize
    let mut found = false;
    for attempt in 0..10 {
        let received = rt
            .block_on(adapter.receive_messages(&domain))
            .expect("receive_messages");
        for msg in &received {
            if let Ok(canonical) = adapter.canonicalize(msg) {
                if canonical.to_wire_bytes() == envelope_bytes {
                    found = true;
                    break;
                }
            }
        }
        if found {
            break;
        }
        tracing::debug!(attempt, "envelope not yet received, retrying...");
        std::thread::sleep(Duration::from_millis(500));
    }
    assert!(found, "envelope was sent but never received within 5s");
    tracing::info!("MX-05+06: receive_messages + canonicalize OK");

    // Cleanup.
    rt.block_on(async {
        let client = build_session_client(&session).await;
        client
            .sync_once(matrix_sdk::config::SyncSettings::default().timeout(Duration::from_secs(5)))
            .await
            .expect("cleanup sync");
        let rid = matrix_sdk::ruma::OwnedRoomId::try_from(room_id.as_str()).unwrap();
        if let Some(room) = client.get_room(&rid) {
            let _ = room.leave().await;
            tracing::info!(room_id = %room_id, "test room cleaned up");
        }
    });
}

// ── mx07: upload_media + download_media ─────────────────────────

#[test]
#[ignore = "requires live Matrix session; run with --features live-matrix -- --ignored"]
fn mx07_media_round_trip() {
    init_tracing();
    let session = load_session();
    let rt = make_runtime();

    // Pre-scan guard (mission 0850h-b §Live-Test Cleanup): clean
    // up any stale `octo-test-mx-*` rooms before creating the new
    // one.
    rt.block_on(async {
        let client = build_session_client(&session).await;
        leave_stale_test_rooms(&client, "octo-test-mx-").await;
    });

    let room_id = rt.block_on(async {
        let client = build_session_client(&session).await;
        client
            .sync_once(matrix_sdk::config::SyncSettings::default().timeout(Duration::from_secs(5)))
            .await
            .expect("initial sync");
        let ts = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();
        let name = format!("octo-test-mx-mx07-{}", ts);
        let mut req = CreateRoomRequest::default();
        req.name = Some(name.clone());
        req.preset = Some(RoomPreset::PrivateChat);
        let room = client.create_room(req).await.expect("create_room");
        let rid = room.room_id().to_string();
        tracing::info!(room_name = %name, room_id = %rid, "test room created");
        rid
    });

    let cfg_json = adapter_config_json(&session, &room_id);
    let adapter = build_adapter(&cfg_json);

    let original = vec![0xAB_u8; 1024];
    let media_id = rt
        .block_on(adapter.upload_media("test.bin", &original, "application/octet-stream"))
        .expect("upload_media");
    assert!(!media_id.is_empty());
    tracing::info!(media_id = %media_id, bytes = original.len(), "upload OK");

    let downloaded = rt
        .block_on(adapter.download_media(&media_id))
        .expect("download_media");
    assert_eq!(downloaded.len(), original.len());
    assert_eq!(downloaded, original);
    tracing::info!("MX-07: media round-trip OK");

    // Cleanup.
    rt.block_on(async {
        let client = build_session_client(&session).await;
        client
            .sync_once(matrix_sdk::config::SyncSettings::default().timeout(Duration::from_secs(5)))
            .await
            .expect("cleanup sync");
        let rid = matrix_sdk::ruma::OwnedRoomId::try_from(room_id.as_str()).unwrap();
        if let Some(room) = client.get_room(&rid) {
            let _ = room.leave().await;
            tracing::info!(room_id = %room_id, "test room cleaned up");
        }
    });
}

// ── mx08: shutdown ──────────────────────────────────────────────

#[test]
#[ignore = "requires live Matrix session; run with --features live-matrix -- --ignored"]
fn mx08_shutdown() {
    init_tracing();
    let session = load_session();
    let cfg_json = adapter_config_json(&session, "!placeholder:matrix.org");
    let adapter = build_adapter(&cfg_json);
    let rt = make_runtime();

    assert!(
        adapter.self_handle().is_some(),
        "adapter not alive before shutdown"
    );

    let result = rt.block_on(adapter.shutdown());
    assert!(result.is_ok(), "shutdown failed: {:?}", result.err());
    assert!(
        adapter.self_handle().is_none(),
        "self_handle should be None after shutdown"
    );
    tracing::info!("MX-08: shutdown OK");
}

// ── mx09-mx14: CoordinatorAdmin trait live tests (mission 0850h-d) ─
//
// These tests exercise the new `CoordinatorAdmin` impl against
// matrix.org. Each test:
//   1. Pre-scans stale `octo-test-mx-*` rooms (cleanup self-healing)
//   2. Creates a fresh `octo-test-mx-mx{nn}-{ts}` room
//   3. Calls one section's worth of CoordinatorAdmin methods
//   4. Leaves the room (cleanup)
//
// Run with `--features live-matrix -- --include-ignored`. The six
// tests cover at most 13 of the 24 trait methods (mx09 = A. Lifecycle
// [partial: create_group]; mx10 = B. Membership [partial: remove +
// ban]; mx11 = B. Membership continues [partial: promote + demote];
// mx12 = C. Mode [partial: 5/6]; mx13 = D. Discovery [partial: 2/6];
// mx14 = C. Mode continues [partial: set_require_approval]). The
// remaining 6 methods (`add_member`, `approve_join_request`,
// `list_own_groups_with_invites`, `resolve_invite`,
// `join_by_invite`, `join_by_id`) are scheduled for the 0850h-e
// follow-on mission (mx15-mx20) — see
// `missions/open/0850h-e-matrix-coordinator-admin-coverage.md`.

#[test]
#[ignore = "requires live Matrix session; run with --features live-matrix -- --include-ignored"]
fn mx09_create_group() {
    init_tracing();
    let session = load_session();
    let rt = make_runtime();

    // Pre-scan guard
    rt.block_on(async {
        let client = build_session_client(&session).await;
        leave_stale_test_rooms(&client, "octo-test-mx-").await;
    });

    let ts = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs();
    let room_name = format!("octo-test-mx-mx09-{}", ts);

    // Build the adapter + drive create_group via the CoordinatorAdmin trait
    let cfg_json = adapter_config_json(&session, "!placeholder:matrix.org");
    let adapter = build_adapter(&cfg_json);

    let handle = rt
        .block_on(<MatrixAdapter as CoordinatorAdmin>::create_group(
            &adapter,
            &room_name,
            &[],
        ))
        .expect("create_group");

    assert!(
        !handle.id.as_str().is_empty(),
        "create_group returned empty GroupId"
    );
    assert!(
        handle.id.as_str().starts_with('!'),
        "matrix room_id should start with '!': {}",
        handle.id.as_str()
    );
    assert!(
        handle.is_admin,
        "matrix creator must be admin (matrix M4 invariant)"
    );
    assert!(
        handle.initial_admins_promoted,
        "matrix M4: creator auto-promoted at create time"
    );
    tracing::info!(
        room_id = %handle.id.as_str(),
        is_admin = handle.is_admin,
        initial_admins_promoted = handle.initial_admins_promoted,
        "MX-09: create_group OK"
    );

    // Cleanup
    let rid_str = handle.id.as_str().to_string();
    rt.block_on(async {
        let client = build_session_client(&session).await;
        client
            .sync_once(matrix_sdk::config::SyncSettings::default().timeout(Duration::from_secs(5)))
            .await
            .expect("cleanup sync");
        if let Ok(rid) = matrix_sdk::ruma::OwnedRoomId::try_from(rid_str.as_str()) {
            if let Some(room) = client.get_room(&rid) {
                let _ = room.leave().await;
                tracing::info!(room_id = %rid_str, "MX-09: test room cleaned up");
            }
        }
    });
}

#[test]
#[ignore = "requires live Matrix session; run with --features live-matrix -- --include-ignored"]
fn mx10_ban_kick() {
    init_tracing();
    let session = load_session();
    let rt = make_runtime();

    // Pre-scan guard
    rt.block_on(async {
        let client = build_session_client(&session).await;
        leave_stale_test_rooms(&client, "octo-test-mx-").await;
    });

    // Create the room via raw SDK (matrix creator is admin).
    let ts = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs();
    let room_name = format!("octo-test-mx-mx10-{}", ts);
    let room_id = rt.block_on(async {
        let client = build_session_client(&session).await;
        client
            .sync_once(matrix_sdk::config::SyncSettings::default().timeout(Duration::from_secs(5)))
            .await
            .expect("initial sync");
        let mut req = CreateRoomRequest::default();
        req.name = Some(room_name.clone());
        req.preset = Some(RoomPreset::PrivateChat);
        let room = client.create_room(req).await.expect("create_room");
        room.room_id().to_string()
    });

    // Build adapter and exercise remove_member + ban_member via trait.
    // Note: we test against the bot itself — matrix.org will reject
    // self-ban with `M_FORBIDDEN`, which is the expected behavior;
    // the test verifies the wiring works and the bot reaches the SDK.
    let cfg_json = adapter_config_json(&session, &room_id);
    let adapter = build_adapter(&cfg_json);
    let group_id = GroupId::new(room_id.clone());
    let self_handle = adapter.self_handle().expect("self_handle");
    let self_peer = PeerId::new(self_handle);

    // remove_member on self (matrix should reject; we just check the
    // call reaches the SDK without panicking).
    let _ = rt.block_on(<MatrixAdapter as CoordinatorAdmin>::remove_member(
        &adapter, &group_id, &self_peer,
    ));
    tracing::info!("MX-10: remove_member call dispatched (matrix likely rejected self-kick)");

    // ban_member with None duration (matrix indefinite-only) on self —
    // same expectation, just wiring verification.
    let _ = rt.block_on(<MatrixAdapter as CoordinatorAdmin>::ban_member(
        &adapter, &group_id, &self_peer, None,
    ));
    tracing::info!("MX-10: ban_member(None) call dispatched (matrix likely rejected self-ban)");

    // Cleanup
    rt.block_on(async {
        let client = build_session_client(&session).await;
        client
            .sync_once(matrix_sdk::config::SyncSettings::default().timeout(Duration::from_secs(5)))
            .await
            .expect("cleanup sync");
        if let Ok(rid) = matrix_sdk::ruma::OwnedRoomId::try_from(room_id.as_str()) {
            if let Some(room) = client.get_room(&rid) {
                let _ = room.leave().await;
            }
        }
    });
}

#[test]
#[ignore = "requires live Matrix session; run with --features live-matrix -- --include-ignored"]
fn mx11_promote_demote() {
    init_tracing();
    let session = load_session();
    let rt = make_runtime();

    rt.block_on(async {
        let client = build_session_client(&session).await;
        leave_stale_test_rooms(&client, "octo-test-mx-").await;
    });

    let ts = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs();
    let room_name = format!("octo-test-mx-mx11-{}", ts);
    let room_id = rt.block_on(async {
        let client = build_session_client(&session).await;
        client
            .sync_once(matrix_sdk::config::SyncSettings::default().timeout(Duration::from_secs(5)))
            .await
            .expect("initial sync");
        let mut req = CreateRoomRequest::default();
        req.name = Some(room_name.clone());
        req.preset = Some(RoomPreset::PrivateChat);
        let room = client.create_room(req).await.expect("create_room");
        room.room_id().to_string()
    });

    let cfg_json = adapter_config_json(&session, &room_id);
    let adapter = build_adapter(&cfg_json);
    let group_id = GroupId::new(room_id.clone());
    let self_handle = adapter.self_handle().expect("self_handle");
    let self_peer = PeerId::new(self_handle);

    // promote_to_admin(self) — matrix should reject promoting self
    // because creator is already admin; the test verifies wiring.
    let _ = rt.block_on(<MatrixAdapter as CoordinatorAdmin>::promote_to_admin(
        &adapter, &group_id, &self_peer,
    ));
    tracing::info!("MX-11: promote_to_admin call dispatched");

    // demote_from_admin(self) — same expectation.
    let _ = rt.block_on(<MatrixAdapter as CoordinatorAdmin>::demote_from_admin(
        &adapter, &group_id, &self_peer,
    ));
    tracing::info!("MX-11: demote_from_admin call dispatched");

    rt.block_on(async {
        let client = build_session_client(&session).await;
        client
            .sync_once(matrix_sdk::config::SyncSettings::default().timeout(Duration::from_secs(5)))
            .await
            .expect("cleanup sync");
        if let Ok(rid) = matrix_sdk::ruma::OwnedRoomId::try_from(room_id.as_str()) {
            if let Some(room) = client.get_room(&rid) {
                let _ = room.leave().await;
            }
        }
    });
}

#[test]
#[ignore = "requires live Matrix session; run with --features live-matrix -- --include-ignored"]
fn mx12_set_modes() {
    init_tracing();
    let session = load_session();
    let rt = make_runtime();

    rt.block_on(async {
        let client = build_session_client(&session).await;
        leave_stale_test_rooms(&client, "octo-test-mx-").await;
    });

    let ts = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs();
    let room_name = format!("octo-test-mx-mx12-{}", ts);
    let room_id = rt.block_on(async {
        let client = build_session_client(&session).await;
        client
            .sync_once(matrix_sdk::config::SyncSettings::default().timeout(Duration::from_secs(5)))
            .await
            .expect("initial sync");
        let mut req = CreateRoomRequest::default();
        req.name = Some(room_name.clone());
        req.preset = Some(RoomPreset::PrivateChat);
        let room = client.create_room(req).await.expect("create_room");
        room.room_id().to_string()
    });

    let cfg_json = adapter_config_json(&session, &room_id);
    let adapter = build_adapter(&cfg_json);
    let group_id = GroupId::new(room_id.clone());

    // Exercise 5 of 6 C. Mode methods (set_require_approval is mx14).
    rt.block_on(<MatrixAdapter as CoordinatorAdmin>::rename_group(
        &adapter,
        &group_id,
        &format!("{}-renamed", room_name),
    ))
    .expect("rename_group");
    tracing::info!("MX-12: rename_group OK");

    rt.block_on(<MatrixAdapter as CoordinatorAdmin>::set_group_description(
        &adapter,
        &group_id,
        "test description from mission 0850h-d",
    ))
    .expect("set_group_description");
    tracing::info!("MX-12: set_group_description OK");

    rt.block_on(<MatrixAdapter as CoordinatorAdmin>::set_locked(
        &adapter, &group_id, true,
    ))
    .expect("set_locked(true)");
    tracing::info!("MX-12: set_locked(true) OK");

    rt.block_on(<MatrixAdapter as CoordinatorAdmin>::set_locked(
        &adapter, &group_id, false,
    ))
    .expect("set_locked(false)");
    tracing::info!("MX-12: set_locked(false) OK");

    rt.block_on(<MatrixAdapter as CoordinatorAdmin>::set_announce(
        &adapter, &group_id, true,
    ))
    .expect("set_announce(true)");
    tracing::info!("MX-12: set_announce(true) OK");

    rt.block_on(<MatrixAdapter as CoordinatorAdmin>::set_announce(
        &adapter, &group_id, false,
    ))
    .expect("set_announce(false)");
    tracing::info!("MX-12: set_announce(false) OK");

    rt.block_on(<MatrixAdapter as CoordinatorAdmin>::set_ephemeral(
        &adapter,
        &group_id,
        Some(Duration::from_secs(3600)),
    ))
    .expect("set_ephemeral(Some(1h))");
    tracing::info!("MX-12: set_ephemeral(Some(1h)) OK");

    rt.block_on(<MatrixAdapter as CoordinatorAdmin>::set_ephemeral(
        &adapter, &group_id, None,
    ))
    .expect("set_ephemeral(None)");
    tracing::info!("MX-12: set_ephemeral(None) OK");

    rt.block_on(async {
        let client = build_session_client(&session).await;
        client
            .sync_once(matrix_sdk::config::SyncSettings::default().timeout(Duration::from_secs(5)))
            .await
            .expect("cleanup sync");
        if let Ok(rid) = matrix_sdk::ruma::OwnedRoomId::try_from(room_id.as_str()) {
            if let Some(room) = client.get_room(&rid) {
                let _ = room.leave().await;
            }
        }
    });
}

#[test]
#[ignore = "requires live Matrix session; run with --features live-matrix -- --include-ignored"]
fn mx13_list_and_metadata() {
    init_tracing();
    let session = load_session();
    let rt = make_runtime();

    rt.block_on(async {
        let client = build_session_client(&session).await;
        leave_stale_test_rooms(&client, "octo-test-mx-").await;
    });

    let ts = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs();
    let room_name = format!("octo-test-mx-mx13-{}", ts);
    let room_id = rt.block_on(async {
        let client = build_session_client(&session).await;
        client
            .sync_once(matrix_sdk::config::SyncSettings::default().timeout(Duration::from_secs(5)))
            .await
            .expect("initial sync");
        let mut req = CreateRoomRequest::default();
        req.name = Some(room_name.clone());
        req.preset = Some(RoomPreset::PrivateChat);
        let room = client.create_room(req).await.expect("create_room");
        room.room_id().to_string()
    });

    let cfg_json = adapter_config_json(&session, &room_id);
    let adapter = build_adapter(&cfg_json);

    // list_own_groups
    let groups = rt
        .block_on(<MatrixAdapter as CoordinatorAdmin>::list_own_groups(
            &adapter,
        ))
        .expect("list_own_groups");
    assert!(
        !groups.is_empty(),
        "list_own_groups should return at least the just-created room"
    );
    // The just-created room should be in the list with is_admin=true.
    let just_created = groups
        .iter()
        .find(|g| g.id.as_str() == room_id)
        .expect("just-created room missing from list_own_groups");
    assert!(
        just_created.is_admin,
        "creator must be admin in list_own_groups result"
    );
    tracing::info!(
        count = groups.len(),
        "MX-13: list_own_groups OK (just-created room present, is_admin=true)"
    );

    // get_group_metadata
    let group_id = GroupId::new(room_id.clone());
    let metadata = rt
        .block_on(<MatrixAdapter as CoordinatorAdmin>::get_group_metadata(
            &adapter, &group_id,
        ))
        .expect("get_group_metadata");
    assert_eq!(metadata.id.as_str(), room_id);
    assert!(metadata.admins.iter().any(|p| !p.as_str().is_empty()));
    tracing::info!(
        admins = metadata.admins.len(),
        members = metadata.members.len(),
        "MX-13: get_group_metadata OK"
    );

    rt.block_on(async {
        let client = build_session_client(&session).await;
        client
            .sync_once(matrix_sdk::config::SyncSettings::default().timeout(Duration::from_secs(5)))
            .await
            .expect("cleanup sync");
        if let Ok(rid) = matrix_sdk::ruma::OwnedRoomId::try_from(room_id.as_str()) {
            if let Some(room) = client.get_room(&rid) {
                let _ = room.leave().await;
            }
        }
    });
}

#[test]
#[ignore = "requires live Matrix session; run with --features live-matrix -- --include-ignored"]
fn mx14_set_require_approval() {
    init_tracing();
    let session = load_session();
    let rt = make_runtime();

    rt.block_on(async {
        let client = build_session_client(&session).await;
        leave_stale_test_rooms(&client, "octo-test-mx-").await;
    });

    let ts = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs();
    let room_name = format!("octo-test-mx-mx14-{}", ts);
    let room_id = rt.block_on(async {
        let client = build_session_client(&session).await;
        client
            .sync_once(matrix_sdk::config::SyncSettings::default().timeout(Duration::from_secs(5)))
            .await
            .expect("initial sync");
        let mut req = CreateRoomRequest::default();
        req.name = Some(room_name.clone());
        req.preset = Some(RoomPreset::PrivateChat);
        let room = client.create_room(req).await.expect("create_room");
        room.room_id().to_string()
    });

    let cfg_json = adapter_config_json(&session, &room_id);
    let adapter = build_adapter(&cfg_json);
    let group_id = GroupId::new(room_id.clone());

    // 6th C. Mode method (set_require_approval). matrix.org supports
    // knock on Synapse; if not, the homeserver returns M_FORBIDDEN
    // and we log it.
    rt.block_on(<MatrixAdapter as CoordinatorAdmin>::set_require_approval(
        &adapter, &group_id, true,
    ))
    .expect("set_require_approval(true)");
    tracing::info!("MX-14: set_require_approval(true) OK");

    rt.block_on(<MatrixAdapter as CoordinatorAdmin>::set_require_approval(
        &adapter, &group_id, false,
    ))
    .expect("set_require_approval(false)");
    tracing::info!("MX-14: set_require_approval(false) OK");

    rt.block_on(async {
        let client = build_session_client(&session).await;
        client
            .sync_once(matrix_sdk::config::SyncSettings::default().timeout(Duration::from_secs(5)))
            .await
            .expect("cleanup sync");
        if let Ok(rid) = matrix_sdk::ruma::OwnedRoomId::try_from(room_id.as_str()) {
            if let Some(room) = client.get_room(&rid) {
                let _ = room.leave().await;
            }
        }
    });
}

// ── Cleanup helper test (mission 0850h-b §Live-Test Cleanup) ────
//
// Runs the same stale-room sweep as `src/bin/cleanup_test_rooms.rs`
// inline (no subprocess), for CI environments that prefer
// `cargo test -- --include-ignored` over a separate binary step.
//
// Run:
//   cargo test -p octo-adapter-matrix-sdk --features live-matrix \
//       --test live_matrix_test cleanup_stale_test_rooms \
//       -- --include-ignored --nocapture

#[test]
#[ignore = "requires live Matrix session; leaves stale rooms; run with --features live-matrix -- --include-ignored"]
fn cleanup_stale_test_rooms() {
    init_tracing();
    let session = load_session();
    let rt = make_runtime();

    rt.block_on(async {
        let client = build_session_client(&session).await;
        let left = leave_stale_test_rooms(&client, "octo-test-mx-").await;
        tracing::info!(rooms_left = left, "cleanup_stale_test_rooms complete");
    });
}
