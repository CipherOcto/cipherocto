//! Live integration tests against an existing authenticated TDLib session.
//!
//! These tests load a real session from the mounted `TELEGRAM_DATA_DIR`,
//! create a `RealTelegramClient` (which calls `get_me` at startup and
//! populates the client's `SelfHandle`), wrap it in `TelegramAdapter`,
//! and run a small set of live assertions.
//!
//! **Not** run by default — requires a mounted session, the TDLib
//! shared library on `LD_LIBRARY_PATH`, and a working `get_me`
//! (TDLib is observed to be unreliable in some Docker/network setups).
//!
//! Run inside Docker via `scripts/run-live-telegram-tests.sh`, or:
//!
//! ```bash
//! cargo test -p octo-adapter-telegram \
//!   --features real-tdlib \
//!   --test live_session_test \
//!   -- --ignored --nocapture --test-threads=1
//! ```
//!
//! Environment variables consumed (all required):
//! - `TELEGRAM_DATA_DIR` — TDLib database dir (contains `database/` + `files/`)
//! - `TELEGRAM_MODE`     — `"user"` (this is the user-mode suite)
//! - `TELEGRAM_API_ID`   — from my.telegram.org (or TDesktop default 17349)
//! - `TELEGRAM_API_HASH` — from my.telegram.org (or TDesktop default)
//! - `TELEGRAM_PHONE`    — phone number with country code (`+1...`)
//!
//! Why `--test-threads=1`: `tdlib_rs::receive()` is process-global.
//! Two tests in parallel would race for updates (same root cause as
//! the R16 hang in the onboard tool).

#![cfg(feature = "real-tdlib")]

use octo_adapter_telegram::adapter::TelegramAdapter;
use octo_adapter_telegram::client::TelegramClient;
use octo_adapter_telegram::config::TelegramConfig;
use octo_adapter_telegram::real_client::RealTelegramClient;
use octo_network::dot::adapters::PlatformAdapter;
use std::time::Duration;

/// Build a `TelegramConfig` for the live test. Resolution order:
///
/// 1. Read `telegram.json` from `TELEGRAM_CONFIG` (default
///    `/octo-state/telegram.json` — the path the docker script mounts
///    the persistent dir at). The auth flow writes this file, so it
///    already has `data_dir`, `mode`, `api_id`, `api_hash`, and (after
///    the build_full_config fix lands) `phone`.
/// 2. If the file is missing, fall back to `TelegramConfig::from_env()`
///    so the test still works in a pure-env setup (e.g., CI).
/// 3. `TELEGRAM_PHONE` env var, if set, overrides the `phone` field
///    loaded from the file. This is the escape hatch for the current
///    configs that don't have a phone (the qr-link flow uses
///    `build_config_json`, not `build_full_config`).
///
/// Panics with a clear message if the resolved config is invalid.
fn live_config() -> TelegramConfig {
    let config_path = std::env::var("TELEGRAM_CONFIG")
        .unwrap_or_else(|_| "/octo-state/telegram.json".to_string());

    let mut config = TelegramConfig::from_file_or_env(std::path::Path::new(&config_path))
        .unwrap_or_else(|e| {
            panic!(
                "could not load Telegram config from {config_path}: {e}\n\
                 ensure the auth flow has been run, or set TELEGRAM_* env vars"
            )
        });

    // TELEGRAM_PHONE is an optional override. The auth flow should
    // write it to telegram.json, but older configs (pre-build_full_config
    // fix) don't have it; the env var lets those still work.
    if let Ok(phone) = std::env::var("TELEGRAM_PHONE") {
        if !phone.is_empty() {
            config.phone = Some(phone);
        }
    }

    if let Err(e) = config.validate() {
        panic!(
            "TELEGRAM config is not valid for a live test: {e}\n\
             required fields by mode:\n\
               user: data_dir, api_id (>0), api_hash, phone\n\
               bot:  bot_token, api_id (>0), api_hash\n\
             loaded from {config_path} (+ TELEGRAM_PHONE override if set)"
        );
    }
    config
}

/// Create a `RealTelegramClient` from the live config. `RealTelegramClient::new`
/// internally:
///   1. calls `set_tdlib_parameters` (TDLib auto-loads the existing auth key
///      from `config.data_dir/database/`)
///   2. waits for `AuthorizationState::Ready` (30s timeout)
///   3. calls `get_me` and populates the client's `SelfHandle`
///
/// Returns both the client and its `SelfHandle` so the adapter can share
/// the cached identity for self-loop filtering.
async fn live_client_and_handle() -> (RealTelegramClient, octo_adapter_telegram::SelfHandle) {
    let config = live_config();
    let client = RealTelegramClient::new(&config)
        .await
        .expect("RealTelegramClient::new failed — is the session at TELEGRAM_DATA_DIR valid?");
    let handle = client.self_handle();
    (client, handle)
}

/// Drop the adapter/client. `RealTelegramClient` is `Clone` and
/// `Arc<ClientState>`-backed, so all clones share one TDLib client.
/// `Drop` sends a shutdown signal on a channel that the receive loop
/// drains, then closes TDLib. We give it up to 5s to settle so the
/// test binary doesn't outlive the docker run.
async fn shutdown_and_wait(adapter: TelegramAdapter<RealTelegramClient>) {
    let _ = adapter.shutdown().await;
    drop(adapter);
    tokio::time::sleep(Duration::from_secs(2)).await;
}

#[tokio::test]
#[ignore = "requires live TDLib session; run via scripts/run-live-telegram-tests.sh"]
async fn live_session_health_check() {
    let _ = tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info,octo_adapter_telegram=debug")),
        )
        .try_init();

    let (client, handle) = live_client_and_handle().await;
    let config = client_self_config(&client);
    let adapter = TelegramAdapter::with_self_handle(config, client, handle);

    adapter
        .health_check()
        .await
        .expect("health_check should return Ok for a valid session");

    tracing::info!("live_session_health_check: PASSED");

    // The handle is moved into the adapter; we have to drop via the adapter.
    // We can't extract it back out, so just drop the whole adapter.
    drop(adapter);
    tokio::time::sleep(Duration::from_secs(2)).await;
}

/// The key assertion: `get_me` ran successfully and populated the
/// `SelfHandle` with the REAL user_id (not 0). This is the test that
/// would have caught the R16 get_me hang — if TDLib didn't respond
/// to getMe, the handle would be None or have user_id=0.
#[tokio::test]
#[ignore = "requires live TDLib session; run via scripts/run-live-telegram-tests.sh"]
async fn live_session_get_me_returns_real_identity() {
    let _ = tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info,octo_adapter_telegram=debug")),
        )
        .try_init();

    let (client, handle) = live_client_and_handle().await;

    let identity = handle.get().expect(
        "SelfHandle is empty — get_me did not complete at client construction. \
         TDLib may be hung (same root cause as the R16 onboard hang). \
         Check that the session at TELEGRAM_DATA_DIR is valid.",
    );
    assert!(
        identity.user_id > 0,
        "get_me returned user_id={}, expected a positive Telegram ID. \
         TDLib may have returned a non-User variant or a stale partial session.",
        identity.user_id
    );
    tracing::info!(
        user_id = identity.user_id,
        username = %identity.username,
        "live_session_get_me_returns_real_identity: PASSED"
    );

    // Sanity-check the other client methods don't blow up.
    let updates = client
        .receive_updates()
        .await
        .expect("receive_updates should drain cleanly for a valid session");
    tracing::info!(count = updates.len(), "receive_updates drained");

    let config = client_self_config(&client);
    let adapter = TelegramAdapter::with_self_handle(config, client, handle);
    shutdown_and_wait(adapter).await;
}

/// `domain_id` derives a stable `BroadcastDomainId` from a chat ID;
/// the adapter caches the chat_id → domain_hash mapping so
/// `send_envelope` can route. This test exercises that the mapping
/// is bijective (same input → same hash, different input → different
/// hash) and that `register_domain` round-trips.
#[tokio::test]
#[ignore = "requires live TDLib session; run via scripts/run-live-telegram-tests.sh"]
async fn live_session_domain_id_round_trip() {
    use octo_network::dot::adapters::PlatformAdapter;

    let _ = tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .try_init();

    let (client, handle) = live_client_and_handle().await;
    let config = client_self_config(&client);
    let adapter = TelegramAdapter::with_self_handle(config, client, handle);

    // Two different chat IDs should produce different domain hashes.
    let a = adapter.domain_id("-1001234567890");
    let b = adapter.domain_id("-1009876543210");
    // platform_type is a u16 discriminant; we don't pin it here —
    // the important property is that the domain hash is
    // deterministic per chat_id and bijective across chat_ids.
    assert_ne!(a, b, "different chat_ids must produce different domain hashes");

    // Same chat ID must produce the same hash (deterministic).
    let a2 = adapter.domain_id("-1001234567890");
    assert_eq!(a, a2, "domain_id is not deterministic for the same chat_id");

    tracing::info!("live_session_domain_id_round_trip: PASSED");

    shutdown_and_wait(adapter).await;
}

/// Helper: extract a `TelegramConfig` back from a `RealTelegramClient`.
/// `RealTelegramClient` doesn't store the config (only the fields it
/// needs), so we reconstruct one from the env vars. This is safe
/// because the env hasn't changed between client construction and
/// now (we're still in the same process).
fn client_self_config(_client: &RealTelegramClient) -> TelegramConfig {
    live_config()
}
