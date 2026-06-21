//! `TelegramMtprotoClient` trait — the abstraction over the
//! grammers transport.
//!
//! The trait is intentionally narrow and uses only std types
//! (no `grammers_client` / `grammers_tl_types` in the
//! signatures) for two reasons:
//!
//! 1. The default build (no features) must not pull
//!    `grammers-client` — the libsql transitive dep would
//!    violate the cipherocto persistence convention. So the
//!    trait must compile against `grammers-session` only (the
//!    storage trait we hand-implement on top of stoolap), and
//!    not against the higher-level `grammers-client` API.
//! 2. Unit tests of the adapter (and of the `PlatformAdapter`
//!    contract) use a pure-Rust mock that lives entirely in
//!    `mock.rs`. The mock and the real client both satisfy
//!    this trait, so the adapter is testable without any
//!    network.
//!
//! ## Two impls
//!
//! - `MockTelegramMtprotoClient` (in this module) — always
//!   available, deterministic, drives adapter tests. The
//!   behaviour matches the TDLib-based adapter's
//!   `MockTelegramClient` so the same test scenarios port
//!   across.
//! - `RealTelegramMtprotoClient` (in `real_client.rs`,
//!   `#[cfg(feature = "real-network")]`) — wraps
//!   `grammers_client::Client` and the `StoolapSession` we
//!   pass to it. Owns the receive loop and the auth state
//!   machine.

use async_trait::async_trait;
use parking_lot::Mutex;
use std::collections::VecDeque;
use std::sync::Arc;

use crate::error::MtprotoTelegramError;

/// Result of sending a message — includes the platform
/// message id and the Unix timestamp (seconds since epoch)
/// when the message was sent.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MtprotoSentMessage {
    pub id: i64,
    pub timestamp: i64,
}

impl MtprotoSentMessage {
    pub fn new(id: i64, timestamp: i64) -> Self {
        Self { id, timestamp }
    }
}

/// Identity of the logged-in user (bot or user). Returned by
/// `sign_in_bot` / `sign_in_user`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SelfUserInfo {
    pub user_id: i64,
    pub username: Option<String>,
    /// Access hash for `InputPeer::Self`. Stored so subsequent
    /// `InputPeer` constructions do not need a re-fetch.
    pub access_hash: i64,
}

/// A new message update from Telegram.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NewMessage {
    pub chat_id: i64,
    pub message: String,
    /// Sender's `user_id`. `None` for channel posts (where the
    /// sender is the channel, not a user). The adapter's
    /// self-loop filter ignores channel posts because they
    /// cannot be self-authored (Telegram does not allow
    /// self-channel posts via the bot API; user-mode
    /// self-posts to own channel would carry the user's
    /// `from_id`, which the filter matches).
    pub from_id: Option<i64>,
    pub message_id: i64,
    /// If the message carries a file (DOT/2 media upload),
    /// `document_id` is the grammers file_id used to download
    /// it. `None` for plain text messages.
    pub document_id: Option<String>,
    /// Timestamp (Unix seconds).
    pub timestamp: i64,
}

/// A message-edited update.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MessageEdited {
    pub chat_id: i64,
    pub message_id: i64,
    pub new_text: String,
    pub timestamp: i64,
}

/// A file-download completion event (Telegram notifies
/// when a large file finishes downloading). Not currently
/// surfaced to the adapter; reserved for the streaming
/// download path in F8.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FileDownloaded {
    pub file_id: String,
    pub local_path: String,
    pub size: u64,
}

/// Telegram update enum — matches the same shape as
/// `octo-adapter-telegram::client::TelegramUpdate` so the
/// two adapters' adapter.rs can share the dispatch logic.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum MtprotoTelegramUpdate {
    NewMessage(NewMessage),
    MessageEdited(MessageEdited),
    FileDownloaded(FileDownloaded),
}

/// Async trait for the MTProto Telegram client. Both
/// `MockTelegramMtprotoClient` (in this module) and
/// `RealTelegramMtprotoClient` (in `real_client.rs`,
/// `real-network` feature) implement this.
#[async_trait]
pub trait MtprotoTelegramClient: Send + Sync {
    /// Send a text message to a chat. Returns the message id
    /// and timestamp.
    async fn send_message(
        &self,
        chat_id: i64,
        text: &str,
    ) -> Result<MtprotoSentMessage, MtprotoTelegramError>;

    /// Send a document (used for `DOT/2/{msg_id}` payloads
    /// exceeding the 4096-char text limit). `caption` is
    /// set as the document's caption (Telegram's round-trip
    /// channel for the wire format); `data` is the file
    /// content uploaded. `filename` is used as the document
    /// filename. Returns the message id and timestamp.
    async fn send_document(
        &self,
        chat_id: i64,
        caption: &str,
        filename: &str,
        data: &[u8],
    ) -> Result<MtprotoSentMessage, MtprotoTelegramError>;

    /// Download a file by grammers file_id. Returns the
    /// raw bytes.
    async fn download_file(
        &self,
        file_id: &str,
    ) -> Result<Vec<u8>, MtprotoTelegramError>;

    /// Receive pending updates. Yields all queued updates.
    /// Takes `&self`; interior mutability is the impl's
    /// responsibility.
    async fn receive_updates(
        &self,
    ) -> Result<Vec<MtprotoTelegramUpdate>, MtprotoTelegramError>;

    /// Bot sign-in (no user interaction). Returns the
    /// bot's `SelfUserInfo` on success.
    async fn sign_in_bot(
        &self,
        bot_token: &str,
        api_id: i32,
        api_hash: &str,
    ) -> Result<SelfUserInfo, MtprotoTelegramError>;

    /// User sign-in: send a login code to the configured
    /// phone number. The login code is delivered to the
    /// user's Telegram app; the caller will subsequently
    /// call `submit_code` and (if needed) `submit_password`.
    async fn request_login_code(
        &self,
        api_id: i32,
        api_hash: &str,
        phone: &str,
    ) -> Result<(), MtprotoTelegramError>;

    /// Submit the login code received from the user.
    /// Returns the user's `SelfUserInfo` on success.
    /// If 2FA is required, returns
    /// `MtprotoTelegramError::Auth("2FA_REQUIRED")` and the
    /// caller must then call `submit_password`.
    async fn submit_code(
        &self,
        code: &str,
    ) -> Result<SelfUserInfo, MtprotoTelegramError>;

    /// Submit a 2FA password (only valid after
    /// `submit_code` returned `2FA_REQUIRED`).
    async fn submit_password(
        &self,
        password: &str,
    ) -> Result<SelfUserInfo, MtprotoTelegramError>;

    /// `auth.logOut` and clear the local session state
    /// (calls `StoolapSession::reset()`).
    async fn sign_out(&self) -> Result<(), MtprotoTelegramError>;

    /// Resolve a message by chat_id and message_id to its
    /// attached file_id. Used by the `download_media`
    /// adapter method for the `DOT/2/{msg_id}` path.
    async fn get_file_id_for_message(
        &self,
        chat_id: i64,
        message_id: i64,
    ) -> Result<String, MtprotoTelegramError>;
}

/// Failure-injection spec for `MockTelegramMtprotoClient`.
/// Mirrors `octo-adapter-telegram::mock::FailureSpec` so the
/// two adapters' tests share semantics.
#[derive(Clone, Debug, Default)]
pub struct MockFailureSpec {
    /// If set, `send_message` returns this error every call.
    pub send_message_error: Option<String>,
    /// If set, `send_document` returns this error every call.
    pub send_document_error: Option<String>,
    /// If set, `download_file` returns this error every call.
    pub download_file_error: Option<String>,
}

/// Pure-Rust mock used by tests. Behaviour:
///
/// - `send_message` returns a fresh `MtprotoSentMessage` with
///   a monotonically-increasing id.
/// - `receive_updates` returns the queue in FIFO order.
/// - The mock does not call any external service; it does
///   not require grammers.
/// - The mock's `sign_in_bot` accepts any token and returns
///   `SelfUserInfo { user_id: 1, username: Some("mock_bot"),
///   access_hash: 0 }`.
/// - `request_login_code` / `submit_code` / `submit_password`
///   are stubs that return `Ok(())` / `Ok(SelfUserInfo { id:
///   2, .. })`. They do not simulate 2FA.
#[derive(Default, Clone)]
pub struct MockTelegramMtprotoClient {
    state: Arc<Mutex<MockState>>,
}

#[derive(Default)]
struct MockState {
    next_message_id: i64,
    next_user_id: i64,
    updates: VecDeque<MtprotoTelegramUpdate>,
    failure: MockFailureSpec,
    signed_in: bool,
}

impl MockTelegramMtprotoClient {
    pub fn new() -> Self {
        Self::default()
    }

    /// Inject an update into the receive queue. Used by
    /// tests to simulate inbound DOT messages.
    pub fn inject_update(&self, update: MtprotoTelegramUpdate) {
        let mut g = self.state.lock();
        g.updates.push_back(update);
    }

    /// Inject a `NewMessage` directly (convenience helper).
    pub fn inject_new_message(
        &self,
        chat_id: i64,
        message: String,
        from_id: Option<i64>,
        message_id: Option<i64>,
    ) {
        let mut g = self.state.lock();
        let mid = message_id.unwrap_or_else(|| {
            g.next_message_id += 1;
            g.next_message_id
        });
        g.updates.push_back(MtprotoTelegramUpdate::NewMessage(NewMessage {
            chat_id,
            message,
            from_id,
            message_id: mid,
            document_id: None,
            timestamp: 0,
        }));
    }

    /// Set the failure-injection spec.
    pub fn set_failure_spec(&self, spec: MockFailureSpec) {
        self.state.lock().failure = spec;
    }

    /// Mark the mock as signed in (used by the adapter's
    /// `connect()` to short-circuit the receive loop test
    /// path).
    pub fn set_signed_in(&self, signed_in: bool) {
        self.state.lock().signed_in = signed_in;
    }

    /// Read the failure spec (for assertions in tests).
    pub fn failure_spec(&self) -> MockFailureSpec {
        self.state.lock().failure.clone()
    }

    fn next_message_id(state: &mut MockState) -> i64 {
        state.next_message_id += 1;
        state.next_message_id
    }
}

#[async_trait]
impl MtprotoTelegramClient for MockTelegramMtprotoClient {
    async fn send_message(
        &self,
        _chat_id: i64,
        _text: &str,
    ) -> Result<MtprotoSentMessage, MtprotoTelegramError> {
        let mut g = self.state.lock();
        if let Some(msg) = &g.failure.send_message_error {
            return Err(MtprotoTelegramError::Rpc { code: -1, message: msg.clone() });
        }
        let id = Self::next_message_id(&mut g);
        Ok(MtprotoSentMessage::new(id, 0))
    }

    async fn send_document(
        &self,
        _chat_id: i64,
        _caption: &str,
        _filename: &str,
        _data: &[u8],
    ) -> Result<MtprotoSentMessage, MtprotoTelegramError> {
        let mut g = self.state.lock();
        if let Some(msg) = &g.failure.send_document_error {
            return Err(MtprotoTelegramError::Rpc { code: -1, message: msg.clone() });
        }
        let id = Self::next_message_id(&mut g);
        Ok(MtprotoSentMessage::new(id, 0))
    }

    async fn download_file(
        &self,
        _file_id: &str,
    ) -> Result<Vec<u8>, MtprotoTelegramError> {
        let g = self.state.lock();
        if let Some(msg) = &g.failure.download_file_error {
            return Err(MtprotoTelegramError::Rpc { code: -1, message: msg.clone() });
        }
        Ok(vec![])
    }

    async fn receive_updates(
        &self,
    ) -> Result<Vec<MtprotoTelegramUpdate>, MtprotoTelegramError> {
        let mut g = self.state.lock();
        let out: Vec<_> = g.updates.drain(..).collect();
        Ok(out)
    }

    async fn sign_in_bot(
        &self,
        _bot_token: &str,
        _api_id: i32,
        _api_hash: &str,
    ) -> Result<SelfUserInfo, MtprotoTelegramError> {
        let mut g = self.state.lock();
        g.next_user_id += 1;
        g.signed_in = true;
        Ok(SelfUserInfo {
            user_id: g.next_user_id,
            username: Some("mock_bot".into()),
            access_hash: 0,
        })
    }

    async fn request_login_code(
        &self,
        _api_id: i32,
        _api_hash: &str,
        _phone: &str,
    ) -> Result<(), MtprotoTelegramError> {
        Ok(())
    }

    async fn submit_code(
        &self,
        _code: &str,
    ) -> Result<SelfUserInfo, MtprotoTelegramError> {
        let mut g = self.state.lock();
        g.next_user_id += 1;
        g.signed_in = true;
        Ok(SelfUserInfo {
            user_id: g.next_user_id,
            username: Some("mock_user".into()),
            access_hash: 0,
        })
    }

    async fn submit_password(
        &self,
        _password: &str,
    ) -> Result<SelfUserInfo, MtprotoTelegramError> {
        let mut g = self.state.lock();
        g.next_user_id += 1;
        g.signed_in = true;
        Ok(SelfUserInfo {
            user_id: g.next_user_id,
            username: Some("mock_user_2fa".into()),
            access_hash: 0,
        })
    }

    async fn sign_out(&self) -> Result<(), MtprotoTelegramError> {
        let mut g = self.state.lock();
        g.signed_in = false;
        Ok(())
    }

    async fn get_file_id_for_message(
        &self,
        _chat_id: i64,
        message_id: i64,
    ) -> Result<String, MtprotoTelegramError> {
        Ok(format!("file_{}", message_id))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn mock_send_message_returns_increasing_ids() {
        let c = MockTelegramMtprotoClient::new();
        let a = c.send_message(123, "hi").await.unwrap();
        let b = c.send_message(123, "hi again").await.unwrap();
        assert!(b.id > a.id);
    }

    #[tokio::test]
    async fn mock_receive_updates_yields_fifo() {
        let c = MockTelegramMtprotoClient::new();
        c.inject_new_message(1, "a".into(), None, Some(1));
        c.inject_new_message(1, "b".into(), None, Some(2));
        let updates = c.receive_updates().await.unwrap();
        assert_eq!(updates.len(), 2);
    }

    #[tokio::test]
    async fn mock_failure_spec_short_circuits() {
        let c = MockTelegramMtprotoClient::new();
        c.set_failure_spec(MockFailureSpec {
            send_message_error: Some("forced".into()),
            ..Default::default()
        });
        let r = c.send_message(1, "x").await;
        assert!(r.is_err());
    }

    #[tokio::test]
    async fn mock_sign_in_bot_records_signed_in() {
        let c = MockTelegramMtprotoClient::new();
        let info = c.sign_in_bot("123:abc", 1, "hash").await.unwrap();
        assert_eq!(info.username.as_deref(), Some("mock_bot"));
        assert!(c.state.lock().signed_in);
    }

    #[tokio::test]
    async fn mock_sign_out_clears_signed_in() {
        let c = MockTelegramMtprotoClient::new();
        c.sign_in_bot("123:abc", 1, "hash").await.unwrap();
        c.sign_out().await.unwrap();
        assert!(!c.state.lock().signed_in);
    }
}
