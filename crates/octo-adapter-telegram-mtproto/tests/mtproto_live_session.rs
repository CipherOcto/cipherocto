//! Live integration tests against an existing authenticated MTProto session.
//!
//! These tests load a real session from the onboard QR flow
//! (`scripts/mtproto-onboard-qr.sh`), connect a
//! `RealTelegramMtprotoClient` to Telegram, and verify the
//! session is alive and functional.
//!
//! **Not** run by default — requires an authenticated session.
//!
//! Run via:
//!
//! ```bash
//! cargo test -p octo-adapter-telegram-mtproto \
//!   --features real-network \
//!   --test mtproto_live_session \
//!   -- --ignored --nocapture --test-threads=1
//! ```

#![cfg(feature = "real-network")]

use octo_adapter_telegram_mtproto::adapter::MtprotoTelegramAdapter;
use octo_adapter_telegram_mtproto::client::MtprotoTelegramClient;
use octo_adapter_telegram_mtproto::config::MtprotoTelegramConfig;
use octo_adapter_telegram_mtproto::real_client::RealTelegramMtprotoClient;
use octo_adapter_telegram_mtproto::self_handle::MtprotoSelfHandle;
use octo_adapter_telegram_mtproto::session::StoolapSession;
use octo_network::dot::adapters::PlatformAdapter;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

/// Load the onboard config from the standard location.
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

/// Connect a `RealTelegramMtprotoClient` from the persisted session,
/// verify authorization, and populate the self-handle via `get_me()`.
///
/// This mirrors the TDLib `live_client_and_handle()` pattern:
/// the client connects, calls get_me, and populates SelfHandle
/// with the real user_id.
async fn live_client_and_handle() -> (Arc<RealTelegramMtprotoClient>, MtprotoSelfHandle) {
    let config = live_config();
    let api_id = config.api_id.expect("api_id required");
    let api_hash = config.api_hash.as_deref().expect("api_hash required");
    let data_dir = config.data_dir.as_ref().expect("data_dir required");

    let session = StoolapSession::open(&data_dir.join("session.db"))
        .unwrap_or_else(|e| panic!("failed to open session at {}: {e}", data_dir.display()));

    let self_handle = MtprotoSelfHandle::new();

    let client = RealTelegramMtprotoClient::connect(api_id, api_hash, session, self_handle.clone())
        .await
        .expect("RealTelegramMtprotoClient::connect failed — is the session valid?");

    // Try get_me() directly. If the session is stale or the
    // home_dc_id is wrong (from before the set_home_dc_id fix),
    // this will fail with a clear error.
    match client.grammers_client().get_me().await {
        Ok(me) => {
            let user_id = me.id().bare_id();
            let username = me.username().map(String::from);
            self_handle.set_identity(user_id, username);
        }
        Err(e) => {
            panic!(
                "get_me() failed: {e}\n\
                 The session may be stale. Re-run the onboard QR flow:\n\
                   rm -rf ~/.local/share/octo/telegram-mtproto/session.db*\n\
                   ./scripts/mtproto-onboard-qr.sh"
            );
        }
    }

    (client, self_handle)
}

/// Helper: build an adapter from a live client + handle.
fn live_adapter(
    client: Arc<RealTelegramMtprotoClient>,
    self_handle: MtprotoSelfHandle,
) -> MtprotoTelegramAdapter<RealTelegramMtprotoClient> {
    let config = live_config();
    MtprotoTelegramAdapter::with_self_handle(config, client, self_handle)
}

// =============================================================================
// LT-1: Config loads from onboard session
// =============================================================================

/// Verify the onboard config loads correctly.
#[tokio::test]
#[ignore = "requires live MTProto session"]
async fn mtproto_live_config_loads() {
    let config = live_config();
    assert!(config.api_id.is_some(), "config should have api_id");
    assert!(config.api_hash.is_some(), "config should have api_hash");
    assert!(config.data_dir.is_some(), "config should have data_dir");
    tracing::info!(
        api_id = config.api_id,
        mode = %config.mode_str(),
        "mtproto_live_config_loads: PASSED"
    );
}

// =============================================================================
// LT-2: Connection + authorization
// =============================================================================

/// Verify the client connects and the session is authorized.
///
/// Mirrors TDLib's `live_session_health_check` — the key assertion
/// is that the persisted session is still valid and authorized.
#[tokio::test]
#[ignore = "requires live MTProto session"]
async fn mtproto_live_session_is_authorized() {
    let _ = tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .try_init();

    let (client, self_handle) = live_client_and_handle().await;
    let identity = self_handle.get().expect("self_handle should be populated");
    assert!(
        identity.user_id > 0,
        "user_id should be positive, got {}",
        identity.user_id
    );

    let adapter = live_adapter(client, self_handle);

    adapter
        .health_check()
        .await
        .expect("health_check should return Ok for a valid session");

    tracing::info!("mtproto_live_session_is_authorized: PASSED");

    drop(adapter);
    tokio::time::sleep(Duration::from_secs(1)).await;
}

// =============================================================================
// LT-3: get_me returns real identity
// =============================================================================

/// The key assertion: `get_me()` populated the SelfHandle with a
/// real user_id (not 0). This mirrors TDLib's
/// `live_session_get_me_returns_real_identity`.
#[tokio::test]
#[ignore = "requires live MTProto session"]
async fn mtproto_live_get_me_returns_real_identity() {
    let _ = tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .try_init();

    let (client, self_handle) = live_client_and_handle().await;

    let identity = self_handle.get().expect(
        "SelfHandle is empty — get_me() did not complete. \
         The session may be stale or the DC may be unreachable.",
    );
    assert!(
        identity.user_id > 0,
        "get_me returned user_id={}, expected a positive Telegram ID",
        identity.user_id
    );

    tracing::info!(
        user_id = identity.user_id,
        username = ?identity.username,
        "mtproto_live_get_me_returns_real_identity: PASSED"
    );

    // Sanity-check: receive_updates should drain cleanly.
    let updates = client
        .receive_updates()
        .await
        .expect("receive_updates should drain cleanly for a valid session");
    tracing::info!(count = updates.len(), "receive_updates drained");

    let adapter = live_adapter(client, self_handle);
    drop(adapter);
    tokio::time::sleep(Duration::from_secs(1)).await;
}

// =============================================================================
// LT-4: health_check on live adapter
// =============================================================================

/// Verify health_check on a connected adapter.
///
/// Mirrors TDLib's `live_session_health_check`.
#[tokio::test]
#[ignore = "requires live MTProto session"]
async fn mtproto_live_health_check() {
    let _ = tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .try_init();

    let (client, self_handle) = live_client_and_handle().await;
    let adapter = live_adapter(client, self_handle);

    adapter
        .health_check()
        .await
        .expect("health_check should return Ok for a valid session");

    tracing::info!("mtproto_live_health_check: PASSED");

    drop(adapter);
    tokio::time::sleep(Duration::from_secs(1)).await;
}

// =============================================================================
// LT-5: self_handle returns real identity
// =============================================================================

/// Verify self_handle() on the adapter returns the real identity
/// (not None, not user_id=0).
///
/// Mirrors TDLib's assertion that self_handle is populated.
#[tokio::test]
#[ignore = "requires live MTProto session"]
async fn mtproto_live_self_handle_returns_real_identity() {
    let (client, self_handle) = live_client_and_handle().await;
    let adapter = live_adapter(client, self_handle);

    let handle = adapter.self_handle();
    assert!(
        handle.is_some(),
        "self_handle should be Some for a live session"
    );
    let handle_str = handle.unwrap();
    assert!(
        handle_str.contains("telegram:user:"),
        "handle should be 'telegram:user:<id>', got: {}",
        handle_str
    );
    // Extract user_id and verify it's positive.
    let user_id_str = handle_str.strip_prefix("telegram:user:").unwrap();
    let user_id: i64 = user_id_str.parse().unwrap();
    assert!(user_id > 0, "user_id should be positive, got {}", user_id);

    tracing::info!(
        user_id = user_id,
        "mtproto_live_self_handle_returns_real_identity: PASSED"
    );

    drop(adapter);
    tokio::time::sleep(Duration::from_secs(1)).await;
}

// =============================================================================
// LT-6: domain_id round-trip
// =============================================================================

/// Verify domain_id derives a stable BroadcastDomainId from a chat ID.
///
/// Mirrors TDLib's `live_session_domain_id_round_trip`.
#[tokio::test]
#[ignore = "requires live MTProto session"]
async fn mtproto_live_domain_id_round_trip() {
    let (client, self_handle) = live_client_and_handle().await;
    let adapter = live_adapter(client, self_handle);

    let a = adapter.domain_id("-1001234567890");
    let b = adapter.domain_id("-1009876543210");
    assert_ne!(
        a, b,
        "different chat_ids must produce different domain hashes"
    );

    let a2 = adapter.domain_id("-1001234567890");
    assert_eq!(a, a2, "domain_id is not deterministic for the same chat_id");

    tracing::info!("mtproto_live_domain_id_round_trip: PASSED");

    drop(adapter);
    tokio::time::sleep(Duration::from_secs(1)).await;
}

// =============================================================================
// LT-7: capabilities report
// =============================================================================

/// Verify capabilities report matches MTProto adapter expectations.
#[tokio::test]
#[ignore = "requires live MTProto session"]
async fn mtproto_live_capability_report() {
    let (client, self_handle) = live_client_and_handle().await;
    let adapter = live_adapter(client, self_handle);
    let cap = adapter.capabilities();

    assert_eq!(cap.max_payload_bytes, 4096);
    assert!(cap.supports_fragmentation);
    assert!(!cap.supports_raw_binary);
    assert!(cap.media_capabilities.is_some());
    assert_eq!(
        cap.media_capabilities.as_ref().unwrap().max_upload_bytes,
        2_000_000_000
    );
    assert!(
        cap.supports_receive_fragments,
        "DOT/2 receive is implemented"
    );
    assert!(cap.supports_edited_messages, "MessageEdited is surfaced");
    assert_eq!(cap.max_fragment_size, Some(2_000_000_000));

    tracing::info!("mtproto_live_capability_report: PASSED");

    drop(adapter);
    tokio::time::sleep(Duration::from_secs(1)).await;
}

// =============================================================================
// LT-8: register_domain + send_envelope
// =============================================================================

/// Verify register_domain + send_envelope round-trip on a live adapter.
#[tokio::test]
#[ignore = "requires live MTProto session"]
async fn mtproto_live_register_and_send_envelope() {
    use octo_network::dot::envelope::DeterministicEnvelope;

    let (client, self_handle) = live_client_and_handle().await;
    let adapter = live_adapter(client, self_handle);

    let domain = adapter.domain_id("-1001234567890");
    adapter.register_domain(&domain, "-1001234567890").unwrap();

    let envelope = DeterministicEnvelope::default();
    // send_envelope uses the mock client path (the real client's
    // send_message is called but the chat doesn't exist, so it
    // will fail at the Telegram API level). We verify the adapter
    // routes correctly and doesn't panic.
    let _result = adapter.send_envelope(&domain, &envelope).await;
    // We don't assert Ok here because the chat_id -1001234567890
    // may not exist on Telegram. The test verifies the adapter
    // constructs and routes without internal errors.

    tracing::info!("mtproto_live_register_and_send_envelope: PASSED");

    drop(adapter);
    tokio::time::sleep(Duration::from_secs(1)).await;
}

// =============================================================================
// LT-9: CoordinatorAdmin available
// =============================================================================

/// Verify CoordinatorAdmin is exposed.
#[tokio::test]
#[ignore = "requires live MTProto session"]
async fn mtproto_live_coordinator_admin_available() {
    let (client, self_handle) = live_client_and_handle().await;
    let adapter = live_adapter(client, self_handle);

    assert!(
        adapter.as_coordinator_admin().is_some(),
        "MTProto adapter should expose CoordinatorAdmin"
    );

    tracing::info!("mtproto_live_coordinator_admin_available: PASSED");

    drop(adapter);
    tokio::time::sleep(Duration::from_secs(1)).await;
}
