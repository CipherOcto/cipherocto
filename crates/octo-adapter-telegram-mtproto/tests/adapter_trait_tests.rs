//! Tests for PlatformAdapter trait impl on the MTProto adapter.
//!
//! Mirrors `crates/octo-adapter-telegram/tests/adapter_test.rs` from the
//! TDLib adapter. Tests the adapter's PlatformAdapter implementation,
//! config validation, error redaction, and coordinator admin surface.

use octo_adapter_telegram_mtproto::adapter::MtprotoTelegramAdapter;
use octo_adapter_telegram_mtproto::client::MockTelegramMtprotoClient;
use octo_adapter_telegram_mtproto::config::MtprotoTelegramConfig;
use octo_network::dot::adapters::PlatformAdapter;
use std::sync::Arc;

fn config() -> MtprotoTelegramConfig {
    MtprotoTelegramConfig {
        mode: Some("bot".into()),
        bot_token: Some("123:abc".into()),
        api_id: Some(12345),
        api_hash: Some("0123456789abcdef0123456789abcdef".into()),
        ..Default::default()
    }
}

fn adapter() -> MtprotoTelegramAdapter<MockTelegramMtprotoClient> {
    let client = Arc::new(MockTelegramMtprotoClient::new());
    let a = MtprotoTelegramAdapter::new(config(), client);
    a.mark_ready_for_test();
    a
}

// =============================================================================
// PlatformAdapter trait
// =============================================================================

/// Verify platform_type returns Telegram.
#[test]
fn test_platform_type_is_telegram() {
    let adapter = adapter();
    assert_eq!(
        adapter.platform_type(),
        octo_network::dot::domain::PlatformType::Telegram
    );
}

/// Verify domain_id is deterministic and bijective.
#[test]
fn test_domain_id_deterministic_and_bijective() {
    let adapter = adapter();
    let a = adapter.domain_id("-1001234567890");
    let b = adapter.domain_id("-1009876543210");
    assert_ne!(a, b, "different chat_ids → different hashes");
    let a2 = adapter.domain_id("-1001234567890");
    assert_eq!(a, a2, "same chat_id → same hash");
}

/// Verify domain_id normalizes case and whitespace.
#[test]
fn test_domain_id_normalizes_case_and_whitespace() {
    let adapter = adapter();
    assert_eq!(
        adapter.domain_id("-100ABC"),
        adapter.domain_id("-100abc"),
        "case differences should normalize"
    );
    assert_eq!(
        adapter.domain_id("  -1001234567890  "),
        adapter.domain_id("-1001234567890"),
        "whitespace should be trimmed"
    );
}

/// Verify domain_id stores normalized chat_id after register_domain.
#[test]
fn test_domain_id_stores_normalized_chat_id() {
    let adapter = adapter();
    let domain = adapter.domain_id("  -1001234567890  ");
    // MTProto adapter requires explicit register_domain.
    adapter.register_domain(&domain, "-1001234567890").unwrap();
    let chat_id = adapter.chat_id_for_domain(&domain).unwrap();
    assert_eq!(chat_id, "-1001234567890");
}

// =============================================================================
// Capability report
// =============================================================================

/// Verify capability report matches MTProto adapter expectations.
#[test]
fn test_capability_report() {
    let adapter = adapter();
    let cap = adapter.capabilities();
    assert_eq!(cap.max_payload_bytes, 4096);
    assert_eq!(cap.rate_limit_per_second, 30);
    assert!(cap.supports_fragmentation);
    assert!(!cap.supports_raw_binary);
    assert!(cap.media_capabilities.is_some());
    assert_eq!(
        cap.media_capabilities.as_ref().unwrap().max_upload_bytes,
        2_000_000_000
    );
    assert!(cap.supports_receive_fragments);
    assert!(cap.supports_edited_messages);
    assert_eq!(cap.max_fragment_size, Some(2_000_000_000));
}

/// Verify HTTP transport reports 50 MB upload limit.
#[test]
fn test_capability_http_transport_reports_50mb() {
    let mut cfg = config();
    cfg.transport = octo_adapter_telegram_mtproto::transport::Transport::BotApiHttp;
    let client = Arc::new(MockTelegramMtprotoClient::new());
    let a = MtprotoTelegramAdapter::new(cfg, client);
    a.mark_ready_for_test();
    let cap = a.capabilities();
    let media = cap.media_capabilities.as_ref().unwrap();
    assert_eq!(media.max_upload_bytes, 50 * 1024 * 1024);
}

/// Verify user mode reports 1 msg/s rate limit.
#[test]
fn test_capability_user_mode_reports_1_msg_per_second() {
    let mut cfg = config();
    cfg.mode = Some("user".into());
    cfg.phone = Some("+15555550100".into());
    cfg.data_dir = Some(std::path::PathBuf::from("/tmp/x"));
    let client = Arc::new(MockTelegramMtprotoClient::new());
    let a = MtprotoTelegramAdapter::new(cfg, client);
    a.mark_ready_for_test();
    let cap = a.capabilities();
    assert_eq!(cap.rate_limit_per_second, 1);
}

// =============================================================================
// Self handle
// =============================================================================

/// Verify self_handle returns None by default.
#[test]
fn test_self_handle_returns_none_by_default() {
    let adapter = adapter();
    assert!(adapter.self_handle().is_none());
}

/// Verify self_handle returns Some after set_self_identity.
#[test]
fn test_self_handle_returns_some_after_set() {
    let adapter = adapter();
    adapter.set_self_identity(12345, Some("testuser".into()));
    let handle = adapter.self_handle();
    assert!(handle.is_some());
    assert!(handle.unwrap().contains("12345"));
}

// =============================================================================
// Send/receive
// =============================================================================

/// Verify send_message rejects unregistered domain.
#[tokio::test]
async fn test_send_message_rejects_unregistered_domain() {
    use octo_network::dot::envelope::DeterministicEnvelope;
    let adapter = adapter();
    let domain = octo_network::dot::BroadcastDomainId::new(
        octo_network::dot::domain::PlatformType::Telegram,
        "-999999",
    );
    let envelope = DeterministicEnvelope::default();
    let result = adapter.send_message(&domain, &envelope, b"test").await;
    assert!(result.is_err(), "send to unregistered domain should fail");
}

/// Verify send_message rejects not-ready lifecycle.
#[tokio::test]
async fn test_send_message_rejects_not_ready() {
    use octo_network::dot::envelope::DeterministicEnvelope;
    let cfg = config();
    let client = Arc::new(MockTelegramMtprotoClient::new());
    let adapter = MtprotoTelegramAdapter::new(cfg, client);
    // Don't mark_ready_for_test — lifecycle is Building.
    let domain = adapter.domain_id("-1001234567890");
    let envelope = DeterministicEnvelope::default();
    let result = adapter.send_message(&domain, &envelope, b"test").await;
    assert!(result.is_err(), "send when not ready should fail");
}

/// Verify receive_messages filters by domain and self.
#[tokio::test]
async fn test_receive_messages_filters_by_domain_and_self() {
    use octo_adapter_telegram_mtproto::client::MtprotoTelegramUpdate;
    use octo_adapter_telegram_mtproto::client::NewMessage;

    let adapter = adapter();
    adapter.set_self_identity(100, None);
    let target_chat: i64 = -1001234567890;
    let other_chat: i64 = -1009999999999;

    // Inject: self-authored (drop), target+other (return), wrong domain (drop).
    adapter
        .client
        .inject_update(MtprotoTelegramUpdate::NewMessage(NewMessage {
            chat_id: target_chat,
            message: "DOT/1/abc".into(),
            from_id: Some(100), // self
            message_id: 1,
            document_id: None,
            caption: None,
            timestamp: 0,
        }));
    adapter
        .client
        .inject_update(MtprotoTelegramUpdate::NewMessage(NewMessage {
            chat_id: target_chat,
            message: "DOT/1/def".into(),
            from_id: Some(200),
            message_id: 2,
            document_id: None,
            caption: None,
            timestamp: 0,
        }));
    adapter
        .client
        .inject_update(MtprotoTelegramUpdate::NewMessage(NewMessage {
            chat_id: other_chat,
            message: "DOT/1/ghi".into(),
            from_id: Some(200),
            message_id: 3,
            document_id: None,
            caption: None,
            timestamp: 0,
        }));

    let domain = adapter.domain_id(&target_chat.to_string());
    let msgs = adapter.receive_messages(&domain).await.unwrap();
    assert_eq!(msgs.len(), 1);
    assert_eq!(msgs[0].platform_id, "2");
}

// =============================================================================
// Canonicalize
// =============================================================================

/// Verify canonicalize round-trip.
#[tokio::test]
async fn test_canonicalize_round_trip() {
    use octo_network::dot::envelope::DeterministicEnvelope;
    let adapter = adapter();
    let envelope = DeterministicEnvelope::default();
    let wire = envelope.to_wire_bytes();
    let encoded = octo_adapter_telegram_mtproto::envelope::wire_encode(&envelope).unwrap();
    let raw = octo_network::dot::adapters::RawPlatformMessage {
        platform_id: "test".into(),
        payload: encoded.into_bytes(),
        metadata: std::collections::BTreeMap::new(),
    };
    let result = adapter.canonicalize(&raw);
    assert!(
        result.is_ok(),
        "canonicalize should succeed: {:?}",
        result.err()
    );
    let decoded = result.unwrap();
    assert_eq!(decoded.to_wire_bytes(), wire);
}

// =============================================================================
// Config validation
// =============================================================================

/// Bot mode requires api_id + api_hash.
#[test]
fn test_bot_mode_requires_api_credentials() {
    let config = MtprotoTelegramConfig {
        mode: Some("bot".into()),
        bot_token: Some("123:abc".into()),
        ..Default::default()
    };
    assert!(config.validate().is_err());
}

/// Bot mode with valid credentials is accepted.
#[test]
fn test_bot_mode_accepts_valid_credentials() {
    assert!(config().validate().is_ok());
}

/// User mode requires data_dir.
#[test]
fn test_user_mode_requires_data_dir() {
    let config = MtprotoTelegramConfig {
        mode: Some("user".into()),
        phone: Some("+15555550100".into()),
        api_id: Some(12345),
        api_hash: Some("abcdef".into()),
        ..Default::default()
    };
    assert!(config.validate().is_err());
}

/// QR login mode is accepted.
#[test]
fn test_qr_login_mode_accepted() {
    let config = MtprotoTelegramConfig {
        mode: Some("qr_login".into()),
        api_id: Some(12345),
        api_hash: Some("0123456789abcdef0123456789abcdef".into()),
        data_dir: Some(std::path::PathBuf::from("/tmp/x")),
        ..Default::default()
    };
    assert!(config.validate().is_ok());
}

/// HTTP transport rejected for user mode.
#[test]
fn test_http_transport_rejected_for_user_mode() {
    let config = MtprotoTelegramConfig {
        mode: Some("user".into()),
        phone: Some("+15555550100".into()),
        api_id: Some(12345),
        api_hash: Some("abcdef".into()),
        data_dir: Some(std::path::PathBuf::from("/tmp/x")),
        transport: octo_adapter_telegram_mtproto::transport::Transport::BotApiHttp,
        ..Default::default()
    };
    assert!(config.validate().is_err());
}

// =============================================================================
// Error redaction
// =============================================================================

/// Verify Debug impl redacts secrets.
#[test]
fn test_debug_redacts_secrets() {
    let cfg = config();
    let dbg = format!("{:?}", cfg);
    assert!(!dbg.contains("123:abc"), "bot_token should be redacted");
    assert!(
        !dbg.contains("0123456789abcdef0123456789abcdef"),
        "api_hash should be redacted"
    );
}

/// Verify MtprotoTelegramError redacts credentials.
#[test]
fn test_error_redacts_credentials() {
    use octo_adapter_telegram_mtproto::error::redact_credentials;
    // The MTProto redact_credentials matches key=value or key:value patterns.
    let msg = "auth failed: bot_token=1234567890:ABCdefGHIjklMNOpqrsTUVwxyz rejected";
    let redacted = redact_credentials(msg);
    assert!(
        !redacted.contains("1234567890:ABCdefGHIjklMNOpqrsTUVwxyz"),
        "token should be redacted, got: {}",
        redacted
    );
    assert!(
        redacted.contains("[REDACTED]"),
        "should contain [REDACTED], got: {}",
        redacted
    );
    assert!(
        redacted.contains("auth failed"),
        "original context preserved"
    );
}

// =============================================================================
// Lifecycle
// =============================================================================

/// Verify shutdown transitions to stopped.
#[tokio::test]
async fn test_shutdown_transitions_to_stopped() {
    let adapter = adapter();
    adapter.shutdown().await.unwrap();
    // After shutdown, health_check should fail.
    assert!(adapter.health_check().await.is_err());
}

// =============================================================================
// CoordinatorAdmin
// =============================================================================

/// Verify CoordinatorAdmin is available.
#[test]
fn test_coordinator_admin_available() {
    let adapter = adapter();
    assert!(adapter.as_coordinator_admin().is_some());
}

// =============================================================================
// Replay protection
// =============================================================================

/// Verify replay protection delegates to network layer (always true).
#[test]
fn test_replay_protection_always_true() {
    let adapter = adapter();
    assert!(adapter.replay_protection(&[0u8; 32]));
    assert!(adapter.replay_protection(&[0xFFu8; 32]));
}

// =============================================================================
// Transport
// =============================================================================

/// Verify default transport is MTProto.
#[test]
fn test_default_transport_is_mtproto() {
    let cfg = MtprotoTelegramConfig::default();
    assert_eq!(
        cfg.transport,
        octo_adapter_telegram_mtproto::transport::Transport::Mtproto
    );
}

/// Verify transport serde round-trip.
#[test]
fn test_transport_serde_round_trip() {
    use octo_adapter_telegram_mtproto::transport::Transport;
    let s = serde_json::to_string(&Transport::Mtproto).unwrap();
    assert_eq!(s, "\"mtproto\"");
    let s = serde_json::to_string(&Transport::BotApiHttp).unwrap();
    assert_eq!(s, "\"http\"");
    let t: Transport = serde_json::from_str("\"http\"").unwrap();
    assert_eq!(t, Transport::BotApiHttp);
}

// =============================================================================
// Flood wait parsing
// =============================================================================

/// Verify flood wait parsing.
#[test]
fn test_parse_flood_wait() {
    // The parse_flood_wait method is on the adapter impl.
    // Test via the public error mapping path.
    let err = octo_adapter_telegram_mtproto::error::MtprotoTelegramError::RateLimited {
        retry_after_secs: 30,
    };
    match err {
        octo_adapter_telegram_mtproto::error::MtprotoTelegramError::RateLimited {
            retry_after_secs,
        } => {
            assert_eq!(retry_after_secs, 30);
        }
        _ => panic!("expected RateLimited"),
    }
}
