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
use octo_adapter_matrix_sdk::{MatrixAdapter, MatrixConfig};
use octo_network::dot::adapters::PlatformAdapter;
use octo_network::dot::domain::BroadcastDomainId;
use octo_network::dot::envelope::DeterministicEnvelope;
use std::path::PathBuf;
use std::time::Duration;

/// Load the session from `~/.config/octo/matrix.json`.
fn load_session() -> serde_json::Value {
    let path = config_path();
    let bytes = std::fs::read(&path).unwrap_or_else(|e| panic!("read {}: {}", path.display(), e));
    serde_json::from_slice(&bytes).unwrap_or_else(|e| panic!("parse {}: {}", path.display(), e))
}

fn config_path() -> PathBuf {
    dirs::config_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("octo")
        .join("matrix.json")
}

/// Build a raw matrix-sdk Client from the session JSON for room
/// setup/teardown (the MatrixAdapter embeds its own runtime, so we
/// use a separate client for admin operations).
async fn build_session_client(session: &serde_json::Value) -> Client {
    use matrix_sdk::authentication::matrix::MatrixSession;
    use matrix_sdk::ruma::{OwnedDeviceId, OwnedUserId};
    use matrix_sdk::{SessionMeta, SessionTokens};

    let homeserver = session["homeserver_url"].as_str().expect("homeserver_url");
    let user_id_str = session["user_id"].as_str().expect("user_id");
    let device_id_str = session["device_id"].as_str().expect("device_id");
    let access_token = session["access_token"].as_str().expect("access_token");
    let refresh_token = session["refresh_token"].as_str().map(|s| s.to_string());

    let user_id = OwnedUserId::try_from(user_id_str).expect("valid user_id");
    let device_id = OwnedDeviceId::from(device_id_str);

    let client = Client::builder()
        .homeserver_url(homeserver)
        .build()
        .await
        .expect("build session client");

    client
        .restore_session(MatrixSession {
            meta: SessionMeta { user_id, device_id },
            tokens: SessionTokens {
                access_token: access_token.to_string(),
                refresh_token,
            },
        })
        .await
        .expect("restore_session");

    client
}

/// Create a test room, run the test closure, then leave the room.
/// Returns the room ID used.
async fn with_test_room<F, Fut>(test_name: &str, f: F) -> String
where
    F: FnOnce(String) -> Fut,
    Fut: std::future::Future<Output = ()>,
{
    let session = load_session();
    let client = build_session_client(&session).await;

    // Sync so the client sees its joined rooms.
    client
        .sync_once(matrix_sdk::config::SyncSettings::default().timeout(Duration::from_secs(5)))
        .await
        .expect("initial sync");

    let timestamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs();

    let room_name = format!("octo-test-mx-{}-{}", test_name, timestamp);
    let mut request = CreateRoomRequest::default();
    request.name = Some(room_name.clone());
    request.preset = Some(RoomPreset::PrivateChat);

    let room = client
        .create_room(request)
        .await
        .unwrap_or_else(|e| panic!("create_room '{}': {}", room_name, e));

    let room_id = room.room_id().to_string();
    tracing::info!(room_name = %room_name, room_id = %room_id, "test room created");

    // Run the test.
    f(room_id.clone()).await;

    // Cleanup: leave the room.
    tracing::info!(room_id = %room_id, "cleaning up test room");
    let _ = room.leave().await;

    room_id
}

/// Build a MatrixConfig pointing at the test room.
fn adapter_config_for_room(session: &serde_json::Value, room_id: &str) -> MatrixConfig {
    MatrixConfig {
        homeserver_url: session["homeserver_url"].as_str().unwrap().to_string(),
        user_id: session["user_id"].as_str().unwrap().to_string(),
        device_id: session["device_id"].as_str().unwrap().to_string(),
        access_token: session["access_token"].as_str().unwrap().to_string(),
        refresh_token: session["refresh_token"].as_str().map(|s| s.to_string()),
        passphrase: None,
        config_path: PathBuf::new(),
        force_writeback: false,
        use_session_store: false,
        session_store_path: PathBuf::new(),
        rooms: vec![room_id.to_string()],
    }
}

/// Build a 282-byte deterministic envelope (same format as the
/// integration test).
fn make_envelope_bytes() -> Vec<u8> {
    let mut wire = Vec::with_capacity(282);
    wire.extend_from_slice(&1u16.to_be_bytes()); // version
    wire.extend_from_slice(&0xDEAD_BEEFu32.to_be_bytes()); // network_id
    wire.extend_from_slice(&1u16.to_be_bytes()); // message_type
    wire.extend_from_slice(&[0u8; 32]); // envelope_id
    wire.extend_from_slice(&[0u8; 32]); // mission_id
    wire.extend_from_slice(&[0u8; 32]); // source_peer
    wire.extend_from_slice(&[0u8; 32]); // origin_gateway
    wire.extend_from_slice(&0u64.to_be_bytes()); // logical_timestamp
    wire.extend_from_slice(&1u16.to_be_bytes()); // ttl_hops
    wire.extend_from_slice(&[0u8; 32]); // payload_hash
    wire.extend_from_slice(&[0u8; 32]); // route_trace_root
    wire.extend_from_slice(&0u64.to_be_bytes()); // flags
    debug_assert_eq!(wire.len(), 218);
    wire.extend_from_slice(&[0u8; 64]); // signature
    debug_assert_eq!(wire.len(), 282);
    wire
}

fn broadcast_domain(adapter: &MatrixAdapter, room_id: &str) -> BroadcastDomainId {
    adapter.domain_id(room_id)
}

fn init_tracing() {
    let _ = tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .try_init();
}

// ── mx01: health_check ──────────────────────────────────────────

#[tokio::test]
#[ignore = "requires live Matrix session; run with --features live-matrix -- --ignored"]
async fn mx01_health_check() {
    init_tracing();
    let session = load_session();
    let room_id = "!placeholder:matrix.org"; // not used for health_check
    let cfg = adapter_config_for_room(&session, room_id);
    let cfg_json = serde_json::to_vec(&cfg).expect("serialize config");
    let adapter = MatrixAdapter::from_config_bytes(&cfg_json).expect("adapter construction");

    let result = adapter.health_check().await;
    assert!(result.is_ok(), "health_check failed: {:?}", result.err());
    tracing::info!("MX-01: health_check OK");
}

// ── mx02: self_handle ───────────────────────────────────────────

#[tokio::test]
#[ignore = "requires live Matrix session; run with --features live-matrix -- --ignored"]
async fn mx02_self_handle() {
    init_tracing();
    let session = load_session();
    let room_id = "!placeholder:matrix.org";
    let cfg = adapter_config_for_room(&session, room_id);
    let cfg_json = serde_json::to_vec(&cfg).expect("serialize config");
    let adapter = MatrixAdapter::from_config_bytes(&cfg_json).expect("adapter construction");

    let handle = adapter.self_handle();
    assert!(handle.is_some(), "self_handle returned None");
    let handle = handle.unwrap();
    let expected = session["user_id"].as_str().unwrap();
    assert_eq!(handle, expected, "self_handle mismatch");
    tracing::info!(handle = %handle, "MX-02: self_handle OK");
}

// ── mx03: capabilities ──────────────────────────────────────────

#[tokio::test]
#[ignore = "requires live Matrix session; run with --features live-matrix -- --ignored"]
async fn mx03_capabilities() {
    init_tracing();
    let session = load_session();
    let room_id = "!placeholder:matrix.org";
    let cfg = adapter_config_for_room(&session, room_id);
    let cfg_json = serde_json::to_vec(&cfg).expect("serialize config");
    let adapter = MatrixAdapter::from_config_bytes(&cfg_json).expect("adapter construction");

    let caps = adapter.capabilities();
    assert!(caps.max_payload_bytes > 0, "max_payload_bytes is 0");
    assert!(caps.supports_fragmentation, "must support fragmentation");
    assert!(
        caps.media_capabilities.is_some(),
        "media_capabilities missing"
    );
    let media = caps.media_capabilities.unwrap();
    assert_eq!(
        media.max_upload_bytes,
        50 * 1024 * 1024,
        "max_upload_bytes != 50MiB"
    );
    assert!(
        !media.supported_mime_types.is_empty(),
        "no supported MIME types"
    );
    assert_eq!(
        adapter.platform_type(),
        octo_network::dot::domain::PlatformType::Matrix
    );
    tracing::info!(
        max_payload = caps.max_payload_bytes,
        "MX-03: capabilities OK"
    );
}

// ── mx04 + mx05 + mx06: send_envelope, receive_messages, canonicalize ──

#[tokio::test]
#[ignore = "requires live Matrix session; run with --features live-matrix -- --ignored"]
async fn mx04_05_06_envelope_round_trip() {
    init_tracing();
    let session = load_session();

    with_test_room("mx04", |room_id| {
        let session = session.clone();
        async move {
            let cfg = adapter_config_for_room(&session, &room_id);
            let cfg_json = serde_json::to_vec(&cfg).expect("serialize config");
            let adapter =
                MatrixAdapter::from_config_bytes(&cfg_json).expect("adapter construction");

            let envelope_bytes = make_envelope_bytes();
            let envelope =
                DeterministicEnvelope::from_wire_bytes(&envelope_bytes).expect("from_wire_bytes");
            let domain = broadcast_domain(&adapter, &room_id);

            // mx04: send_envelope
            let receipt = adapter
                .send_envelope(&domain, &envelope)
                .await
                .expect("send_envelope");
            assert!(
                !receipt.platform_message_id.is_empty(),
                "platform_message_id is empty"
            );
            assert!(
                receipt.platform_message_id.starts_with('$'),
                "expected Matrix event id ($-prefix), got: {}",
                receipt.platform_message_id
            );
            assert!(receipt.delivered_at > 0, "delivered_at should be > 0");
            tracing::info!(
                event_id = %receipt.platform_message_id,
                "MX-04: send_envelope OK"
            );

            // mx05 + mx06: receive_messages + canonicalize
            let mut found = false;
            for attempt in 0..10 {
                let received = adapter
                    .receive_messages(&domain)
                    .await
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
                tokio::time::sleep(Duration::from_millis(500)).await;
            }
            assert!(found, "envelope was sent but never received within 5s");
            tracing::info!("MX-05+06: receive_messages + canonicalize OK");
        }
    })
    .await;
}

// ── mx07: upload_media + download_media ─────────────────────────

#[tokio::test]
#[ignore = "requires live Matrix session; run with --features live-matrix -- --ignored"]
async fn mx07_media_round_trip() {
    init_tracing();
    let session = load_session();

    with_test_room("mx07", |room_id| {
        let session = session.clone();
        async move {
            let cfg = adapter_config_for_room(&session, &room_id);
            let cfg_json = serde_json::to_vec(&cfg).expect("serialize config");
            let adapter =
                MatrixAdapter::from_config_bytes(&cfg_json).expect("adapter construction");

            // Upload a small payload.
            let original = vec![0xAB_u8; 1024];
            let media_id = adapter
                .upload_media("test.bin", &original, "application/octet-stream")
                .await
                .expect("upload_media");
            assert!(!media_id.is_empty(), "upload_media returned empty media_id");
            tracing::info!(media_id = %media_id, bytes = original.len(), "upload OK");

            // Download and verify.
            let downloaded = adapter
                .download_media(&media_id)
                .await
                .expect("download_media");
            assert_eq!(
                downloaded.len(),
                original.len(),
                "downloaded length mismatch"
            );
            assert_eq!(downloaded, original, "downloaded bytes mismatch");
            tracing::info!("MX-07: media round-trip OK");
        }
    })
    .await;
}

// ── mx08: shutdown ──────────────────────────────────────────────

#[tokio::test]
#[ignore = "requires live Matrix session; run with --features live-matrix -- --ignored"]
async fn mx08_shutdown() {
    init_tracing();
    let session = load_session();
    let room_id = "!placeholder:matrix.org";
    let cfg = adapter_config_for_room(&session, room_id);
    let cfg_json = serde_json::to_vec(&cfg).expect("serialize config");
    let adapter = MatrixAdapter::from_config_bytes(&cfg_json).expect("adapter construction");

    // Verify adapter is alive first.
    assert!(
        adapter.self_handle().is_some(),
        "adapter not alive before shutdown"
    );

    let result = adapter.shutdown().await;
    assert!(result.is_ok(), "shutdown failed: {:?}", result.err());

    // After shutdown, self_handle should return None.
    assert!(
        adapter.self_handle().is_none(),
        "self_handle should be None after shutdown"
    );
    tracing::info!("MX-08: shutdown OK");
}
