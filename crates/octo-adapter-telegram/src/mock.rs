//! Mock Telegram client for tests.
//! Mission AC line 143: "Unit tests use a mock TDLib client (no real TDLib instance required for cargo test)"

use crate::client::{NewMessage, SentMessage, TelegramClient, TelegramUpdate};
use crate::error::Result;
use async_trait::async_trait;
use std::collections::BTreeMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

/// Type alias for the sent-document data map (chat_id, filename) → bytes.
type DocDataMap = BTreeMap<(String, String), Vec<u8>>;

/// In-memory mock that records sends and queues injected updates.
#[derive(Clone)]
pub struct MockTelegramClient {
    /// API-H3: queue of auth states for authenticate() to process.
    auth_queue: Arc<Mutex<std::collections::VecDeque<crate::auth::AuthStateKey>>>,
    sent_messages: Arc<Mutex<Vec<(String, String)>>>,
    sent_documents: Arc<Mutex<Vec<(String, String, usize)>>>,
    /// Tracks data sent via send_envelope/send_file, keyed by (chat_id, filename).
    /// Used to inject NewMessage updates for the document receive path.
    /// Drained only via `drain_received_documents()`; `receive_updates`
    /// re-injects on every call so callers can re-poll until they choose
    /// to drain (H4 fix — matches at-least-once semantics of receive loops).
    ///
    /// H6: prior to the H6 split, this was named `sent_doc_data` and only
    /// fed by `send_document`. After the split, both `send_envelope` and
    /// `send_file` populate it (the doc round-trip is the same in both
    /// cases — only the caption differs).
    sent_doc_data: Arc<Mutex<DocDataMap>>,
    pending_updates: Arc<Mutex<Vec<TelegramUpdate>>>,
    next_msg_id: Arc<AtomicU64>,
    /// Sender id stamped onto doc-derived `NewMessage.from` during
    /// `receive_updates`. When `0` (default), the field stays empty,
    /// matching the pre-H5 behavior. When set to a non-zero value, the
    /// mock uses that value as the `from` string so the adapter's
    /// self-loop filter (H5) is exercised for document round-trips.
    mock_sender_id: Arc<Mutex<i64>>,
    /// M6: failure injection for `send_message` and `send_envelope`. While
    /// the counter is non-zero, the next call decrements it and returns
    /// the configured error. When it reaches 0, the call returns `Ok`
    /// as normal. Used by tests to exercise `send_with_retry`'s retry
    /// paths for `RateLimited` and `Transient` errors.
    ///
    /// NOTE: Only `send_message` and `send_envelope` call this helper.
    /// `send_file` and `download_file` do NOT consume the failure counter
    /// — they bypass failure injection entirely. Test authors relying on
    /// `fail_next_n_sends` must ensure the code path only exercises
    /// `send_message`/`send_envelope` during the retry window.
    ///
    /// We store a `FailureSpec` enum (Clone-friendly) rather than a
    /// `TelegramError` directly, because `TelegramError` derives `Debug`
    /// and `thiserror::Error` but not `Clone` (its `Io` and `Json` payloads
    /// are not `Clone`). Each `FailureSpec` is reconstructed into the
    /// corresponding `TelegramError` on each call.
    fail_send_message: Arc<Mutex<Option<FailureSpec>>>,
    fail_send_message_remaining: Arc<Mutex<u32>>,
    /// M6: monotonically-increasing counter of every `send_message` /
    /// `send_envelope` / `send_file` call, success or failure-injected. Lets tests
    /// assert the retry loop re-invoked the operation.
    send_call_total: Arc<Mutex<u64>>,
}

/// M6: cloneable failure-injection spec. We can't store a `TelegramError`
/// directly because it is not `Clone` (its `Io`/`Json` variants embed
/// non-`Clone` payloads). Instead we store the spec and rebuild the error
/// on each injected call.
#[derive(Debug, Clone)]
pub enum FailureSpec {
    RateLimited { retry_after_secs: u64 },
    Transient(String),
}

impl FailureSpec {
    fn into_error(self) -> crate::error::TelegramError {
        match self {
            FailureSpec::RateLimited { retry_after_secs } => {
                crate::error::TelegramError::RateLimited { retry_after_secs }
            }
            FailureSpec::Transient(msg) => crate::error::TelegramError::Transient(msg),
        }
    }
}

impl MockTelegramClient {
    /// API-H3: set the auth state queue for authenticate() to step through.
    /// Add states in the order they should be processed (e.g., WaitCode, Ready).
    pub fn set_auth_queue(&self, states: Vec<crate::auth::AuthStateKey>) {
        *self.auth_queue.lock().unwrap() = states.into();
    }
    pub fn new() -> Self {
        Self {
            auth_queue: Arc::new(Mutex::new(std::collections::VecDeque::new())),
            sent_messages: Arc::new(Mutex::new(Vec::new())),
            sent_documents: Arc::new(Mutex::new(Vec::new())),
            sent_doc_data: Arc::new(Mutex::new(BTreeMap::new())),
            pending_updates: Arc::new(Mutex::new(Vec::new())),
            next_msg_id: Arc::new(AtomicU64::new(1)),
            mock_sender_id: Arc::new(Mutex::new(0)),
            fail_send_message: Arc::new(Mutex::new(None)),
            fail_send_message_remaining: Arc::new(Mutex::new(0)),
            send_call_total: Arc::new(Mutex::new(0)),
        }
    }

    /// M6: inject a failure for the next `n` `send_message` /
    /// `send_envelope` / `send_file` calls. Each call decrements `n`;
    /// once `n` reaches zero the mock returns `Ok` as normal. Used by
    /// tests to exercise the adapter's `with_retry` retry path.
    pub fn fail_next_n_sends(&self, n: u32, spec: FailureSpec) {
        *self.fail_send_message.lock().unwrap() = Some(spec);
        *self.fail_send_message_remaining.lock().unwrap() = n;
    }

    /// M6: total number of `send_message` / `send_envelope` calls so far,
    /// including failed-injection calls. Used by tests to assert the
    /// retry loop actually re-invokes the operation.
    pub fn send_call_count(&self) -> u64 {
        *self.send_call_total.lock().unwrap()
    }

    /// Inject an update that the next `receive_updates` call will yield.
    pub fn inject_update(&self, update: TelegramUpdate) {
        self.pending_updates.lock().unwrap().push(update);
    }

    /// Set the sender id used when re-injecting document-derived
    /// `NewMessage` updates. Pass `0` (the default) to keep the `from`
    /// field empty, matching the pre-H5 behavior. Pass a non-zero id to
    /// exercise the adapter's self-loop filter for document round-trips.
    pub fn set_mock_sender(&self, id: i64) {
        *self.mock_sender_id.lock().unwrap() = id;
    }

    pub fn sent_messages(&self) -> Vec<(String, String)> {
        self.sent_messages.lock().unwrap().clone()
    }

    pub fn sent_documents(&self) -> Vec<(String, String, usize)> {
        self.sent_documents.lock().unwrap().clone()
    }

    /// Drain the sent-doc map. After this call, subsequent `receive_updates`
    /// will not re-inject document-derived `NewMessage` updates.
    ///
    /// H4: previously `receive_updates` drained `sent_doc_data` on every call,
    /// which broke at-least-once semantics — a second poll missed the
    /// document round-trip. Callers now opt in to draining once they have
    /// observed the doc-derived message.
    pub fn drain_received_documents(&self) {
        self.sent_doc_data.lock().unwrap().clear();
    }

    /// M6: if a failure has been injected and there are still calls left
    /// to fail, decrement the counter and return the configured error.
    /// Otherwise return `None`. Bumps `send_call_total` on every call so
    /// tests can count attempts (success or failure-injected). Used by
    /// `send_message` / `send_envelope` to honor `fail_next_n_sends`.
    ///
    /// The spec is `clone`d (not `take`n) because the same `FailureSpec`
    /// must be returned for every injected call — the adapter's retry
    /// loop may invoke `op()` multiple times, and each one needs to see
    /// the same failure type.
    fn maybe_consume_failure_injection(&self) -> Option<crate::error::TelegramError> {
        *self.send_call_total.lock().unwrap() += 1;
        let mut remaining = self.fail_send_message_remaining.lock().unwrap();
        if *remaining == 0 {
            return None;
        }
        *remaining = remaining.saturating_sub(1);
        self.fail_send_message
            .lock()
            .unwrap()
            .clone()
            .map(FailureSpec::into_error)
    }
}

impl Default for MockTelegramClient {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl TelegramClient for MockTelegramClient {
    async fn send_message(&self, chat_id: &str, text: &str) -> Result<SentMessage> {
        // H7: validate chat_id via the shared helper so mock and real agree.
        // The mock previously accepted any string, which let tests pass on
        // mock and fail on real client.
        let _chat_id_i64: i64 = crate::client::parse_chat_id(chat_id).map_err(|e| {
            crate::error::TelegramError::InvalidChatId(format!("{}: {}", e, chat_id))
        })?;
        // M6: failure-injection. `fail_next_n_sends` decrements a counter on
        // every call; while it is non-zero, return the configured error so
        // the adapter's `send_with_retry` retry path can be exercised.
        if let Some(err) = self.maybe_consume_failure_injection() {
            return Err(err);
        }
        let id = format!(
            "mock-msg-{}",
            self.next_msg_id.fetch_add(1, Ordering::Relaxed)
        );
        self.sent_messages
            .lock()
            .unwrap()
            .push((chat_id.to_string(), text.to_string()));
        // Mock timestamp: use current Unix time
        let timestamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs() as i64)
            .unwrap_or(0);
        Ok(SentMessage::new(id, timestamp))
    }

    async fn send_envelope(
        &self,
        chat_id: &str,
        encoded_envelope: &str,
        filename: &str,
        data: &[u8],
    ) -> Result<SentMessage> {
        // H7: shared chat_id validation with real client.
        let _chat_id_i64: i64 = crate::client::parse_chat_id(chat_id).map_err(|e| {
            crate::error::TelegramError::InvalidChatId(format!("{}: {}", e, chat_id))
        })?;
        // M6: shared failure-injection with `send_message`.
        if let Some(err) = self.maybe_consume_failure_injection() {
            return Err(err);
        }
        // H6: send_envelope records the encoded envelope in `sent_messages`
        // (caption path) AND the doc in `sent_documents` (round-trip path).
        let id = format!(
            "mock-env-{}",
            self.next_msg_id.fetch_add(1, Ordering::Relaxed)
        );
        self.sent_messages
            .lock()
            .unwrap()
            .push((chat_id.to_string(), encoded_envelope.to_string()));
        self.sent_documents.lock().unwrap().push((
            chat_id.to_string(),
            filename.to_string(),
            data.len(),
        ));
        // Store data for receive-path injection (document envelope round-trip).
        self.sent_doc_data
            .lock()
            .unwrap()
            .insert((chat_id.to_string(), filename.to_string()), data.to_vec());
        let timestamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs() as i64)
            .unwrap_or(0);
        Ok(SentMessage::new(id, timestamp))
    }

    async fn send_file(&self, chat_id: &str, filename: &str, data: &[u8]) -> Result<SentMessage> {
        // H7: shared chat_id validation with real client.
        let _chat_id_i64: i64 = crate::client::parse_chat_id(chat_id).map_err(|e| {
            crate::error::TelegramError::InvalidChatId(format!("{}: {}", e, chat_id))
        })?;
        // M6: shared failure-injection with `send_message` / `send_envelope`.
        // NOTE: this also increments `send_call_total`.
        if let Some(err) = self.maybe_consume_failure_injection() {
            return Err(err);
        }
        // H6: send_file records the doc but NOT a caption (the raw upload
        // path has no envelope to round-trip via the caption channel).
        let id = format!(
            "mock-file-{}",
            self.next_msg_id.fetch_add(1, Ordering::Relaxed)
        );
        self.sent_documents.lock().unwrap().push((
            chat_id.to_string(),
            filename.to_string(),
            data.len(),
        ));
        // Store data for receive-path injection (document round-trip).
        self.sent_doc_data
            .lock()
            .unwrap()
            .insert((chat_id.to_string(), filename.to_string()), data.to_vec());
        let timestamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs() as i64)
            .unwrap_or(0);
        Ok(SentMessage::new(id, timestamp))
    }

    /// API-L2: return an error with context instead of silent empty vec.
    /// Tests should inject real data via sent_doc_data or override as needed.
    async fn download_file(&self, _file_id: &str) -> Result<Vec<u8>> {
        Err(crate::error::TelegramError::File(
            "mock: no data — inject via MockTelegramClient::set_download_data".into()
        ))
    }

    async fn receive_updates(&self) -> Result<Vec<TelegramUpdate>> {
        // Re-inject (do NOT drain) sent documents so repeated `receive_updates`
        // calls yield the document-derived `NewMessage` until the caller
        // explicitly drains via `drain_received_documents()`. H4 fix: this
        // mirrors at-least-once receive semantics.
        // R6 MEM-C3: iterate under the lock, cloning only the header fields (chat_id, filename)
        // but NOT the full Vec<u8> data. The data is encoded to base64 while still under
        // the lock, and only the encoded string (much smaller) is kept for the push loop.
        // This reduces per-poll allocations from N×(key.clone() + value.clone()) to N×(key.clone()).
        let pending: Vec<_> = {
            let guard = self.sent_doc_data.lock().unwrap();
            guard.iter().map(|((chat_id, _filename), data)| {
                let encoded = crate::envelope::encode_envelope(data);
                let sender_id = *self.mock_sender_id.lock().unwrap();
                let (from, from_legacy) = if sender_id == 0 {
                    (crate::client::MessageSender::Unknown, String::new())
                } else {
                    (crate::client::MessageSender::User(sender_id), sender_id.to_string())
                };
                (chat_id.clone(), encoded, from, from_legacy)
            }).collect()
        };
        for (chat_id, encoded, from, from_legacy) in pending {
            self.pending_updates
                .lock()
                .unwrap()
                .push(TelegramUpdate::NewMessage(NewMessage {
                    chat_id: chat_id.parse().unwrap_or(0),
                    message: encoded,
                    from,
                    from_legacy,
                }));
        }
        let mut pending = self.pending_updates.lock().unwrap();
        Ok(std::mem::take(&mut *pending))
    }

    /// Authenticate by stepping through a configurable auth state queue.
    /// API-H3: the mock now supports setting auth states via set_auth_queue.
    async fn authenticate(&self) -> Result<()> {
        // Process the auth queue if set, otherwise return Ok as before
        let mut queue = self.auth_queue.lock().unwrap();
        if let Some(state) = queue.pop_front() {
            match state {
                crate::auth::AuthStateKey::WaitTdlibParameters |
                crate::auth::AuthStateKey::WaitPhoneNumber |
                crate::auth::AuthStateKey::WaitCode |
                crate::auth::AuthStateKey::WaitPassword => {
                    // These states require user interaction — return error
                    return Err(crate::error::TelegramError::Auth(
                        format!("mock auth requires user input for {:?}", state)
                    ));
                }
                crate::auth::AuthStateKey::Ready => {
                    // Auth completed successfully
                    return Ok(());
                }
                crate::auth::AuthStateKey::Closed |
                crate::auth::AuthStateKey::Other => {
                    return Err(crate::error::TelegramError::Auth(
                        format!("mock auth failed: {:?}", state)
                    ));
                }
            }
        }
        Ok(())
    }

    /// R4 C2: Override the default `get_file_id_for_message` (which returns
    /// `Unimplemented`) with a stub that synthesises a file_id from the
    /// chat_id and message_id. The mock does not have real TDLib message
    /// content, so any lookup by this id will yield empty bytes from
    /// `download_file` — but the call will not fail with `Unimplemented`,
    /// matching the real client's contract.
    async fn get_file_id_for_message(&self, chat_id: i64, message_id: i64) -> Result<String> {
        // API-L6: synthesized mock file_id, never matches download_file
        Ok(format!("mock-file-{}-{}", chat_id, message_id))
    }
}
