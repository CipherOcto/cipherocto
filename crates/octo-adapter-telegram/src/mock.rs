//! Mock Telegram client for tests.
//! Mission AC line 143: "Unit tests use a mock TDLib client (no real TDLib instance required for cargo test)"

use crate::client::{TelegramClient, TelegramUpdate};
use crate::error::Result;
use async_trait::async_trait;
use std::sync::Mutex;

/// In-memory mock that records sends and queues injected updates.
pub struct MockTelegramClient {
    sent_messages: Mutex<Vec<(String, String)>>,
    sent_documents: Mutex<Vec<(String, String, usize)>>,
    pending_updates: Mutex<Vec<TelegramUpdate>>,
    next_msg_id: Mutex<u64>,
}

impl MockTelegramClient {
    pub fn new() -> Self {
        Self {
            sent_messages: Mutex::new(Vec::new()),
            sent_documents: Mutex::new(Vec::new()),
            pending_updates: Mutex::new(Vec::new()),
            next_msg_id: Mutex::new(1),
        }
    }

    /// Inject an update that the next `receive_updates` call will yield.
    pub fn inject_update(&mut self, update: TelegramUpdate) {
        self.pending_updates.lock().unwrap().push(update);
    }

    pub fn sent_messages(&self) -> Vec<(String, String)> {
        self.sent_messages.lock().unwrap().clone()
    }

    pub fn sent_documents(&self) -> Vec<(String, String, usize)> {
        self.sent_documents.lock().unwrap().clone()
    }
}

impl Default for MockTelegramClient {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl TelegramClient for MockTelegramClient {
    async fn send_message(&self, chat_id: &str, text: &str) -> Result<String> {
        let id = format!("mock-msg-{}", self.next_msg_id.lock().unwrap());
        *self.next_msg_id.lock().unwrap() += 1;
        self.sent_messages
            .lock()
            .unwrap()
            .push((chat_id.to_string(), text.to_string()));
        Ok(id)
    }

    async fn send_document(&self, chat_id: &str, filename: &str, data: &[u8]) -> Result<String> {
        let id = format!("mock-doc-{}", self.next_msg_id.lock().unwrap());
        *self.next_msg_id.lock().unwrap() += 1;
        self.sent_documents.lock().unwrap().push((
            chat_id.to_string(),
            filename.to_string(),
            data.len(),
        ));
        Ok(id)
    }

    async fn download_file(&self, _message_id: &str) -> Result<Vec<u8>> {
        Ok(Vec::new())
    }

    async fn receive_updates(&mut self) -> Result<Vec<TelegramUpdate>> {
        let mut pending = self.pending_updates.lock().unwrap();
        Ok(std::mem::take(&mut *pending))
    }

    async fn authenticate(&mut self) -> Result<()> {
        Ok(())
    }
}
