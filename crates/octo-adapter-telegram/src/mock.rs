//! Mock Telegram client for tests.
//! Mission AC line 143: "Unit tests use a mock TDLib client (no real TDLib instance required for cargo test)"

use crate::client::{NewMessage, SentMessage, TelegramClient, TelegramUpdate};
use crate::error::Result;
use async_trait::async_trait;
use parking_lot::Mutex;
use std::collections::BTreeMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

/// Type alias for the sent-document data map.
/// Stores the cached base64 encoding so that the inject-once path
/// (H1) can construct the doc-derived `NewMessage` without re-encoding.
type DocDataMap = BTreeMap<(String, String), String>;

/// In-memory mock that records sends and queues injected updates.
#[derive(Clone)]
pub struct MockTelegramClient {
    /// API-H3: queue of auth states for authenticate() to process.
    auth_queue: Arc<Mutex<std::collections::VecDeque<crate::auth::AuthStateKey>>>,
    sent_messages: Arc<Mutex<Vec<(String, String)>>>,
    sent_documents: Arc<Mutex<Vec<(String, String, usize)>>>,
    /// Tracks data sent via `send_envelope`/`send_file`, keyed by
    /// `(chat_id, filename)`. Used at send time to construct the
    /// doc-derived `NewMessage` for the inject-once path (H1).
    /// Not drained on `receive_updates` — each document is injected
    /// exactly once at send time, matching real TDLib behavior.
    ///
    /// H6: prior to the H6 split, this was named `sent_doc_data` and only
    /// fed by `send_document`. After the split, both `send_envelope` and
    /// `send_file` populate it (the doc round-trip is the same in both
    /// cases — only the caption differs).
    sent_doc_data: Arc<Mutex<DocDataMap>>,
    pending_updates: Arc<Mutex<Vec<TelegramUpdate>>>,
    next_msg_id: Arc<AtomicU64>,
    /// L2: sender id stamped onto doc-derived `NewMessage.from`.
    /// `None` (default) keeps the `from` field empty. `Some(0)` means
    /// "sender is user_id 0" (a real, if rare, Telegram user_id).
    /// `Some(n)` for any n exercises the adapter's self-loop filter.
    mock_sender_id: Arc<Mutex<Option<i64>>>,
    /// M6: failure injection for `send_message`, `send_envelope`, and
    /// `send_file`. While the counter is non-zero, the next call
    /// decrements it and returns the configured error. When it reaches 0,
    /// the call returns `Ok` as normal. Used by tests to exercise
    /// `send_with_retry`'s retry paths for `RateLimited` and `Transient`
    /// errors.
    ///
    /// `download_file` does NOT consume the failure counter — it bypasses
    /// failure injection entirely. Test authors relying on
    /// `fail_next_n_sends` must ensure the code path only exercises
    /// `send_message`/`send_envelope`/`send_file` during the retry window.
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
    send_call_total: Arc<AtomicU64>,
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
        *self.auth_queue.lock() = states.into();
    }
    pub fn new() -> Self {
        Self {
            auth_queue: Arc::new(Mutex::new(std::collections::VecDeque::new())),
            sent_messages: Arc::new(Mutex::new(Vec::new())),
            sent_documents: Arc::new(Mutex::new(Vec::new())),
            sent_doc_data: Arc::new(Mutex::new(BTreeMap::new())),
            pending_updates: Arc::new(Mutex::new(Vec::new())),
            next_msg_id: Arc::new(AtomicU64::new(1)),
            mock_sender_id: Arc::new(Mutex::new(None)),
            fail_send_message: Arc::new(Mutex::new(None)),
            fail_send_message_remaining: Arc::new(Mutex::new(0)),
            send_call_total: Arc::new(AtomicU64::new(0)),
        }
    }

    /// M6: inject a failure for the next `n` `send_message` /
    /// `send_envelope` / `send_file` calls. Each call decrements `n`;
    /// once `n` reaches zero the mock returns `Ok` as normal. Used by
    /// tests to exercise the adapter's `with_retry` retry path.
    pub fn fail_next_n_sends(&self, n: u32, spec: FailureSpec) {
        *self.fail_send_message.lock() = Some(spec);
        *self.fail_send_message_remaining.lock() = n;
    }

    /// M6: total number of `send_message` / `send_envelope` calls so far,
    /// including failed-injection calls. Used by tests to assert the
    /// retry loop actually re-invokes the operation.
    pub fn send_call_count(&self) -> u64 {
        self.send_call_total.load(Ordering::Relaxed)
    }

    /// Inject an update that the next `receive_updates` call will yield.
    pub fn inject_update(&self, update: TelegramUpdate) {
        self.pending_updates.lock().push(update);
    }

    /// Set the sender id used when re-injecting document-derived
    /// `NewMessage` updates. Pass `None` (the default) to keep the `from`
    /// field empty, matching the pre-H5 behavior. Pass `Some(id)` to
    /// exercise the adapter's self-loop filter for document round-trips.
    /// L2: `Some(0)` now correctly represents "sender is user_id 0"
    /// instead of being used as the "no sender" sentinel.
    pub fn set_mock_sender(&self, id: Option<i64>) {
        *self.mock_sender_id.lock() = id;
    }

    pub fn sent_messages(&self) -> Vec<(String, String)> {
        self.sent_messages.lock().clone()
    }

    pub fn sent_documents(&self) -> Vec<(String, String, usize)> {
        self.sent_documents.lock().clone()
    }

    /// H1: helper to build the sender fields from mock_sender_id.
    fn build_sender(&self) -> (crate::client::MessageSender, String) {
        match *self.mock_sender_id.lock() {
            Some(id) => (crate::client::MessageSender::User(id), id.to_string()),
            None => (crate::client::MessageSender::Unknown, String::new()),
        }
    }

    /// Drain the sent-doc map. After this call, subsequent `receive_updates`
    /// will not re-inject document-derived `NewMessage` updates.
    ///
    /// **Deprecated (H1):** docs are now injected exactly once at send time,
    /// not re-injected on every poll. This method is a no-op kept for
    /// backward compatibility with existing tests.
    #[deprecated(
        since = "0.2.0",
        note = "docs are now injected once at send time; this is a no-op"
    )]
    pub fn drain_received_documents(&self) {
        // No-op: H1 makes re-injection unnecessary.
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
        self.send_call_total.fetch_add(1, Ordering::Relaxed);
        let mut remaining = self.fail_send_message_remaining.lock();
        if *remaining == 0 {
            return None;
        }
        *remaining = remaining.saturating_sub(1);
        self.fail_send_message
            .lock()
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
            .push((chat_id.to_string(), encoded_envelope.to_string()));
        self.sent_documents
            .lock()
            .push((chat_id.to_string(), filename.to_string(), data.len()));
        // Store data for receive-path injection (document envelope round-trip).
        // PERF-M2: pre-encode once; receive_updates reads the cached form.
        // H1: inject the doc-derived NewMessage NOW (exactly-once), not
        // on every receive_updates poll.
        let encoded_cached = crate::envelope::encode_envelope(data);
        self.sent_doc_data.lock().insert(
            (chat_id.to_string(), filename.to_string()),
            encoded_cached.clone(),
        );
        let (from, from_legacy) = self.build_sender();
        self.pending_updates
            .lock()
            .push(TelegramUpdate::NewMessage(NewMessage {
                chat_id: chat_id.parse().unwrap_or(0),
                message: encoded_cached,
                from,
                from_legacy,
            }));
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
        self.sent_documents
            .lock()
            .push((chat_id.to_string(), filename.to_string(), data.len()));
        // Store data for receive-path injection (document round-trip).
        // H1: inject the doc-derived NewMessage NOW (exactly-once).
        let encoded_cached = crate::envelope::encode_envelope(data);
        self.sent_doc_data.lock().insert(
            (chat_id.to_string(), filename.to_string()),
            encoded_cached.clone(),
        );
        let (from, from_legacy) = self.build_sender();
        self.pending_updates
            .lock()
            .push(TelegramUpdate::NewMessage(NewMessage {
                chat_id: chat_id.parse().unwrap_or(0),
                message: encoded_cached,
                from,
                from_legacy,
            }));
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
            "mock: no data — use inject_update() with FileDownloaded or override in a test-specific mock".into()
        ))
    }

    async fn receive_updates(&self) -> Result<Vec<TelegramUpdate>> {
        // H1: docs are now injected exactly once at send time
        // (send_envelope/send_file push to pending_updates immediately).
        // No re-injection on poll — each message appears exactly once,
        // matching real TDLib behavior.
        let mut pending = self.pending_updates.lock();
        Ok(std::mem::take(&mut *pending))
    }

    /// Authenticate by stepping through a configurable auth state queue.
    /// API-H3: the mock now supports setting auth states via set_auth_queue.
    ///
    /// **L5 note:** Error strings returned by this mock are NOT API-stable.
    /// Tests should assert on the error variant (e.g. `matches!(err,
    /// TelegramError::Auth(_))`) rather than the message text, since the
    /// real client returns different error strings from TDLib.
    async fn authenticate(&self) -> Result<()> {
        // Process the auth queue if set, otherwise return Ok as before
        let mut queue = self.auth_queue.lock();
        if let Some(state) = queue.pop_front() {
            match state {
                crate::auth::AuthStateKey::WaitTdlibParameters
                | crate::auth::AuthStateKey::WaitPhoneNumber
                | crate::auth::AuthStateKey::WaitCode
                | crate::auth::AuthStateKey::WaitPassword => {
                    // These states require user interaction — return error
                    return Err(crate::error::TelegramError::Auth(format!(
                        "mock auth requires user input for {:?}",
                        state
                    )));
                }
                crate::auth::AuthStateKey::Ready => {
                    // Auth completed successfully
                    return Ok(());
                }
                crate::auth::AuthStateKey::Closed | crate::auth::AuthStateKey::Other => {
                    return Err(crate::error::TelegramError::Auth(format!(
                        "mock auth failed: {:?}",
                        state
                    )));
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
