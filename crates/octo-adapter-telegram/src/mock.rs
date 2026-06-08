//! Mock Telegram client for tests.
//! Mission AC line 143: "Unit tests use a mock TDLib client (no real TDLib instance required for cargo test)"

use crate::client::{NewMessage, SentMessage, TelegramClient, TelegramUpdate};
use crate::error::Result;
use async_trait::async_trait;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

/// Type alias for the sent-document data map (chat_id, filename) → bytes.
type DocDataMap = HashMap<(String, String), Vec<u8>>;

/// In-memory mock that records sends and queues injected updates.
#[derive(Clone)]
pub struct MockTelegramClient {
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
    next_msg_id: Arc<Mutex<u64>>,
    /// Sender id stamped onto doc-derived `NewMessage.from` during
    /// `receive_updates`. When `0` (default), the field stays empty,
    /// matching the pre-H5 behavior. When set to a non-zero value, the
    /// mock uses that value as the `from` string so the adapter's
    /// self-loop filter (H5) is exercised for document round-trips.
    mock_sender_id: Arc<Mutex<i64>>,
}

impl MockTelegramClient {
    pub fn new() -> Self {
        Self {
            sent_messages: Arc::new(Mutex::new(Vec::new())),
            sent_documents: Arc::new(Mutex::new(Vec::new())),
            sent_doc_data: Arc::new(Mutex::new(HashMap::new())),
            pending_updates: Arc::new(Mutex::new(Vec::new())),
            next_msg_id: Arc::new(Mutex::new(1)),
            mock_sender_id: Arc::new(Mutex::new(0)),
        }
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
}

impl Default for MockTelegramClient {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl TelegramClient for MockTelegramClient {
    async fn send_message(&self, chat_id: &str, text: &str) -> Result<SentMessage> {
        let id = format!("mock-msg-{}", self.next_msg_id.lock().unwrap());
        *self.next_msg_id.lock().unwrap() += 1;
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
        // H6: send_envelope records the encoded envelope in `sent_messages`
        // (caption path) AND the doc in `sent_documents` (round-trip path).
        let id = format!("mock-env-{}", self.next_msg_id.lock().unwrap());
        *self.next_msg_id.lock().unwrap() += 1;
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
        // H6: send_file records the doc but NOT a caption (the raw upload
        // path has no envelope to round-trip via the caption channel).
        let id = format!("mock-file-{}", self.next_msg_id.lock().unwrap());
        *self.next_msg_id.lock().unwrap() += 1;
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

    async fn download_file(&self, _file_id: &str) -> Result<Vec<u8>> {
        Ok(Vec::new())
    }

    async fn receive_updates(&self) -> Result<Vec<TelegramUpdate>> {
        // Re-inject (do NOT drain) sent documents so repeated `receive_updates`
        // calls yield the document-derived `NewMessage` until the caller
        // explicitly drains via `drain_received_documents()`. H4 fix: this
        // mirrors at-least-once receive semantics.
        let doc_data: Vec<_> = self
            .sent_doc_data
            .lock()
            .unwrap()
            .iter()
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect();
        for ((chat_id, _filename), data) in doc_data {
            let encoded = crate::envelope::encode_envelope(&data);
            // H5: when the caller has set a non-zero mock_sender_id, stamp
            // it onto the doc-injected NewMessage's `from` field so the
            // adapter's self-loop filter can match it. When the default
            // (`0`) is in effect, keep `from` empty to preserve the
            // pre-H5 behavior (the parser rejects empty, the filter
            // falls through).
            let sender_id = *self.mock_sender_id.lock().unwrap();
            let from = if sender_id == 0 {
                String::new()
            } else {
                sender_id.to_string()
            };
            self.pending_updates
                .lock()
                .unwrap()
                .push(TelegramUpdate::NewMessage(NewMessage {
                    chat_id: chat_id.parse().unwrap_or(0),
                    message: encoded,
                    from,
                }));
        }
        let mut pending = self.pending_updates.lock().unwrap();
        Ok(std::mem::take(&mut *pending))
    }

    async fn authenticate(&self) -> Result<()> {
        Ok(())
    }
}
