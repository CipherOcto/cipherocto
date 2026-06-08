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

/// Result of sending a message — includes the platform message id and
/// the Unix timestamp (seconds since epoch) when the message was sent.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SentMessage {
    pub id: String,
    pub timestamp: i64,
}

impl SentMessage {
    pub fn new(id: String, timestamp: i64) -> Self {
        Self { id, timestamp }
    }
}

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
    /// Send a text message to a chat. Returns the message id and timestamp.
    async fn send_message(&self, chat_id: &str, text: &str) -> Result<SentMessage>;

    /// Send a binary envelope. The `encoded_envelope` is set as the caption
    /// (Telegram's round-trip channel for the wire format); the `data` is
    /// the file content uploaded. Used by `send_envelope` in the adapter for
    /// envelopes that exceed the 4096-char text threshold.
    ///
    /// H6: split out of the prior unified `send_document` so callers can
    /// request a raw file upload (`send_file`) without forcing a caption.
    async fn send_envelope(
        &self,
        chat_id: &str,
        encoded_envelope: &str,
        filename: &str,
        data: &[u8],
    ) -> Result<SentMessage>;

    /// Send a raw file (no caption). Used by `upload_media_to_domain` for
    /// arbitrary media uploads that should not round-trip through the
    /// envelope encoder. H6: replaces the prior `send_document` for the
    /// no-caption upload path.
    async fn send_file(&self, chat_id: &str, filename: &str, data: &[u8]) -> Result<SentMessage>;

    /// Download a file by TDLib file_id. Returns the raw bytes.
    ///
    /// NOTE: The parameter is named `file_id` (not `message_id`) because
    /// TDLib uses file_ids for downloads. Callers that only have a message
    /// id must first resolve the message via their platform-specific
    /// message lookup (this trait does not expose that — the `adapter`
    /// module's `download_media` is the high-level entry point).
    async fn download_file(&self, file_id: &str) -> Result<Vec<u8>>;

    /// Receive pending updates. Yields all queued updates.
    /// Takes `&self` so the trait composes with `PlatformAdapter::receive_messages`
    /// (which also takes `&self`); interior mutability (Mutex/RwLock) is the
    /// impl's responsibility.
    async fn receive_updates(&self) -> Result<Vec<TelegramUpdate>>;

    /// Authenticate (for user mode). For bot mode, this is a no-op.
    /// Takes `&self`; the real TDLib client tracks auth state inside the
    /// tdjson client (not in our struct), so no&mut self is needed.
    async fn authenticate(&self) -> Result<()>;
}
