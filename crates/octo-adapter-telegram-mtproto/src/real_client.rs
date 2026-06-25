//! ## Status: Phase 2 in progress
//!
//! Bot-mode `sign_in_bot`, `sign_out`, and the user-mode auth
//! flow (`request_login_code` / `submit_code` /
//! `submit_password`) are all wired to the real `grammers`
//! client. The user-mode flow drives the `UserAuthLifecycle`
//! state machine (`NoCredentials → PhoneProvided → SmsCodeSent
//! → SmsCodeProvided → SignedIn`, or via `PasswordRequired →
//! PasswordProvided → SignedIn` if the account has 2FA
//! enabled). Phase 2.5 (QR login) and Phase 2.7 (session
//! persistence integration) are still pending.
//!
//! ## Storage
//!
//! The `StoolapSession` is the canonical session store. The
//! `SenderPool` reads/writes via the `grammers_session::Session`
//! trait which our `StoolapSession` impls. The
//! `RealTelegramMtprotoClient` additionally holds a typed
//! `Arc<StoolapSession>` so `sign_out` can call
//! `StoolapSession::reset()` to wipe the on-disk store.
//!
//! ## User-mode state
//!
//! Across the multi-step user-mode flow, the real client holds:
//! - `user_auth_state: Mutex<UserAuthLifecycle>` — the
//!   state-machine cursor. Every action goes through
//!   `next_user_auth_state` (client-side) and
//!   `next_user_auth_state_server` (server-side) so the
//!   adapter can audit transitions.
//! - `pending_login: Mutex<Option<grammers_client::LoginToken>>` —
//!   returned by `Client::request_login_code` and consumed by
//!   `Client::sign_in`. Lives only between `request_login_code`
//!   and `submit_code`.
//! - `pending_password: Mutex<Option<grammers_client::PasswordToken>>` —
//!   returned by `Client::sign_in` on `SignInError::PasswordRequired`
//!   and consumed by `Client::check_password`. Lives only between
//!   `submit_code` (when it returns `2FA_REQUIRED`) and
//!   `submit_password`.
//!
//! All three are reset on `sign_out` so a fresh sign-in
//! attempt starts from `NoCredentials`.

#![cfg(feature = "real-network")]

use std::sync::Arc;

use async_trait::async_trait;
use grammers_client::client::{LoginToken, PasswordToken};
use grammers_client::media::Downloadable;
use grammers_client::sender::SenderPool;
use grammers_client::SignInError;
use grammers_tl_types as tl;
use grammers_tl_types::{Deserializable, Serializable};
use tokio::task::JoinHandle;
use tracing::{error, warn};

use crate::auth::{
    next_user_auth_state, next_user_auth_state_server, MtprotoAuthError, UserAuthAction,
    UserAuthServerEvent,
};
use crate::client::{
    build_qr_url, GroupInfo, InvitePreview, MtprotoSentMessage, MtprotoTelegramClient,
    MtprotoTelegramUpdate, SelfUserInfo,
};
use crate::error::MtprotoTelegramError;
use crate::lifecycle::UserAuthLifecycle;
use crate::peer_resolve::{
    peer_to_input_channel, peer_to_input_peer, peer_to_input_user, resolve_chat, resolve_user,
};
use crate::self_handle::MtprotoSelfHandle;
use crate::session::StoolapSession;

/// Extract a `SelfUserInfo` from a `tl::enums::auth::Authorization`.
///
/// `LoginTokenSuccess.authorization` is `tl::enums::auth::Authorization`
/// (the enum). Its only payload variant carries the
/// `tl::types::auth::Authorization` struct, which itself
/// holds `user: tl::enums::User` (the user enum: `Empty`
/// or `User`). For the `SignUpRequired` variant we fall
/// back to zeros — same behaviour as the legacy Phase 2.4
/// code.
fn extract_self_user_info(authorization: tl::enums::auth::Authorization) -> SelfUserInfo {
    match authorization {
        tl::enums::auth::Authorization::Authorization(inner) => {
            // `tl::enums::User::id()` collapses both
            // `Empty(UserEmpty)` and `User(User)` to the
            // inner i64.
            let user_id = inner.user.id();
            // Username lives on the inner `User` struct
            // only (the `UserEmpty` variant has no
            // username). Filter out empty strings so the
            // optional is well-defined.
            let username = match &inner.user {
                tl::enums::User::User(u) => u.username.clone(),
                tl::enums::User::Empty(_) => None,
            }
            .filter(|s| !s.is_empty());
            SelfUserInfo {
                user_id,
                username,
                access_hash: 0,
            }
        }
        _ => SelfUserInfo {
            user_id: 0,
            username: None,
            access_hash: 0,
        },
    }
}

/// Branch on the chat_id kind. We can't reuse
/// `peer_resolve::chat_id_to_peer_id` here because the
/// kind is used to drive a `match` at the call site; the
/// full `PeerId` isn't needed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum PeerKindChoice {
    Basic,
    Supergroup,
}

fn chat_id_kind(chat_id: i64) -> PeerKindChoice {
    use crate::peer_resolve::chat_id_to_peer_id;
    use grammers_session::types::PeerKind;
    match chat_id_to_peer_id(chat_id).kind() {
        PeerKind::Channel => PeerKindChoice::Supergroup,
        _ => PeerKindChoice::Basic,
    }
}

/// Extract the freshly-created channel id from a
/// `Updates` payload returned by `channels.createChannel`.
///
/// The TL shape of `Updates` varies across schema layers;
/// the canonical pattern is `Updates::Update` containing a
/// `tl::types::UpdateChat` (carrying the chat_id) or a
/// `tl::enums::Chat` list (`Channel`) with the channel_id.
/// We handle both shapes plus the wrapper-by-Chat
/// variants.
fn extract_created_channel_id(updates: &tl::enums::Updates) -> Option<i64> {
    use tl::enums::Updates as UpdatesEnum;
    match updates {
        UpdatesEnum::Combined(combined) => {
            for upd in &combined.updates {
                if let Some(id) = extract_channel_id_from_update_obj(upd) {
                    return Some(id);
                }
            }
            for chat in &combined.chats {
                if let Some(id) = channel_id_from_chat_enum(chat) {
                    return Some(id);
                }
            }
            None
        }
        UpdatesEnum::Updates(updates_list) => {
            for upd in &updates_list.updates {
                if let Some(id) = extract_channel_id_from_update_obj(upd) {
                    return Some(id);
                }
            }
            for chat in &updates_list.chats {
                if let Some(id) = channel_id_from_chat_enum(chat) {
                    return Some(id);
                }
            }
            None
        }
        _ => None,
    }
}

/// Pull a channel id from a single `Update` enum (one
/// variant of `tl::enums::Update`).
fn extract_channel_id_from_update_obj(upd: &tl::enums::Update) -> Option<i64> {
    use tl::enums::Update as UpdateEnum;
    match upd {
        UpdateEnum::Channel(u) => {
            // UpdateChannel carries the channel id.
            Some(u.channel_id)
        }
        _ => None,
    }
}

/// Pull a channel id from a `tl::enums::Chat` (the chat
/// enum). Returns the chat id if the variant carries a
/// `Channel` (supergroup) or `Chat` (basic group).
fn channel_id_from_chat_enum(chat: &tl::enums::Chat) -> Option<i64> {
    use tl::enums::Chat as ChatEnum;
    match chat {
        ChatEnum::Channel(c) => Some(c.id),
        ChatEnum::Chat(c) => Some(c.id),
        _ => None,
    }
}

/// Extract a single `GroupInfo` from the response of
/// `messages::GetChats` or `channels::GetChannels`. Both
/// return `tl::enums::messages::Chats`. We pick the first
/// chat in the list (caller requests one at a time).
fn extract_chat_info(chats_response: tl::enums::messages::Chats) -> Option<GroupInfo> {
    use tl::enums::messages::Chats as ChatsEnum;
    let chats = match chats_response {
        ChatsEnum::Chats(c) => c.chats,
        ChatsEnum::Slice(c) => c.chats,
    };
    // Take the first chat (caller requests one at a time).
    let first = chats.into_iter().next()?;
    chat_enum_to_group_info(&first)
}

/// Convert a `tl::enums::Chat` to a `GroupInfo`. Returns
/// `None` for chat variants we don't surface (e.g.,
/// `ChatForbidden`, `ChannelForbidden`).
fn chat_enum_to_group_info(chat: &tl::enums::Chat) -> Option<GroupInfo> {
    use tl::enums::Chat as ChatEnum;
    match chat {
        ChatEnum::Chat(c) => Some(GroupInfo {
            chat_id: c.id,
            title: c.title.clone(),
            member_count: u32::try_from(c.participants_count.max(0)).ok(),
            is_admin: c.admin_rights.as_ref().map(|_| true),
            about: None,
        }),
        ChatEnum::Channel(c) => Some(GroupInfo {
            chat_id: c.id,
            title: c.title.clone(),
            member_count: c
                .participants_count
                .and_then(|n| u32::try_from(n.max(0)).ok()),
            is_admin: c.admin_rights.as_ref().map(|_| true),
            about: None,
        }),
        _ => None,
    }
}

/// Wrapper around `grammers_client::Client` that implements
/// `MtprotoTelegramClient`. Constructed via
/// `RealTelegramMtprotoClient::connect`.
pub struct RealTelegramMtprotoClient {
    #[allow(dead_code)]
    client: Arc<grammers_client::Client>,
    /// Join handle for the SenderPool runner task. Dropped
    /// (and aborted) on `shutdown`.
    #[allow(dead_code)]
    runner: parking_lot::Mutex<Option<JoinHandle<()>>>,
    /// Typed handle to the session so `sign_out` can call
    /// `StoolapSession::reset()`. The same `Arc<StoolapSession>`
    /// is also held by the SenderPool, so a single reset wipes
    /// the on-disk store from both sides.
    session: Arc<StoolapSession>,
    /// Shared self-handle. Populated by the `sign_in_*`
    /// methods after a successful `get_me()`.
    self_handle: MtprotoSelfHandle,
    /// User-mode lifecycle cursor. Always present; starts at
    /// `NoCredentials` after `connect` and is reset to
    /// `NoCredentials` on `sign_out`.
    user_auth_state: parking_lot::Mutex<UserAuthLifecycle>,
    /// `LoginToken` returned by `Client::request_login_code`.
    /// Set by `request_login_code`, consumed by
    /// `submit_code`. `None` outside the request_login_code
    /// → submit_code window.
    pending_login: parking_lot::Mutex<Option<LoginToken>>,
    /// `PasswordToken` returned by `Client::sign_in` on
    /// `SignInError::PasswordRequired`. Set when
    /// `submit_code` returns `2FA_REQUIRED`, consumed by
    /// `submit_password`. `None` outside the
    /// submit_code(2FA) → submit_password window.
    pending_password: parking_lot::Mutex<Option<PasswordToken>>,
    /// Phase 2.5: api_id used for the current QR login
    /// attempt. Set by `qr_login`, used by `poll_qr_login`
    /// and `import_login_token` to re-invoke the same TL
    /// functions.
    qr_api_id: parking_lot::Mutex<Option<i32>>,
    /// Phase 2.5: api_hash used for the current QR login
    /// attempt. Set by `qr_login`, used by `poll_qr_login`.
    /// Wrapped in `Zeroizing<String>` (R15-C17 fix) so the
    /// sensitive `api_hash` is wiped from memory on drop
    /// (the API hash is a credential — anyone with both
    /// the api_id and api_hash can sign MTProto requests
    /// as our app, which would let them re-use our session
    /// if they also obtained the auth_key).
    qr_api_hash: parking_lot::Mutex<Option<zeroize::Zeroizing<String>>>,
    /// Phase 2.5: token bytes returned by the most recent
    /// successful `auth.exportLoginToken` call. Used by
    /// `poll_qr_login` to detect when the token changes
    /// (the user scanned) and by `import_login_token`
    /// to finalize the import.
    qr_token: parking_lot::Mutex<Option<Vec<u8>>>,
    /// Target DC for QR login. Set when the first
    /// `exportLoginToken` returns `MigrateTo`. Subsequent
    /// poll/import calls use `invoke_in_dc` on this DC
    /// instead of the home DC, avoiding repeated MigrateTo
    /// cycles and token rotation.
    qr_dc_id: parking_lot::Mutex<Option<i32>>,
    /// Update channel from SenderPool. The SenderPool's
    /// `updates` receiver is captured at connect time and
    /// drained by `receive_updates()`. Wrapped in an async
    /// Mutex for interior mutability (the trait method takes
    /// `&self`).
    updates_rx: tokio::sync::Mutex<Option<grammers_client::client::UpdateStream>>,
}

impl RealTelegramMtprotoClient {
    /// Connect to Telegram and prepare a client. Does NOT
    /// sign in; the caller chooses the auth mode (bot or
    /// user) and calls `sign_in_bot` /
    /// `request_login_code` accordingly.
    ///
    /// `api_id` and `api_hash` are required (from
    /// my.telegram.org). `session` is the persistence
    /// handle; pass a `StoolapSession::open(path)` or
    /// `StoolapSession::open_in_memory()`.
    pub async fn connect(
        api_id: i32,
        _api_hash: &str,
        session: Arc<StoolapSession>,
        self_handle: MtprotoSelfHandle,
    ) -> Result<Arc<Self>, MtprotoTelegramError> {
        // The `SenderPool::new<S: Session + 'static>(session: Arc<S>, api_id: i32)`
        // signature requires a concrete `Arc<S>` (not `Arc<dyn Session>`).
        // `StoolapSession` implements `Session`, so the clone here is
        // straightforward.
        let SenderPool {
            runner,
            updates,
            handle,
        } = SenderPool::new(session.clone(), api_id);
        let client = Arc::new(grammers_client::Client::new(handle.clone()));
        let runner_task = tokio::spawn(runner.run());

        // Create the update stream from the SenderPool's
        // update channel. The stream handles gap resolution,
        // channel differences, and ordered delivery.
        let update_stream = client
            .stream_updates(
                updates,
                grammers_client::client::UpdatesConfiguration {
                    catch_up: true,
                    ..Default::default()
                },
            )
            .await;

        Ok(Arc::new(Self {
            client,
            runner: parking_lot::Mutex::new(Some(runner_task)),
            session,
            self_handle,
            user_auth_state: parking_lot::Mutex::new(UserAuthLifecycle::NoCredentials),
            pending_login: parking_lot::Mutex::new(None),
            pending_password: parking_lot::Mutex::new(None),
            qr_api_id: parking_lot::Mutex::new(None),
            qr_api_hash: parking_lot::Mutex::new(None),
            qr_token: parking_lot::Mutex::new(None),
            qr_dc_id: parking_lot::Mutex::new(None),
            updates_rx: tokio::sync::Mutex::new(Some(update_stream)),
        }))
    }

    /// Read-only accessor for the underlying grammers
    /// client. Used by the `MtprotoTelegramAdapter` when it
    /// needs access to RPCs that are not modelled on the
    /// `MtprotoTelegramClient` trait (e.g., `iter_dialogs`
    /// for group discovery).
    #[allow(dead_code)]
    pub fn grammers_client(&self) -> &grammers_client::Client {
        &self.client
    }

    /// Helper: drive the user-mode state machine through the
    /// two SignOut transitions (`SignedIn → SigningOut →
    /// SignedOut`). Called from `sign_out` and from
    /// any other place that tears down user-mode state.
    /// Errors are deliberately swallowed: sign-out is a
    /// best-effort cleanup and we don't want a state-machine
    /// mismatch to block the session reset.
    fn maybe_transition_user_signout(&self) -> Result<(), MtprotoAuthError> {
        use UserAuthLifecycle::*;
        match *self.user_auth_state.lock() {
            SignedIn => {
                let s = next_user_auth_state(UserAuthAction::SignOut, SignedIn)?;
                *self.user_auth_state.lock() = s;
                let s = next_user_auth_state(UserAuthAction::SignOut, SigningOut)?;
                *self.user_auth_state.lock() = s;
            }
            SigningOut => {
                let s = next_user_auth_state(UserAuthAction::SignOut, SigningOut)?;
                *self.user_auth_state.lock() = s;
            }
            _ => {
                // NoCredentials, PhoneProvided, SmsCodeSent,
                // SmsCodeProvided, PasswordRequired,
                // PasswordProvided, QrLoginPending,
                // QrLoginConfirmed, SignedOut: no transition
                // to perform.
            }
        }
        Ok(())
    }
}

#[async_trait]
impl MtprotoTelegramClient for RealTelegramMtprotoClient {
    async fn send_message(
        &self,
        chat_id: i64,
        text: &str,
    ) -> Result<MtprotoSentMessage, MtprotoTelegramError> {
        let prefix = "send_message";
        // Resolve the chat to an InputPeer. The peer-kind
        // boundary (basic vs. supergroup) is handled
        // transparently by `peer_resolve::resolve_chat`.
        let peer_kind = chat_id_kind(chat_id);
        let chat_peer = resolve_chat(
            &self.client,
            chat_id,
            peer_kind == PeerKindChoice::Supergroup,
        )
        .await?;
        let input_peer = peer_to_input_peer(&chat_peer).await?;
        let message = self
            .client
            .invoke(&tl::functions::messages::SendMessage {
                no_webpage: false,
                silent: false,
                background: false,
                clear_draft: false,
                noforwards: false,
                update_stickersets_order: false,
                invert_media: false,
                allow_paid_floodskip: false,
                peer: input_peer,
                reply_to: None,
                message: text.to_string(),
                random_id: generate_random_id_i64(),
                reply_markup: None,
                entities: None,
                schedule_date: None,
                schedule_repeat_period: None,
                send_as: None,
                quick_reply_shortcut: None,
                effect: None,
                allow_paid_stars: None,
                suggested_post: None,
            })
            .await
            .map_err(|e| crate::peer_resolve::map_invoke_err(prefix, e))?;
        // Extract the first sent message from the Updates
        // payload. `messages.SendMessage` always returns a
        // single UpdateNewMessage / UpdateNewScheduledMessage
        // variant.
        extract_first_message_id_and_date(&message).ok_or_else(|| MtprotoTelegramError::Rpc {
            code: 500,
            message: format!("{prefix}: SendMessage returned Updates without a message id"),
        })
    }

    async fn send_document(
        &self,
        chat_id: i64,
        caption: &str,
        filename: &str,
        data: &[u8],
    ) -> Result<MtprotoSentMessage, MtprotoTelegramError> {
        let prefix = "send_document";
        // Step 1: upload the file in chunks. The high-level
        // `Client::upload_stream` does the chunked
        // `upload.saveFilePart` calls and returns an
        // `Uploaded { raw: InputFile }` we can wrap into
        // `InputMediaUploadedDocument`. We wrap `&data[..]`
        // in a `Cursor` so it implements `AsyncRead`.
        use std::io::Cursor;
        let mut cursor = Cursor::new(data);
        let size = data.len();
        let uploaded = self
            .client
            .upload_stream(&mut cursor, size, filename.to_string())
            .await
            .map_err(|e| MtprotoTelegramError::Rpc {
                code: 500,
                message: format!("{prefix}: upload_stream: {e}"),
            })?;

        // Step 2: resolve the chat (peer resolve).
        let peer_kind = chat_id_kind(chat_id);
        let chat_peer = resolve_chat(
            &self.client,
            chat_id,
            peer_kind == PeerKindChoice::Supergroup,
        )
        .await?;
        let input_peer = peer_to_input_peer(&chat_peer).await?;

        // Step 3: send via `messages.sendMedia` with an
        // `InputMediaUploadedDocument`. Telegram treats this
        // as a "document" attachment; caption is the same
        // field as text messages.
        let req = tl::functions::messages::SendMedia {
            silent: false,
            background: false,
            clear_draft: false,
            noforwards: false,
            update_stickersets_order: false,
            invert_media: false,
            allow_paid_floodskip: false,
            peer: input_peer,
            reply_to: None,
            media: tl::enums::InputMedia::UploadedDocument(tl::types::InputMediaUploadedDocument {
                nosound_video: false,
                force_file: false,
                spoiler: false,
                file: uploaded.raw,
                thumb: None,
                mime_type: guess_mime_type(filename),
                attributes: vec![tl::enums::DocumentAttribute::Filename(
                    tl::types::DocumentAttributeFilename {
                        file_name: filename.to_string(),
                    },
                )],
                stickers: None,
                ttl_seconds: None,
                video_cover: None,
                video_timestamp: None,
            }),
            message: caption.to_string(),
            random_id: generate_random_id_i64(),
            reply_markup: None,
            entities: None,
            schedule_date: None,
            schedule_repeat_period: None,
            send_as: None,
            quick_reply_shortcut: None,
            effect: None,
            allow_paid_stars: None,
            suggested_post: None,
        };
        let updates = self
            .client
            .invoke(&req)
            .await
            .map_err(|e| crate::peer_resolve::map_invoke_err(prefix, e))?;
        extract_first_message_id_and_date(&updates).ok_or_else(|| MtprotoTelegramError::Rpc {
            code: 500,
            message: format!("{prefix}: SendMedia returned Updates without a message id"),
        })
    }

    async fn download_file(&self, file_id: &str) -> Result<Vec<u8>, MtprotoTelegramError> {
        let prefix = "download_file";
        // The `file_id` contract is a hex-encoded
        // `tl::enums::InputFileLocation`. The mock client
        // stores this as a placeholder; the real client
        // round-trips the same format.
        let bytes = hex_decode(file_id).ok_or_else(|| MtprotoTelegramError::Rpc {
            code: 400,
            message: format!("{prefix}: file_id is not valid hex ({file_id:?})"),
        })?;
        let location: tl::enums::InputFileLocation =
            tl::enums::InputFileLocation::from_bytes(&bytes).map_err(|e| {
                MtprotoTelegramError::Rpc {
                    code: 400,
                    message: format!("{prefix}: deserialize InputFileLocation: {e}"),
                }
            })?;
        // Wrap the InputFileLocation in a Downloadable and
        // stream chunks into a Vec<u8>.
        let downloadable = InputFileLocationDownloadable { location };
        let mut download = self.client.iter_download(&downloadable);
        let mut out = Vec::new();
        loop {
            match download.next().await {
                Ok(Some(chunk)) => out.extend_from_slice(&chunk),
                Ok(None) => break,
                Err(e) => {
                    return Err(crate::peer_resolve::map_invoke_err(prefix, e));
                }
            }
        }
        Ok(out)
    }

    async fn download_file_to_writer(
        &self,
        file_id: &str,
        writer: &mut (dyn tokio::io::AsyncWrite + Unpin + Send),
    ) -> Result<u64, MtprotoTelegramError> {
        let prefix = "download_file_to_writer";
        let bytes = hex_decode(file_id).ok_or_else(|| MtprotoTelegramError::Rpc {
            code: 400,
            message: format!("{prefix}: file_id is not valid hex ({file_id:?})"),
        })?;
        let location: tl::enums::InputFileLocation =
            tl::enums::InputFileLocation::from_bytes(&bytes).map_err(|e| {
                MtprotoTelegramError::Rpc {
                    code: 400,
                    message: format!("{prefix}: deserialize InputFileLocation: {e}"),
                }
            })?;
        let downloadable = InputFileLocationDownloadable { location };
        let mut download = self.client.iter_download(&downloadable);
        let mut total: u64 = 0;
        use tokio::io::AsyncWriteExt;
        loop {
            match download.next().await {
                Ok(Some(chunk)) => {
                    writer.write_all(&chunk).await.map_err(|e| {
                        MtprotoTelegramError::Network(format!("{prefix}: write: {e}"))
                    })?;
                    total += chunk.len() as u64;
                }
                Ok(None) => break,
                Err(e) => {
                    return Err(crate::peer_resolve::map_invoke_err(prefix, e));
                }
            }
        }
        Ok(total)
    }

    async fn receive_updates(&self) -> Result<Vec<MtprotoTelegramUpdate>, MtprotoTelegramError> {
        use grammers_client::update::Update as GUpdate;

        let mut stream_guard = self.updates_rx.lock().await;
        let stream = match stream_guard.as_mut() {
            Some(s) => s,
            None => return Ok(Vec::new()),
        };

        let mut result = Vec::new();
        // Drain all currently-available updates. We use a
        // short timeout so we don't block indefinitely if no
        // updates are pending.
        loop {
            let update =
                match tokio::time::timeout(std::time::Duration::from_millis(50), stream.next())
                    .await
                {
                    Ok(Ok(update)) => update,
                    Ok(Err(e)) => {
                        warn!("receive_updates: stream error: {}", e);
                        break;
                    }
                    Err(_) => break, // timeout — no more pending
                };

            match update {
                GUpdate::NewMessage(msg) => {
                    let peer_id = msg.peer_id();
                    let chat_id = peer_id.bare_id();
                    let from_id = msg.sender_id().map(|id| id.bare_id());
                    let document_id = msg.media().and_then(|media| {
                        if let grammers_client::media::Media::Document(doc) = media {
                            let location = doc.to_raw_input_location()?;
                            let bytes = location.to_bytes();
                            Some(
                                bytes
                                    .iter()
                                    .map(|b| format!("{:02x}", b))
                                    .collect::<String>(),
                            )
                        } else {
                            None
                        }
                    });
                    let caption = if document_id.is_some() {
                        Some(msg.text().to_string())
                    } else {
                        None
                    };
                    result.push(MtprotoTelegramUpdate::NewMessage(
                        crate::client::NewMessage {
                            chat_id,
                            message: msg.text().to_string(),
                            from_id,
                            message_id: msg.id() as i64,
                            document_id,
                            caption,
                            timestamp: msg.date().timestamp(),
                        },
                    ));
                }
                GUpdate::MessageEdited(msg) => {
                    let peer_id = msg.peer_id();
                    let chat_id = peer_id.bare_id();
                    result.push(MtprotoTelegramUpdate::MessageEdited(
                        crate::client::MessageEdited {
                            chat_id,
                            message_id: msg.id() as i64,
                            new_text: msg.text().to_string(),
                            timestamp: msg.date().timestamp(),
                        },
                    ));
                }
                // CallbackQuery, InlineQuery, InlineSend,
                // MessageDeleted, Raw — not surfaced to the
                // adapter's receive path.
                _ => {}
            }
        }

        Ok(result)
    }

    async fn sign_in_bot(
        &self,
        bot_token: &str,
        _api_id: i32,
        api_hash: &str,
    ) -> Result<SelfUserInfo, MtprotoTelegramError> {
        match self.client.bot_sign_in(bot_token, api_hash).await {
            Ok(user) => {
                let info = SelfUserInfo {
                    user_id: user.id().bare_id(),
                    username: user.username().map(String::from),
                    // grammers' `User` does not expose `access_hash`
                    // directly; the cache_peer call inside
                    // `bot_sign_in` stores it for us, so we
                    // don't need it here.
                    access_hash: 0,
                };
                self.self_handle
                    .set_identity(info.user_id, info.username.clone());
                Ok(info)
            }
            Err(e) => {
                error!(error = %e, "bot_sign_in failed");
                Err(MtprotoTelegramError::Auth(format!(
                    "bot_sign_in: {}",
                    crate::error::redact_credentials(&e.to_string())
                )))
            }
        }
    }

    async fn request_login_code(
        &self,
        _api_id: i32,
        api_hash: &str,
        phone: &str,
    ) -> Result<(), MtprotoTelegramError> {
        // 1. Drive the state machine (client-side):
        //    `NoCredentials → PhoneProvided` on `RequestCode`.
        let new_state = {
            let current = *self.user_auth_state.lock();
            next_user_auth_state(
                UserAuthAction::RequestCode {
                    phone: phone.to_string(),
                },
                current,
            )?
        };
        *self.user_auth_state.lock() = new_state;

        // 2. Call grammers' `Client::request_login_code`. On
        //    success, stash the `LoginToken` and advance
        //    `PhoneProvided → SmsCodeSent` (server-side).
        match self.client.request_login_code(phone, api_hash).await {
            Ok(login_token) => {
                let new_state = {
                    let current = *self.user_auth_state.lock();
                    next_user_auth_state_server(UserAuthServerEvent::RequestCodeSucceeded, current)?
                };
                *self.user_auth_state.lock() = new_state;
                *self.pending_login.lock() = Some(login_token);
                Ok(())
            }
            Err(e) => {
                // Server didn't accept the phone. Roll the
                // state machine back to NoCredentials so the
                // operator can retry with a corrected phone.
                *self.user_auth_state.lock() = UserAuthLifecycle::NoCredentials;
                error!(error = %e, "Client::request_login_code failed");
                Err(MtprotoTelegramError::Auth(format!(
                    "request_login_code: {}",
                    crate::error::redact_credentials(&e.to_string())
                )))
            }
        }
    }

    async fn submit_code(&self, code: &str) -> Result<SelfUserInfo, MtprotoTelegramError> {
        // 1. Pull the stashed LoginToken. If missing, the
        //    caller skipped `request_login_code` — that's a
        //    state-machine violation.
        let token = self.pending_login.lock().take().ok_or_else(|| {
            MtprotoTelegramError::Auth(
                "submit_code called without a prior request_login_code".into(),
            )
        })?;

        // 2. Drive the state machine (client-side):
        //    `SmsCodeSent → SmsCodeProvided` on `SubmitCode`.
        let new_state = {
            let current = *self.user_auth_state.lock();
            next_user_auth_state(
                UserAuthAction::SubmitCode {
                    code: code.to_string(),
                },
                current,
            )?
        };
        *self.user_auth_state.lock() = new_state;

        // 3. Call grammers' `Client::sign_in`.
        match self.client.sign_in(&token, code).await {
            Ok(user) => {
                // 4a. Server succeeded. Advance
                //     `SmsCodeProvided → SignedIn` and
                //     populate the self-handle.
                let new_state = {
                    let current = *self.user_auth_state.lock();
                    next_user_auth_state_server(UserAuthServerEvent::SignInSucceeded, current)?
                };
                *self.user_auth_state.lock() = new_state;
                let info = SelfUserInfo {
                    user_id: user.id().bare_id(),
                    username: user.username().map(String::from),
                    access_hash: 0,
                };
                self.self_handle
                    .set_identity(info.user_id, info.username.clone());
                Ok(info)
            }
            Err(SignInError::PasswordRequired(password_token)) => {
                // 4b. Server returned SESSION_PASSWORD_NEEDED.
                //     Stash the password token, advance
                //     `SmsCodeProvided → PasswordRequired`, and
                //     signal the caller via the trait-level
                //     sentinel `MtprotoTelegramError::Auth("2FA_REQUIRED")`.
                let new_state = {
                    let current = *self.user_auth_state.lock();
                    next_user_auth_state_server(UserAuthServerEvent::PasswordRequired, current)?
                };
                *self.user_auth_state.lock() = new_state;
                *self.pending_password.lock() = Some(password_token);
                Err(MtprotoTelegramError::Auth("2FA_REQUIRED".into()))
            }
            Err(SignInError::InvalidCode) => {
                // Roll the state back to SmsCodeSent so the
                // operator can retry with a corrected code.
                // The next `submit_code` call is then valid.
                *self.user_auth_state.lock() = UserAuthLifecycle::SmsCodeSent;
                Err(MtprotoTelegramError::Auth("invalid code".into()))
            }
            Err(SignInError::Other(e)) => {
                // Generic failure — roll back to SmsCodeSent
                // so the operator can retry.
                *self.user_auth_state.lock() = UserAuthLifecycle::SmsCodeSent;
                error!(error = %e, "Client::sign_in failed");
                Err(MtprotoTelegramError::Auth(format!(
                    "sign_in: {}",
                    crate::error::redact_credentials(&e.to_string())
                )))
            }
            Err(SignInError::SignUpRequired) => {
                // grammers does not support third-party sign-up.
                // Reset state to NoCredentials; the user must
                // create their account on an official client
                // first.
                *self.user_auth_state.lock() = UserAuthLifecycle::NoCredentials;
                Err(MtprotoTelegramError::Auth(
                    "sign-up required (use an official Telegram client first)".into(),
                ))
            }
            Err(SignInError::InvalidPassword(_)) => {
                // Not expected from `sign_in` — `sign_in`
                // returns `InvalidPassword` only from
                // `check_password`. Treat as a generic
                // failure.
                *self.user_auth_state.lock() = UserAuthLifecycle::SmsCodeSent;
                Err(MtprotoTelegramError::Auth(
                    "unexpected invalid-password from sign_in".into(),
                ))
            }
        }
    }

    async fn submit_password(&self, password: &str) -> Result<SelfUserInfo, MtprotoTelegramError> {
        // 1. Pull the stashed PasswordToken. If missing, the
        //    caller skipped `submit_code` (or `submit_code`
        //    did not return `2FA_REQUIRED`).
        let password_token = self.pending_password.lock().take().ok_or_else(|| {
            MtprotoTelegramError::Auth(
                "submit_password called without a 2FA_REQUIRED from submit_code".into(),
            )
        })?;

        // 2. Drive the state machine (client-side):
        //    `PasswordRequired → PasswordProvided` on `SubmitPassword`.
        let new_state = {
            let current = *self.user_auth_state.lock();
            next_user_auth_state(
                UserAuthAction::SubmitPassword {
                    password: password.to_string(),
                },
                current,
            )?
        };
        *self.user_auth_state.lock() = new_state;

        // 3. Call grammers' `Client::check_password`.
        match self
            .client
            .check_password(password_token, password.as_bytes())
            .await
        {
            Ok(user) => {
                // 4a. Server accepted the password. Advance
                //     `PasswordProvided → SignedIn` and
                //     populate the self-handle.
                let new_state = {
                    let current = *self.user_auth_state.lock();
                    next_user_auth_state_server(
                        UserAuthServerEvent::CheckPasswordSucceeded,
                        current,
                    )?
                };
                *self.user_auth_state.lock() = new_state;
                let info = SelfUserInfo {
                    user_id: user.id().bare_id(),
                    username: user.username().map(String::from),
                    access_hash: 0,
                };
                self.self_handle
                    .set_identity(info.user_id, info.username.clone());
                Ok(info)
            }
            Err(SignInError::InvalidPassword(_)) => {
                // 4b. Wrong password. Roll back to
                //     `PasswordRequired` so the operator can
                //     retry.
                *self.user_auth_state.lock() = UserAuthLifecycle::PasswordRequired;
                Err(MtprotoTelegramError::Auth("invalid password".into()))
            }
            Err(SignInError::Other(e)) => {
                *self.user_auth_state.lock() = UserAuthLifecycle::PasswordRequired;
                error!(error = %e, "Client::check_password failed");
                Err(MtprotoTelegramError::Auth(format!(
                    "check_password: {}",
                    crate::error::redact_credentials(&e.to_string())
                )))
            }
            // The remaining SignInError variants
            // (SignUpRequired, InvalidCode) are not produced
            // by `check_password`. Treat them as
            // programmer-error / internal failures.
            Err(other) => {
                *self.user_auth_state.lock() = UserAuthLifecycle::PasswordRequired;
                error!(error = %other, "unexpected SignInError from check_password");
                Err(MtprotoTelegramError::Internal(format!(
                    "check_password: unexpected {}",
                    other
                )))
            }
        }
    }

    async fn sign_out(&self) -> Result<(), MtprotoTelegramError> {
        // 1. Drive the state machine: if currently
        //    `SignedIn` or `SigningOut`, advance to `SignedOut`
        //    so a fresh sign-in attempt can start from
        //    `NoCredentials`. Errors here are non-fatal: the
        //    user might be in `NoCredentials` (never signed
        //    in) and the rest of the sign-out still needs to
        //    run.
        let _ = self.maybe_transition_user_signout();

        // 2. Call Telegram's auth.logOut to invalidate the
        //    server-side session.
        if let Err(e) = self.client.sign_out().await {
            warn!(error = %e, "auth.logOut RPC failed; continuing to wipe local state");
        }
        // 3. Wipe the local session store (DD6:
        //    mtproto_dc_option rows including auth_key;
        //    mtproto_peer_info including self_user).
        if let Err(e) = self.session.reset() {
            error!(error = %e, "StoolapSession::reset failed; signing out left on-disk artifacts");
            return Err(MtprotoTelegramError::Session(format!(
                "session reset: {}",
                e
            )));
        }
        // 4. Clear the cached self-handle.
        self.self_handle.clear();
        // 5. Reset the user-mode state machine cursor and
        //    drop any stashed login/password tokens so a
        //    fresh sign-in attempt starts clean.
        *self.user_auth_state.lock() = UserAuthLifecycle::NoCredentials;
        *self.pending_login.lock() = None;
        *self.pending_password.lock() = None;
        *self.qr_api_id.lock() = None;
        *self.qr_api_hash.lock() = None;
        *self.qr_token.lock() = None;
        Ok(())
    }

    // ----- Phase 2.5: QR login -----

    async fn qr_login(&self, api_id: i32, api_hash: &str) -> Result<(), MtprotoTelegramError> {
        // 1. Drive the state machine: NoCredentials →
        //    QrLoginPending (client).
        let new_state = {
            let current = *self.user_auth_state.lock();
            next_user_auth_state(UserAuthAction::QrLoginStart, current)?
        };
        *self.user_auth_state.lock() = new_state;

        // 2. Invoke `auth.exportLoginToken` and parse the
        //    response. The response is one of:
        //    - `LoginToken::Token { token, expires }` — emit
        //      the handle for the caller to display as a QR
        //      code. Stash the token + api_id/api_hash for
        //      the subsequent `poll_qr_login` and
        //      `import_login_token` calls.
        //    - `LoginToken::Success(Authorization)` — we're
        //      already authorized (this is a no-op QR flow).
        //      Return Ok(SelfUserInfo) and drive the state
        //      machine to SignedIn.
        //    - `LoginToken::MigrateTo { dc_id, token }` —
        //      not implemented in Phase 2.5; treat as an
        //      internal error.
        let request = tl::functions::auth::ExportLoginToken {
            api_id,
            api_hash: api_hash.to_string(),
            except_ids: Vec::new(),
        };
        let response: tl::enums::auth::LoginToken =
            self.client.invoke(&request).await.map_err(|e| {
                MtprotoTelegramError::Auth(format!(
                    "auth.exportLoginToken: {}",
                    crate::error::redact_credentials(&e.to_string())
                ))
            })?;
        match response {
            tl::enums::auth::LoginToken::Token(t) => {
                // Stash the api_id / api_hash / token for
                // the subsequent poll and import calls.
                *self.qr_api_id.lock() = Some(api_id);
                *self.qr_api_hash.lock() = Some(zeroize::Zeroizing::new(api_hash.to_string()));
                *self.qr_token.lock() = Some(t.token.clone());
                let url = build_qr_url(&t.token);
                Err(MtprotoTelegramError::QrLoginHandle {
                    token: t.token,
                    url,
                })
            }
            tl::enums::auth::LoginToken::Success(login_token_success) => {
                // Already authorized: drive the state
                // machine QrLoginPending → SignedIn.
                let new_state = {
                    let current = *self.user_auth_state.lock();
                    next_user_auth_state_server(UserAuthServerEvent::SignInSucceeded, current)?
                };
                *self.user_auth_state.lock() = new_state;
                // Pull user_id / username via the inner
                // `Authorization::Authorization` variant,
                // which carries `tl::enums::User` (itself
                // an enum: `Empty(UserEmpty)` or `User(User)`).
                // Note: `qr_login` returns `Result<(), _>`
                // (the user_id/username is exposed via the
                // `self_handle` for the adapter to read);
                // a successful `LoginToken::Success` here
                // is unusual (the session is already
                // authorised) but we still populate the
                // self-handle so the adapter can detect
                // it.
                let info = extract_self_user_info(login_token_success.authorization);
                self.self_handle
                    .set_identity(info.user_id, info.username.clone());
                Ok(())
            }
            tl::enums::auth::LoginToken::MigrateTo(migrate) => {
                // DC migration (TDesktop pattern: importTo).
                // Import the MigrateTo token on the target DC.
                // If the user already scanned, import returns
                // Success. If not, it returns a Token for QR
                // display. The token is valid on the target DC.
                let target_dc = migrate.dc_id;
                tracing::info!(
                    target_dc,
                    "exportLoginToken: MigrateTo DC {}; importing token",
                    target_dc
                );
                *self.qr_dc_id.lock() = Some(target_dc);
                // Stash credentials for poll/import.
                *self.qr_api_id.lock() = Some(api_id);
                *self.qr_api_hash.lock() = Some(zeroize::Zeroizing::new(api_hash.to_string()));
                let import_req = tl::functions::auth::ImportLoginToken {
                    token: migrate.token,
                };
                let import_resp: tl::enums::auth::LoginToken = self
                    .client
                    .invoke_in_dc(target_dc, &import_req)
                    .await
                    .map_err(|e| {
                        MtprotoTelegramError::Auth(format!(
                            "auth.importLoginToken on DC {}: {}",
                            target_dc,
                            crate::error::redact_credentials(&e.to_string())
                        ))
                    })?;
                match import_resp {
                    tl::enums::auth::LoginToken::Token(t) => {
                        *self.qr_token.lock() = Some(t.token.clone());
                        let url = build_qr_url(&t.token);
                        Err(MtprotoTelegramError::QrLoginHandle {
                            token: t.token,
                            url,
                        })
                    }
                    tl::enums::auth::LoginToken::Success(login_token_success) => {
                        let new_state = {
                            let current = *self.user_auth_state.lock();
                            next_user_auth_state_server(
                                UserAuthServerEvent::SignInSucceeded,
                                current,
                            )?
                        };
                        *self.user_auth_state.lock() = new_state;
                        let info = extract_self_user_info(login_token_success.authorization);
                        self.self_handle
                            .set_identity(info.user_id, info.username.clone());
                        Ok(())
                    }
                    tl::enums::auth::LoginToken::MigrateTo(migrate2) => {
                        // Double migration: follow the chain.
                        tracing::info!(
                            target_dc2 = migrate2.dc_id,
                            "importLoginToken: second MigrateTo"
                        );
                        *self.qr_dc_id.lock() = Some(migrate2.dc_id);
                        let import_req2 = tl::functions::auth::ImportLoginToken {
                            token: migrate2.token,
                        };
                        let import_resp2: tl::enums::auth::LoginToken = self
                            .client
                            .invoke_in_dc(migrate2.dc_id, &import_req2)
                            .await
                            .map_err(|e| {
                                MtprotoTelegramError::Auth(format!(
                                    "auth.importLoginToken on DC {}: {}",
                                    migrate2.dc_id,
                                    crate::error::redact_credentials(&e.to_string())
                                ))
                            })?;
                        match import_resp2 {
                            tl::enums::auth::LoginToken::Token(t) => {
                                *self.qr_token.lock() = Some(t.token.clone());
                                let url = build_qr_url(&t.token);
                                Err(MtprotoTelegramError::QrLoginHandle {
                                    token: t.token,
                                    url,
                                })
                            }
                            tl::enums::auth::LoginToken::Success(s) => {
                                let new_state = {
                                    let current = *self.user_auth_state.lock();
                                    next_user_auth_state_server(
                                        UserAuthServerEvent::SignInSucceeded,
                                        current,
                                    )?
                                };
                                *self.user_auth_state.lock() = new_state;
                                let info = extract_self_user_info(s.authorization);
                                self.self_handle
                                    .set_identity(info.user_id, info.username.clone());
                                Ok(())
                            }
                            tl::enums::auth::LoginToken::MigrateTo(_) => {
                                *self.user_auth_state.lock() = UserAuthLifecycle::NoCredentials;
                                Err(MtprotoTelegramError::Internal(
                                    "triple MigrateTo; giving up".into(),
                                ))
                            }
                        }
                    }
                }
            }
        }
    }

    async fn poll_qr_login(&self) -> Result<SelfUserInfo, MtprotoTelegramError> {
        let (api_id, api_hash) = {
            let id = self.qr_api_id.lock();
            let hash = self.qr_api_hash.lock();
            match (id.as_ref(), hash.as_ref()) {
                (Some(id), Some(hash)) => (*id, hash.clone()),
                _ => {
                    return Err(MtprotoTelegramError::Auth(
                        "poll_qr_login called without a prior qr_login".into(),
                    ));
                }
            }
        };
        let request = tl::functions::auth::ExportLoginToken {
            api_id,
            api_hash: api_hash.as_str().to_string(),
            except_ids: Vec::new(),
        };
        // If the initial qr_login migrated to a different DC,
        // invoke directly on that DC to avoid repeated MigrateTo
        // cycles and token rotation.
        let cached_dc = *self.qr_dc_id.lock();
        let response: tl::enums::auth::LoginToken = if let Some(dc_id) = cached_dc {
            self.client
                .invoke_in_dc(dc_id, &request)
                .await
                .map_err(|e| {
                    MtprotoTelegramError::Auth(format!(
                        "auth.exportLoginToken on DC {}: {}",
                        dc_id,
                        crate::error::redact_credentials(&e.to_string())
                    ))
                })?
        } else {
            self.client.invoke(&request).await.map_err(|e| {
                MtprotoTelegramError::Auth(format!(
                    "auth.exportLoginToken: {}",
                    crate::error::redact_credentials(&e.to_string())
                ))
            })?
        };
        match response {
            tl::enums::auth::LoginToken::Token(t) => {
                *self.qr_token.lock() = Some(t.token.clone());
                let url = build_qr_url(&t.token);
                Err(MtprotoTelegramError::QrLoginHandle {
                    token: t.token,
                    url,
                })
            }
            tl::enums::auth::LoginToken::Success(login_token_success) => {
                let new_state = {
                    let current = *self.user_auth_state.lock();
                    next_user_auth_state(UserAuthAction::QrLoginConfirm, current)?
                };
                *self.user_auth_state.lock() = new_state;
                let new_state = {
                    let current = *self.user_auth_state.lock();
                    next_user_auth_state_server(UserAuthServerEvent::SignInSucceeded, current)?
                };
                *self.user_auth_state.lock() = new_state;
                let info = extract_self_user_info(login_token_success.authorization);
                self.self_handle
                    .set_identity(info.user_id, info.username.clone());
                Ok(info)
            }
            tl::enums::auth::LoginToken::MigrateTo(migrate) => {
                // MigrateTo during poll (TDesktop pattern: importTo).
                // Import the MigrateTo token on the target DC.
                let target_dc = migrate.dc_id;
                tracing::info!(target_dc, "poll_qr_login: MigrateTo DC {}", target_dc);
                *self.qr_dc_id.lock() = Some(target_dc);
                let import_req = tl::functions::auth::ImportLoginToken {
                    token: migrate.token,
                };
                let import_resp: tl::enums::auth::LoginToken = self
                    .client
                    .invoke_in_dc(target_dc, &import_req)
                    .await
                    .map_err(|e| {
                        MtprotoTelegramError::Auth(format!(
                            "auth.importLoginToken on DC {}: {}",
                            target_dc,
                            crate::error::redact_credentials(&e.to_string())
                        ))
                    })?;
                match import_resp {
                    tl::enums::auth::LoginToken::Token(t) => {
                        *self.qr_token.lock() = Some(t.token.clone());
                        let url = build_qr_url(&t.token);
                        Err(MtprotoTelegramError::QrLoginHandle {
                            token: t.token,
                            url,
                        })
                    }
                    tl::enums::auth::LoginToken::Success(login_token_success) => {
                        let new_state = {
                            let current = *self.user_auth_state.lock();
                            next_user_auth_state(UserAuthAction::QrLoginConfirm, current)?
                        };
                        *self.user_auth_state.lock() = new_state;
                        let new_state = {
                            let current = *self.user_auth_state.lock();
                            next_user_auth_state_server(
                                UserAuthServerEvent::SignInSucceeded,
                                current,
                            )?
                        };
                        *self.user_auth_state.lock() = new_state;
                        let info = extract_self_user_info(login_token_success.authorization);
                        self.self_handle
                            .set_identity(info.user_id, info.username.clone());
                        Ok(info)
                    }
                    tl::enums::auth::LoginToken::MigrateTo(migrate2) => {
                        // Double migration: follow chain.
                        tracing::info!(target_dc2 = migrate2.dc_id, "poll: second MigrateTo");
                        *self.qr_dc_id.lock() = Some(migrate2.dc_id);
                        let import_req2 = tl::functions::auth::ImportLoginToken {
                            token: migrate2.token,
                        };
                        let import_resp2: tl::enums::auth::LoginToken = self
                            .client
                            .invoke_in_dc(migrate2.dc_id, &import_req2)
                            .await
                            .map_err(|e| {
                                MtprotoTelegramError::Auth(format!(
                                    "auth.importLoginToken on DC {}: {}",
                                    migrate2.dc_id,
                                    crate::error::redact_credentials(&e.to_string())
                                ))
                            })?;
                        match import_resp2 {
                            tl::enums::auth::LoginToken::Token(t) => {
                                *self.qr_token.lock() = Some(t.token.clone());
                                let url = build_qr_url(&t.token);
                                Err(MtprotoTelegramError::QrLoginHandle {
                                    token: t.token,
                                    url,
                                })
                            }
                            tl::enums::auth::LoginToken::Success(s) => {
                                let new_state = {
                                    let current = *self.user_auth_state.lock();
                                    next_user_auth_state(UserAuthAction::QrLoginConfirm, current)?
                                };
                                *self.user_auth_state.lock() = new_state;
                                let new_state = {
                                    let current = *self.user_auth_state.lock();
                                    next_user_auth_state_server(
                                        UserAuthServerEvent::SignInSucceeded,
                                        current,
                                    )?
                                };
                                *self.user_auth_state.lock() = new_state;
                                let info = extract_self_user_info(s.authorization);
                                self.self_handle
                                    .set_identity(info.user_id, info.username.clone());
                                Ok(info)
                            }
                            tl::enums::auth::LoginToken::MigrateTo(_) => {
                                Err(MtprotoTelegramError::Internal(
                                    "triple MigrateTo in poll; giving up".into(),
                                ))
                            }
                        }
                    }
                }
            }
        }
    }

    async fn import_login_token(&self, token: &[u8]) -> Result<SelfUserInfo, MtprotoTelegramError> {
        // Drive the state machine: QrLoginPending →
        // QrLoginConfirmed (client) via QrLoginConfirm.
        // (After a successful poll, the state is
        // QrLoginPending; this drives the transition to
        // QrLoginConfirmed so the import call can advance
        // to SignedIn.)
        let new_state = {
            let current = *self.user_auth_state.lock();
            next_user_auth_state(UserAuthAction::QrLoginConfirm, current)?
        };
        *self.user_auth_state.lock() = new_state;

        // Invoke `auth.importLoginToken` with the token
        // bytes. The response is `LoginToken::Success`
        // (signed in) or `LoginToken::Token` (a new token
        // to be re-imported — not expected in normal
        // flow) or error variants.
        let request = tl::functions::auth::ImportLoginToken {
            token: token.to_vec(),
        };
        let cached_dc = *self.qr_dc_id.lock();
        let response: tl::enums::auth::LoginToken = if let Some(dc_id) = cached_dc {
            self.client
                .invoke_in_dc(dc_id, &request)
                .await
                .map_err(|e| {
                    MtprotoTelegramError::Auth(format!(
                        "auth.importLoginToken on DC {}: {}",
                        dc_id,
                        crate::error::redact_credentials(&e.to_string())
                    ))
                })?
        } else {
            self.client.invoke(&request).await.map_err(|e| {
                MtprotoTelegramError::Auth(format!(
                    "auth.importLoginToken: {}",
                    crate::error::redact_credentials(&e.to_string())
                ))
            })?
        };
        match response {
            tl::enums::auth::LoginToken::Success(login_token_success) => {
                let new_state = {
                    let current = *self.user_auth_state.lock();
                    next_user_auth_state_server(UserAuthServerEvent::SignInSucceeded, current)?
                };
                *self.user_auth_state.lock() = new_state;
                let info = extract_self_user_info(login_token_success.authorization);
                self.self_handle
                    .set_identity(info.user_id, info.username.clone());
                Ok(info)
            }
            tl::enums::auth::LoginToken::Token(_) => {
                // Unexpected: the import returned a new
                // token. Roll back to QrLoginPending and
                // tell the caller to re-poll.
                *self.user_auth_state.lock() = UserAuthLifecycle::QrLoginPending;
                Err(MtprotoTelegramError::Auth(
                    "auth.importLoginToken returned a new token; re-poll required".into(),
                ))
            }
            tl::enums::auth::LoginToken::MigrateTo(migrate) => {
                let target_dc = migrate.dc_id;
                tracing::info!(
                    target_dc,
                    "import_login_token: MigrateTo DC {}; reconnecting",
                    target_dc
                );
                let request_on_target = tl::functions::auth::ImportLoginToken {
                    token: token.to_vec(),
                };
                let response_on_target: tl::enums::auth::LoginToken = self
                    .client
                    .invoke_in_dc(target_dc, &request_on_target)
                    .await
                    .map_err(|e| {
                        MtprotoTelegramError::Auth(format!(
                            "auth.importLoginToken on DC {}: {}",
                            target_dc,
                            crate::error::redact_credentials(&e.to_string())
                        ))
                    })?;
                match response_on_target {
                    tl::enums::auth::LoginToken::Success(login_token_success) => {
                        let new_state = {
                            let current = *self.user_auth_state.lock();
                            next_user_auth_state_server(
                                UserAuthServerEvent::SignInSucceeded,
                                current,
                            )?
                        };
                        *self.user_auth_state.lock() = new_state;
                        let info = extract_self_user_info(login_token_success.authorization);
                        self.self_handle
                            .set_identity(info.user_id, info.username.clone());
                        Ok(info)
                    }
                    tl::enums::auth::LoginToken::Token(_) => {
                        *self.user_auth_state.lock() = UserAuthLifecycle::QrLoginPending;
                        Err(MtprotoTelegramError::Auth(
                            "auth.importLoginToken on DC returned a new token; re-poll required"
                                .into(),
                        ))
                    }
                    tl::enums::auth::LoginToken::MigrateTo(_) => {
                        *self.user_auth_state.lock() = UserAuthLifecycle::QrLoginPending;
                        Err(MtprotoTelegramError::Internal(format!(
                            "import on DC {} also returned MigrateTo",
                            target_dc
                        )))
                    }
                }
            }
        }
    }

    async fn get_file_id_for_message(
        &self,
        chat_id: i64,
        message_id: i64,
    ) -> Result<String, MtprotoTelegramError> {
        // Resolve the chat to a PeerRef so we can call
        // get_messages_by_id.
        let peer_kind = chat_id_kind(chat_id);
        let chat_peer = resolve_chat(
            &self.client,
            chat_id,
            peer_kind == PeerKindChoice::Supergroup,
        )
        .await?;

        let peer_ref = crate::peer_resolve::peer_to_ref(&chat_peer).await?;

        let messages = self
            .client
            .get_messages_by_id(peer_ref, &[message_id as i32])
            .await
            .map_err(|e| MtprotoTelegramError::Rpc {
                code: 500,
                message: format!("get_file_id_for_message: {}", e),
            })?;

        let message =
            messages
                .into_iter()
                .flatten()
                .next()
                .ok_or_else(|| MtprotoTelegramError::Rpc {
                    code: 404,
                    message: format!("message {} not found in chat {}", message_id, chat_id),
                })?;

        let media = message.media().ok_or_else(|| MtprotoTelegramError::Rpc {
            code: 404,
            message: format!("message {} has no media (not a document)", message_id),
        })?;

        match media {
            grammers_client::media::Media::Document(doc) => {
                let location =
                    doc.to_raw_input_location()
                        .ok_or_else(|| MtprotoTelegramError::Rpc {
                            code: 404,
                            message: format!(
                                "document in message {} has no file location",
                                message_id
                            ),
                        })?;
                let bytes = location.to_bytes();
                Ok(bytes
                    .iter()
                    .map(|b| format!("{:02x}", b))
                    .collect::<String>())
            }
            _ => Err(MtprotoTelegramError::Rpc {
                code: 404,
                message: format!(
                    "message {} media is not a document ({:?})",
                    message_id,
                    std::mem::discriminant(&media)
                ),
            }),
        }
    }

    // ── Real group / Coordinator operations (RFC-0850 §8) ─────────
    //
    // Phase 2 implementations: use the raw `tl::functions::*`
    // RPCs directly via `self.client.invoke(&request)`. The
    // mock impl in `client.rs` is kept for unit tests; the
    // real impls here match the same "one trait method = one
    // RPC" contract so the two impls are interchangeable
    // from the adapter's perspective.
    //
    // Basic-group vs. supergroup disambiguation happens at
    // the `chat_id_to_peer_id` boundary in `peer_resolve`:
    // positive or small-negative chat_ids route to
    // `messages.*` RPCs; very negative chat_ids route to
    // `channels.*` RPCs. The trait surface doesn't expose
    // the distinction, so the disambiguation lives here.

    async fn create_group(
        &self,
        title: &str,
        user_ids: &[i64],
    ) -> Result<GroupInfo, MtprotoTelegramError> {
        use std::time::{SystemTime, UNIX_EPOCH};
        let prefix = "create_group";

        // Telegram's MTProto API distinguishes basic groups
        // (created via `messages.createChat`) from
        // supergroups/channels (created via
        // `channels.createChannel`). The decision rule for
        // the real client matches the TL contracts:
        //
        // * `messages.createChat` requires `Vec<InputUser>`
        //   and an initial title; it produces a `Chat` (basic
        //   group). It is being deprecated for new groups;
        //   Telegram's official clients migrate newly
        //   created basic groups to supergroups within a
        //   few seconds.
        // * `channels.createChannel` requires `title` +
        //   `about` and produces a `Channel` (supergroup).
        //
        // For our purposes, we always create a supergroup
        // (the modern contract) and add the users as
        // participants via `channels.inviteToChannel`. This
        // gives us a stable chat_id range and matches the
        // expectation set by `get_chat` (which classifies
        // channels and basic groups by chat_id magnitude).
        let about = "";
        let request = tl::functions::channels::CreateChannel {
            broadcast: false,
            megagroup: true, // supergroup (megagroup), not a broadcast channel
            for_import: false,
            forum: false,
            title: title.to_string(),
            about: about.to_string(),
            geo_point: None,
            address: None,
            ttl_period: None,
        };
        let updates = self
            .client
            .invoke(&request)
            .await
            .map_err(|e| crate::peer_resolve::map_invoke_err(prefix, e))?;

        // The RPC returns `Updates`. Walk the chat list to
        // find the freshly-created channel; the position
        // varies across TL schema versions so we use the
        // pattern-match path.
        let channel_id =
            extract_created_channel_id(&updates).ok_or_else(|| MtprotoTelegramError::Rpc {
                code: 500,
                message: format!("{prefix}: created channel not present in Updates"),
            })?;

        // Now invite the initial users (if any). For an
        // empty user list we skip the second RPC.
        if !user_ids.is_empty() {
            let mut input_users = Vec::with_capacity(user_ids.len());
            for &uid in user_ids {
                let peer = resolve_user(&self.client, uid).await.map_err(|e| {
                    MtprotoTelegramError::Rpc {
                        code: 500,
                        message: format!("{prefix}: resolve_user({uid}): {e}"),
                    }
                })?;
                let input_user =
                    peer_to_input_user(&peer)
                        .await
                        .map_err(|e| MtprotoTelegramError::Rpc {
                            code: 500,
                            message: format!("{prefix}: peer_to_input_user({uid}): {e}"),
                        })?;
                input_users.push(input_user);
            }
            // Re-resolve the chat as a channel; the
            // `CreateChannel` response already populated the
            // session cache, so this should be a no-network
            // hit on the cache.
            let chat_peer = resolve_chat(&self.client, channel_id, true)
                .await
                .map_err(|e| MtprotoTelegramError::Rpc {
                    code: 500,
                    message: format!("{prefix}: resolve_chat({channel_id}): {e}"),
                })?;
            let input_channel =
                peer_to_input_channel(&chat_peer)
                    .await
                    .map_err(|e| MtprotoTelegramError::Rpc {
                        code: 500,
                        message: format!("{prefix}: peer_to_input_channel: {e}"),
                    })?;
            let _ = self
                .client
                .invoke(&tl::functions::channels::InviteToChannel {
                    channel: input_channel,
                    users: input_users,
                })
                .await
                .map_err(|e| crate::peer_resolve::map_invoke_err(prefix, e))?;
        }

        // Build the GroupInfo. member_count is unknown at
        // create time without a separate RPC, so we report
        // None (the caller treats None as "unknown, refresh
        // later").
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_secs() as i64)
            .unwrap_or(0);
        let _ = timestamp;
        Ok(GroupInfo {
            chat_id: channel_id,
            title: title.to_string(),
            member_count: None,
            is_admin: Some(true), // creator is always admin
            about: None,
        })
    }

    async fn add_participant(
        &self,
        chat_id: i64,
        user_id: i64,
    ) -> Result<(), MtprotoTelegramError> {
        let prefix = "add_participant";
        let peer_kind = chat_id_kind(chat_id);
        let user_peer = resolve_user(&self.client, user_id).await?;
        let input_user = peer_to_input_user(&user_peer).await?;
        match peer_kind {
            PeerKindChoice::Basic => {
                let req = tl::functions::messages::AddChatUser {
                    chat_id,
                    user_id: input_user,
                    fwd_limit: 0,
                };
                self.client
                    .invoke(&req)
                    .await
                    .map_err(|e| crate::peer_resolve::map_invoke_err(prefix, e))?;
            }
            PeerKindChoice::Supergroup => {
                let chat_peer = resolve_chat(&self.client, chat_id, true).await?;
                let input_channel = peer_to_input_channel(&chat_peer).await?;
                let req = tl::functions::channels::InviteToChannel {
                    channel: input_channel,
                    users: vec![input_user],
                };
                self.client
                    .invoke(&req)
                    .await
                    .map_err(|e| crate::peer_resolve::map_invoke_err(prefix, e))?;
            }
        }
        Ok(())
    }

    async fn kick_participant(
        &self,
        chat_id: i64,
        user_id: i64,
    ) -> Result<(), MtprotoTelegramError> {
        let prefix = "kick_participant";
        let peer_kind = chat_id_kind(chat_id);
        let user_peer = resolve_user(&self.client, user_id).await?;
        let input_user = peer_to_input_user(&user_peer).await?;
        match peer_kind {
            PeerKindChoice::Basic => {
                let req = tl::functions::messages::DeleteChatUser {
                    revoke_history: false,
                    chat_id,
                    user_id: input_user,
                };
                self.client
                    .invoke(&req)
                    .await
                    .map_err(|e| crate::peer_resolve::map_invoke_err(prefix, e))?;
            }
            PeerKindChoice::Supergroup => {
                let chat_peer = resolve_chat(&self.client, chat_id, true).await?;
                let input_channel = peer_to_input_channel(&chat_peer).await?;
                // Kicking in a supergroup = ban (so they
                // can't rejoin unless explicitly unbanned).
                let req = tl::functions::channels::EditBanned {
                    channel: input_channel,
                    participant: tl::enums::InputPeer::User(tl::types::InputPeerUser {
                        user_id: user_peer.id().bare_id(),
                        access_hash: user_peer.to_ref().await.map(|r| r.auth.hash()).unwrap_or(0),
                    }),
                    banned_rights: tl::enums::ChatBannedRights::Rights(
                        tl::types::ChatBannedRights {
                            view_messages: true,
                            send_messages: true,
                            send_media: true,
                            send_stickers: true,
                            send_gifs: true,
                            send_games: true,
                            send_inline: true,
                            embed_links: true,
                            send_polls: true,
                            change_info: true,
                            invite_users: true,
                            pin_messages: true,
                            manage_topics: true,
                            send_photos: true,
                            send_videos: true,
                            send_roundvideos: true,
                            send_audios: true,
                            send_voices: true,
                            send_docs: true,
                            send_plain: true,
                            until_date: 0,
                        },
                    ),
                };
                self.client
                    .invoke(&req)
                    .await
                    .map_err(|e| crate::peer_resolve::map_invoke_err(prefix, e))?;
            }
        }
        Ok(())
    }

    async fn promote_participant(
        &self,
        chat_id: i64,
        user_id: i64,
    ) -> Result<(), MtprotoTelegramError> {
        let prefix = "promote_participant";
        let peer_kind = chat_id_kind(chat_id);
        if peer_kind != PeerKindChoice::Supergroup {
            return Err(MtprotoTelegramError::Rpc {
                code: 400,
                message: format!(
                    "{prefix}: basic groups do not have admin rights; chat_id={chat_id}"
                ),
            });
        }
        let chat_peer = resolve_chat(&self.client, chat_id, true).await?;
        let input_channel = peer_to_input_channel(&chat_peer).await?;
        let user_peer = resolve_user(&self.client, user_id).await?;
        let input_user = peer_to_input_user(&user_peer).await?;
        let req = tl::functions::channels::EditAdmin {
            channel: input_channel,
            user_id: input_user,
            admin_rights: tl::enums::ChatAdminRights::Rights(tl::types::ChatAdminRights {
                change_info: true,
                post_messages: true,
                edit_messages: true,
                delete_messages: true,
                ban_users: true,
                invite_users: true,
                pin_messages: true,
                add_admins: false,
                anonymous: false,
                manage_call: true,
                other: true,
                manage_topics: true,
                post_stories: true,
                edit_stories: true,
                delete_stories: true,
                manage_direct_messages: true,
            }),
            rank: "admin".to_string(),
        };
        self.client
            .invoke(&req)
            .await
            .map_err(|e| crate::peer_resolve::map_invoke_err(prefix, e))?;
        Ok(())
    }

    async fn demote_participant(
        &self,
        chat_id: i64,
        user_id: i64,
    ) -> Result<(), MtprotoTelegramError> {
        let prefix = "demote_participant";
        let peer_kind = chat_id_kind(chat_id);
        if peer_kind != PeerKindChoice::Supergroup {
            return Err(MtprotoTelegramError::Rpc {
                code: 400,
                message: format!(
                    "{prefix}: basic groups do not have admin rights; chat_id={chat_id}"
                ),
            });
        }
        let chat_peer = resolve_chat(&self.client, chat_id, true).await?;
        let input_channel = peer_to_input_channel(&chat_peer).await?;
        let user_peer = resolve_user(&self.client, user_id).await?;
        let input_user = peer_to_input_user(&user_peer).await?;
        // Demote by issuing `EditAdmin` with all rights set
        // to `false`. This is the canonical Telegram demote
        // recipe (there is no `channels.demoteAdmin` RPC).
        let req = tl::functions::channels::EditAdmin {
            channel: input_channel,
            user_id: input_user,
            admin_rights: tl::enums::ChatAdminRights::Rights(tl::types::ChatAdminRights {
                change_info: false,
                post_messages: false,
                edit_messages: false,
                delete_messages: false,
                ban_users: false,
                invite_users: false,
                pin_messages: false,
                add_admins: false,
                anonymous: false,
                manage_call: false,
                other: false,
                manage_topics: false,
                post_stories: false,
                edit_stories: false,
                delete_stories: false,
                manage_direct_messages: false,
            }),
            rank: String::new(),
        };
        self.client
            .invoke(&req)
            .await
            .map_err(|e| crate::peer_resolve::map_invoke_err(prefix, e))?;
        Ok(())
    }

    async fn set_chat_title(&self, chat_id: i64, title: &str) -> Result<(), MtprotoTelegramError> {
        let prefix = "set_chat_title";
        let peer_kind = chat_id_kind(chat_id);
        match peer_kind {
            PeerKindChoice::Basic => {
                let req = tl::functions::messages::EditChatTitle {
                    chat_id,
                    title: title.to_string(),
                };
                self.client
                    .invoke(&req)
                    .await
                    .map_err(|e| crate::peer_resolve::map_invoke_err(prefix, e))?;
            }
            PeerKindChoice::Supergroup => {
                let chat_peer = resolve_chat(&self.client, chat_id, true).await?;
                let input_channel = peer_to_input_channel(&chat_peer).await?;
                let req = tl::functions::channels::EditTitle {
                    channel: input_channel,
                    title: title.to_string(),
                };
                self.client
                    .invoke(&req)
                    .await
                    .map_err(|e| crate::peer_resolve::map_invoke_err(prefix, e))?;
            }
        }
        Ok(())
    }

    async fn set_chat_about(&self, chat_id: i64, about: &str) -> Result<(), MtprotoTelegramError> {
        let prefix = "set_chat_about";
        let peer_kind = chat_id_kind(chat_id);
        match peer_kind {
            PeerKindChoice::Basic => {
                let req = tl::functions::messages::EditChatAbout {
                    peer: tl::enums::InputPeer::Chat(tl::types::InputPeerChat { chat_id }),
                    about: about.to_string(),
                };
                self.client
                    .invoke(&req)
                    .await
                    .map_err(|e| crate::peer_resolve::map_invoke_err(prefix, e))?;
            }
            PeerKindChoice::Supergroup => {
                let chat_peer = resolve_chat(&self.client, chat_id, true).await?;
                let input_peer = peer_to_input_peer(&chat_peer).await?;
                let req = tl::functions::messages::EditChatAbout {
                    peer: input_peer,
                    about: about.to_string(),
                };
                self.client
                    .invoke(&req)
                    .await
                    .map_err(|e| crate::peer_resolve::map_invoke_err(prefix, e))?;
            }
        }
        Ok(())
    }

    async fn delete_chat(&self, chat_id: i64) -> Result<(), MtprotoTelegramError> {
        let prefix = "delete_chat";
        let peer_kind = chat_id_kind(chat_id);
        match peer_kind {
            PeerKindChoice::Basic => {
                let req = tl::functions::messages::DeleteChat { chat_id };
                self.client
                    .invoke(&req)
                    .await
                    .map_err(|e| crate::peer_resolve::map_invoke_err(prefix, e))?;
            }
            PeerKindChoice::Supergroup => {
                let chat_peer = resolve_chat(&self.client, chat_id, true).await?;
                let input_channel = peer_to_input_channel(&chat_peer).await?;
                let req = tl::functions::channels::DeleteChannel {
                    channel: input_channel,
                };
                self.client
                    .invoke(&req)
                    .await
                    .map_err(|e| crate::peer_resolve::map_invoke_err(prefix, e))?;
            }
        }
        Ok(())
    }

    async fn leave_chat(&self, chat_id: i64) -> Result<(), MtprotoTelegramError> {
        let prefix = "leave_chat";
        let peer_kind = chat_id_kind(chat_id);
        match peer_kind {
            PeerKindChoice::Basic => {
                // `messages.deleteChatUser` is also how you
                // leave a basic group: call it on self.
                let self_user = self.self_handle.get().map(|i| i.user_id).ok_or_else(|| {
                    MtprotoTelegramError::Auth(format!(
                        "{prefix}: cannot leave chat_id={chat_id} before sign-in"
                    ))
                })?;
                let self_peer = resolve_user(&self.client, self_user).await?;
                let input_user = peer_to_input_user(&self_peer).await?;
                let req = tl::functions::messages::DeleteChatUser {
                    revoke_history: false,
                    chat_id,
                    user_id: input_user,
                };
                self.client
                    .invoke(&req)
                    .await
                    .map_err(|e| crate::peer_resolve::map_invoke_err(prefix, e))?;
            }
            PeerKindChoice::Supergroup => {
                let chat_peer = resolve_chat(&self.client, chat_id, true).await?;
                let input_channel = peer_to_input_channel(&chat_peer).await?;
                let req = tl::functions::channels::LeaveChannel {
                    channel: input_channel,
                };
                self.client
                    .invoke(&req)
                    .await
                    .map_err(|e| crate::peer_resolve::map_invoke_err(prefix, e))?;
            }
        }
        Ok(())
    }

    async fn get_chat(&self, chat_id: i64) -> Result<GroupInfo, MtprotoTelegramError> {
        let prefix = "get_chat";
        let peer_kind = chat_id_kind(chat_id);
        match peer_kind {
            PeerKindChoice::Basic => {
                let chats = self
                    .client
                    .invoke(&tl::functions::messages::GetChats { id: vec![chat_id] })
                    .await
                    .map_err(|e| crate::peer_resolve::map_invoke_err(prefix, e))?;
                extract_chat_info(chats).ok_or_else(|| MtprotoTelegramError::Rpc {
                    code: 404,
                    message: format!(
                        "{prefix}: chat_id={chat_id} not found in messages.GetChats response"
                    ),
                })
            }
            PeerKindChoice::Supergroup => {
                let chat_peer = resolve_chat(&self.client, chat_id, true).await?;
                let input_channel = peer_to_input_channel(&chat_peer).await?;
                let chats = self
                    .client
                    .invoke(&tl::functions::channels::GetChannels {
                        id: vec![input_channel],
                    })
                    .await
                    .map_err(|e| crate::peer_resolve::map_invoke_err(prefix, e))?;
                extract_chat_info(chats).ok_or_else(|| MtprotoTelegramError::Rpc {
                    code: 404,
                    message: format!(
                        "{prefix}: chat_id={chat_id} not found in channels.GetChannels response"
                    ),
                })
            }
        }
    }

    async fn list_dialog_ids(&self) -> Result<Vec<i64>, MtprotoTelegramError> {
        // The high-level `Client::iter_dialogs()` walks the
        // SenderPool's update channel and yields `Dialog`
        // values. We map each to its `chat_id()` via the
        // peer-resolve inverse helper. Note: this method
        // doesn't require authentication on the client
        // side (the SenderPool is set up at `connect`
        // time); the actual chat list is filled by Telegram
        // once the user is signed in.
        let prefix = "list_dialog_ids";
        let mut iter = self.client.iter_dialogs();
        let mut ids = Vec::new();
        loop {
            let dialog = match iter.next().await {
                Ok(Some(d)) => d,
                Ok(None) => break,
                Err(e) => {
                    tracing::warn!(prefix, error = %e, "iter_dialogs yielded error");
                    break;
                }
            };
            let peer = dialog.peer();
            let peer_id = peer.id();
            ids.push(crate::peer_resolve::peer_id_to_chat_id(peer_id));
        }
        Ok(ids)
    }

    async fn check_invite(&self, hash: &str) -> Result<InvitePreview, MtprotoTelegramError> {
        // Telegram's `messages.CheckChatInvite` returns a
        // `ChatInvite` enum. Three relevant variants:
        // - `ChatInviteAlready { chat }` — the user is already
        //   a member; the chat's id and title are available.
        // - `ChatInvite { ... }` — the standard invite payload
        //   with title, participants_count, megagroup flag, etc.
        // - `ChatInvitePeek` — minimal preview.
        let prefix = "check_invite";
        let req = tl::functions::messages::CheckChatInvite {
            hash: hash.to_string(),
        };
        let result = self
            .client
            .invoke(&req)
            .await
            .map_err(|e| crate::peer_resolve::map_invoke_err(prefix, e))?;
        let invite_enum: tl::enums::ChatInvite = result;
        match invite_enum {
            tl::enums::ChatInvite::Already(already) => {
                let (chat_id, title) = match &already.chat {
                    tl::enums::Chat::Channel(c) => (
                        crate::peer_resolve::channel_id_to_chat_id(c.id),
                        c.title.clone(),
                    ),
                    tl::enums::Chat::Chat(c) => (c.id, c.title.clone()),
                    tl::enums::Chat::Forbidden(f) => (f.id, f.title.clone()),
                    tl::enums::Chat::ChannelForbidden(f) => (
                        crate::peer_resolve::channel_id_to_chat_id(f.id),
                        f.title.clone(),
                    ),
                    tl::enums::Chat::Empty(_) => {
                        return Err(MtprotoTelegramError::Rpc {
                            code: 404,
                            message: format!("{prefix}: ChatInviteAlready returned empty Chat"),
                        });
                    }
                };
                Ok(InvitePreview {
                    chat_id: Some(chat_id),
                    title,
                    member_count: None,
                    is_public: false,
                    is_megagroup: false,
                })
            }
            tl::enums::ChatInvite::Invite(inv) => Ok(InvitePreview {
                chat_id: None,
                title: inv.title,
                member_count: Some(inv.participants_count.max(0) as u32),
                is_public: inv.public,
                is_megagroup: inv.megagroup,
            }),
            tl::enums::ChatInvite::Peek(peek) => {
                // ChatInvitePeek carries a `chat: Chat` plus
                // an `expires: i32`. Extract whatever we can.
                let (chat_id, title) = match &peek.chat {
                    tl::enums::Chat::Channel(c) => (
                        Some(crate::peer_resolve::channel_id_to_chat_id(c.id)),
                        c.title.clone(),
                    ),
                    tl::enums::Chat::Chat(c) => (Some(c.id), c.title.clone()),
                    tl::enums::Chat::Forbidden(f) => (Some(f.id), f.title.clone()),
                    tl::enums::Chat::ChannelForbidden(f) => (
                        Some(crate::peer_resolve::channel_id_to_chat_id(f.id)),
                        f.title.clone(),
                    ),
                    tl::enums::Chat::Empty(_) => (None, String::new()),
                };
                Ok(InvitePreview {
                    chat_id,
                    title,
                    member_count: None,
                    is_public: false,
                    is_megagroup: false,
                })
            }
        }
    }

    async fn import_invite(&self, hash: &str) -> Result<i64, MtprotoTelegramError> {
        // Telegram's `messages.ImportChatInvite` returns an
        // `Updates` payload. Walk it for the chat id of the
        // group the bot just joined. The chat id is in
        // either `Updates::Combined.chats` /
        // `Updates::Updates.chats`, or in
        // `Update::Channel.channel_id` /
        // `Update::Chat.chat_id`.
        let prefix = "import_invite";
        let req = tl::functions::messages::ImportChatInvite {
            hash: hash.to_string(),
        };
        let updates = self
            .client
            .invoke(&req)
            .await
            .map_err(|e| crate::peer_resolve::map_invoke_err(prefix, e))?;
        use tl::enums::{Chat as ChatEnum, Update as UpdateEnum, Updates as UpdatesEnum};
        // First pass: chats list (covers Channel + Chat).
        let chats_iter: Box<dyn Iterator<Item = &tl::enums::Chat>> = match &updates {
            UpdatesEnum::Combined(c) => Box::new(c.chats.iter()),
            UpdatesEnum::Updates(u) => Box::new(u.chats.iter()),
            _ => Box::new(std::iter::empty()),
        };
        for chat in chats_iter {
            match chat {
                ChatEnum::Channel(ch) => {
                    return Ok(crate::peer_resolve::channel_id_to_chat_id(ch.id));
                }
                ChatEnum::Chat(ch) => return Ok(ch.id),
                _ => continue,
            }
        }
        // Second pass: updates list (covers `Update::Channel`).
        let updates_iter: Box<dyn Iterator<Item = &tl::enums::Update>> = match &updates {
            UpdatesEnum::Combined(c) => Box::new(c.updates.iter()),
            UpdatesEnum::Updates(u) => Box::new(u.updates.iter()),
            _ => Box::new(std::iter::empty()),
        };
        for upd in updates_iter {
            if let UpdateEnum::Channel(channel_upd) = upd {
                return Ok(crate::peer_resolve::channel_id_to_chat_id(
                    channel_upd.channel_id,
                ));
            }
        }
        Err(MtprotoTelegramError::Rpc {
            code: 404,
            message: format!("{prefix}: could not extract chat id from import response"),
        })
    }

    async fn edit_creator(
        &self,
        chat_id: i64,
        new_owner_user_id: i64,
        password: Option<&str>,
    ) -> Result<(), MtprotoTelegramError> {
        // Telegram's `channels.EditCreator` transfers
        // ownership of a supergroup to `user_id`. The caller
        // must be authenticated as the current owner. The
        // `password` field is a Cloud 2FA SRP check; we
        // currently only support the empty-password form
        // (`InputCheckPasswordEmpty`).
        let prefix = "edit_creator";
        if chat_id >= 0 {
            return Err(MtprotoTelegramError::Capability(format!(
                "{prefix}: chat_id {chat_id} is not a supergroup (must be negative)"
            )));
        }
        if chat_id > crate::peer_resolve::SUPERGROUP_CHAT_ID_MAX_NEG {
            return Err(MtprotoTelegramError::Capability(format!(
                "{prefix}: chat_id {chat_id} is a basic group; EditCreator requires a supergroup"
            )));
        }
        if password.is_some() {
            return Err(MtprotoTelegramError::Capability(format!(
                "{prefix}: 2FA password not supported in this build; pass None"
            )));
        }
        let channel = crate::peer_resolve::resolve_chat(&self.client, chat_id, true)
            .await
            .map_err(|e| MtprotoTelegramError::Rpc {
                code: -1,
                message: format!("{prefix}: resolve_chat: {e}"),
            })?;
        let channel_input = crate::peer_resolve::peer_to_input_channel(&channel)
            .await
            .map_err(|e| MtprotoTelegramError::Rpc {
                code: -1,
                message: format!("{prefix}: peer_to_input_channel: {e}"),
            })?;
        let user_input =
            crate::peer_resolve::user_id_to_input_user(&self.client, new_owner_user_id)
                .await
                .map_err(|e| MtprotoTelegramError::Rpc {
                    code: -1,
                    message: format!("{prefix}: resolve new owner: {e}"),
                })?;
        let req = tl::functions::channels::EditCreator {
            channel: channel_input,
            user_id: user_input,
            password: tl::enums::InputCheckPasswordSrp::InputCheckPasswordEmpty,
        };
        self.client
            .invoke(&req)
            .await
            .map_err(|e| crate::peer_resolve::map_invoke_err(prefix, e))?;
        Ok(())
    }
}

// ── Helpers for send_message / send_document / download_file ───────

/// Extract the first `(message_id, timestamp)` from an
/// `Updates` payload returned by `messages.sendMessage` or
/// `messages.sendMedia`. Both TL contracts return a single
/// Update carrying the freshly-sent Message.
fn extract_first_message_id_and_date(updates: &tl::enums::Updates) -> Option<MtprotoSentMessage> {
    use tl::enums::{Update as UpdateEnum, Updates as UpdatesEnum};
    let candidate = match updates {
        UpdatesEnum::Combined(combined) => combined.updates.first()?,
        UpdatesEnum::Updates(updates_list) => updates_list.updates.first()?,
        _ => return None,
    };
    let update = match candidate {
        UpdateEnum::NewMessage(u) => &u.message,
        UpdateEnum::NewScheduledMessage(u) => &u.message,
        UpdateEnum::MessageId(u) => {
            return Some(MtprotoSentMessage {
                id: i64::from(u.id),
                timestamp: 0,
            });
        }
        _ => return None,
    };
    let message = match update {
        tl::enums::Message::Message(m) => m,
        tl::enums::Message::Service(_) => return None,
        tl::enums::Message::Empty(_) => return None,
    };
    Some(MtprotoSentMessage {
        id: i64::from(message.id),
        timestamp: i64::from(message.date),
    })
}

/// Wrapper around `tl::enums::InputFileLocation` that
/// implements grammers' `Downloadable` trait. Used by
/// `download_file` to drive `Client::iter_download` from
/// an arbitrary `InputFileLocation` (e.g.,
/// `InputDocumentFileLocation`) without going through a
/// `Message` object first.
struct InputFileLocationDownloadable {
    location: tl::enums::InputFileLocation,
}

impl grammers_client::media::Downloadable for InputFileLocationDownloadable {
    fn to_raw_input_location(&self) -> Option<tl::enums::InputFileLocation> {
        Some(self.location.clone())
    }
}

/// Generate a 64-bit random ID for the
/// `random_id` field of `messages.SendMessage` /
/// `messages.SendMedia`. Telegram uses this to dedupe
/// retries (a sender with the same random_id is treated as
/// the same message).
fn generate_random_id_i64() -> i64 {
    // Tiny LCG seeded from SystemTime. Avoids pulling in
    // the `rand` crate. Telegram's random_id is 64 bits;
    // collision probability is negligible for our message
    // rate (one per request), and a collision would only
    // cause a "duplicate" detection on the server side,
    // which we can detect and retry. (The `rand` crate is
    // intentionally not pulled in here — the trait surface
    // stays lean and the determinism of the random_id
    // space is acceptable for our use case.)
    use std::time::{SystemTime, UNIX_EPOCH};
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos() as u64)
        .unwrap_or(0);
    // Simple xorshift mix (Marsaglia).
    let mut x = nanos ^ 0x9E3779B97F4A7C15;
    x ^= x << 13;
    x ^= x >> 7;
    x ^= x << 17;
    x as i64
}

/// Tiny MIME-type guess by filename extension. Falls back
/// to `application/octet-stream` (Telegram's default for
/// unknown types). The DOT/2 payloads we send are JSON;
/// the upload tests use `.bin` which we map to
/// `application/octet-stream`.
fn guess_mime_type(filename: &str) -> String {
    let ext = filename
        .rsplit('.')
        .next()
        .unwrap_or("")
        .to_ascii_lowercase();
    match ext.as_str() {
        "json" => "application/json",
        "txt" | "log" => "text/plain",
        "md" => "text/markdown",
        "html" | "htm" => "text/html",
        "pdf" => "application/pdf",
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "webp" => "image/webp",
        "mp4" => "video/mp4",
        "ogg" | "oga" => "audio/ogg",
        "mp3" => "audio/mpeg",
        "zip" => "application/zip",
        "tar" => "application/x-tar",
        _ => "application/octet-stream",
    }
    .to_string()
}

/// Decode a hex string to bytes. Strict (no whitespace,
/// no padding). Used to deserialize a `file_id` into
/// the underlying `InputFileLocation` bytes.
fn hex_decode(s: &str) -> Option<Vec<u8>> {
    if !s.len().is_multiple_of(2) {
        return None;
    }
    let mut out = Vec::with_capacity(s.len() / 2);
    let bytes = s.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        let hi = hex_nibble(bytes[i])?;
        let lo = hex_nibble(bytes[i + 1])?;
        out.push((hi << 4) | lo);
        i += 2;
    }
    Some(out)
}

fn hex_nibble(c: u8) -> Option<u8> {
    match c {
        b'0'..=b'9' => Some(c - b'0'),
        b'a'..=b'f' => Some(c - b'a' + 10),
        b'A'..=b'F' => Some(c - b'A' + 10),
        _ => None,
    }
}
