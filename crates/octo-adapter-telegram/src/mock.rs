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
    /// Tracks data sent via send_document, keyed by (chat_id, filename).
    /// Used to inject NewMessage updates for the document receive path.
    /// Drained only via `drain_received_documents()`; `receive_updates`
    /// re-injects on every call so callers can re-poll until they choose
    /// to drain (H4 fix — matches at-least-once semantics of receive loops).
    sent_doc_data: Arc<Mutex<DocDataMap>>,
    pending_updates: Arc<Mutex<Vec<TelegramUpdate>>>,
    next_msg_id: Arc<Mutex<u64>>,
}

impl MockTelegramClient {
    pub fn new() -> Self {
        Self {
            sent_messages: Arc::new(Mutex::new(Vec::new())),
            sent_documents: Arc::new(Mutex::new(Vec::new())),
            sent_doc_data: Arc::new(Mutex::new(HashMap::new())),
            pending_updates: Arc::new(Mutex::new(Vec::new())),
            next_msg_id: Arc::new(Mutex::new(1)),
        }
    }

    /// Inject an update that the next `receive_updates` call will yield.
    pub fn inject_update(&self, update: TelegramUpdate) {
        self.pending_updates.lock().unwrap().push(update);
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

    async fn send_document(
        &self,
        chat_id: &str,
        filename: &str,
        data: &[u8],
    ) -> Result<SentMessage> {
        let id = format!("mock-doc-{}", self.next_msg_id.lock().unwrap());
        *self.next_msg_id.lock().unwrap() += 1;
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
            self.pending_updates
                .lock()
                .unwrap()
                .push(TelegramUpdate::NewMessage(NewMessage {
                    chat_id: chat_id.parse().unwrap_or(0),
                    message: encoded,
                    from: String::new(),
                }));
        }
        let mut pending = self.pending_updates.lock().unwrap();
        Ok(std::mem::take(&mut *pending))
    }

    async fn authenticate(&self) -> Result<()> {
        Ok(())
    }
}
