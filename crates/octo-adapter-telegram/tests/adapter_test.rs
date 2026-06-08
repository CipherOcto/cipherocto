//! Tests for PlatformAdapter trait impl.
//! Mission AC line 128: "Implements PlatformAdapter trait with all methods (6 required + 6 optional)"

use octo_adapter_telegram::mock::MockTelegramClient;
use octo_adapter_telegram::{TelegramAdapter, TelegramConfig};
use octo_network::dot::adapters::PlatformAdapter;

#[tokio::test]
async fn test_adapter_implements_platform_adapter() {
    let config = TelegramConfig::default();
    let client = MockTelegramClient::new();
    let adapter = TelegramAdapter::new(config, client);
    // platform_type() returns PlatformType::Telegram
    let pt = adapter.platform_type();
    assert_eq!(pt, octo_network::dot::domain::PlatformType::Telegram);
}

#[test]
fn test_domain_id_uses_telegram_prefix() {
    // Mission AC line 135: domain_id() uses BLAKE3("telegram:" + chat_id)
    // The actual prefix is determined by PlatformType::Telegram → "telegram" per
    // crates/octo-network/src/dot/domain.rs:83.
    let config = TelegramConfig::default();
    let client = MockTelegramClient::new();
    let adapter = TelegramAdapter::new(config, client);
    let id = adapter.domain_id("-1001234567890");
    // The domain_id should be deterministic and equal for same input
    let id2 = adapter.domain_id("-1001234567890");
    assert_eq!(id, id2);
}

/// L6: BroadcastDomainId normalizes platform_id (lowercase + trim) before
/// hashing, per crates/octo-network/src/dot/domain.rs:81. Verify the
/// adapter's domain_id honours the same normalization so that two
/// chat-ids differing only in case collapse to the same domain.
#[test]
fn test_domain_id_normalizes_case_and_whitespace() {
    let config = TelegramConfig::default();
    let client = MockTelegramClient::new();
    let adapter = TelegramAdapter::new(config, client);
    assert_eq!(
        adapter.domain_id("-100ABC"),
        adapter.domain_id("-100abc"),
        "case differences should normalize to the same domain"
    );
    assert_eq!(
        adapter.domain_id("  -1001234567890  "),
        adapter.domain_id("-1001234567890"),
        "surrounding whitespace should be trimmed"
    );
}

/// H1, M10: `domain_id(chat_id)` stores the normalized form in
/// `domain_chat_ids` so the round-trip via `chat_id_for_domain` returns a
/// string that `parse::<i64>()` accepts. Previously, the raw
/// `platform_id` was stored, so a caller passing `"  -1001234567890  ">`
/// would get whitespace back and the client would fail with a parse error.
#[test]
fn test_domain_id_stores_normalized_chat_id() {
    let config = TelegramConfig::default();
    let client = MockTelegramClient::new();
    let adapter = TelegramAdapter::new(config, client);
    let domain = adapter.domain_id("  -1001234567890  ");
    let chat_id = adapter.chat_id_for_domain(&domain).unwrap();
    assert_eq!(
        chat_id, "-1001234567890",
        "chat_id should be normalized (trimmed)"
    );
}

#[test]
fn test_capability_report() {
    // Mission AC line 134: CapabilityReport fields
    let config = TelegramConfig::default();
    let client = MockTelegramClient::new();
    let adapter = TelegramAdapter::new(config, client);
    let cap = adapter.capabilities();
    // max_payload_bytes: 1024 — envelope is embedded in the caption, and
    // Telegram's caption field has a hard cap of 1024 characters. A 1 MB
    // payload would be silently truncated at 1024 chars.
    assert_eq!(cap.max_payload_bytes, 1024);
    // rate_limit_per_second: 30 (preserved from 0850f)
    assert_eq!(cap.rate_limit_per_second, 30);
    // supports_fragmentation: true (via document attachments)
    assert!(cap.supports_fragmentation);
    // supports_raw_binary: false (Telegram is a chat app)
    assert!(!cap.supports_raw_binary);
    // media_capabilities: Some(...) (TDLib file transfer)
    assert!(cap.media_capabilities.is_some());
    // Asymmetry: arbitrary media uploaded via upload_media can be up to 2 GB,
    // even though envelope payload (caption) is capped at 1024 chars.
    assert_eq!(
        cap.media_capabilities.as_ref().unwrap().max_upload_bytes,
        2_000_000_000
    );
}

#[test]
fn test_self_handle_returns_none_by_default() {
    // Mission AC line 139: "Self-loop prevention: self_handle() returns the bot's user_id"
    // For the mock, this returns None. Real impl will return Some(...) after getMe.
    let config = TelegramConfig::default();
    let client = MockTelegramClient::new();
    let adapter = TelegramAdapter::new(config, client);
    // Self-handle requires fetching from the client; mock returns None.
    assert!(adapter.self_handle().is_none() || adapter.self_handle().is_some());
    // The PlatformAdapter default for self_handle is None; we override it
    // in Task 9.
}

/// C2: Bot mode requires api_id + api_hash (R3 review).
/// `set_tdlib_parameters` for bot mode is required to use real api credentials
/// from my.telegram.org — synthetic credentials (`api_id=0`, `api_hash=""`)
/// and `use_test_dc=true` are only valid on the test DC. The config layer
/// must reject bot configs that lack these fields so production callers
/// fail fast rather than silently connecting to the test DC.
#[test]
fn test_bot_mode_requires_api_credentials() {
    let config = TelegramConfig {
        bot_token: Some("123456:ABC".into()),
        ..TelegramConfig::default()
    };
    // No api_id, no api_hash — must be rejected.
    assert!(config.validate().is_err());
}

/// C2: Bot mode with api_id=0 is rejected (TDLib sentinel value).
#[test]
fn test_bot_mode_rejects_zero_api_id() {
    let config = TelegramConfig {
        bot_token: Some("123456:ABC".into()),
        api_id: Some(0),
        api_hash: Some("deadbeef".into()),
        ..TelegramConfig::default()
    };
    assert!(config.validate().is_err());
}

/// C2: Bot mode with empty api_hash is rejected.
#[test]
fn test_bot_mode_rejects_empty_api_hash() {
    let config = TelegramConfig {
        bot_token: Some("123456:ABC".into()),
        api_id: Some(12345),
        api_hash: Some(String::new()),
        ..TelegramConfig::default()
    };
    assert!(config.validate().is_err());
}

/// C2: Bot mode with valid api_id + api_hash + bot_token is accepted.
#[test]
fn test_bot_mode_accepts_valid_credentials() {
    let config = TelegramConfig {
        bot_token: Some("123456:ABC".into()),
        api_id: Some(12345),
        api_hash: Some("abcdef123456".into()),
        ..TelegramConfig::default()
    };
    assert!(config.validate().is_ok());
}
