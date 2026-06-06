//! Mock TDLib client tests.
//! Mission AC line 143: "Unit tests use a mock TDLib client (no real TDLib instance required for cargo test)"

use octo_adapter_telegram::client::{NewMessage, TelegramClient, TelegramUpdate};
use octo_adapter_telegram::mock::MockTelegramClient;

#[tokio::test]
async fn test_mock_client_send_message_returns_id() {
    let mock = MockTelegramClient::new();
    let result = mock.send_message("-1001234567890", "hello").await;
    assert!(result.is_ok());
    let id = result.unwrap();
    assert!(!id.is_empty(), "message id should not be empty");
}

#[tokio::test]
async fn test_mock_client_receive_empty() {
    let mut mock = MockTelegramClient::new();
    let updates = mock.receive_updates().await.unwrap();
    assert!(updates.is_empty(), "fresh mock has no updates");
}

#[tokio::test]
async fn test_mock_client_inject_update() {
    let mut mock = MockTelegramClient::new();
    mock.inject_update(TelegramUpdate::NewMessage(NewMessage {
        chat_id: -1001234567890,
        message: "test".to_string(),
        from: "alice".to_string(),
    }));
    let updates = mock.receive_updates().await.unwrap();
    assert_eq!(updates.len(), 1);
}
