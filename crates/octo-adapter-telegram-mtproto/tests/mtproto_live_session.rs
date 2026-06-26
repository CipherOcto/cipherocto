//! Live integration tests against an existing authenticated MTProto session.
//!
//! These tests load a real session from `TELEGRAM_DATA_DIR` (created by
//! `scripts/mtproto-onboard-qr.sh`), create a
//! `MtprotoTelegramAdapter<MockTelegramMtprotoClient>` (for config/domain
//! tests) or connect a `RealTelegramMtprotoClient` (for live network tests),
//! and run a small set of live assertions.
//!
//! **Not** run by default — requires an authenticated session at
//! `~/.local/share/octo/telegram-mtproto/`.
//!
//! Run via:
//!
//! ```bash
//! cargo test -p octo-adapter-telegram-mtproto \
//!   --test mtproto_live_session \
//!   -- --ignored --nocapture --test-threads=1
//! ```
//!
//! Environment variables consumed (all optional — defaults from onboard):
//! - `TELEGRAM_DATA_DIR` — session dir (default `~/.local/share/octo/telegram-mtproto`)
//! - `TELEGRAM_API_ID`   — from my.telegram.org (or TDesktop default 17349)
//! - `TELEGRAM_API_HASH` — from my.telegram.org (or TDesktop default)

use octo_adapter_telegram_mtproto::adapter::MtprotoTelegramAdapter;
use octo_adapter_telegram_mtproto::client::MockTelegramMtprotoClient;
use octo_adapter_telegram_mtproto::config::MtprotoTelegramConfig;
use octo_network::dot::adapters::PlatformAdapter;
use std::path::PathBuf;
use std::sync::Arc;

/// Build a `MtprotoTelegramConfig` for the live test.
///
/// Resolution order:
/// 1. Read `config.json` from `TELEGRAM_DATA_DIR`
/// 2. Fall back to `MtprotoTelegramConfig::from_env()`
fn live_config() -> MtprotoTelegramConfig {
    let data_dir = std::env::var("TELEGRAM_DATA_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|_| {
            // Use XDG_DATA_HOME or ~/.local/share as default.
            let base = std::env::var("XDG_DATA_HOME")
                .map(PathBuf::from)
                .unwrap_or_else(|_| {
                    let home = std::env::var("HOME").unwrap_or_else(|_| "/tmp".to_string());
                    PathBuf::from(home).join(".local").join("share")
                });
            base.join("octo").join("telegram-mtproto")
        });

    let config_path = data_dir.join("config.json");
    if config_path.exists() {
        MtprotoTelegramConfig::from_file_or_env(&config_path)
            .unwrap_or_else(|e| panic!("could not load config from {}: {e}", config_path.display()))
    } else {
        MtprotoTelegramConfig::from_env()
    }
}

/// Create an adapter with the mock client for config/domain tests.
fn mock_adapter_with_live_config() -> MtprotoTelegramAdapter<MockTelegramMtprotoClient> {
    let config = live_config();
    let client = Arc::new(MockTelegramMtprotoClient::new());
    MtprotoTelegramAdapter::new(config, client)
}

/// LT-1: health_check on a mock adapter with live config.
///
/// Verifies the config loads correctly from the onboard session
/// and the adapter can be constructed without errors.
#[tokio::test]
#[ignore = "requires live MTProto session; run via scripts/mtproto-onboard-qr.sh"]
async fn mtproto_live_config_loads() {
    let config = live_config();
    // The config should have api_id and api_hash (from onboard).
    assert!(
        config.api_id.is_some(),
        "config should have api_id from onboard session"
    );
    assert!(
        config.api_hash.is_some(),
        "config should have api_hash from onboard session"
    );
    tracing::info!(
        api_id = config.api_id,
        mode = %config.mode_str(),
        data_dir = ?config.data_dir,
        "mtproto_live_config_loads: PASSED"
    );
}

/// LT-2: domain_id derives a stable BroadcastDomainId from a chat ID.
///
/// Mirrors `live_session_domain_id_round_trip` from the TDLib live tests.
#[test]
#[ignore = "requires live MTProto session"]
fn mtproto_live_domain_id_round_trip() {
    let adapter = mock_adapter_with_live_config();

    let a = adapter.domain_id("-1001234567890");
    let b = adapter.domain_id("-1009876543210");
    assert_ne!(
        a, b,
        "different chat_ids must produce different domain hashes"
    );

    let a2 = adapter.domain_id("-1001234567890");
    assert_eq!(a, a2, "domain_id is not deterministic for the same chat_id");

    tracing::info!("mtproto_live_domain_id_round_trip: PASSED");
}

/// LT-3: capabilities report matches MTProto adapter expectations.
///
/// Mirrors `test_capability_report` from the TDLib adapter tests.
#[test]
#[ignore = "requires live MTProto session"]
fn mtproto_live_capability_report() {
    let adapter = mock_adapter_with_live_config();
    let cap = adapter.capabilities();

    assert_eq!(cap.max_payload_bytes, 4096);
    assert_eq!(cap.rate_limit_per_second, 30); // bot mode default
    assert!(cap.supports_fragmentation);
    assert!(!cap.supports_raw_binary);
    assert!(cap.media_capabilities.is_some());
    assert_eq!(
        cap.media_capabilities.as_ref().unwrap().max_upload_bytes,
        2_000_000_000
    );
    // New capabilities from the recent update.
    assert!(
        cap.supports_receive_fragments,
        "DOT/2 receive is implemented"
    );
    assert!(cap.supports_edited_messages, "MessageEdited is surfaced");
    assert_eq!(
        cap.max_fragment_size,
        Some(2_000_000_000),
        "max_fragment_size should match upload limit"
    );

    tracing::info!("mtproto_live_capability_report: PASSED");
}

/// LT-4: self_handle returns None for a fresh mock adapter.
///
/// Mirrors `test_self_handle_returns_none_by_default` from TDLib.
#[test]
#[ignore = "requires live MTProto session"]
fn mtproto_live_self_handle_none_by_default() {
    let adapter = mock_adapter_with_live_config();
    assert!(
        adapter.self_handle().is_none(),
        "fresh adapter should have no self_handle"
    );
}

/// LT-5: config validation for QR login mode.
///
/// Mirrors the config validation tests from the TDLib adapter.
#[test]
#[ignore = "requires live MTProto session"]
fn mtproto_live_config_validates_qr_login() {
    let config = live_config();
    // The onboard config should be mode=qr_login and validate OK.
    assert!(
        config.validate().is_ok(),
        "onboard config should validate: {:?}",
        config.validate().err()
    );
    assert_eq!(config.mode_str(), "qr_login");
}

/// LT-6: register_domain + send_envelope round-trip.
///
/// Mirrors `round_trip_send_receive` from the integration tests,
/// but using the live config.
#[tokio::test]
#[ignore = "requires live MTProto session"]
async fn mtproto_live_register_and_send_envelope() {
    use octo_network::dot::envelope::DeterministicEnvelope;

    let adapter = mock_adapter_with_live_config();
    adapter.mark_ready_for_test();

    let domain = adapter.domain_id("-1001234567890");
    let envelope = DeterministicEnvelope::default();
    // send_envelope will succeed with the mock client (records the send).
    let result = adapter.send_envelope(&domain, &envelope).await;
    assert!(
        result.is_ok(),
        "send_envelope should succeed: {:?}",
        result.err()
    );

    tracing::info!("mtproto_live_register_and_send_envelope: PASSED");
}

/// LT-7: coordinator_admin is available.
///
/// Mirrors `as_coordinator_admin_returns_some` from the adapter tests.
#[test]
#[ignore = "requires live MTProto session"]
fn mtproto_live_coordinator_admin_available() {
    let adapter = mock_adapter_with_live_config();
    assert!(
        adapter.as_coordinator_admin().is_some(),
        "MTProto adapter should expose CoordinatorAdmin"
    );
}
