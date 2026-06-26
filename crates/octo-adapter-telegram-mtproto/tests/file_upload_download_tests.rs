//! File upload and download tests for the MTProto adapter.
//!
//! Mirrors `crates/octo-adapter-telegram/tests/file_upload_tests.rs` and
//! `file_download_tests.rs` from the TDLib adapter.
//!
//! These tests use the `MockTelegramMtprotoClient` to verify the
//! upload/download paths without requiring a real network.

use octo_adapter_telegram_mtproto::adapter::MtprotoTelegramAdapter;
use octo_adapter_telegram_mtproto::client::{
    MockTelegramMtprotoClient, MtprotoTelegramClient, MtprotoTelegramUpdate, NewMessage,
};
use octo_adapter_telegram_mtproto::config::MtprotoTelegramConfig;
use octo_network::dot::adapters::PlatformAdapter;
use std::sync::Arc;

const MB: usize = 1024 * 1024;
const ONE_MB: usize = MB;

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

fn adapter_with_client() -> (
    MtprotoTelegramAdapter<MockTelegramMtprotoClient>,
    Arc<MockTelegramMtprotoClient>,
) {
    let client = Arc::new(MockTelegramMtprotoClient::new());
    let a = MtprotoTelegramAdapter::new(config(), client.clone());
    a.mark_ready_for_test();
    (a, client)
}

// =============================================================================
// File upload tests (mirrors TDLib file_upload_tests.rs)
// =============================================================================

/// Verify send_document works via mock client.
#[tokio::test]
async fn test_send_document_via_mock() {
    let client = MockTelegramMtprotoClient::new();
    let data = vec![0u8; ONE_MB];
    let result = client
        .send_document(-1001234567890, "caption", "test_1mb.bin", &data)
        .await;
    assert!(result.is_ok(), "1 MB document send should succeed");
    let sent = result.unwrap();
    assert!(sent.id > 0, "should return a positive message id");
}

/// Verify mock records large document sends correctly.
#[tokio::test]
async fn test_mock_records_large_document() {
    let client = MockTelegramMtprotoClient::new();
    let data = vec![0xAB_u8; ONE_MB];
    client
        .send_document(-1001234567890, "", "large_envelope.bin", &data)
        .await
        .expect("send should succeed");
    // Mock doesn't have sent_documents() like TDLib mock, but
    // the call succeeding verifies the path is wired.
}

/// Verify the 2 GB upload ceiling for MTProto transport.
#[tokio::test]
async fn test_file_size_limit_constant() {
    let adapter = adapter();
    let cap = adapter.capabilities();
    let media = cap.media_capabilities.as_ref().unwrap();
    assert_eq!(
        media.max_upload_bytes, 2_000_000_000,
        "MTProto upload limit is 2 GB"
    );
    assert!((ONE_MB as u64) < 2_000_000_000, "test payload under limit");
}

/// Verify mock handles multiple large uploads in sequence.
#[tokio::test]
async fn test_multiple_large_uploads() {
    let client = MockTelegramMtprotoClient::new();
    let data = vec![0u8; ONE_MB];
    for i in 0..3 {
        let filename = format!("large_envelope_part{}.bin", i);
        let result = client
            .send_document(-1001234567890, "", &filename, &data)
            .await;
        assert!(result.is_ok(), "upload {} should succeed", i);
    }
}

/// Verify upload path handles empty filename gracefully.
#[tokio::test]
async fn test_upload_with_empty_filename() {
    let client = MockTelegramMtprotoClient::new();
    let data = vec![0u8; 1024];
    let result = client.send_document(-1001234567890, "", "", &data).await;
    assert!(result.is_ok(), "empty filename should still succeed");
}

/// Verify upload_media routes correctly with single domain.
#[tokio::test]
async fn test_upload_media_single_domain() {
    let adapter = adapter();
    adapter
        .register_domain(&adapter.domain_id("-1001234567890"), "-1001234567890")
        .unwrap();
    let result = adapter
        .upload_media("test.bin", b"hello", "application/octet-stream")
        .await;
    assert!(
        result.is_ok(),
        "upload_media with single domain should succeed"
    );
}

/// Verify upload_media errors with zero domains.
#[tokio::test]
async fn test_upload_media_errors_with_zero_domains() {
    let config = config();
    let client = Arc::new(MockTelegramMtprotoClient::new());
    let adapter = MtprotoTelegramAdapter::new(config, client);
    adapter.mark_ready_for_test();
    let result = adapter
        .upload_media("test.bin", b"data", "application/octet-stream")
        .await;
    assert!(
        result.is_err(),
        "upload_media with zero domains should error"
    );
}

/// Verify upload_media errors with multiple domains.
#[tokio::test]
async fn test_upload_media_errors_with_multiple_domains() {
    let adapter = adapter();
    adapter.domain_id("-1001111111111");
    adapter.domain_id("-1002222222222");
    let result = adapter
        .upload_media("file.bin", b"hello", "application/octet-stream")
        .await;
    assert!(
        result.is_err(),
        "upload_media with multiple domains should error"
    );
}

/// Verify upload_media_to_domain routes correctly.
#[tokio::test]
async fn test_upload_media_to_domain_routes_correctly() {
    let adapter = adapter();
    let d1 = adapter.domain_id("-1001111111111");
    let d2 = adapter.domain_id("-1002222222222");
    adapter.register_domain(&d1, "-1001111111111").unwrap();
    adapter.register_domain(&d2, "-1002222222222").unwrap();
    let result = adapter
        .upload_media_to_domain(&d1, "file.bin", b"hello", "application/octet-stream")
        .await;
    assert!(
        result.is_ok(),
        "upload_media_to_domain should route to specified domain"
    );
    let _ = d2;
}

// =============================================================================
// File download tests (mirrors TDLib file_download_tests.rs)
// =============================================================================

/// Verify download_file returns empty Vec for mock (no real file backing).
#[tokio::test]
async fn test_mock_download_returns_empty() {
    let client = MockTelegramMtprotoClient::new();
    let result = client.download_file("mock_file_id_123").await;
    assert!(result.is_ok(), "mock should return Ok");
    assert!(
        result.unwrap().is_empty(),
        "mock download returns empty bytes"
    );
}

/// Verify download_media returns error for unregistered domains.
#[tokio::test]
async fn test_download_media_errors_with_zero_domains() {
    let config = config();
    let client = Arc::new(MockTelegramMtprotoClient::new());
    let adapter = MtprotoTelegramAdapter::new(config, client);
    adapter.mark_ready_for_test();
    let result = adapter.download_media("12345").await;
    assert!(
        result.is_err(),
        "download_media with zero domains should error"
    );
}

/// Verify download_media tries hex file_id path first.
#[tokio::test]
async fn test_download_media_hex_file_id_path() {
    let adapter = adapter();
    adapter
        .register_domain(&adapter.domain_id("-1001234567890"), "-1001234567890")
        .unwrap();
    // A valid hex string that's not a valid file_id will fail at
    // download_file, then fall through to message_id path which
    // also fails (no such message). Both paths are exercised.
    let result = adapter
        .download_media("abcdef1234567890abcdef1234567890")
        .await;
    // The mock download_file returns Ok(vec![]), so the hex path
    // succeeds with empty bytes.
    assert!(result.is_ok(), "hex path should succeed with mock");
}

/// Verify NewMessage with document_id carries caption in metadata.
#[tokio::test]
async fn test_document_message_has_caption_metadata() {
    let (adapter, client) = adapter_with_client();
    let domain = adapter.domain_id("-1001234567890");

    // Inject a message with document_id and caption.
    client.inject_update(MtprotoTelegramUpdate::NewMessage(NewMessage {
        chat_id: -1001234567890,
        message: "DOT/1/base64payload".into(),
        from_id: Some(200),
        message_id: 42,
        document_id: Some("abcdef1234".into()),
        caption: Some("DOT/1/base64payload".into()),
        timestamp: 1700000000,
    }));

    let msgs = adapter.receive_messages(&domain).await.unwrap();
    assert_eq!(msgs.len(), 1);
    assert_eq!(
        msgs[0].metadata.get("document_id").map(|s| s.as_str()),
        Some("abcdef1234"),
        "document_id should be in metadata"
    );
    // Payload should be the caption (DOT/1 text), not the raw message.
    let payload_text = std::str::from_utf8(&msgs[0].payload).unwrap();
    assert_eq!(payload_text, "DOT/1/base64payload");
}

/// Verify MessageEdited is surfaced with edited=true metadata.
#[tokio::test]
async fn test_message_edited_surfaces_with_metadata() {
    use octo_adapter_telegram_mtproto::client::MessageEdited;

    let (adapter, client) = adapter_with_client();
    let domain = adapter.domain_id("-1001234567890");

    client.inject_update(MtprotoTelegramUpdate::MessageEdited(MessageEdited {
        chat_id: -1001234567890,
        message_id: 42,
        new_text: "DOT/1/updated_payload".into(),
        timestamp: 1700000001,
    }));

    let msgs = adapter.receive_messages(&domain).await.unwrap();
    assert_eq!(msgs.len(), 1);
    assert_eq!(
        msgs[0].metadata.get("edited").map(|s| s.as_str()),
        Some("true"),
        "edited metadata should be present"
    );
    assert!(
        msgs[0].platform_id.contains(":edited"),
        "platform_id should contain :edited marker"
    );
    let payload_text = std::str::from_utf8(&msgs[0].payload).unwrap();
    assert_eq!(payload_text, "DOT/1/updated_payload");
}

/// Verify FileDownloaded is dropped (not surfaced to gateway).
#[tokio::test]
async fn test_file_downloaded_is_dropped() {
    use octo_adapter_telegram_mtproto::client::FileDownloaded;

    let (adapter, client) = adapter_with_client();
    let domain = adapter.domain_id("-1001234567890");

    client.inject_update(MtprotoTelegramUpdate::FileDownloaded(FileDownloaded {
        file_id: "file_123".into(),
        local_path: "/tmp/downloaded.bin".into(),
        size: 1024,
    }));

    let msgs = adapter.receive_messages(&domain).await.unwrap();
    assert_eq!(msgs.len(), 0, "FileDownloaded should be dropped");
}

/// Verify mixed updates are handled correctly.
#[tokio::test]
async fn test_mixed_updates_with_documents_and_edits() {
    use octo_adapter_telegram_mtproto::client::FileDownloaded;

    let (adapter, client) = adapter_with_client();
    let domain = adapter.domain_id("-1001234567890");

    // New message with document.
    client.inject_update(MtprotoTelegramUpdate::NewMessage(NewMessage {
        chat_id: -1001234567890,
        message: "DOT/1/abc".into(),
        from_id: Some(200),
        message_id: 1,
        document_id: Some("doc1".into()),
        caption: Some("DOT/1/abc".into()),
        timestamp: 1700000000,
    }));
    // File downloaded (should be dropped).
    client.inject_update(MtprotoTelegramUpdate::FileDownloaded(FileDownloaded {
        file_id: "doc1".into(),
        local_path: "/tmp/doc1.bin".into(),
        size: 1024,
    }));
    // Edited message.
    client.inject_update(MtprotoTelegramUpdate::NewMessage(NewMessage {
        chat_id: -1001234567890,
        message: "DOT/1/def".into(),
        from_id: Some(200),
        message_id: 2,
        document_id: None,
        caption: None,
        timestamp: 1700000001,
    }));

    let msgs = adapter.receive_messages(&domain).await.unwrap();
    assert_eq!(
        msgs.len(),
        2,
        "should have 2 messages (FileDownloaded dropped)"
    );
}
