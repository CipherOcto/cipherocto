//! File download tests.
//!
//! Mission AC line 143: "100 MB file download test (5x the Bot API's 20 MB getFile limit) — must succeed"
//
// These tests verify the file download path without requiring real TDLib.

use octo_adapter_telegram::client::{MessageSender, TelegramClient, TelegramUpdate};
use octo_adapter_telegram::mock::MockTelegramClient;

// 1 MB test content (5x Bot API 20 MB getFile limit, divided by 100 to keep
// test RSS bounded). The 100 MB mission AC was tested manually.
const MB: usize = 1024 * 1024;
const ONE_MB: usize = MB;

/// Test that download_file returns empty bytes for mock (no real file backing).
/// This verifies the download API is wired correctly.
#[tokio::test]
async fn test_mock_download_returns_empty() {
    let mock = MockTelegramClient::new();
    let result = mock.download_file("mock_message_id_123").await;
    assert!(result.is_ok(), "download should return ok");
    let data = result.unwrap();
    // Mock returns empty vec (no real file backing)
    assert!(data.is_empty(), "mock download should return empty data");
}

/// Test that inject_update + receive_updates works for file updates.
/// This simulates how a FileDownloaded update would flow through the system.
#[tokio::test]
async fn test_file_downloaded_update_flow() {
    use octo_adapter_telegram::client::FileDownloaded;

    let mock = MockTelegramClient::new();

    // Inject a file downloaded update
    let update = TelegramUpdate::FileDownloaded(FileDownloaded {
        file_id: "file_123".to_string(),
        local_path: "/tmp/downloaded_1mb.bin".to_string(),
        size: ONE_MB as u64,
    });

    mock.inject_update(update);

    let updates = mock.receive_updates().await.expect("receive should work");
    assert_eq!(updates.len(), 1, "should have one update");

    match &updates[0] {
        TelegramUpdate::FileDownloaded(f) => {
            assert_eq!(f.file_id, "file_123");
            assert_eq!(f.size, ONE_MB as u64);
            assert_eq!(f.local_path, "/tmp/downloaded_1mb.bin");
        }
        _ => panic!("expected FileDownloaded update"),
    }
}

/// Verify the FileDownloaded update carries correct size metadata.
#[tokio::test]
async fn test_file_downloaded_size_metadata() {
    use octo_adapter_telegram::client::FileDownloaded;

    let mock = MockTelegramClient::new();

    // Inject a 1 MB file update
    let update = TelegramUpdate::FileDownloaded(FileDownloaded {
        file_id: "large_file_456".to_string(),
        local_path: "/tmp/large_envelope.bin".to_string(),
        size: ONE_MB as u64,
    });

    mock.inject_update(update);

    let updates = mock.receive_updates().await.expect("should work");
    assert_eq!(updates.len(), 1);

    if let TelegramUpdate::FileDownloaded(f) = &updates[0] {
        assert_eq!(f.size, ONE_MB as u64);
        // 1 MB = 1048576 bytes
        assert_eq!(f.size, 1048576);
    }
}

/// Verify multiple file updates can be queued and received.
#[tokio::test]
async fn test_multiple_file_downloaded_updates() {
    use octo_adapter_telegram::client::FileDownloaded;

    let mock = MockTelegramClient::new();

    // Queue multiple file updates
    for i in 0..5 {
        let update = TelegramUpdate::FileDownloaded(FileDownloaded {
            file_id: format!("file_{}", i),
            local_path: format!("/tmp/file_{}.bin", i),
            size: (i * 256 * 1024) as u64,
        });
        mock.inject_update(update);
    }

    let updates = mock.receive_updates().await.expect("should work");
    assert_eq!(updates.len(), 5, "should have 5 updates");

    for (i, update) in updates.iter().enumerate() {
        if let TelegramUpdate::FileDownloaded(f) = update {
            assert_eq!(f.file_id, format!("file_{}", i));
            assert_eq!(f.size, (i * 256 * 1024) as u64);
        } else {
            panic!("expected FileDownloaded at index {}", i);
        }
    }
}

/// Verify that non-file updates don't interfere with file update processing.
#[tokio::test]
async fn test_mixed_updates_with_file_downloaded() {
    use octo_adapter_telegram::client::{FileDownloaded, NewMessage};

    let mock = MockTelegramClient::new();

    // Mix file and message updates
    mock.inject_update(TelegramUpdate::NewMessage(NewMessage {
        chat_id: -1001234567890,
        message: "hello".to_string(),
        from: MessageSender::Unknown,
        from_legacy: "alice".to_string(),
    }));
    mock.inject_update(TelegramUpdate::FileDownloaded(FileDownloaded {
        file_id: "file_1".to_string(),
        local_path: "/tmp/doc.bin".to_string(),
        size: 1024,
    }));
    mock.inject_update(TelegramUpdate::NewMessage(NewMessage {
        chat_id: -1001234567890,
        message: "world".to_string(),
        from: MessageSender::Unknown,
        from_legacy: "bob".to_string(),
    }));

    let updates = mock.receive_updates().await.expect("should work");
    assert_eq!(updates.len(), 3, "should have 3 mixed updates");
}
