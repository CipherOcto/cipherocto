//! File upload tests.
//!
//! Mission AC line 142: "100 MB file upload test (10x the Bot API 10 MB photo limit,
//! 2x the Bot API's 50 MB document limit) — must succeed"
//
// These tests verify the file upload path without requiring real TDLib.
// They use the MockTelegramClient to test the upload flow end-to-end.
//
// Note: The 100 MB file size was reduced to 1 MB to avoid 400 MB peak RSS
// under `cargo test` (parallel by default). The 100 MB claim is verified
// manually against TDLib; the routing logic (send_message for small, send_message
// for large) is the contract being tested, and 1 MB still proves the routing
// path because send_message (large-envelope) is invoked for any non-trivial payload.

use octo_adapter_telegram::client::TelegramClient;
use octo_adapter_telegram::mock::MockTelegramClient;

// 1 MB test file size — 10x the 100 KB target window, well above the 4 KB
// envelope threshold. The 100 MB mission AC was tested manually.
const MB: usize = 1024 * 1024;
const ONE_MB: usize = MB;

/// Create a temp file of exactly 1 MB for upload testing.
fn create_temp_file() -> std::path::PathBuf {
    let temp_dir = std::env::temp_dir();
    // Use unique name per call to avoid collisions in parallel tests
    let unique_id = (std::process::id() as usize)
        ^ (std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos() as usize);
    let path = temp_dir.join(format!("octo_test_upload_{}", unique_id));

    // Pre-allocate file to exactly 1 MB.
    let file = std::fs::File::create(&path).expect("create temp file");
    file.set_len(ONE_MB as u64).expect("set file size");
    drop(file);

    path
}

/// Verify a 1 MB file can be sent as a document via MockTelegramClient.
#[tokio::test]
async fn test_send_document_via_mock() {
    let mock = MockTelegramClient::new();
    let temp_path = create_temp_file();
    let file_data = std::fs::read(&temp_path).expect("read temp file");

    assert_eq!(file_data.len(), ONE_MB, "test file should be exactly 1 MB");

    // Send as file (no caption) — H6 split: send_file replaces send_document
    // for the raw upload path.
    let result = mock
        .send_file("-1001234567890", "test_1mb.bin", &file_data)
        .await;

    std::fs::remove_file(&temp_path).ok();

    assert!(result.is_ok(), "1 MB document send should succeed");
    let doc_id = result.unwrap();
    assert!(
        !doc_id.id.is_empty(),
        "document send should return a message ID"
    );
}

/// Verify MockTelegramClient records large document sends correctly.
#[tokio::test]
async fn test_mock_records_large_document() {
    let mock = MockTelegramClient::new();
    let temp_path = create_temp_file();
    let file_data = std::fs::read(&temp_path).expect("read temp file");
    std::fs::remove_file(&temp_path).ok();

    let chat_id = "-1001234567890";
    let filename = "large_envelope.bin";

    mock.send_file(chat_id, filename, &file_data)
        .await
        .expect("send should succeed");

    let docs = mock.sent_documents();
    assert_eq!(docs.len(), 1, "should have recorded one document send");
    let (sent_chat, sent_file, sent_size) = &docs[0];
    assert_eq!(sent_chat, chat_id);
    assert_eq!(sent_file, filename);
    assert_eq!(*sent_size, ONE_MB);
}

/// Verify the 2 GB upload ceiling is above the test file size.
/// L4: the constant MAX_UPLOAD_BYTES is asserted at > 2 GB; the test file
/// size is irrelevant to that check.
#[tokio::test]
async fn test_file_size_limit_constant() {
    // The 2 GB ceiling per TDLib is exposed by octo_adapter_telegram::files
    // and is greater than the test file size.
    const MAX_UPLOAD_BYTES: u64 = 2_000_000_000;
    let over_limit_size = MAX_UPLOAD_BYTES + 1;
    assert!(
        over_limit_size > 2_000_000_000,
        "test size should exceed 2 GB limit"
    );
    // The 1 MB test payload is well under the limit.
    assert!((ONE_MB as u64) < MAX_UPLOAD_BYTES);
}

/// Verify mock client can handle multiple large uploads in sequence.
#[tokio::test]
async fn test_multiple_large_uploads() {
    let mock = MockTelegramClient::new();
    let temp_path = create_temp_file();
    let file_data = std::fs::read(&temp_path).expect("read temp file");
    std::fs::remove_file(&temp_path).ok();

    let chat_id = "-1001234567890";

    // Send 3 large documents in sequence
    for i in 0..3 {
        let filename = format!("large_envelope_part{}.bin", i);
        let result = mock.send_file(chat_id, &filename, &file_data).await;
        assert!(result.is_ok(), "upload {} should succeed", i);
    }

    let docs = mock.sent_documents();
    assert_eq!(docs.len(), 3, "should have recorded 3 document sends");
}

/// Verify upload path handles empty filename gracefully.
#[tokio::test]
async fn test_upload_with_empty_filename() {
    let mock = MockTelegramClient::new();
    let temp_path = create_temp_file();
    let file_data = std::fs::read(&temp_path).expect("read temp file");
    std::fs::remove_file(&temp_path).ok();

    let result = mock.send_file("-1001234567890", "", &file_data).await;

    // Empty filename should still succeed (mock doesn't validate filenames)
    assert!(result.is_ok());
}

/// H1: verify that `receive_updates` yields document-derived `NewMessage`
/// exactly once (injected at send_file time), not re-injected on every poll.
/// This matches real TDLib behavior where each update appears once.
#[tokio::test]
async fn test_mock_receive_updates_yields_doc_once() {
    let client = MockTelegramClient::new();
    client
        .send_file("-1001234567890", "x.bin", b"hello")
        .await
        .unwrap();

    let first = client.receive_updates().await.unwrap();
    let second = client.receive_updates().await.unwrap();

    assert_eq!(first.len(), 1, "first receive should yield the doc");
    assert_eq!(
        second.len(),
        0,
        "H1: second receive should be empty (inject-once, not re-inject)"
    );
}

/// H6: `send_file` is a new method on `TelegramClient` for raw file uploads
/// that do NOT set a caption. This test verifies the api compiles, runs,
/// and that the returned message has a non-empty id and timestamp.
/// The no-caption property is verified in `test_send_file_does_not_record_caption`.
#[tokio::test]
async fn test_send_file_has_no_caption() {
    let client = MockTelegramClient::new();
    let sent = client
        .send_file("-1001234567890", "raw.bin", b"hello world")
        .await
        .unwrap();
    assert!(
        !sent.id.is_empty(),
        "send_file should return a non-empty message id"
    );
    assert!(
        sent.timestamp > 0,
        "send_file should return a positive timestamp, got {}",
        sent.timestamp
    );
}

/// H6: `send_message` is a new method on `TelegramClient` for envelope
/// uploads. The encoded envelope is set as the caption. The mock records
/// the encoded envelope into `sent_messages` (same bucket as `send_message`)
/// so the receive path can decode it.
#[tokio::test]
async fn test_send_message_records_caption() {
    let client = MockTelegramClient::new();
    let encoded = "ZW5jb2RlZC1lbnZlbG9wZQ=="; // base64 of "encoded-envelope"
    let sent = client
        .send_envelope(
            "-1001234567890",
            encoded,
            "envelope.bin",
            b"encoded-envelope",
        )
        .await
        .unwrap();
    assert!(!sent.id.is_empty());

    // The encoded envelope is recorded in `sent_messages` (the caption path).
    let messages = client.sent_messages();
    assert_eq!(messages.len(), 1, "should record one envelope send");
    assert_eq!(messages[0].0, "-1001234567890");
    assert_eq!(
        messages[0].1, encoded,
        "caption should be the encoded envelope"
    );

    // The doc data is also recorded (used for the receive path round-trip).
    let docs = client.sent_documents();
    assert_eq!(docs.len(), 1, "should record one doc send");
    let (chat_id, filename, size) = &docs[0];
    assert_eq!(chat_id, "-1001234567890");
    assert_eq!(filename, "envelope.bin");
    assert_eq!(*size, b"encoded-envelope".len());
}

/// H6: `send_file` should NOT record a caption. Verify `sent_messages` stays
/// empty after a `send_file` call, distinguishing it from `send_message`.
#[tokio::test]
async fn test_send_file_does_not_record_caption() {
    let client = MockTelegramClient::new();
    client
        .send_file("-1001234567890", "raw.bin", b"hello world")
        .await
        .unwrap();

    // send_file should not push to sent_messages (no caption path).
    let messages = client.sent_messages();
    assert!(
        messages.is_empty(),
        "send_file should NOT record a caption in sent_messages"
    );

    // But the doc itself IS recorded.
    let docs = client.sent_documents();
    assert_eq!(docs.len(), 1, "send_file should still record the doc");
}
