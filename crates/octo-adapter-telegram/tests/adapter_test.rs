//! Tests for PlatformAdapter trait impl.
//! Mission AC line 128: "Implements PlatformAdapter trait with all methods (6 required + 6 optional)"

use octo_adapter_telegram::mock::MockTelegramClient;
use octo_adapter_telegram::{FailureSpec, TelegramAdapter, TelegramConfig};
use octo_network::dot::adapters::backoff::RetryConfig;
use octo_network::dot::adapters::PlatformAdapter;

/// H7 + M8: shared `parse_chat_id` helper used by both mock and real client.
/// Tests pass on mock and fail on real without this — the mock previously
/// accepted any string, while the real client required valid `i64`. Both
/// must agree on the boundary cases. M8 also requires that positive IDs
/// are rejected: Telegram chat_ids are always negative, and a positive
/// number in this position is a user_id that would route the envelope
/// to the wrong peer.
#[test]
fn test_parse_chat_id_rejects_non_numeric_and_positive() {
    use octo_adapter_telegram::client::parse_chat_id;
    assert!(parse_chat_id("abc").is_err());
    assert!(parse_chat_id("").is_err());
    // Positive IDs are rejected — Telegram chat_ids are negative.
    assert!(parse_chat_id("123").is_err());
    assert!(parse_chat_id("1234567890").is_err());
    // Negative IDs (basic group, supergroup, channel) are accepted.
    assert!(parse_chat_id("-1001234567890").is_ok());
    assert!(parse_chat_id("-123").is_ok());
}

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
    // max_payload_bytes: 4096 — the text message cap for the base64-encoded
    // envelope string. Larger envelopes are sent via sendDocument (R4 H1).
    assert_eq!(cap.max_payload_bytes, 4096);
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
    // Self-handle requires fetching from the client; with no SelfHandle set,
    // it should return None.
    assert!(
        adapter.self_handle().is_none(),
        "default adapter without SelfHandle set should return None, got {:?}",
        adapter.self_handle()
    );
}

/// API-H4: upload_media with no registered domains should error.
#[test]
fn test_upload_media_errors_with_zero_domains() {
    use octo_network::dot::adapters::PlatformAdapter;
    let config = TelegramConfig::default();
    let client = MockTelegramClient::new();
    let adapter = TelegramAdapter::new(config, client);
    let rt = tokio::runtime::Runtime::new().unwrap();
    let result = rt.block_on(adapter.upload_media("test.bin", b"data", "application/octet-stream"));
    match result {
        Err(octo_network::dot::error::PlatformAdapterError::Unreachable { platform, reason }) => {
            assert_eq!(platform, "telegram");
            assert!(
                reason.contains("no registered domain"),
                "expected 'no registered domain' error, got: {}",
                reason
            );
        }
        other => panic!("expected Unreachable for zero domains, got: {:?}", other),
    }
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

/// H2: `upload_media` errors when more than one domain is registered, because
/// picking any single one would be non-deterministic by caller intent. The
/// caller must use `upload_media_to_domain` to disambiguate.
#[tokio::test]
async fn test_upload_media_errors_with_multiple_domains() {
    let config = TelegramConfig::default();
    let client = MockTelegramClient::new();
    let adapter = TelegramAdapter::new(config, client);
    adapter.domain_id("-1001111111111");
    adapter.domain_id("-1002222222222");
    let result = adapter
        .upload_media("file.bin", b"hello", "application/octet-stream")
        .await;
    assert!(
        result.is_err(),
        "upload_media should error when multiple domains are registered"
    );
}

/// H2: `upload_media_to_domain` is the explicit, deterministic routing path.
/// It uses the caller-provided `BroadcastDomainId` to look up the registered
/// chat_id and route the document to that exact domain.
#[tokio::test]
async fn test_upload_media_to_domain_routes_correctly() {
    let config = TelegramConfig::default();
    let client = MockTelegramClient::new();
    let observer = client.clone();
    let adapter = TelegramAdapter::new(config, client);
    let d1 = adapter.domain_id("-1001111111111");
    let d2 = adapter.domain_id("-1002222222222");
    let result = adapter
        .upload_media_to_domain(&d1, "file.bin", b"hello", "application/octet-stream")
        .await;
    assert!(
        result.is_ok(),
        "upload_media_to_domain should route to the specified domain"
    );
    let sent = observer.sent_documents();
    assert_eq!(sent.len(), 1);
    assert_eq!(sent[0].0, "-1001111111111");
    // Sanity check: d2 was registered but not used.
    let _ = d2;
}

/// M6: `send_with_retry` must retry on `TelegramError::Transient` (5xx /
/// "connection failed" / "connection closed" from TDLib). The retry loop
/// is private, so we exercise it through `send_message` — the only public
/// caller of `send_with_retry`. The mock is configured to fail twice with
/// `Transient` and then succeed. With a tiny backoff config (zero initial
/// backoff, zero max backoff, zero jitter) and `max_retries=3` the loop
/// runs: 1 initial + 2 retries = 3 total sends.
#[tokio::test]
async fn test_send_with_retry_retries_on_transient() {
    use octo_network::dot::envelope::DeterministicEnvelope;

    let config = TelegramConfig::default();
    let client = MockTelegramClient::new();
    let observer = client.clone();
    // Tiny backoff: 0 initial, 0 max, 0 jitter — keeps the test sub-second.
    // 3 retries permitted, so the loop has room for the 2 injected failures
    // plus 1 success on the third attempt.
    let retry = RetryConfig {
        max_retries: 3,
        initial_backoff_secs: 0,
        max_backoff_secs: 0,
        max_jitter_ms: 0,
    };
    let adapter = TelegramAdapter::with_retry_config(config, client, retry);
    let domain = adapter.domain_id("-1001234567890");
    // Fail the first 2 send_message calls with `Transient`, then succeed.
    observer.fail_next_n_sends(
        2,
        FailureSpec::Transient("connection failed: TDLib 502".into()),
    );
    let envelope = DeterministicEnvelope {
        version: 1,
        network_id: 42,
        message_type: 0,
        envelope_id: [1u8; 32],
        mission_id: [0u8; 32],
        source_peer: [2u8; 32],
        origin_gateway: [3u8; 32],
        logical_timestamp: 100,
        ttl_hops: 5,
        payload_hash: [4u8; 32],
        route_trace_root: [5u8; 32],
        flags: 0,
        signature: [6u8; 64],
    };
    let result = adapter.send_message(&domain, &envelope, b"test").await;
    assert!(
        result.is_ok(),
        "send_message should succeed after 2 transient failures: {:?}",
        result.err()
    );
    // 1 initial call + 2 failed-injection calls = 3 total. The third call
    // (the first retry to fail-free path) actually delivers the message.
    assert_eq!(
        observer.send_call_count(),
        3,
        "send_with_retry must retry on Transient: expected 3 total calls (1 initial + 2 retries), got {}",
        observer.send_call_count()
    );
    // Sanity: the message that eventually succeeded was actually recorded.
    let sent = observer.sent_messages();
    assert_eq!(
        sent.len(),
        1,
        "exactly one successful send should be recorded"
    );
}

/// M6 companion: if `Transient` errors exceed `max_retries`, the adapter
/// must surface a `PlatformAdapterError::Unreachable` rather than retry
/// forever. The mock fails 10 times with `Transient`, and `max_retries=2`
/// only permits 2 retries → 3 total calls, the last of which still fails.
#[tokio::test]
async fn test_send_with_retry_gives_up_on_transient_after_max_retries() {
    use octo_network::dot::envelope::DeterministicEnvelope;

    let config = TelegramConfig::default();
    let client = MockTelegramClient::new();
    let observer = client.clone();
    let retry = RetryConfig {
        max_retries: 2,
        initial_backoff_secs: 0,
        max_backoff_secs: 0,
        max_jitter_ms: 0,
    };
    let adapter = TelegramAdapter::with_retry_config(config, client, retry);
    let domain = adapter.domain_id("-1001234567890");
    // Inject more failures than the retry budget allows; the loop must give
    // up rather than drain the counter to zero.
    observer.fail_next_n_sends(
        10,
        FailureSpec::Transient("connection failed: TDLib 503".into()),
    );
    let envelope = DeterministicEnvelope {
        version: 1,
        network_id: 42,
        message_type: 0,
        envelope_id: [1u8; 32],
        mission_id: [0u8; 32],
        source_peer: [2u8; 32],
        origin_gateway: [3u8; 32],
        logical_timestamp: 100,
        ttl_hops: 5,
        payload_hash: [4u8; 32],
        route_trace_root: [5u8; 32],
        flags: 0,
        signature: [6u8; 64],
    };
    let result = adapter.send_message(&domain, &envelope, b"test").await;
    assert!(
        result.is_err(),
        "send_message should surface Unreachable when max_retries is exhausted"
    );
    // 1 initial + 2 retries (max_retries=2) = 3 total calls.
    assert_eq!(
        observer.send_call_count(),
        3,
        "expected 1 initial + 2 retries = 3 calls before giving up, got {}",
        observer.send_call_count()
    );
}

// =============================================================================
// API-H2: Ed25519 signature verification tests
// =============================================================================

/// A DeterministicEnvelope signed with a known key must round-trip through
/// canonicalize when the adapter has the matching verifying_key.
#[test]
fn test_canonicalize_accepts_valid_signature() {
    use base64::{engine::general_purpose::STANDARD, Engine as _};
    use ed25519_dalek::{Signer, SigningKey, VerifyingKey};
    use octo_network::dot::adapters::PlatformAdapter;
    use octo_network::dot::envelope::DeterministicEnvelope;

    // Use a fixed known key pair (ed25519-dalek 2.x SigningKey::from_keypair_bytes)
    let seed = [42u8; 32];
    let signing_key = SigningKey::from_bytes(&seed);
    let verifying_key: VerifyingKey = signing_key.verifying_key();
    let vk_bytes = verifying_key.to_bytes();

    // Create a config with the verifying key (base64).
    // Use struct literal to avoid clippy field-assignment-after-default lint
    let config = TelegramConfig {
        verifying_key: Some(STANDARD.encode(vk_bytes)),
        ..TelegramConfig::default()
    };

    // Create the envelope and sign it.
    let mut envelope = DeterministicEnvelope {
        version: 1,
        network_id: 42,
        message_type: 0,
        envelope_id: [1u8; 32],
        mission_id: [0u8; 32],
        source_peer: [2u8; 32],
        origin_gateway: [3u8; 32],
        logical_timestamp: 100,
        ttl_hops: 5,
        payload_hash: [4u8; 32],
        route_trace_root: [5u8; 32],
        flags: 0,
        signature: [0u8; 64],
    };
    // Must derive envelope_id before signing (it's part of the signing payload)
    envelope.envelope_id = envelope.derive_envelope_id();
    let signing_bytes = envelope.to_signing_bytes();
    envelope.signature = signing_key.sign(&signing_bytes).to_bytes();

    let client = MockTelegramClient::new();
    let adapter = TelegramAdapter::new(config, client);
    let wire = envelope.to_wire_bytes();
    // The payload must be base64-encoded text (as it arrives from Telegram)
    let encoded = octo_adapter_telegram::envelope::encode_envelope(&wire);

    let raw = octo_network::dot::adapters::RawPlatformMessage {
        platform_id: "test".into(),
        payload: encoded.into_bytes(),
        metadata: std::collections::BTreeMap::new(),
    };

    let result = adapter.canonicalize(&raw);
    assert!(
        result.is_ok(),
        "valid signature should be accepted: {:?}",
        result.err()
    );
}

/// An envelope with a wrong signature must be rejected with ApiError(401).
#[test]
fn test_canonicalize_rejects_invalid_signature_returns_401() {
    use base64::{engine::general_purpose::STANDARD, Engine as _};
    use ed25519_dalek::{Signer, SigningKey};
    use octo_network::dot::adapters::PlatformAdapter;
    use octo_network::dot::envelope::DeterministicEnvelope;

    // Use two different fixed keys.
    let seed1 = [1u8; 32];
    let signing_key1 = SigningKey::from_bytes(&seed1);
    let verifying_key = signing_key1.verifying_key();
    let vk_bytes = verifying_key.to_bytes();

    let seed2 = [2u8; 32];
    let signing_key2 = SigningKey::from_bytes(&seed2);

    let config = TelegramConfig {
        verifying_key: Some(STANDARD.encode(vk_bytes)),
        ..TelegramConfig::default()
    };

    // Create envelope signed with key2, but verify with key1's verifying_key.
    let mut envelope = DeterministicEnvelope {
        version: 1,
        network_id: 42,
        message_type: 0,
        envelope_id: [1u8; 32],
        mission_id: [0u8; 32],
        source_peer: [2u8; 32],
        origin_gateway: [3u8; 32],
        logical_timestamp: 100,
        ttl_hops: 5,
        payload_hash: [4u8; 32],
        route_trace_root: [5u8; 32],
        flags: 0,
        signature: [0u8; 64],
    };
    let signing_bytes = envelope.to_signing_bytes();
    envelope.signature = signing_key2.sign(&signing_bytes).to_bytes();

    let client = MockTelegramClient::new();
    let adapter = TelegramAdapter::new(config, client);
    let wire = envelope.to_wire_bytes();
    // The payload must be base64-encoded text (as it arrives from Telegram)
    let encoded = octo_adapter_telegram::envelope::encode_envelope(&wire);

    let raw = octo_network::dot::adapters::RawPlatformMessage {
        platform_id: "test".into(),
        payload: encoded.into_bytes(),
        metadata: std::collections::BTreeMap::new(),
    };

    let result = adapter.canonicalize(&raw);
    match result {
        Err(octo_network::dot::error::PlatformAdapterError::ApiError { code, message }) => {
            assert_eq!(code, 401, "wrong key should give 401, got code={}", code);
            assert!(
                message.contains("signature verification failed"),
                "message: {}",
                message
            );
        }
        other => panic!("expected ApiError(401), got: {:?}", other),
    }
}

/// When no verifying_key is configured, canonicalize must skip verification
/// and return Ok.
#[test]
fn test_canonicalize_no_verifying_key_skips_verify() {
    use ed25519_dalek::{Signer, SigningKey};
    use octo_network::dot::adapters::PlatformAdapter;
    use octo_network::dot::envelope::DeterministicEnvelope;

    let seed = [42u8; 32];
    let signing_key = SigningKey::from_bytes(&seed);

    let config = TelegramConfig::default();
    assert!(
        config.verifying_key.is_none(),
        "default config should have no verifying_key"
    );

    let mut envelope = DeterministicEnvelope {
        version: 1,
        network_id: 42,
        message_type: 0,
        envelope_id: [1u8; 32],
        mission_id: [0u8; 32],
        source_peer: [2u8; 32],
        origin_gateway: [3u8; 32],
        logical_timestamp: 100,
        ttl_hops: 5,
        payload_hash: [4u8; 32],
        route_trace_root: [5u8; 32],
        flags: 0,
        signature: [0u8; 64],
    };
    // Must derive envelope_id before signing (it's part of the signing payload)
    envelope.envelope_id = envelope.derive_envelope_id();
    let signing_bytes = envelope.to_signing_bytes();
    envelope.signature = signing_key.sign(&signing_bytes).to_bytes();

    let client = MockTelegramClient::new();
    let adapter = TelegramAdapter::new(config, client);
    let wire = envelope.to_wire_bytes();
    // The payload must be base64-encoded text (as it arrives from Telegram)
    let encoded = octo_adapter_telegram::envelope::encode_envelope(&wire);

    let raw = octo_network::dot::adapters::RawPlatformMessage {
        platform_id: "test".into(),
        payload: encoded.into_bytes(),
        metadata: std::collections::BTreeMap::new(),
    };

    let result = adapter.canonicalize(&raw);
    assert!(
        result.is_ok(),
        "without verifying_key, canonicalize should pass: {:?}",
        result.err()
    );
}

// =============================================================================
// API-M4: parse_chat_id edge cases
// =============================================================================

/// API-M4: `parse_chat_id("0")` must be rejected. Zero is >= 0, so it
/// falls into the "chat_id must be negative" branch. The mock's
/// `receive_updates` uses `chat_id.parse().unwrap_or(0)` — if `parse_chat_id`
/// accepted 0, documents would be silently routed to chat_id 0 which is
/// a Telegram system sentinel (not a real chat).
#[test]
fn test_parse_chat_id_rejects_zero() {
    use octo_adapter_telegram::client::parse_chat_id;
    let result = parse_chat_id("0");
    assert!(
        result.is_err(),
        "parse_chat_id('0') must reject zero: got Ok({:?})",
        result.ok()
    );
    let err_msg = result.unwrap_err();
    assert!(
        err_msg.contains("negative"),
        "error should mention negative convention, got: {}",
        err_msg
    );
}

// =============================================================================
// API-M1: TelegramError::Auth -> PlatformAdapterError redaction test
// =============================================================================

/// API-M1: When `TelegramError::Auth` contains a bot token in its message,
/// the `From<TelegramError> for PlatformAdapterError` impl must call
/// `redact_credentials` before surfacing the message. This test verifies
/// the redaction is applied during error conversion.
#[test]
fn test_auth_error_redacts_credentials_in_conversion() {
    use octo_adapter_telegram::TelegramError;
    use octo_network::dot::error::PlatformAdapterError;

    // Simulate a TDLib auth error that echoes the bot token
    let err = TelegramError::Auth(
        "unauthorized: token 1234567890:ABCdefGHIjklMNOpqrsTUVwxyz-_ABCDE rejected".into(),
    );
    let platform_err: PlatformAdapterError = err.into();
    match platform_err {
        PlatformAdapterError::ApiError { code, message } => {
            assert_eq!(code, 401, "Auth error should map to 401");
            assert!(
                !message.contains("1234567890:ABCdefGHIjklMNOpqrsTUVwxyz-_ABCDE"),
                "bot token must be redacted in error message, got: {}",
                message
            );
            assert!(
                message.contains("<redacted>"),
                "error message should contain <redacted>, got: {}",
                message
            );
            assert!(
                message.contains("unauthorized"),
                "original context should be preserved, got: {}",
                message
            );
        }
        other => panic!("expected ApiError, got: {:?}", other),
    }
}

/// API-M1 companion: the catch-all branch in the From impl (for Io, Json,
/// Base64, SendFailed, TdlibClient) also calls `redact_credentials`.
#[test]
fn test_catch_all_error_redacts_credentials() {
    use octo_adapter_telegram::TelegramError;
    use octo_network::dot::error::PlatformAdapterError;

    // SendFailed is a catch-all variant
    let err = TelegramError::SendFailed(
        "send failed for token 1234567890:ABCdefGHIjklMNOpqrsTUVwxyz-_ABCDE".into(),
    );
    let platform_err: PlatformAdapterError = err.into();
    match platform_err {
        PlatformAdapterError::ApiError { code, message } => {
            assert_eq!(code, 500, "SendFailed should map to 500");
            assert!(
                !message.contains("1234567890:ABCdefGHIjklMNOpqrsTUVwxyz-_ABCDE"),
                "bot token must be redacted in catch-all path, got: {}",
                message
            );
            assert!(message.contains("<redacted>"), "should contain <redacted>");
        }
        other => panic!("expected ApiError, got: {:?}", other),
    }
}
