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
use std::collections::{BTreeMap, VecDeque};
use std::fmt;
use std::sync::Arc;

use crate::error::MtprotoTelegramError;

/// Build a `tg://login?token=<base64>` URL from the raw
/// token bytes. Uses standard base64 (with padding) as
/// the format Telegram's mobile client expects. The
/// input token is typically 16 random bytes from
/// `auth.exportLoginToken`.
///
/// Hand-rolled to avoid pulling in the `base64` crate
/// for the `no-default-features` build (where
/// `grammers-client` is not compiled in).
pub(crate) fn build_qr_url(token: &[u8]) -> String {
    const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(token.len().div_ceil(3) * 4);
    let mut i = 0;
    while i + 3 <= token.len() {
        let n = ((token[i] as u32) << 16) | ((token[i + 1] as u32) << 8) | (token[i + 2] as u32);
        out.push(ALPHABET[((n >> 18) & 0x3F) as usize] as char);
        out.push(ALPHABET[((n >> 12) & 0x3F) as usize] as char);
        out.push(ALPHABET[((n >> 6) & 0x3F) as usize] as char);
        out.push(ALPHABET[(n & 0x3F) as usize] as char);
        i += 3;
    }
    let rem = token.len() - i;
    if rem == 1 {
        let n = (token[i] as u32) << 16;
        out.push(ALPHABET[((n >> 18) & 0x3F) as usize] as char);
        out.push(ALPHABET[((n >> 12) & 0x3F) as usize] as char);
        out.push('=');
        out.push('=');
    } else if rem == 2 {
        let n = ((token[i] as u32) << 16) | ((token[i + 1] as u32) << 8);
        out.push(ALPHABET[((n >> 18) & 0x3F) as usize] as char);
        out.push(ALPHABET[((n >> 12) & 0x3F) as usize] as char);
        out.push(ALPHABET[((n >> 6) & 0x3F) as usize] as char);
        out.push('=');
    }
    format!("tg://login?token={}", out)
}

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

/// A QR code handle returned to the caller. Used by
/// `MtprotoTelegramAdapter::connect_qr_login` /
/// `MtprotoTelegramAdapter::poll_qr_login`.
///
/// `token` is the raw `auth.LoginToken.token` bytes from
/// Telegram (NOT base64-encoded). `url` is the
/// `tg://login?token=<base64>` form the caller embeds in
/// the QR code (Telegram's mobile clients expect this URL
/// when scanned).
///
/// R17-C1: the derived `Debug` would print the raw token
/// bytes and the base64-encoded URL on any `dbg!()`,
/// `tracing::error!(?handle)`, or panic message. The
/// token is the QR-session authorization credential (paired
/// with the user scanning the QR) — same threat class as
/// the R15-C3 / R16-C1 fixes on `MtprotoAuthAction` /
/// `UserAuthAction`. Hand-written `Debug` redacts both
/// fields; the `Display` impl is not provided (callers that
/// need the URL reach it via the field accessor).
#[derive(Clone, PartialEq, Eq)]
pub struct QrLoginHandle {
    pub token: Vec<u8>,
    pub url: String,
}

// R17-C1: hand-written Debug that prints the byte count
// instead of the raw token bytes, and the literal string
// "<redacted>" instead of the base64-encoded URL. The
// `Display` impl is intentionally absent (no caller path
// needs Display on this struct).
impl fmt::Debug for QrLoginHandle {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("QrLoginHandle")
            .field(
                "token",
                &format_args!("<redacted {} bytes>", self.token.len()),
            )
            .field("url", &"<redacted>")
            .finish()
    }
}

impl QrLoginHandle {
    /// Construct a handle from a `MtprotoTelegramError`.
    /// Returns `None` if the error is not a `QrLoginHandle`.
    /// Used by the adapter to extract the handle from the
    /// client's error return.
    pub fn from_error(err: &MtprotoTelegramError) -> Option<Self> {
        match err {
            MtprotoTelegramError::QrLoginHandle { token, url } => Some(Self {
                token: token.clone(),
                url: url.clone(),
            }),
            _ => None,
        }
    }

    /// True if the handle carries real QR data (token + url).
    /// Always true today; the field is reserved for future
    /// variants (e.g., a "session is already authorised"
    /// marker returned by `connect_qr_login` if the user
    /// re-scans while signed in).
    pub fn is_pending(&self) -> bool {
        !self.token.is_empty() && !self.url.is_empty()
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

/// Group metadata returned by `get_chat` and `create_group`.
///
/// Used by the `CoordinatorAdmin` impl to populate the
/// platform-agnostic `GroupMetadata` returned to callers.
/// `i64`-typed `chat_id` is the platform-native identifier
/// (Telegram `chat_id` is a signed 64-bit integer; supergroups
/// and channels have negative IDs).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GroupInfo {
    pub chat_id: i64,
    /// Group title (Telegram calls it "title"; "subject" is
    /// WhatsApp terminology).
    pub title: String,
    /// Number of members. None if the platform did not surface
    /// it (the mock returns `Some(2)` for default-created
    /// groups).
    pub member_count: Option<u32>,
    /// Whether the bot is admin in this group. None if
    /// unknown.
    pub is_admin: Option<bool>,
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
    async fn download_file(&self, file_id: &str) -> Result<Vec<u8>, MtprotoTelegramError>;

    /// Receive pending updates. Yields all queued updates.
    /// Takes `&self`; interior mutability is the impl's
    /// responsibility.
    async fn receive_updates(&self) -> Result<Vec<MtprotoTelegramUpdate>, MtprotoTelegramError>;

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
    async fn submit_code(&self, code: &str) -> Result<SelfUserInfo, MtprotoTelegramError>;

    /// Submit a 2FA password (only valid after
    /// `submit_code` returned `2FA_REQUIRED`).
    async fn submit_password(&self, password: &str) -> Result<SelfUserInfo, MtprotoTelegramError>;

    /// `auth.logOut` and clear the local session state
    /// (calls `StoolapSession::reset()`).
    async fn sign_out(&self) -> Result<(), MtprotoTelegramError>;

    /// Phase 2.5: start a QR login flow. Calls Telegram's
    /// `auth.exportLoginToken` and returns the result as
    /// `Err(MtprotoTelegramError::QrLoginHandle { token, url })`
    /// so the caller can extract the data and display it
    /// as a QR code. The caller then loops on
    /// `poll_qr_login` until the user has scanned the QR
    /// and the import finalizes.
    async fn qr_login(&self, api_id: i32, api_hash: &str) -> Result<(), MtprotoTelegramError>;

    /// Phase 2.5: poll the QR login status by re-invoking
    /// `auth.exportLoginToken`. Returns:
    /// - `Ok(SelfUserInfo)` if the import has finalized
    ///   (i.e., the user has scanned the QR and the
    ///   `auth.loginTokenSuccess` response was received).
    /// - `Err(MtprotoTelegramError::QrLoginHandle { token, url })`
    ///   if the user has not yet scanned, OR has scanned
    ///   but the import is not yet ready (the token may
    ///   have been refreshed; the caller should re-display
    ///   the QR with the new URL and call `poll_qr_login`
    ///   again).
    /// - `Err(MtprotoTelegramError::Auth("2FA_REQUIRED"))`
    ///   if the primary device has 2FA enabled; the
    ///   caller should then call `submit_password`.
    async fn poll_qr_login(&self) -> Result<SelfUserInfo, MtprotoTelegramError>;

    /// Phase 2.5: import the login token after the QR
    /// scan has finalized. Calls
    /// `auth.importLoginToken { token: bytes }` which
    /// returns the authorization (or signals 2FA).
    /// Returns `Ok(SelfUserInfo)` on success, or
    /// `Err(MtprotoTelegramError::Auth("2FA_REQUIRED"))`
    /// if the primary has 2FA enabled.
    async fn import_login_token(&self, token: &[u8]) -> Result<SelfUserInfo, MtprotoTelegramError>;

    /// Resolve a message by chat_id and message_id to its
    /// attached file_id. Used by the `download_media`
    /// adapter method for the `DOT/2/{msg_id}` path.
    async fn get_file_id_for_message(
        &self,
        chat_id: i64,
        message_id: i64,
    ) -> Result<String, MtprotoTelegramError>;

    // ── Group / Coordinator operations ─────────────────────────
    //
    // These methods back the `CoordinatorAdmin` trait impl
    // on the adapter (RFC-0850 §8 extension). They are
    // implemented by the mock with test-friendly defaults
    // (a counter-based "synthetic chat id" generator and
    //  accept-no-error semantics) and by the real client
    // (gated `real-network`) with the corresponding grammers
    // RPCs. All methods take `&self`; interior mutability is
    // the impl's responsibility (matching the rest of the
    // trait).

    /// Create a new basic group / supergroup with the given
    /// `title`. `user_ids` is the list of user_ids to add
    /// (Telegram requires at least one; the bot itself is
    /// automatically added as admin). Returns the new
    /// group's `GroupInfo`. Telegram's
    /// `messages.createChat` is used for basic groups;
    /// the real client picks the right RPC based on the
    /// available configuration.
    async fn create_group(
        &self,
        title: &str,
        user_ids: &[i64],
    ) -> Result<GroupInfo, MtprotoTelegramError>;

    /// Add a user to a chat (Telegram's
    /// `messages.addChatUser` for basic groups,
    /// `channels.inviteToChannel` for supergroups). Returns
    /// `Ok(())` on success.
    async fn add_participant(&self, chat_id: i64, user_id: i64)
        -> Result<(), MtprotoTelegramError>;

    /// Remove a user from a chat. For basic groups, this is
    /// the same as "kick". For supergroups, the user can
    /// rejoin via invite link unless banned — for the
    /// `CoordinatorAdmin::remove_member` semantic, this is
    /// the correct call.
    async fn kick_participant(
        &self,
        chat_id: i64,
        user_id: i64,
    ) -> Result<(), MtprotoTelegramError>;

    /// Promote a user to admin in a supergroup
    /// (`channels.editAdmin` with `ChatAdminRights`). For
    /// basic groups, returns `Err(NotSupergroup)`.
    async fn promote_participant(
        &self,
        chat_id: i64,
        user_id: i64,
    ) -> Result<(), MtprotoTelegramError>;

    /// Demote an admin back to regular user
    /// (`channels.editAdmin` with `ChatAdminRights::empty`).
    async fn demote_participant(
        &self,
        chat_id: i64,
        user_id: i64,
    ) -> Result<(), MtprotoTelegramError>;

    /// Set the title of a chat (`messages.editChatTitle`
    /// for basic groups, `channels.editTitle` for
    /// supergroups).
    async fn set_chat_title(&self, chat_id: i64, title: &str) -> Result<(), MtprotoTelegramError>;

    /// Set the about / description of a chat
    /// (`messages.editChatAbout`).
    async fn set_chat_about(&self, chat_id: i64, about: &str) -> Result<(), MtprotoTelegramError>;

    /// Delete a chat (Telegram's `messages.deleteChat`
    /// for basic groups; supergroups use
    /// `channels.deleteChannel`). For the
    /// `CoordinatorAdmin::destroy_group` semantic, this is
    /// the right call when the bot owns the chat.
    async fn delete_chat(&self, chat_id: i64) -> Result<(), MtprotoTelegramError>;

    /// Leave a chat (`messages.deleteChatUser` with
    /// `user_id = self` for basic groups;
    /// `channels.leaveChannel` for supergroups).
    async fn leave_chat(&self, chat_id: i64) -> Result<(), MtprotoTelegramError>;

    /// Fetch the full `GroupInfo` for a chat. Returns
    /// `Err(NotFound)` if the chat does not exist or the
    /// bot is not a member.
    async fn get_chat(&self, chat_id: i64) -> Result<GroupInfo, MtprotoTelegramError>;

    /// List the chat ids of all groups the bot is currently
    /// a member of. Used by `CoordinatorAdmin::list_own_groups`
    /// to enumerate managed domains.
    async fn list_dialog_ids(&self) -> Result<Vec<i64>, MtprotoTelegramError>;
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
    /// Phase 2.4: if set, `submit_code` returns
    /// `MtprotoTelegramError::Auth("2FA_REQUIRED")` instead of
    /// `Ok(SelfUserInfo {..})`. The caller is then expected to
    /// call `submit_password` next, which the mock always
    /// accepts. Use `MockTelegramMtprotoClient::set_require_2fa`
    /// to toggle.
    pub require_2fa: bool,
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
    /// Phase 2.5: number of times `poll_qr_login` has been
    /// called since the last `qr_login`.
    qr_poll_count: u32,
    /// Phase 2.5: how many `poll_qr_login` calls before the
    /// mock returns `Ok(SelfUserInfo)`. Default is 0
    /// (i.e., the very first poll returns success — handy
    /// for adapter tests that don't care about the
    /// polling loop). Tests can set this to 2 or 3 to
    /// exercise the still-pending path.
    qr_polls_to_success: u32,
    /// CoordinatorAdmin mock state: counter for synthetic
    /// chat ids returned by `create_group`. Starts at 0
    /// and increments by 1 per call (the first created
    /// group has `chat_id == 1`).
    next_chat_id: i64,
    /// CoordinatorAdmin mock state: created/known groups.
    /// Populated by `create_group` and used by `get_chat` /
    /// `list_dialog_ids`. Tests can pre-seed with
    /// `set_mock_group` for read paths.
    groups: BTreeMap<i64, GroupInfo>,
    /// CoordinatorAdmin mock state: members per group.
    /// Populated by `create_group` (with `user_ids` plus the
    /// bot) and updated by `add_participant` /
    /// `kick_participant`. Used by `get_chat`'s
    /// `member_count` computation.
    group_members: BTreeMap<i64, Vec<i64>>,
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
        g.updates
            .push_back(MtprotoTelegramUpdate::NewMessage(NewMessage {
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

    /// Phase 2.4: set the `require_2fa` flag on the failure
    /// spec. When `true`, `submit_code` returns
    /// `MtprotoTelegramError::Auth("2FA_REQUIRED")` instead of
    /// `Ok(SelfUserInfo {..})` so adapter tests can drive the
    /// full 2FA flow.
    pub fn set_require_2fa(&self, require: bool) {
        self.state.lock().failure.require_2fa = require;
    }

    /// Read the current `require_2fa` flag (for assertions in
    /// tests).
    pub fn require_2fa(&self) -> bool {
        self.state.lock().failure.require_2fa
    }

    /// Mark the mock as signed in (used by the adapter's
    /// `connect()` to short-circuit the receive loop test
    /// path).
    pub fn set_signed_in(&self, signed_in: bool) {
        self.state.lock().signed_in = signed_in;
    }

    /// Phase 2.5: configure the mock's `poll_qr_login`
    /// behaviour. With `polls=N`, the next `N` calls to
    /// `poll_qr_login` (after the most recent `qr_login`)
    /// return `Err(QrLoginHandle { .. })` and the (N+1)th
    /// returns `Ok(SelfUserInfo)`. Default is 0 (first
    /// poll succeeds). Each call to `qr_login` resets the
    /// poll counter.
    pub fn set_qr_polls_to_success(&self, polls: u32) {
        let mut g = self.state.lock();
        g.qr_polls_to_success = polls;
        g.qr_poll_count = 0;
    }

    /// Read the failure spec (for assertions in tests).
    pub fn failure_spec(&self) -> MockFailureSpec {
        self.state.lock().failure.clone()
    }

    /// CoordinatorAdmin mock helper: pre-seed a known
    /// group so tests of `get_chat` / `list_dialog_ids`
    /// can drive read paths without going through
    /// `create_group`. Idempotent: re-seeding replaces
    /// the existing entry.
    pub fn set_mock_group(&self, info: GroupInfo, members: Vec<i64>) {
        let mut g = self.state.lock();
        let chat_id = info.chat_id;
        g.groups.insert(chat_id, info);
        g.group_members.insert(chat_id, members);
    }

    /// CoordinatorAdmin mock helper: read all known
    /// groups. Used by tests to assert the mock's state
    /// after a sequence of operations.
    pub fn mock_groups(&self) -> Vec<GroupInfo> {
        self.state.lock().groups.values().cloned().collect()
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
            return Err(MtprotoTelegramError::Rpc {
                code: -1,
                message: msg.clone(),
            });
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
            return Err(MtprotoTelegramError::Rpc {
                code: -1,
                message: msg.clone(),
            });
        }
        let id = Self::next_message_id(&mut g);
        Ok(MtprotoSentMessage::new(id, 0))
    }

    async fn download_file(&self, _file_id: &str) -> Result<Vec<u8>, MtprotoTelegramError> {
        let g = self.state.lock();
        if let Some(msg) = &g.failure.download_file_error {
            return Err(MtprotoTelegramError::Rpc {
                code: -1,
                message: msg.clone(),
            });
        }
        Ok(vec![])
    }

    async fn receive_updates(&self) -> Result<Vec<MtprotoTelegramUpdate>, MtprotoTelegramError> {
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

    async fn submit_code(&self, _code: &str) -> Result<SelfUserInfo, MtprotoTelegramError> {
        let mut g = self.state.lock();
        // Phase 2.4: simulate 2FA-required when the mock's
        // `require_2fa` flag is set (set via
        // `set_require_2fa(true)`). The real-network impl
        // signals the same condition by returning
        // `MtprotoTelegramError::Auth("2FA_REQUIRED")` after
        // receiving `SignInError::PasswordRequired` from
        // grammers. Without the flag, the mock returns
        // `Ok(SelfUserInfo {..})` and is signed in.
        if g.failure.require_2fa {
            return Err(MtprotoTelegramError::Auth("2FA_REQUIRED".into()));
        }
        g.next_user_id += 1;
        g.signed_in = true;
        Ok(SelfUserInfo {
            user_id: g.next_user_id,
            username: Some("mock_user".into()),
            access_hash: 0,
        })
    }

    async fn submit_password(&self, _password: &str) -> Result<SelfUserInfo, MtprotoTelegramError> {
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

    // ----- Phase 2.5: QR login (mock) -----

    async fn qr_login(&self, api_id: i32, api_hash: &str) -> Result<(), MtprotoTelegramError> {
        // Reset the poll counter so the next
        // `poll_qr_login` call starts fresh.
        let mut g = self.state.lock();
        g.qr_poll_count = 0;
        // Deterministic mock: emit a fixed 16-byte token
        // built from the api_id + api_hash so tests are
        // reproducible. The real client uses random bytes
        // from Telegram.
        let mut token = Vec::with_capacity(16);
        let a = api_id.to_le_bytes();
        let h = api_hash.as_bytes();
        for i in 0..16 {
            token.push(a[i % a.len()].wrapping_add(h[i % h.len()]));
        }
        let url = build_qr_url(&token);
        Err(MtprotoTelegramError::QrLoginHandle { token, url })
    }

    async fn poll_qr_login(&self) -> Result<SelfUserInfo, MtprotoTelegramError> {
        // Mock: increment a poll counter. After
        // `qr_polls_to_success` polls, return
        // `Ok(SelfUserInfo)`. Until then, re-emit the
        // same handle (the test controls the QR data
        // by re-calling `qr_login` if it wants a new
        // token, but the default is to keep the same
        // handle).
        let mut g = self.state.lock();
        g.qr_poll_count += 1;
        if g.qr_poll_count > g.qr_polls_to_success {
            g.next_user_id += 1;
            g.signed_in = true;
            Ok(SelfUserInfo {
                user_id: g.next_user_id,
                username: Some("mock_qr_user".into()),
                access_hash: 0,
            })
        } else {
            // Re-emit a deterministic handle. The test
            // can call `qr_login` again to get a fresh
            // one.
            let token = vec![0u8; 16];
            let url = build_qr_url(&token);
            Err(MtprotoTelegramError::QrLoginHandle { token, url })
        }
    }

    async fn import_login_token(
        &self,
        _token: &[u8],
    ) -> Result<SelfUserInfo, MtprotoTelegramError> {
        // Mock: always succeed.
        let mut g = self.state.lock();
        g.next_user_id += 1;
        g.signed_in = true;
        Ok(SelfUserInfo {
            user_id: g.next_user_id,
            username: Some("mock_qr_user".into()),
            access_hash: 0,
        })
    }

    async fn get_file_id_for_message(
        &self,
        _chat_id: i64,
        message_id: i64,
    ) -> Result<String, MtprotoTelegramError> {
        Ok(format!("file_{}", message_id))
    }

    // ── Mock group / Coordinator operations ────────────────────
    //
    // All methods below mutate `MockState` in a way that
    // matches the real client's contract (errors are returned
    //  for the same conditions: not-found chat, member-already-
    //  present, etc.). Tests can drive both happy and failure
    // paths.

    async fn create_group(
        &self,
        title: &str,
        user_ids: &[i64],
    ) -> Result<GroupInfo, MtprotoTelegramError> {
        let mut g = self.state.lock();
        g.next_chat_id += 1;
        let chat_id = g.next_chat_id;
        // The bot is implicitly added as admin; user_ids are
        // the additional members.
        let mut members: Vec<i64> = user_ids.to_vec();
        members.push(0); // 0 = the bot (mock convention)
        let info = GroupInfo {
            chat_id,
            title: title.to_string(),
            member_count: Some(members.len() as u32),
            is_admin: Some(true),
        };
        g.groups.insert(chat_id, info.clone());
        g.group_members.insert(chat_id, members);
        Ok(info)
    }

    async fn add_participant(
        &self,
        chat_id: i64,
        user_id: i64,
    ) -> Result<(), MtprotoTelegramError> {
        let mut g = self.state.lock();
        let new_count;
        {
            let entry = g.group_members.entry(chat_id).or_default();
            if entry.contains(&user_id) {
                // Idempotent: the real client's contract is
                // that a re-add is a no-op (Telegram returns
                // success for already-present members in
                // `addChatUser`).
                return Ok(());
            }
            entry.push(user_id);
            new_count = entry.len() as u32;
        }
        if let Some(info) = g.groups.get_mut(&chat_id) {
            info.member_count = Some(new_count);
        }
        Ok(())
    }

    async fn kick_participant(
        &self,
        chat_id: i64,
        user_id: i64,
    ) -> Result<(), MtprotoTelegramError> {
        let mut g = self.state.lock();
        let new_count;
        {
            let entry = g.group_members.entry(chat_id).or_default();
            if let Some(pos) = entry.iter().position(|u| *u == user_id) {
                entry.remove(pos);
            }
            new_count = entry.len() as u32;
        }
        // Idempotent: kicking an absent user is Ok.
        if let Some(info) = g.groups.get_mut(&chat_id) {
            info.member_count = Some(new_count);
        }
        Ok(())
    }

    async fn promote_participant(
        &self,
        _chat_id: i64,
        _user_id: i64,
    ) -> Result<(), MtprotoTelegramError> {
        // Mock: the real client uses channels.editAdmin for
        // supergroups; for basic groups Telegram does not
        // have a "promote" concept (no admin/owner split).
        // The adapter layer's CoordinatorAdmin impl checks
        // `admin_capabilities().can_promote` before calling
        // this, and reports `true` only for supergroups.
        // The mock just accepts.
        Ok(())
    }

    async fn demote_participant(
        &self,
        _chat_id: i64,
        _user_id: i64,
    ) -> Result<(), MtprotoTelegramError> {
        Ok(())
    }

    async fn set_chat_title(&self, chat_id: i64, title: &str) -> Result<(), MtprotoTelegramError> {
        let mut g = self.state.lock();
        match g.groups.get_mut(&chat_id) {
            Some(info) => {
                info.title = title.to_string();
                Ok(())
            }
            None => Err(MtprotoTelegramError::Config(format!(
                "set_chat_title: chat_id {chat_id} not found"
            ))),
        }
    }

    async fn set_chat_about(
        &self,
        _chat_id: i64,
        _about: &str,
    ) -> Result<(), MtprotoTelegramError> {
        // Mock: no-op (the about text is a UI nicety; the
        // trait signature requires the call regardless).
        Ok(())
    }

    async fn delete_chat(&self, chat_id: i64) -> Result<(), MtprotoTelegramError> {
        let mut g = self.state.lock();
        g.groups.remove(&chat_id);
        g.group_members.remove(&chat_id);
        Ok(())
    }

    async fn leave_chat(&self, chat_id: i64) -> Result<(), MtprotoTelegramError> {
        let mut g = self.state.lock();
        // Idempotent: leaving a chat you're not in is Ok.
        let new_count = if let Some(entry) = g.group_members.get_mut(&chat_id) {
            entry.retain(|u| *u != 0);
            Some(entry.len() as u32)
        } else {
            None
        };
        if let (Some(c), Some(info)) = (new_count, g.groups.get_mut(&chat_id)) {
            info.member_count = Some(c);
        }
        Ok(())
    }

    async fn get_chat(&self, chat_id: i64) -> Result<GroupInfo, MtprotoTelegramError> {
        let g = self.state.lock();
        g.groups.get(&chat_id).cloned().ok_or_else(|| {
            MtprotoTelegramError::Config(format!("get_chat: chat_id {chat_id} not found"))
        })
    }

    async fn list_dialog_ids(&self) -> Result<Vec<i64>, MtprotoTelegramError> {
        let g = self.state.lock();
        Ok(g.groups.keys().copied().collect())
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

    #[tokio::test]
    async fn mock_submit_code_signals_2fa_required() {
        // Phase 2.4: with `require_2fa` set, `submit_code`
        // returns `MtprotoTelegramError::Auth("2FA_REQUIRED")`
        // matching the real-network impl's signal. Without
        // the flag, `submit_code` returns `Ok(SelfUserInfo)`.
        let c = MockTelegramMtprotoClient::new();
        c.set_require_2fa(true);
        let r = c.submit_code("12345").await;
        match r {
            Err(MtprotoTelegramError::Auth(msg)) => {
                assert_eq!(msg, "2FA_REQUIRED");
            }
            other => panic!("expected Auth(2FA_REQUIRED), got {:?}", other),
        }
        // After 2FA, submit_password completes the sign-in.
        let info = c.submit_password("hunter2").await.unwrap();
        assert_eq!(info.username.as_deref(), Some("mock_user_2fa"));
        assert!(c.state.lock().signed_in);
    }

    #[tokio::test]
    async fn mock_submit_code_no_2fa_succeeds() {
        // Default: no 2FA flag, submit_code succeeds.
        let c = MockTelegramMtprotoClient::new();
        let info = c.submit_code("12345").await.unwrap();
        assert_eq!(info.username.as_deref(), Some("mock_user"));
        assert!(c.state.lock().signed_in);
    }

    // ----- Phase 2.5: QR login mock tests -----

    #[test]
    fn build_qr_url_standard_base64_encoding() {
        // "hello" (5 bytes) → "aGVsbG8="
        // Hand-rolled base64 in build_qr_url uses the same
        // alphabet and padding as RFC 4648 §4.
        assert_eq!(build_qr_url(b"hello"), "tg://login?token=aGVsbG8=");
    }

    #[test]
    fn build_qr_url_empty_input() {
        // 0 bytes → 0 chars of base64 + "tg://login?token="
        assert_eq!(build_qr_url(b""), "tg://login?token=");
    }

    #[test]
    fn build_qr_url_one_byte_input() {
        // "a" (1 byte) → "YQ==" (2 chars + padding)
        assert_eq!(build_qr_url(b"a"), "tg://login?token=YQ==");
    }

    #[test]
    fn build_qr_url_two_bytes_input() {
        // "ab" (2 bytes) → "YWI=" (3 chars + 1 padding)
        assert_eq!(build_qr_url(b"ab"), "tg://login?token=YWI=");
    }

    #[test]
    fn build_qr_url_three_bytes_input_no_padding() {
        // "abc" (3 bytes) → "YWJj" (no padding)
        assert_eq!(build_qr_url(b"abc"), "tg://login?token=YWJj");
    }

    #[test]
    fn build_qr_url_sixteen_bytes() {
        // 16 bytes (the typical token size) → 24 base64 chars
        // (no padding because 16 is not divisible by 3, but
        // 16 % 3 == 1 → 2 padding chars).
        let token = [0u8; 16];
        let url = build_qr_url(&token);
        assert_eq!(url.len(), "tg://login?token=".len() + 24);
        assert!(url.ends_with("=="));
    }

    #[test]
    fn qr_login_handle_from_error_extracts_token_and_url() {
        let err = MtprotoTelegramError::QrLoginHandle {
            token: vec![1, 2, 3, 4],
            url: "tg://login?token=ABCD".into(),
        };
        let h = QrLoginHandle::from_error(&err).expect("from_error");
        assert_eq!(h.token, vec![1, 2, 3, 4]);
        assert_eq!(h.url, "tg://login?token=ABCD");
        assert!(h.is_pending());
    }

    #[test]
    fn qr_login_handle_from_error_returns_none_for_other_errors() {
        let err = MtprotoTelegramError::Auth("nope".into());
        assert!(QrLoginHandle::from_error(&err).is_none());
        let err = MtprotoTelegramError::Network("timeout".into());
        assert!(QrLoginHandle::from_error(&err).is_none());
    }

    #[tokio::test]
    async fn mock_qr_login_returns_qr_login_handle() {
        let c = MockTelegramMtprotoClient::new();
        let r = c.qr_login(12345, "0123456789abcdef0123456789abcdef").await;
        match r {
            Err(MtprotoTelegramError::QrLoginHandle { token, url }) => {
                assert_eq!(token.len(), 16);
                assert!(url.starts_with("tg://login?token="));
            }
            other => panic!("expected QrLoginHandle, got {:?}", other),
        }
    }

    // ---- R17-C1: QrLoginHandle struct Debug redaction ----

    #[test]
    fn qr_login_handle_struct_debug_does_not_leak_token_or_url() {
        // R17-C1: the hand-written Debug for the
        // `QrLoginHandle` struct (this file) must NOT
        // contain the raw token bytes or the
        // base64-encoded URL. Sister to the
        // `MtprotoTelegramError::QrLoginHandle` variant
        // Debug fix in error.rs. The struct is returned
        // to the caller of `connect_qr_login` as
        // `Ok(QrLoginHandle)`, so a real caller doing
        // `dbg!(handle)` or `tracing::error!(?handle)`
        // would otherwise leak the credential.
        let h = QrLoginHandle {
            token: vec![0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x08],
            url: "tg://login?token=ABCD_SECRET_BASE64_DATA".into(),
        };
        let dbg = format!("{:?}", h);
        // Token / URL must not appear in any form.
        assert!(
            !dbg.contains("ABCD_SECRET_BASE64_DATA"),
            "Debug leaked URL token: {}",
            dbg
        );
        assert!(
            !dbg.contains("[1, 2, 3"),
            "Debug leaked raw token bytes: {}",
            dbg
        );
        assert!(
            !dbg.contains("0x01") && !dbg.contains("0x08"),
            "Debug leaked raw token bytes (hex): {}",
            dbg
        );
        // The redaction marker must be present so an
        // operator reading a log line knows the field is
        // redacted (and not silently missing).
        assert!(
            dbg.contains("<redacted 8 bytes>"),
            "Debug missing token redaction marker: {}",
            dbg
        );
        assert!(
            dbg.contains("url") && dbg.contains("<redacted>"),
            "Debug missing url redaction marker: {}",
            dbg
        );
        // Variant / struct name must still be present so
        // the log line is still useful for triage.
        assert!(
            dbg.contains("QrLoginHandle"),
            "Debug missing struct name: {}",
            dbg
        );
    }

    #[tokio::test]
    async fn mock_poll_qr_login_first_call_succeeds_by_default() {
        // Default mock: qr_polls_to_success = 0, so the very
        // first poll_qr_login call returns Ok(SelfUserInfo).
        let c = MockTelegramMtprotoClient::new();
        c.qr_login(12345, "0123456789abcdef0123456789abcdef")
            .await
            .ok();
        let info = c.poll_qr_login().await.unwrap();
        assert_eq!(info.username.as_deref(), Some("mock_qr_user"));
        assert!(c.state.lock().signed_in);
    }

    #[tokio::test]
    async fn mock_poll_qr_login_returns_handle_until_threshold() {
        // Configure 2 polls before success: the next 2
        // poll_qr_login calls return Err(QrLoginHandle)
        // and the 3rd returns Ok.
        let c = MockTelegramMtprotoClient::new();
        c.qr_login(12345, "0123456789abcdef0123456789abcdef")
            .await
            .ok();
        c.set_qr_polls_to_success(2);
        for i in 0..2 {
            match c.poll_qr_login().await {
                Err(MtprotoTelegramError::QrLoginHandle { .. }) => {}
                other => panic!("poll #{}: expected QrLoginHandle, got {:?}", i, other),
            }
        }
        let info = c.poll_qr_login().await.unwrap();
        assert_eq!(info.username.as_deref(), Some("mock_qr_user"));
    }

    #[tokio::test]
    async fn mock_import_login_token_succeeds() {
        let c = MockTelegramMtprotoClient::new();
        let info = c.import_login_token(b"any-token-bytes").await.unwrap();
        assert_eq!(info.username.as_deref(), Some("mock_qr_user"));
        assert!(c.state.lock().signed_in);
    }

    #[tokio::test]
    async fn mock_qr_login_resets_poll_counter() {
        // After a successful poll, calling qr_login again
        // resets the poll counter so the next poll is
        // again counted from 0 against the existing
        // threshold.
        let c = MockTelegramMtprotoClient::new();
        c.set_qr_polls_to_success(2);
        c.qr_login(12345, "0123456789abcdef0123456789abcdef")
            .await
            .ok();
        // First 2 polls return handle (counter 1, 2).
        assert!(matches!(
            c.poll_qr_login().await,
            Err(MtprotoTelegramError::QrLoginHandle { .. })
        ));
        assert!(matches!(
            c.poll_qr_login().await,
            Err(MtprotoTelegramError::QrLoginHandle { .. })
        ));
        // 3rd poll succeeds (counter 3 > threshold 2).
        let info = c.poll_qr_login().await.unwrap();
        assert_eq!(info.username.as_deref(), Some("mock_qr_user"));
        // Calling qr_login again resets the counter so the
        // next 2 polls return handle again.
        c.qr_login(12345, "0123456789abcdef0123456789abcdef")
            .await
            .ok();
        assert!(matches!(
            c.poll_qr_login().await,
            Err(MtprotoTelegramError::QrLoginHandle { .. })
        ));
        assert!(matches!(
            c.poll_qr_login().await,
            Err(MtprotoTelegramError::QrLoginHandle { .. })
        ));
    }

    // ── CoordinatorAdmin mock tests (Phase 4 / MTProto) ──────────

    #[tokio::test]
    async fn mock_create_group_assigns_chat_id_and_bot_as_admin() {
        let c = MockTelegramMtprotoClient::new();
        let info = c
            .create_group("Phase 4 test group", &[42, 43])
            .await
            .unwrap();
        // Synthetic chat_id is monotonically
        // increasing starting at 1.
        assert_eq!(info.chat_id, 1);
        // Bot is added as admin; 2 user_ids + 1 bot = 3.
        assert_eq!(info.member_count, Some(3));
        assert_eq!(info.is_admin, Some(true));
    }

    #[tokio::test]
    async fn mock_add_and_kick_participant_updates_member_count() {
        let c = MockTelegramMtprotoClient::new();
        let info = c.create_group("g", &[10, 20]).await.unwrap();
        assert_eq!(info.member_count, Some(3)); // 2 + bot
        c.add_participant(info.chat_id, 30).await.unwrap();
        let after_add = c.get_chat(info.chat_id).await.unwrap();
        assert_eq!(after_add.member_count, Some(4));
        // Idempotent: re-adding 30 is a no-op.
        c.add_participant(info.chat_id, 30).await.unwrap();
        let after_redup = c.get_chat(info.chat_id).await.unwrap();
        assert_eq!(after_redup.member_count, Some(4));
        c.kick_participant(info.chat_id, 30).await.unwrap();
        let after_kick = c.get_chat(info.chat_id).await.unwrap();
        assert_eq!(after_kick.member_count, Some(3));
        // Idempotent: kicking an absent user is Ok.
        c.kick_participant(info.chat_id, 999).await.unwrap();
        let after_absent = c.get_chat(info.chat_id).await.unwrap();
        assert_eq!(after_absent.member_count, Some(3));
    }

    #[tokio::test]
    async fn mock_set_chat_title_updates_title() {
        let c = MockTelegramMtprotoClient::new();
        let info = c.create_group("original", &[]).await.unwrap();
        c.set_chat_title(info.chat_id, "renamed").await.unwrap();
        let after = c.get_chat(info.chat_id).await.unwrap();
        assert_eq!(after.title, "renamed");
    }

    #[tokio::test]
    async fn mock_get_chat_unknown_returns_config_error() {
        let c = MockTelegramMtprotoClient::new();
        let err = c.get_chat(99999).await.unwrap_err();
        match err {
            MtprotoTelegramError::Config(msg) => {
                assert!(msg.contains("99999"), "msg should mention chat_id");
            }
            other => panic!("expected Config error, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn mock_delete_chat_removes_from_state() {
        let c = MockTelegramMtprotoClient::new();
        let info = c.create_group("g", &[]).await.unwrap();
        c.delete_chat(info.chat_id).await.unwrap();
        // Subsequent get_chat returns Config error.
        let err = c.get_chat(info.chat_id).await.unwrap_err();
        assert!(matches!(err, MtprotoTelegramError::Config(_)));
        // list_dialog_ids is now empty.
        let ids = c.list_dialog_ids().await.unwrap();
        assert!(ids.is_empty());
    }

    #[tokio::test]
    async fn mock_list_dialog_ids_returns_created_groups_sorted() {
        let c = MockTelegramMtprotoClient::new();
        c.create_group("first", &[]).await.unwrap();
        c.create_group("second", &[]).await.unwrap();
        c.create_group("third", &[]).await.unwrap();
        let ids = c.list_dialog_ids().await.unwrap();
        // BTreeMap iteration is sorted; created ids are
        // 1, 2, 3.
        assert_eq!(ids, vec![1, 2, 3]);
    }

    #[tokio::test]
    async fn mock_set_mock_group_pre_seeds_for_read_path() {
        let c = MockTelegramMtprotoClient::new();
        // Pre-seed a group with chat_id = 42.
        c.set_mock_group(
            crate::client::GroupInfo {
                chat_id: 42,
                title: "preexisting".into(),
                member_count: Some(5),
                is_admin: Some(false),
            },
            vec![100, 101, 102, 103, 104],
        );
        // get_chat finds it.
        let info = c.get_chat(42).await.unwrap();
        assert_eq!(info.title, "preexisting");
        assert_eq!(info.member_count, Some(5));
        // list_dialog_ids includes it.
        let ids = c.list_dialog_ids().await.unwrap();
        assert!(ids.contains(&42));
    }
}
