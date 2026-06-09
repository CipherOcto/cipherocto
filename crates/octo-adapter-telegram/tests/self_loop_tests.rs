//! Self-loop prevention tests.
//!
//! Mission AC line 140: "Self-loop prevention: `self_handle()` returns the
//! bot's user_id (or user_id for user mode) to drop self-authored messages"
//
// These tests verify that messages from the bot itself are correctly identified
//! and can be filtered out.

use octo_adapter_telegram::client::{NewMessage, TelegramClient, TelegramUpdate};
use octo_adapter_telegram::mock::MockTelegramClient;
use octo_adapter_telegram::self_handle::{SelfHandle, SelfIdentity};

/// Verify SelfHandle starts with no cached identity.
#[tokio::test]
async fn test_self_handle_starts_empty() {
    let handle = SelfHandle::new();
    assert_eq!(
        handle.get(),
        None,
        "new SelfHandle should have no cached identity"
    );
}

/// Verify SelfHandle can cache an identity.
#[tokio::test]
async fn test_self_handle_set_and_get() {
    let handle = SelfHandle::new();
    handle.set_user_id(42);
    handle.set_username("test_bot".to_string());
    assert_eq!(
        handle.get(),
        Some(SelfIdentity {
            user_id: 42,
            username: "test_bot".to_string()
        })
    );
}

/// Verify SelfHandle can clear the cached identity.
#[tokio::test]
async fn test_self_handle_clear() {
    let handle = SelfHandle::new();
    handle.set_username("test_bot".to_string());
    assert_eq!(handle.get().unwrap().username, "test_bot");
    handle.clear();
    assert_eq!(handle.get(), None);
}

/// Verify SelfHandle can update the cached username.
#[tokio::test]
async fn test_self_handle_update_username() {
    let handle = SelfHandle::new();
    handle.set_user_id(1);
    handle.set_username("first_bot".to_string());
    handle.set_username("updated_bot".to_string());
    assert_eq!(handle.get().unwrap().username, "updated_bot");
    assert_eq!(handle.get().unwrap().user_id, 1, "user_id preserved");
}

/// Verify set_identity sets both fields atomically.
#[tokio::test]
async fn test_self_handle_set_identity() {
    let handle = SelfHandle::new();
    handle.set_identity(987_654_321, "atomic_bot".to_string());
    assert_eq!(
        handle.get(),
        Some(SelfIdentity {
            user_id: 987_654_321,
            username: "atomic_bot".to_string()
        })
    );
}

/// Test self-loop prevention: inject a message from self, verify it can be filtered.
/// This simulates the scenario where the bot sends a message and should ignore it.
#[tokio::test]
async fn test_self_message_filtering_scenario() {
    let mock = MockTelegramClient::new();
    let self_handle = SelfHandle::new();

    // Set the bot's identity (simulating getMe result)
    self_handle.set_identity(123_456_789, "my_bot".to_string());
    let my_user_id = self_handle.user_id().unwrap();

    // Inject a message FROM the bot (self-authored)
    mock.inject_update(TelegramUpdate::NewMessage(NewMessage {
        chat_id: -1001234567890,
        message: "Hello from bot".to_string(),
        from: my_user_id.to_string(),
    }));

    // Inject a message FROM another user
    mock.inject_update(TelegramUpdate::NewMessage(NewMessage {
        chat_id: -1001234567890,
        message: "Hello from alice".to_string(),
        from: "999_999_999".to_string(),
    }));

    let updates = mock.receive_updates().await.unwrap();
    assert_eq!(updates.len(), 2, "should have 2 updates");

    // Filter out self-authored messages by user_id (H4: numeric comparison).
    let my_id = self_handle.user_id().unwrap();
    let filtered: Vec<_> = updates
        .iter()
        .filter(|u| {
            if let TelegramUpdate::NewMessage(msg) = u {
                msg.from
                    .parse::<i64>()
                    .map(|id| id != my_id)
                    .unwrap_or(true)
            } else {
                true
            }
        })
        .collect();

    assert_eq!(filtered.len(), 1, "should filter out 1 self message");
    if let TelegramUpdate::NewMessage(msg) = &filtered[0] {
        assert_eq!(msg.from, "999_999_999");
        assert_eq!(msg.message, "Hello from alice");
    }
}

/// Test self-loop prevention: is_self is the canonical helper.
#[tokio::test]
async fn test_is_self_helper() {
    let handle = SelfHandle::new();
    handle.set_user_id(42);
    assert!(handle.is_self(42));
    assert!(!handle.is_self(43));
    assert!(!handle.is_self(0));

    // Empty handle: is_self is always false (does not suppress any messages).
    let empty = SelfHandle::new();
    assert!(!empty.is_self(42));
}

/// Test self-loop prevention with explicit numeric user_id format.
#[tokio::test]
async fn test_self_message_filtering_numeric_user_id() {
    let mock = MockTelegramClient::new();
    let self_handle = SelfHandle::new();

    // Bot's numeric user_id
    self_handle.set_user_id(123_456_789);
    let my_user_id = self_handle.user_id().unwrap();

    // Message from self
    mock.inject_update(TelegramUpdate::NewMessage(NewMessage {
        chat_id: -1001234567890,
        message: "My own message".to_string(),
        from: my_user_id.to_string(),
    }));

    // Message from other user
    mock.inject_update(TelegramUpdate::NewMessage(NewMessage {
        chat_id: -1001234567890,
        message: "Other message".to_string(),
        from: "987654321".to_string(),
    }));

    let updates = mock.receive_updates().await;

    // Filter self using is_self (the canonical helper).
    let filtered: Vec<_> = updates
        .unwrap()
        .into_iter()
        .filter(|u| {
            if let TelegramUpdate::NewMessage(msg) = u {
                msg.from
                    .parse::<i64>()
                    .map(|id| !self_handle.is_self(id))
                    .unwrap_or(true)
            } else {
                true
            }
        })
        .collect();

    assert_eq!(filtered.len(), 1);
    if let TelegramUpdate::NewMessage(msg) = &filtered[0] {
        assert_eq!(msg.from, "987654321");
    }
}

/// Test that SelfHandle is thread-safe via Arc concurrency.
#[tokio::test]
async fn test_self_handle_thread_safety() {
    use std::sync::Arc;

    let handle = Arc::new(SelfHandle::new());
    let handle_clone = handle.clone();

    // Set identity on handle_clone
    handle_clone.set_identity(7, "concurrent_bot".to_string());

    // Get from original handle (same thread, different Arc)
    let result = handle.get();
    assert_eq!(
        result,
        Some(SelfIdentity {
            user_id: 7,
            username: "concurrent_bot".to_string()
        })
    );
}

/// Test filtering with empty self_handle (not yet fetched).
#[tokio::test]
async fn test_filtering_with_empty_self_handle() {
    let mock = MockTelegramClient::new();
    let self_handle = SelfHandle::new(); // No identity set

    mock.inject_update(TelegramUpdate::NewMessage(NewMessage {
        chat_id: -1001234567890,
        message: "Any message".to_string(),
        from: "12345".to_string(),
    }));

    let updates = mock.receive_updates().await.unwrap();

    // With no self_handle set, no messages should be filtered as self.
    let filtered: Vec<_> = updates
        .into_iter()
        .filter(|u| {
            if let TelegramUpdate::NewMessage(msg) = u {
                msg.from
                    .parse::<i64>()
                    .map(|id| !self_handle.is_self(id))
                    .unwrap_or(true)
            } else {
                true
            }
        })
        .collect();

    // All messages pass through when self_handle is empty
    assert_eq!(filtered.len(), 1);
}

/// Test Default implementation for SelfHandle.
#[tokio::test]
async fn test_self_handle_default() {
    let handle = SelfHandle::default();
    assert_eq!(handle.get(), None);
}

/// H5: When the bot sends a document, the mock re-injects a doc-derived
/// `NewMessage` on `receive_updates`. The adapter's self-loop filter should
/// drop that message if the `from` field matches the bot's `self_user_id`.
/// Previously, the mock injected `from: String::new()`, which the parser
/// rejected, so the filter never fired for document round-trips.
#[tokio::test]
async fn test_document_self_loop_is_filtered() {
    use octo_adapter_telegram::{TelegramAdapter, TelegramConfig};
    use octo_network::dot::adapters::PlatformAdapter;
    use octo_network::dot::domain::BroadcastDomainId;

    let config = TelegramConfig::default();
    let mock = MockTelegramClient::new();
    let adapter = TelegramAdapter::new(config, mock.clone());

    // Tell the mock to stamp the bot's own user_id onto doc-injected NewMessages.
    let bot_user_id = 42_i64;
    mock.set_mock_sender(bot_user_id);

    // Register a domain and set the bot's user_id on the adapter.
    let chat_id_str = "-1001234567890".to_string();
    let domain: BroadcastDomainId = adapter.domain_id(&chat_id_str);
    adapter.set_self_user_id(bot_user_id);

    // Bot sends a document — this enqueues a doc-derived NewMessage.
    mock.send_file(&chat_id_str, "x.bin", b"hello")
        .await
        .unwrap();

    // The adapter's receive_messages pulls updates from the mock and applies
    // the self-loop filter. The doc-injected NewMessage has `from = "42"`,
    // matching the bot's self_user_id, so it must be dropped.
    let received: Vec<_> = adapter.receive_messages(&domain).await.unwrap();
    assert_eq!(
        received.len(),
        0,
        "self-loop filter must drop doc-injected message when from matches self_user_id"
    );

    // Sanity check: change the mock sender to a different id and verify
    // the doc-injected message now survives (so the test is actually
    // exercising the filter, not just the absence of any message).
    mock.set_mock_sender(99);
    // Drain the prior doc so the receive path doesn't re-inject it again.
    mock.drain_received_documents();
    mock.send_file(&chat_id_str, "y.bin", b"world")
        .await
        .unwrap();
    let received2: Vec<_> = adapter.receive_messages(&domain).await.unwrap();
    assert_eq!(
        received2.len(),
        1,
        "doc-injected message should survive when from != self_user_id"
    );
    // The receive path carries the base64-encoded envelope as the
    // message body. Decoding it should round-trip back to "world".
    let payload_str = std::str::from_utf8(&received2[0].payload).unwrap();
    let decoded = octo_adapter_telegram::envelope::decode_envelope(payload_str).unwrap();
    assert_eq!(decoded, b"world");
}

/// H8: verify the adapter shares the `SelfHandle` with the underlying client
/// via the `with_self_handle` constructor. The shared handle's `user_id`
/// must drive the adapter's self-loop filter exactly as a local
/// `set_self_user_id` would. This is the production path: the real client
/// populates its own `SelfHandle` from `get_me`, and the adapter is wired
/// to that same instance.
#[tokio::test]
async fn test_adapter_with_shared_self_handle() {
    use octo_adapter_telegram::{TelegramAdapter, TelegramConfig};
    use octo_network::dot::adapters::PlatformAdapter;
    use octo_network::dot::domain::BroadcastDomainId;

    let config = TelegramConfig::default();
    let client = MockTelegramClient::new();

    // Build a self_handle and stamp the bot's user_id on it BEFORE
    // constructing the adapter. In production, the real client populates
    // this from `get_me` and the gateway hands the same handle to the
    // adapter.
    let handle = SelfHandle::new();
    handle.set_user_id(42);
    assert_eq!(handle.user_id(), Some(42));

    let adapter = TelegramAdapter::with_self_handle(config, client.clone(), handle.clone());

    // Clone the handle into a second owner to prove the Arc semantics —
    // a mutation through one owner must be visible to the adapter.
    let handle2 = handle.clone();
    assert_eq!(handle2.user_id(), Some(42));
    assert_eq!(handle.user_id(), Some(42));

    // Register the domain on the adapter.
    let chat_id_str = "-1001234567890".to_string();
    let domain: BroadcastDomainId = adapter.domain_id(&chat_id_str);

    // Stamp the same user_id onto the mock so doc-derived NewMessages
    // get a `from` that the shared handle can match.
    client.set_mock_sender(42);

    // Bot sends a document — this enqueues a doc-derived NewMessage
    // with `from = "42"`.
    client
        .send_file(&chat_id_str, "x.bin", b"hello")
        .await
        .unwrap();

    // Pull updates from the mock and apply the adapter's self-loop filter.
    // The doc-injected NewMessage (from=42) must be dropped because the
    // shared SelfHandle knows self_user_id=42.
    let received: Vec<_> = adapter.receive_messages(&domain).await.unwrap();
    assert_eq!(
        received.len(),
        0,
        "shared SelfHandle must drop doc-injected message when from matches"
    );

    // Sanity check: a different self_user_id (or none) lets the doc
    // message through. We use a NEW shared handle with a different id
    // to confirm the filter is reading from the shared instance, not
    // from a stale copy.
    let handle3 = SelfHandle::new();
    let adapter3 =
        TelegramAdapter::with_self_handle(TelegramConfig::default(), client.clone(), handle3);
    let domain3: BroadcastDomainId = adapter3.domain_id(&chat_id_str);
    // Drain the prior doc so the receive path does not re-inject it
    // again alongside the new one.
    client.drain_received_documents();
    client
        .send_file(&chat_id_str, "y.bin", b"world")
        .await
        .unwrap();
    let received2 = adapter3.receive_messages(&domain3).await.unwrap();
    assert_eq!(
        received2.len(),
        1,
        "doc-injected message should survive when shared SelfHandle has no self id"
    );
    // The receive path carries the base64-encoded envelope as the
    // message body. Decoding it should round-trip back to "world".
    let payload_str = std::str::from_utf8(&received2[0].payload).unwrap();
    let decoded = octo_adapter_telegram::envelope::decode_envelope(payload_str).unwrap();
    assert_eq!(decoded, b"world");

    // Verify the handle is independently cloneable across threads and
    // mutations from one clone are visible to the adapter reading through
    // a separate clone.
    let handle4 = SelfHandle::new();
    let adapter4 = TelegramAdapter::with_self_handle(
        TelegramConfig::default(),
        client.clone(),
        handle4.clone(),
    );
    let domain4: BroadcastDomainId = adapter4.domain_id(&chat_id_str);
    client.drain_received_documents();
    client
        .send_file(&chat_id_str, "z.bin", b"alpha")
        .await
        .unwrap();
    // No identity on handle4 yet — message should pass through.
    let pre = adapter4.receive_messages(&domain4).await.unwrap();
    assert_eq!(pre.len(), 1, "no-self-id handle should not filter");

    // Set the identity through a separate clone of the same handle.
    handle4.set_user_id(42);
    client.drain_received_documents();
    client
        .send_file(&chat_id_str, "w.bin", b"beta")
        .await
        .unwrap();
    // The adapter's handle4 is a clone — it should see the new identity
    // and drop the doc message.
    let post = adapter4.receive_messages(&domain4).await.unwrap();
    assert_eq!(
        post.len(),
        0,
        "mutation on a separate clone of the shared SelfHandle must be visible to the adapter"
    );
}

/// Test adapter-level self-loop filtering (H5). Uses the live TelegramAdapter.
#[tokio::test]
async fn test_adapter_filters_self_messages() {
    use octo_adapter_telegram::{TelegramAdapter, TelegramConfig};
    use octo_network::dot::adapters::PlatformAdapter;
    use octo_network::dot::domain::BroadcastDomainId;

    let config = TelegramConfig::default();
    let mock = MockTelegramClient::new();
    let adapter = TelegramAdapter::new(config, mock.clone());

    // Register a domain and set the bot's user_id.
    let chat_id_str = "-1001234567890".to_string();
    let domain: BroadcastDomainId = adapter.domain_id(&chat_id_str);
    adapter.set_self_user_id(111_111_111);

    // Inject a self-authored message (from == cached user_id).
    mock.inject_update(TelegramUpdate::NewMessage(NewMessage {
        chat_id: -1001234567890,
        message: "from self".to_string(),
        from: "111111111".to_string(),
    }));
    // Inject a message from a different user.
    mock.inject_update(TelegramUpdate::NewMessage(NewMessage {
        chat_id: -1001234567890,
        message: "from other".to_string(),
        from: "222222222".to_string(),
    }));

    let received: Vec<_> = adapter.receive_messages(&domain).await.unwrap();
    // H5: the self message must be filtered out by the adapter.
    assert_eq!(
        received.len(),
        1,
        "self-loop filter should drop self message"
    );
    assert_eq!(received[0].payload, b"from other");
}
