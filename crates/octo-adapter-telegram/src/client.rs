//! Telegram client wrapper behind a trait so the rest of the adapter
//! is independent of TDLib specifics.
//!
//! Mission Architecture line 57: "Telegram client wrapper (src/client.rs) —
//! Owns the TDLib Client, runs the receive loop on a dedicated OS thread,
//! and exposes an async API to the rest of the adapter."
//!
//! Default impl: `MockTelegramClient` (see `src/mock.rs`).
//! Real TDLib impl: behind `--features real-tdlib`.

use crate::error::Result;
use async_trait::async_trait;

/// A new message update from Telegram.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NewMessage {
    pub chat_id: i64,
    pub message: String,
    pub from: String,
}

/// A message-edited update.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MessageEdited {
    pub chat_id: i64,
    pub message_id: String,
    pub new_text: String,
}

/// A file-downloaded update.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FileDownloaded {
    pub file_id: String,
    pub local_path: String,
    pub size: u64,
}

/// Telegram update enum — matches the 3 example enums from the mission's
/// Architecture section (line 57), but does NOT pin specific TDLib type names
/// since the actual tdlib-rs API may differ (see R6-C-R2).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TelegramUpdate {
    NewMessage(NewMessage),
    MessageEdited(MessageEdited),
    FileDownloaded(FileDownloaded),
}

/// Async trait for the Telegram client. Both `MockTelegramClient` and the
/// real TDLib client implement this.
#[async_trait]
pub trait TelegramClient: Send + Sync {
    /// Send a text message to a chat. Returns the platform message id.
    async fn send_message(&self, chat_id: &str, text: &str) -> Result<String>;

    /// Send a binary document to a chat. Returns the platform message id.
    async fn send_document(&self, chat_id: &str, filename: &str, data: &[u8]) -> Result<String>;

    /// Download a file by message id. Returns the raw bytes.
    async fn download_file(&self, message_id: &str) -> Result<Vec<u8>>;

    /// Receive pending updates. Yields all queued updates.
    async fn receive_updates(&mut self) -> Result<Vec<TelegramUpdate>>;

    /// Authenticate (for user mode). For bot mode, this is a no-op.
    async fn authenticate(&mut self) -> Result<()>;
}
