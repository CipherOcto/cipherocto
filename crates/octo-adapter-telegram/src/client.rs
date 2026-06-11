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

/// Structured sender of a Telegram message.
///
/// M7: previously the `from` field on `NewMessage` was a `String` (e.g. `"12345"`).
/// TDLib distinguishes between user-sourced and chat-sourced messages
/// (channels/supergroups post on behalf of the chat, not a user), and
/// disambiguating numeric IDs in string form is brittle — a chat_id and
/// a user_id could collide as strings. This enum is the canonical form.
///
/// Use the enum for self-loop filtering (`MessageSender::User(id)`) and
/// any other identity-based logic. The legacy string form is kept on
/// `NewMessage::from_legacy` for back-compat with downstream consumers
/// that haven't migrated.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum MessageSender {
    /// A real user. The wrapped value is the TDLib user_id (i64).
    User(i64),
    /// A chat (channel, supergroup, basic group) posting on its own
    /// behalf. The wrapped value is the TDLib chat_id (i64).
    Chat(i64),
    /// A message forwarded from a hidden/anonymous source. TDLib's
    /// current `MessageSender` binding does not emit this variant; it is
    /// reserved for future TDLib growth. Filter code should treat it as
    /// "not self-authored" and let the message through.
    Hidden,
    /// Unknown / future variant we don't model yet. Filter code should
    /// treat this as "not self-authored" and let the message through.
    Unknown,
}

/// A new message update from Telegram.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NewMessage {
    pub chat_id: i64,
    pub message: String,
    /// Structured sender. Use this for self-loop filtering and any other
    /// identity-based logic. See `MessageSender` for variant semantics.
    pub from: MessageSender,
    /// Legacy string form. Kept for back-compat with downstream consumers
    /// (e.g. metadata exports to the gateway) that haven't migrated to
    /// `MessageSender`. For numeric senders (User/Chat) this carries the
    /// decimal string of the wrapped id; for `Hidden`/`Unknown` it is
    /// the empty string.
    pub from_legacy: String,
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
#[non_exhaustive]
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
    /// R4 H13: The returned `Vec<u8>` is heap-allocated in full. For files
    /// exceeding ~100 MB, this will cause significant memory pressure.
    /// Streaming download (returning `impl AsyncRead`) is planned but not
    /// yet implemented. A 2 GB file will OOM the adapter — ensure your
    /// deployment has memory limits or use the size cap in `download_media`.
    ///
    /// NOTE: The parameter is named `file_id` (not `message_id`) because
    /// TDLib uses file_ids for downloads. Callers that only have a message
    /// id must first resolve the message via their platform-specific
    /// message lookup (this trait does not expose that — the `adapter`
    /// module's `download_media` is the high-level entry point).
    async fn download_file(&self, file_id: &str) -> Result<Vec<u8>>;

    /// Receive pending updates. Yields all queued updates.
    ///
    /// # Ordering (API-H1)
    /// Updates are yielded in **FIFO order** (insertion order). The real TDLib
    /// client uses an `mpsc` channel which guarantees FIFO. The mock client
    /// re-injects doc-derived updates after the initial pending_updates are
    /// consumed, in send order. Downstream consumers should not depend on
    /// cross-platform ordering between immediate text messages and
    /// doc-derived re-injections.
    ///
    /// Takes `&self` so the trait composes with `PlatformAdapter::receive_messages`
    /// (which also takes `&self`); interior mutability (Mutex/RwLock) is the
    /// impl's responsibility.
    async fn receive_updates(&self) -> Result<Vec<TelegramUpdate>>;

    /// Authenticate (for user mode). For bot mode, this is a no-op.
    /// Takes `&self`; the real TDLib client tracks auth state inside the
    /// tdjson client (not in our struct), so no&mut self is needed.
    async fn authenticate(&self) -> Result<()>;

    /// Resolve a message by chat_id and message_id to its attached file_id.
    /// Used by the `download_media_from_message` adapter method.
    /// Default impl returns Unimplemented for clients that don't support it.
    async fn get_file_id_for_message(&self, _chat_id: i64, _message_id: i64) -> Result<String> {
        Err(crate::error::TelegramError::Unimplemented(
            "get_file_id_for_message not implemented for this client".into(),
        ))
    }
}

/// Parse a chat_id string. Both mock and real client must use this helper
/// so they agree on the boundary cases (H7). Without the shared helper,
/// tests pass on mock (which accepted any string) and fail on real client
/// (which required valid `i64`).
///
/// M8: also rejects positive IDs. Telegram chat_ids are always negative —
/// `-100…` for supergroups/channels, `-…` for basic groups. Positive
/// numbers in this position are user IDs and would silently route the
/// envelope to the wrong peer.
pub fn parse_chat_id(s: &str) -> std::result::Result<i64, &'static str> {
    if s.is_empty() {
        return Err("chat_id is empty");
    }
    let n: i64 = s.parse().map_err(|_| "chat_id is not a valid i64")?;
    if n >= 0 {
        return Err("chat_id must be negative (Telegram convention)");
    }
    Ok(n)
}
