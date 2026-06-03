//! Integration test against a real Matrix homeserver.
//!
//! Mission 0850h-a §Acceptance Criteria:
//! - `Integration test in
//!   `crates/octo-adapter-matrix-sdk/tests/integration_matrix.rs`,
//!   feature-gated `integration-matrix`
//! - `scripts/integration-matrix.sh up|down` driver
//! - Integration test asserts: whoami, room-join, envelope round-trip
//!
//! Driver: `scripts/integration-matrix.sh up --homeserver {synapse|conduit}`
//! starts a Synapse (default) or Conduit container with
//! password-only registration and no rate limits. The test then:
//! 1. Logs in with the test credentials via
//!    `Client::builder().matrix_auth().login_username(...)` (the
//!    CLI's `octo-matrix-onboard login` subcommand wraps the same
//!    SDK API; the integration test inlines it to avoid spawning a
//!    subprocess for the auth flow).
//! 2. Loads the on-disk config, calls /whoami via the SDK, asserts
//!    user matches.
//! 3. Joins the test room, drives the adapter through
//!    `send_envelope` → `receive_messages`, asserts the envelope
//!    round-trips end-to-end through the homeserver.
//!
//! Run: `cargo test -p octo-adapter-matrix-sdk --features integration-matrix
//! --test integration_matrix -- --nocapture`.

#![cfg(feature = "integration-matrix")]

use matrix_sdk::Client;
use octo_matrix_onboard_core::session;
use octo_network::dot::adapters::PlatformAdapter;
use octo_network::dot::domain::BroadcastDomainId;
use octo_network::dot::envelope::DeterministicEnvelope;
use std::time::Duration;

const DEFAULT_HOMESERVER: &str = "http://localhost:8008";
const DEFAULT_USER: &str = "@ci:localhost";
const DEFAULT_PASSWORD: &str = "ci-password";
const DEFAULT_ROOM: &str = "!integration-test:localhost";

fn homeserver() -> String {
    std::env::var("INTEGRATION_HOMESERVER").unwrap_or_else(|_| DEFAULT_HOMESERVER.to_string())
}

fn user() -> String {
    std::env::var("INTEGRATION_USER").unwrap_or_else(|_| DEFAULT_USER.to_string())
}

fn password() -> String {
    std::env::var("INTEGRATION_PASSWORD").unwrap_or_else(|_| DEFAULT_PASSWORD.to_string())
}

fn room_id() -> String {
    std::env::var("INTEGRATION_ROOM").unwrap_or_else(|_| DEFAULT_ROOM.to_string())
}

async fn login_and_save_config() -> octo_matrix_onboard_core::Session {
    let client = Client::builder()
        .homeserver_url(homeserver())
        .build()
        .await
        .expect("build client against integration homeserver");

    client
        .matrix_auth()
        .login_username(user(), &password())
        .request_refresh_token()
        .send()
        .await
        .expect("login_username against integration homeserver");

    session::extract(&client, &homeserver()).expect("extract session after login")
}

/// Build a logged-in client and ensure the test user is joined to
/// the test room. Idempotent: joining an already-joined room is a
/// no-op on most homeservers.
async fn login_and_join() -> (octo_matrix_onboard_core::Session, Client) {
    let sess = login_and_save_config().await;
    let user_id =
        matrix_sdk::ruma::OwnedUserId::try_from(sess.user_id.as_str()).expect("user_id is valid");
    let device_id = matrix_sdk::ruma::OwnedDeviceId::from(sess.device_id.as_str());
    let session = matrix_sdk::authentication::matrix::MatrixSession {
        meta: matrix_sdk::SessionMeta { user_id, device_id },
        tokens: matrix_sdk::SessionTokens {
            access_token: sess.access_token().to_string(),
            refresh_token: sess.refresh_token.clone(),
        },
    };
    let client = Client::builder()
        .homeserver_url(&sess.homeserver_url)
        .build()
        .await
        .expect("rebuild client for join");
    client
        .restore_session(session)
        .await
        .expect("restore_session for join");
    // First sync to populate the room state, then join.
    client
        .sync_once(matrix_sdk::config::SyncSettings::default().timeout(Duration::from_secs(5)))
        .await
        .expect("initial sync");
    let room_id_typed =
        matrix_sdk::ruma::OwnedRoomId::try_from(room_id().as_str()).expect("room_id is valid");
    // Idempotent: ignore AlreadyJoined errors.
    let _ = client.join_room_by_id(&room_id_typed).await;
    (sess, client)
}

#[tokio::test]
async fn integration_login_and_whoami() {
    let sess = login_and_save_config().await;
    assert_eq!(sess.homeserver_url, homeserver());
    assert!(!sess.access_token().is_empty());
    assert!(!sess.user_id.is_empty());
    assert!(!sess.device_id.is_empty());

    // whoami sanity-check: re-build a client and call /whoami.
    let user_id =
        matrix_sdk::ruma::OwnedUserId::try_from(sess.user_id.as_str()).expect("user_id is valid");
    let device_id = matrix_sdk::ruma::OwnedDeviceId::from(sess.device_id.as_str());

    let session = matrix_sdk::authentication::matrix::MatrixSession {
        meta: matrix_sdk::SessionMeta {
            user_id: user_id.clone(),
            device_id,
        },
        tokens: matrix_sdk::SessionTokens {
            access_token: sess.access_token().to_string(),
            refresh_token: sess.refresh_token.clone(),
        },
    };

    let client = Client::builder()
        .homeserver_url(&sess.homeserver_url)
        .build()
        .await
        .expect("rebuild client for whoami");
    client
        .restore_session(session)
        .await
        .expect("restore_session in whoami check");

    let who = client
        .whoami()
        .await
        .expect("whoami against integration homeserver");
    assert_eq!(who.user_id, user_id);
}

#[tokio::test]
async fn integration_envelope_round_trip() {
    // R1-H6: the test now actually round-trips an envelope through
    // the homeserver (send_envelope → server → receive_messages).
    // The earlier version only round-tripped base64 in-process and
    // never touched `send_envelope` or `receive_messages`.
    let (sess, _client) = login_and_join().await;

    let cfg = octo_adapter_matrix_sdk::MatrixConfig {
        homeserver_url: sess.homeserver_url.clone(),
        user_id: sess.user_id.clone(),
        device_id: sess.device_id.clone(),
        access_token: sess.access_token().to_string(),
        refresh_token: sess.refresh_token.clone(),
        passphrase: None,
        // Mission 0850h-c: empty config_path disables on-disk writeback
        // for the integration test (the test only round-trips in-memory;
        // it does not exercise the 401 + refresh + persist path).
        config_path: std::path::PathBuf::new(),
        force_writeback: false,
        // Mission 0850h-d: `use_session_store: false` keeps the
        // integration test on the in-config path (the test doesn't
        // persist the session to the multi-account store; that's a
        // separate integration covered by octo-matrix-session-store's
        // own tests).
        use_session_store: false,
        session_store_path: std::path::PathBuf::new(),
        rooms: vec![room_id()],
    };
    let cfg_json = serde_json::to_vec(&cfg).expect("serialize config");
    let adapter = octo_adapter_matrix_sdk::MatrixAdapter::from_config_bytes(&cfg_json)
        .expect("adapter construction from config");

    // Capability report sanity check.
    let caps = adapter.capabilities();
    assert!(caps.max_payload_bytes > 0);
    assert_eq!(
        octo_adapter_matrix_sdk::MatrixAdapter::PLATFORM_TYPE,
        0x0003
    );

    // Build a deterministic envelope. `DeterministicEnvelope` is the
    // real type the adapter accepts. For an integration round-trip,
    // we build a wire-format byte string and let `from_wire_bytes`
    // validate the structure. The signature field is zero-filled —
    // the adapter does not verify it (it round-trips the bytes
    // verbatim, so any 282-byte payload works for the homeserver
    // leg of the test). Real signature verification is exercised
    // by the unit tests in `octo-network`.
    let wire = b"DOT/1 integration round-trip payload";
    let mut envelope_bytes = Vec::with_capacity(282);
    // Signing bytes (218 bytes):
    envelope_bytes.extend_from_slice(&1u16.to_be_bytes()); // version
    envelope_bytes.extend_from_slice(&0xDEAD_BEEFu32.to_be_bytes()); // network_id
    envelope_bytes.extend_from_slice(&1u16.to_be_bytes()); // message_type
    envelope_bytes.extend_from_slice(&[0u8; 32]); // envelope_id
    envelope_bytes.extend_from_slice(&[0u8; 32]); // mission_id
    envelope_bytes.extend_from_slice(&[0u8; 32]); // source_peer
    envelope_bytes.extend_from_slice(&[0u8; 32]); // origin_gateway
    envelope_bytes.extend_from_slice(&0u64.to_be_bytes()); // logical_timestamp
    envelope_bytes.extend_from_slice(&1u16.to_be_bytes()); // ttl_hops
    envelope_bytes.extend_from_slice(&[0u8; 32]); // payload_hash
    envelope_bytes.extend_from_slice(&[0u8; 32]); // route_trace_root
    envelope_bytes.extend_from_slice(&0u64.to_be_bytes()); // flags
    debug_assert_eq!(envelope_bytes.len(), 218);
    // Signature (64 bytes):
    envelope_bytes.extend_from_slice(&[0u8; 64]);
    debug_assert_eq!(envelope_bytes.len(), 282);

    let envelope = DeterministicEnvelope::from_wire_bytes(&envelope_bytes)
        .expect("from_wire_bytes accepts the constructed bytes");
    let domain = broadcast_domain_for(&adapter, &room_id());

    // SEND: the receipt MUST have a non-empty, non-synthesised
    // platform_message_id (R1-H5). The SDK's `sent.event_id` is the
    // authoritative ID.
    let receipt = adapter
        .send_envelope(&domain, &envelope)
        .await
        .expect("send_envelope must succeed against joined test room");
    assert!(
        !receipt.platform_message_id.is_empty(),
        "receipt platform_message_id is empty"
    );
    assert!(
        receipt.platform_message_id.starts_with('$'),
        "receipt platform_message_id should be a Matrix event id ($-prefixed), got: {}",
        receipt.platform_message_id
    );
    assert!(
        receipt.delivered_at > 0,
        "delivered_at should be a unix epoch"
    );

    // RECEIVE: the loop should pick up the event we just sent.
    // `receive_messages` is synchronous in spirit (driven by an
    // internal sync token), so we retry with a short backoff.
    let mut found = false;
    for _ in 0..6 {
        let received = adapter
            .receive_messages(&domain)
            .await
            .expect("receive_messages must not error");
        for msg in &received {
            let canonical = adapter
                .canonicalize(msg)
                .expect("canonicalize our own sent envelope");
            if canonical.to_wire_bytes() == wire {
                found = true;
                break;
            }
        }
        if found {
            break;
        }
        tokio::time::sleep(Duration::from_millis(500)).await;
    }
    assert!(
        found,
        "envelope was sent but never received within 3s of polling"
    );
}

/// Compute the `BroadcastDomainId` for a Matrix room using the
/// adapter's `domain_id` helper. The helper applies the
/// platform-type prefix to the BLAKE3-256 hash (RFC-0850 §3.1), so
/// we route through the adapter rather than recomputing the hash
/// inline (the prefix could change in a future spec revision).
fn broadcast_domain_for(
    adapter: &octo_adapter_matrix_sdk::MatrixAdapter,
    room_id: &str,
) -> BroadcastDomainId {
    use octo_network::dot::adapters::PlatformAdapter;
    adapter.domain_id(room_id)
}

#[tokio::test]
async fn integration_persist_session_to_disk_writes_rotated_pair() {
    // R1-H1: end-to-end check that the adapter's
    // `persist_session_to_disk` writes the rotated pair to the
    // on-disk config when the SDK's in-memory tokens differ from
    // the pre-start snapshot. We simulate the "rotated pair" by
    // mutating the adapter's in-memory `access_token` (the test
    // does not need to actually trigger a 401 + refresh against
    // the homeserver; the writeback path is what we're testing).
    let (sess, _client) = login_and_join().await;

    let dir = tempfile::TempDir::new().expect("tempdir");
    let cfg_path = dir.path().join("config.json");

    // Pre-seed the on-disk config with the initial session tokens.
    let on_disk = serde_json::json!({
        "homeserver_url": sess.homeserver_url,
        "user_id": sess.user_id,
        "device_id": sess.device_id,
        "access_token": sess.access_token(),
        "refresh_token": sess.refresh_token,
        "rooms": [room_id()],
    });
    std::fs::write(&cfg_path, serde_json::to_vec_pretty(&on_disk).unwrap())
        .expect("seed on-disk config");

    let cfg = octo_adapter_matrix_sdk::MatrixConfig {
        homeserver_url: sess.homeserver_url.clone(),
        user_id: sess.user_id.clone(),
        device_id: sess.device_id.clone(),
        access_token: sess.access_token().to_string(),
        refresh_token: sess.refresh_token.clone(),
        passphrase: None,
        config_path: cfg_path.clone(),
        force_writeback: false,
        use_session_store: false,
        session_store_path: std::path::PathBuf::new(),
        rooms: vec![room_id()],
    };
    let cfg_json = serde_json::to_vec(&cfg).expect("serialize config");
    let adapter = octo_adapter_matrix_sdk::MatrixAdapter::from_config_bytes(&cfg_json)
        .expect("adapter construction from config");

    // First call: no rotation yet → no-op (the SDK's session
    // tokens match the on-disk file).
    let outcome1 = adapter
        .persist_session_to_disk()
        .expect("persist_session_to_disk first call");
    assert!(
        !outcome1.written,
        "expected no-op when tokens match the on-disk file"
    );

    // Simulate a rotation by mutating the in-memory access token
    // (the real flow is: SDK sees 401 → refresh → in-memory
    // session_tokens() returns the new pair; we test the
    // writeback directly).
    //
    // We can't mutate the SDK's internal session_tokens from
    // outside, but we can build a *new* adapter whose in-memory
    // session is rotated. The writeback protocol is the same.
    let rotated = octo_adapter_matrix_sdk::MatrixConfig {
        access_token: "syt_rotated_after_401".to_string(),
        refresh_token: Some("syr_rotated_after_401".to_string()),
        ..cfg.clone()
    };
    let cfg_json_rotated = serde_json::to_vec(&rotated).expect("serialize rotated config");
    let adapter_rotated =
        octo_adapter_matrix_sdk::MatrixAdapter::from_config_bytes(&cfg_json_rotated)
            .expect("adapter construction from rotated config");

    // The new adapter's session tokens are the rotated pair. The
    // on-disk file still has the old pair. Calling persist should
    // write the rotated pair to disk.
    let outcome2 = adapter_rotated
        .persist_session_to_disk()
        .expect("persist_session_to_disk after rotation");
    assert!(
        outcome2.written,
        "expected write when tokens differ from on-disk file"
    );

    let after: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&cfg_path).unwrap()).unwrap();
    assert_eq!(
        after["access_token"], "syt_rotated_after_401",
        "on-disk access_token should be the rotated value"
    );
    assert_eq!(
        after["refresh_token"], "syr_rotated_after_401",
        "on-disk refresh_token should be the rotated value"
    );
}

/// R1-M19: encrypted-room round-trip test (mission 0850h-b §Acceptance).
///
/// Sets up an encrypted room between the two CI users (`ci` and
/// `ci2`), has `ci` send a deterministic envelope via the adapter,
/// and asserts `ci2`'s adapter receives the message with the
/// plaintext bytes intact (the SDK handles the
/// encrypt-on-send / decrypt-on-receive leg automatically).
///
/// The script (`scripts/integration-matrix.sh up`) provisions
/// both users with the same password; the test only needs distinct
/// MXIDs, not distinct credentials.
#[tokio::test]
async fn integration_encrypted_room_round_trip() {
    use matrix_sdk::ruma::events::room::encryption::RoomEncryptionEventContent;
    use matrix_sdk::ruma::EventEncryptionAlgorithm;

    // --- 1. Login both users ---
    let sess1 = login_and_save_config().await;
    let sess2 = login_user("ci2", "ci2-password-env-override-not-needed", None).await;

    // --- 2. Rebuild two clients + a 2nd MatrixAdapter ---
    let (client1, user_id1, device_id1) = rebuild_client(&sess1).await;
    let (client2, _user_id2, _device_id2) = rebuild_client(&sess2).await;

    // --- 3. Bootstrap cross-signing on both (R1-M19 acceptance) ---
    // Each user signs their own device. Without this, the SDK
    // would refuse to establish the Olm sessions needed to share
    // Megolm keys in the encrypted room.
    client1
        .encryption()
        .bootstrap_cross_signing(None)
        .await
        .expect("ci bootstrap_cross_signing");
    client2
        .encryption()
        .bootstrap_cross_signing(None)
        .await
        .expect("ci2 bootstrap_cross_signing");

    // --- 4. Create an encrypted room, invite ci2, ci2 joins ---
    client1
        .sync_once(matrix_sdk::config::SyncSettings::default().timeout(Duration::from_secs(5)))
        .await
        .expect("ci1 initial sync");
    let room = client1
        .create_room(matrix_sdk::ruma::api::client::room::create_room::v3::Request::default())
        .await
        .expect("ci1 create room");
    let room_id_typed = room.room_id().to_owned();

    // Set the m.room.encryption state event. The SDK will then
    // auto-encrypt every subsequent message we send in this room.
    let encryption_content: RoomEncryptionEventContent =
        RoomEncryptionEventContent::new(EventEncryptionAlgorithm::MegolmV1AesSha2);
    room.send_state_event(encryption_content)
        .await
        .expect("set m.room.encryption");

    // Invite ci2 and have ci2 accept.
    let user_id2_typed = matrix_sdk::ruma::OwnedUserId::try_from(sess2.user_id.as_str())
        .expect("ci2 user_id is valid");
    room.invite_user_by_id(&user_id2_typed)
        .await
        .expect("ci1 invite ci2");

    // ci2 must sync to see the invite.
    client2
        .sync_once(matrix_sdk::config::SyncSettings::default().timeout(Duration::from_secs(5)))
        .await
        .expect("ci2 sync to see invite");
    let room2 = client2
        .join_room_by_id(&room_id_typed)
        .await
        .expect("ci2 join room");

    // --- 5. Build the adapter and send an envelope from ci1 ---
    // R2-H1: set a real on-disk `config_path` so the
    // `MatrixAdapter::new` gate at lib.rs:267 (passphrase is Some
    // AND config_path is non-empty) actually wires the
    // `sqlite_store`. The previous test left `config_path` empty
    // and the SDK fell back to its in-memory crypto store, which
    // defeated the R1-H8 acceptance that the encrypted-room
    // payload exercises the on-disk store wiring. Use a
    // `tempfile::TempDir` (dev-dep) so the store file is cleaned
    // up at end of test.
    let ci1_store_dir = tempfile::TempDir::new().expect("create ci1 tempdir");
    let ci1_config_path = ci1_store_dir.path().join("ci1-matrix.json");
    let ci2_store_dir = tempfile::TempDir::new().expect("create ci2 tempdir");
    let ci2_config_path = ci2_store_dir.path().join("ci2-matrix.json");
    let cfg1 = octo_adapter_matrix_sdk::MatrixConfig {
        homeserver_url: sess1.homeserver_url.clone(),
        user_id: user_id1.to_string(),
        device_id: device_id1.to_string(),
        access_token: sess1.access_token().to_string(),
        refresh_token: sess1.refresh_token.clone(),
        // Encrypted-room test: passphrase + non-empty config_path
        // so the SDK's `sqlite_store` is wired (R1-H8, R2-H1).
        // The store file path is derived by the adapter as
        // `<dir>/<stem>.store.sqlite`.
        passphrase: Some("ci-test-passphrase".to_string()),
        config_path: ci1_config_path.clone(),
        force_writeback: false,
        use_session_store: false,
        session_store_path: std::path::PathBuf::new(),
        rooms: vec![room_id_typed.to_string()],
    };
    let cfg1_json = serde_json::to_vec(&cfg1).expect("serialize cfg1");
    let adapter1 = octo_adapter_matrix_sdk::MatrixAdapter::from_config_bytes(&cfg1_json)
        .expect("adapter1 construction");

    // Wire the adapter for ci2 (its own adapter instance, listening
    // for incoming events on the same room). R2-H1: ci2 needs its
    // OWN on-disk store path so the second `sqlite_store` is opened
    // against a different file.
    let cfg2 = octo_adapter_matrix_sdk::MatrixConfig {
        access_token: sess2.access_token().to_string(),
        refresh_token: sess2.refresh_token.clone(),
        passphrase: Some("ci2-test-passphrase".to_string()),
        config_path: ci2_config_path.clone(),
        ..cfg1.clone()
    };
    let cfg2_json = serde_json::to_vec(&cfg2).expect("serialize cfg2");
    let adapter2 = octo_adapter_matrix_sdk::MatrixAdapter::from_config_bytes(&cfg2_json)
        .expect("adapter2 construction");

    // Build the wire bytes (same as integration_envelope_round_trip).
    let wire = b"DOT/1 e2ee round-trip payload";
    let mut envelope_bytes = Vec::with_capacity(282);
    envelope_bytes.extend_from_slice(&1u16.to_be_bytes()); // version
    envelope_bytes.extend_from_slice(&0xDEAD_BEEFu32.to_be_bytes()); // network_id
    envelope_bytes.extend_from_slice(&1u16.to_be_bytes()); // message_type
    envelope_bytes.extend_from_slice(&[0u8; 32]); // envelope_id
    envelope_bytes.extend_from_slice(&[0u8; 32]); // mission_id
    envelope_bytes.extend_from_slice(&[0u8; 32]); // source_peer
    envelope_bytes.extend_from_slice(&[0u8; 32]); // origin_gateway
    envelope_bytes.extend_from_slice(&0u64.to_be_bytes()); // logical_timestamp
    envelope_bytes.extend_from_slice(&1u16.to_be_bytes()); // ttl_hops
    envelope_bytes.extend_from_slice(&[0u8; 32]); // payload_hash
    envelope_bytes.extend_from_slice(&[0u8; 32]); // route_trace_root
    envelope_bytes.extend_from_slice(&0u64.to_be_bytes()); // flags
    debug_assert_eq!(envelope_bytes.len(), 218);
    envelope_bytes.extend_from_slice(&[0u8; 64]); // signature
    debug_assert_eq!(envelope_bytes.len(), 282);
    let envelope = DeterministicEnvelope::from_wire_bytes(&envelope_bytes)
        .expect("from_wire_bytes accepts the constructed bytes");

    let domain = broadcast_domain_for(&adapter1, room_id_typed.as_ref());
    let receipt = adapter1
        .send_envelope(&domain, &envelope)
        .await
        .expect("send_envelope into encrypted room");
    assert!(
        !receipt.platform_message_id.is_empty(),
        "encrypted-room send returned empty event id"
    );

    // --- 6. ci2 receives; SDK decrypts; adapter surfaces plaintext ---
    // The receiver may not have the Megolm key on the very first
    // poll (the key-sharing dance via Olm is async), so retry.
    let mut decrypted = false;
    for _ in 0..10 {
        let received = adapter2
            .receive_messages(&domain)
            .await
            .expect("receive_messages must not error");
        for msg in &received {
            // The SDK decrypts the m.ciphertext content; the
            // adapter's `canonicalize` returns the plaintext
            // bytes (the structured 282-byte envelope).
            if let Ok(canonical) = adapter2.canonicalize(msg) {
                if canonical.to_wire_bytes() == wire {
                    decrypted = true;
                    break;
                }
            }
        }
        if decrypted {
            break;
        }
        tokio::time::sleep(Duration::from_millis(500)).await;
    }
    assert!(
        decrypted,
        "ci2's adapter did not see the decrypted envelope within 5s"
    );

    // Suppress the unused warning for `room2` (we keep the join
    // result to ensure the join actually succeeded before we
    // start sending).
    let _ = room2;
}

/// Log in a user by `localpart` (e.g. `ci2`) and return the
/// extracted session material. Distinct from `login_and_save_config`
/// which hard-codes the `ci` user for the single-user tests.
async fn login_user(
    _localpart_unused: &str,
    _pw_unused: &str,
    _override_homeserver: Option<String>,
) -> octo_matrix_onboard_core::Session {
    // The single helper covers the canonical user; for the second
    // user we use env-overridable homeserver/user/password with
    // defaults that match the script's `ci2` provisioning.
    let homeserver =
        std::env::var("INTEGRATION_HOMESERVER2").unwrap_or_else(|_| DEFAULT_HOMESERVER.to_string());
    let user = std::env::var("INTEGRATION_USER2").unwrap_or_else(|_| "@ci2:localhost".to_string());
    let password =
        std::env::var("INTEGRATION_PASSWORD2").unwrap_or_else(|_| "ci-password".to_string());
    let client = Client::builder()
        .homeserver_url(&homeserver)
        .build()
        .await
        .expect("build client for ci2");
    client
        .matrix_auth()
        .login_username(&user, &password)
        .request_refresh_token()
        .send()
        .await
        .expect("ci2 login_username");
    session::extract(&client, &homeserver).expect("extract ci2 session")
}

/// Rebuild a logged-in `Client` and return `(client, user_id,
/// device_id)`. Helper for the encrypted-room test.
async fn rebuild_client(
    sess: &octo_matrix_onboard_core::Session,
) -> (
    Client,
    matrix_sdk::ruma::OwnedUserId,
    matrix_sdk::ruma::OwnedDeviceId,
) {
    use matrix_sdk::authentication::matrix::MatrixSession;
    use matrix_sdk::{SessionMeta, SessionTokens};
    let user_id =
        matrix_sdk::ruma::OwnedUserId::try_from(sess.user_id.as_str()).expect("user_id is valid");
    let device_id = matrix_sdk::ruma::OwnedDeviceId::from(sess.device_id.as_str());
    let session = MatrixSession {
        meta: SessionMeta {
            user_id: user_id.clone(),
            device_id: device_id.clone(),
        },
        tokens: SessionTokens {
            access_token: sess.access_token().to_string(),
            refresh_token: sess.refresh_token.clone(),
        },
    };
    let client = Client::builder()
        .homeserver_url(&sess.homeserver_url)
        .build()
        .await
        .expect("rebuild client");
    client
        .restore_session(session)
        .await
        .expect("restore_session");
    (client, user_id, device_id)
}
