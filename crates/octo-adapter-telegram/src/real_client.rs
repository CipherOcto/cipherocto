//! Real TDLib client implementation.
//!
//! Mission Architecture line 57: "TDLib client wrapper — Owns the TDLib Client,
//! runs the receive loop on a dedicated tokio task, and exposes an async API."
//!
//! The receive loop lives in this file (`real_client.rs`) — the comment in
//! `client.rs` referencing `src/client.rs` for the receive loop is stale.
//!
//! Auth flow is driven entirely from the receive loop: the constructor only
//! calls `tdlib_rs::create_client()` and waits for `auth_ready`. When TDLib
//! emits an `AuthorizationState::WaitTdlibParameters` (both bot and user
//! mode), the loop calls `set_tdlib_parameters`. When the state machine
//! progresses to `WaitPhoneNumber` (bot mode) or `WaitCode` / `WaitPassword`
//! (user mode), the loop calls the appropriate TDLib function. This avoids
//! the brittle "set parameters with api_id=0 then immediately call
//! check_authentication_bot_token" pattern from R1.

use crate::auth::{create_auth_dirs, AuthAction, UserAuth};
use crate::client::{SentMessage, TelegramClient, TelegramUpdate};
use crate::error::{Result, TelegramError};
use crate::self_handle::SelfHandle;
use async_trait::async_trait;
use std::path::PathBuf;
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc, Mutex,
};
use tokio::sync::{mpsc, Notify};
use tokio::task::spawn_blocking;

/// The TDLib client ID typealias for clarity.
type ClientId = i32;

/// Shared state for the real TDLib client.
struct ClientState {
    /// The TDLib client identifier (used in Drop for cleanup).
    client_id: ClientId,
    /// Flag to control the receive loop. `false` ⇒ loop should exit.
    running: AtomicBool,
    /// Pending updates queue drained by receive_updates().
    pending_updates: Mutex<Vec<TelegramUpdate>>,
    /// Notified when auth reaches Ready state.
    auth_ready: Notify,
    /// Auth has completed successfully.
    auth_done: AtomicBool,
    /// Last auth error (set when auth fails; drained by the constructor).
    auth_error: Mutex<Option<String>>,
    /// User-mode auth config (None for bot mode).
    user_auth: Option<UserAuth>,
    /// Bot token for bot mode (None for user mode).
    bot_token: Option<String>,
    /// TDLib base data directory.
    data_dir: Option<PathBuf>,
    /// Tracks whether set_tdlib_parameters has been called for bot mode.
    bot_params_set: AtomicBool,
    /// Channel for inbound verification codes (user mode).
    code_tx: mpsc::Sender<String>,
    /// Receiver end of the verification-code channel. The receive loop
    /// drains this on every `WaitCode` update and forwards the most recent
    /// code via `tdlib_rs::functions::check_authentication_code`. Held
    /// inside `Option<...>` so the receive loop can take ownership once
    /// auth completes (or the client is dropped) and the channel can
    /// close cleanly. `Arc<Mutex<...>>` because `RealTelegramClient` is
    /// `Clone` and the receive loop runs on a `tokio::spawn`'d task.
    code_rx: Arc<Mutex<Option<mpsc::Receiver<String>>>>,
    /// Self handle — populated after a successful `get_me` call.
    self_handle: SelfHandle,
}

impl ClientState {
    fn new(
        client_id: ClientId,
        user_auth: Option<UserAuth>,
        bot_token: Option<String>,
        data_dir: Option<PathBuf>,
    ) -> Self {
        let (code_tx, code_rx) = mpsc::channel(8);
        Self {
            client_id,
            running: AtomicBool::new(true),
            pending_updates: Mutex::new(Vec::new()),
            auth_ready: Notify::new(),
            auth_done: AtomicBool::new(false),
            auth_error: Mutex::new(None),
            user_auth,
            bot_token,
            data_dir,
            bot_params_set: AtomicBool::new(false),
            code_tx,
            code_rx: Arc::new(Mutex::new(Some(code_rx))),
            self_handle: SelfHandle::new(),
        }
    }
}

/// Real TDLib-based Telegram client.
/// Implements `TelegramClient` using tdlib-rs for actual Telegram connectivity.
#[derive(Clone)]
pub struct RealTelegramClient {
    state: Arc<ClientState>,
}

impl RealTelegramClient {
    /// Create a new RealTelegramClient for bot mode.
    /// `bot_token` is the Telegram BotFather token.
    /// `data_dir` is the directory for TDLib's database and files.
    pub async fn new(bot_token: Option<String>, data_dir: Option<PathBuf>) -> Result<Self> {
        Self::new_internal(None, bot_token, data_dir).await
    }

    /// Create a new RealTelegramClient for user mode with the given auth config.
    /// `data_dir` is the directory for TDLib's database and files.
    pub async fn new_user(user_auth: UserAuth, data_dir: Option<PathBuf>) -> Result<Self> {
        Self::new_internal(Some(user_auth), None, data_dir).await
    }

    /// Internal constructor. `user_auth = None` for bot mode, `Some(...)` for user mode.
    async fn new_internal(
        user_auth: Option<UserAuth>,
        bot_token: Option<String>,
        data_dir: Option<PathBuf>,
    ) -> Result<Self> {
        let client_id = tdlib_rs::create_client();

        // Create auth directories up-front so set_tdlib_parameters does not
        // fail with "database directory does not exist".
        if let Some(ref dir) = data_dir {
            create_auth_dirs(dir).map_err(TelegramError::Io)?;
        }

        let state = Arc::new(ClientState::new(
            client_id,
            user_auth.clone(),
            bot_token.clone(),
            data_dir.clone(),
        ));

        // Spawn the receive loop.
        let state_clone = state.clone();
        tokio::spawn(async move {
            Self::receive_loop(state_clone).await;
        });

        // Wait for Ready state (or Closed / auth error on failure), with 30 s timeout.
        let notified = state.auth_ready.notified();
        let timeout_result =
            tokio::time::timeout(std::time::Duration::from_secs(30), notified).await;

        if timeout_result.is_err() {
            // Drain any stored auth error so the caller gets the cause.
            let err_msg = state
                .auth_error
                .lock()
                .unwrap()
                .clone()
                .unwrap_or_else(|| "auth timeout".into());
            return Err(TelegramError::TdlibClient(err_msg));
        }

        if !state.auth_done.load(Ordering::Acquire) {
            let err_msg = state
                .auth_error
                .lock()
                .unwrap()
                .clone()
                .unwrap_or_else(|| "auth failed".into());
            return Err(TelegramError::TdlibClient(err_msg));
        }

        Ok(Self { state })
    }

    /// Submit a verification code for user mode. Called by the gateway in
    /// response to a `WaitCode` auth state. Returns `Err` if the channel is
    /// closed (client was dropped).
    pub async fn submit_verification_code(&self, code: String) -> Result<()> {
        self.state
            .code_tx
            .send(code)
            .await
            .map_err(|_| TelegramError::TdlibClient("code channel closed".into()))
    }

    /// The receive loop that processes TDLib updates.
    async fn receive_loop(state: Arc<ClientState>) {
        loop {
            if !state.running.load(Ordering::Acquire) {
                break;
            }
            // Use spawn_blocking for the blocking receive() call so we do not
            // block the tokio runtime. New blocking thread per iteration is
            // cheap because tdlib_rs::receive blocks until an update arrives.
            let result = spawn_blocking(tdlib_rs::receive).await;

            match result {
                Ok(Some((update, _client_id))) => {
                    if let Err(e) = Self::handle_update(&state, update).await {
                        tracing::debug!(error = %e, "tdlib update handler error");
                    }
                    // Yield so a flood of updates does not starve the runtime.
                    tokio::task::yield_now().await;
                }
                Ok(None) => {
                    // No update available right now — yield to avoid CPU spinning.
                    tokio::task::yield_now().await;
                }
                Err(_) => {
                    // Receive error (likely a panic in spawn_blocking or runtime shutdown).
                    state.running.store(false, Ordering::Release);
                    break;
                }
            }
        }
    }

    /// Process one TDLib update. Returns `Err` for unrecoverable auth errors
    /// so the loop can record them and the constructor can surface them.
    async fn handle_update(
        state: &Arc<ClientState>,
        update: tdlib_rs::enums::Update,
    ) -> std::result::Result<(), String> {
        if let tdlib_rs::enums::Update::AuthorizationState(auth_update) = update {
            let auth_state = auth_update.authorization_state.clone();
            return Self::handle_auth_state(state, auth_state).await;
        }
        if let Some(telegram_update) = Self::convert_update(update) {
            state.pending_updates.lock().unwrap().push(telegram_update);
        }
        Ok(())
    }

    /// Drive the TDLib auth state machine from the receive loop.
    async fn handle_auth_state(
        state: &Arc<ClientState>,
        auth_state: tdlib_rs::enums::AuthorizationState,
    ) -> std::result::Result<(), String> {
        match &auth_state {
            tdlib_rs::enums::AuthorizationState::Ready => {
                state.auth_done.store(true, Ordering::Release);
                state.auth_ready.notify_waiters();
                // Populate SelfHandle from get_me. Done best-effort: a failure
                // here does not invalidate the auth state.
                match tdlib_rs::functions::get_me(state.client_id).await {
                    Ok(tdlib_rs::enums::User::User(u)) => {
                        // Extract primary username from the Usernames struct.
                        let username = u
                            .usernames
                            .as_ref()
                            .and_then(|us| us.active_usernames.first().cloned())
                            .unwrap_or_default();
                        state.self_handle.set_identity(u.id, username);
                    }
                    Err(e) => {
                        tracing::debug!(error = %e.message, "get_me failed; SelfHandle left empty");
                    }
                }
                Ok(())
            }
            tdlib_rs::enums::AuthorizationState::Closed => {
                // L11: no need to store `running = false` here — the loop
                // exits via the `Err` returned below. The notify_waiters
                // unblocks the constructor's `notified().await`.
                let mut err = state.auth_error.lock().unwrap();
                if err.is_none() {
                    *err = Some("tdlib session closed".into());
                }
                state.auth_ready.notify_waiters();
                Err("tdlib session closed".into())
            }
            tdlib_rs::enums::AuthorizationState::WaitTdlibParameters => {
                if state.user_auth.is_none() {
                    // Bot mode
                    if !state.bot_params_set.swap(true, Ordering::AcqRel) {
                        let base = state
                            .data_dir
                            .clone()
                            .unwrap_or_else(|| PathBuf::from("octo_telegram_bot"));
                        let db_dir = base.join("database");
                        let files_dir = base.join("files");
                        let _ = std::fs::create_dir_all(&db_dir);
                        let _ = std::fs::create_dir_all(&files_dir);
                        // For bot mode we use the test DC and synthetic api credentials.
                        // Real bots need real api_id/api_hash from my.telegram.org,
                        // but TDLib's bot flow does not require them at the param step.
                        let resp = tdlib_rs::functions::set_tdlib_parameters(
                            true,                                     // use_test_dc
                            db_dir.to_string_lossy().into_owned(),    // database_directory
                            files_dir.to_string_lossy().into_owned(), // files_directory
                            String::new(),                            // database_encryption_key
                            true,                                     // use_file_database
                            true,                                     // use_chat_info_database
                            true,                                     // use_message_database
                            false,                                    // use_secret_chats
                            0,                   // api_id (bot mode accepts 0 on test DC)
                            String::new(),       // api_hash
                            "en".into(),         // language
                            "CipherOcto".into(), // device_model
                            String::new(),       // system_version
                            env!("CARGO_PKG_VERSION").into(), // app_version
                            state.client_id,
                        )
                        .await;
                        if let Err(e) = resp {
                            let msg = format!("set_tdlib_parameters: {}", e.message);
                            *state.auth_error.lock().unwrap() = Some(msg.clone());
                            return Err(msg);
                        }
                    }
                } else if let Some(ref user_auth) = state.user_auth {
                    if let Err(e) = user_auth
                        .handle_authorization_state(
                            auth_state.clone(),
                            state.client_id,
                            state.data_dir.as_deref(),
                        )
                        .await
                    {
                        let msg = format!("user auth: {}", e);
                        *state.auth_error.lock().unwrap() = Some(msg.clone());
                        return Err(msg);
                    }
                }
                Ok(())
            }
            tdlib_rs::enums::AuthorizationState::WaitPhoneNumber => {
                if state.user_auth.is_none() {
                    // Bot mode: this is where the bot token is submitted.
                    if let Some(ref token) = state.bot_token {
                        let token_resp = tdlib_rs::functions::check_authentication_bot_token(
                            token.clone(),
                            state.client_id,
                        )
                        .await;
                        if let Err(e) = token_resp {
                            let msg = format!("bot auth: {}", e.message);
                            *state.auth_error.lock().unwrap() = Some(msg.clone());
                            return Err(msg);
                        }
                    }
                } else if let Some(ref user_auth) = state.user_auth {
                    if let Err(e) = user_auth
                        .handle_authorization_state(
                            auth_state.clone(),
                            state.client_id,
                            state.data_dir.as_deref(),
                        )
                        .await
                    {
                        let msg = format!("user auth: {}", e);
                        *state.auth_error.lock().unwrap() = Some(msg.clone());
                        return Err(msg);
                    }
                }
                Ok(())
            }
            tdlib_rs::enums::AuthorizationState::WaitCode(_) => {
                if let Some(ref user_auth) = state.user_auth {
                    // C1: drive the auth flow off the new `decide` decision.
                    // `AwaitCode` is the load-bearing action — it tells the
                    // receive loop to drain `code_rx` and forward the most
                    // recent submitted code to TDLib via
                    // `check_authentication_code`. Previously this path
                    // returned `Err(AuthenticationFailed("verification code
                    // required"))` and the loop silently swallowed the
                    // error, causing the 30 s constructor timeout to fire.
                    match user_auth.decide(&auth_state) {
                        AuthAction::AwaitCode => {
                            if let Some(code) = Self::drain_latest_code(state) {
                                if let Err(e) = tdlib_rs::functions::check_authentication_code(
                                    code,
                                    state.client_id,
                                )
                                .await
                                {
                                    let msg = format!("check_authentication_code: {}", e.message);
                                    *state.auth_error.lock().unwrap() = Some(msg.clone());
                                    return Err(msg);
                                }
                            } else {
                                // No code submitted yet — TDLib will re-emit
                                // `WaitCode` on the next update tick, at which
                                // point we will try again. The constructor's
                                // 30 s timeout still bounds the wait.
                                tracing::debug!("WaitCode: no verification code submitted yet");
                            }
                        }
                        other => {
                            // Unreachable: `decide` always returns
                            // `AwaitCode` for `WaitCode(_)` regardless of
                            // `UserAuth` config. Log defensively in case
                            // future enum variants change the mapping.
                            tracing::debug!(
                                action = ?other,
                                "WaitCode: unexpected AuthAction from decide"
                            );
                        }
                    }
                }
                Ok(())
            }
            _ => {
                if let Some(ref user_auth) = state.user_auth {
                    if let Err(e) = user_auth
                        .handle_authorization_state(
                            auth_state.clone(),
                            state.client_id,
                            state.data_dir.as_deref(),
                        )
                        .await
                    {
                        tracing::debug!(error = %e, "user auth state error");
                    }
                }
                Ok(())
            }
        }
    }

    /// Drain any pending verification codes from `code_rx` and return the
    /// most recent one. Returns `None` if the channel is empty (no code has
    /// been submitted yet) or if the receiver has already been taken
    /// (auth completed or the client was dropped). The receive loop calls
    /// this on every `WaitCode` update; if it returns `Some(code)`, the
    /// loop forwards the code via `check_authentication_code`.
    fn drain_latest_code(state: &Arc<ClientState>) -> Option<String> {
        let mut guard = state.code_rx.lock().unwrap();
        let rx = guard.as_mut()?;
        let mut latest: Option<String> = None;
        loop {
            match rx.try_recv() {
                Ok(code) => latest = Some(code),
                Err(mpsc::error::TryRecvError::Empty) => break,
                Err(mpsc::error::TryRecvError::Disconnected) => break,
            }
        }
        latest
    }

    /// Convert tdlib-rs Update to our TelegramUpdate enum.
    fn convert_update(update: tdlib_rs::enums::Update) -> Option<TelegramUpdate> {
        match update {
            tdlib_rs::enums::Update::NewMessage(new_msg) => {
                // L16: keep user_id as i64 on NewMessage. The legacy `from: String`
                // field is kept for backward-compat but is no longer the canonical
                // identity (use the structured accessor instead).
                let _from_id = match &new_msg.message.sender_id {
                    tdlib_rs::enums::MessageSender::User(user_id) => user_id.user_id,
                    tdlib_rs::enums::MessageSender::Chat(c) => c.chat_id,
                };
                let from = _from_id.to_string();
                Some(TelegramUpdate::NewMessage(crate::client::NewMessage {
                    chat_id: new_msg.message.chat_id,
                    message: Self::extract_message_text(&new_msg.message.content),
                    from,
                }))
            }
            tdlib_rs::enums::Update::MessageEdited(edited) => Some(TelegramUpdate::MessageEdited(
                crate::client::MessageEdited {
                    chat_id: edited.chat_id,
                    message_id: edited.message_id.to_string(),
                    new_text: String::new(),
                },
            )),
            tdlib_rs::enums::Update::File(file_update) => {
                if !file_update.file.local.path.is_empty() {
                    Some(TelegramUpdate::FileDownloaded(
                        crate::client::FileDownloaded {
                            file_id: file_update.file.id.to_string(),
                            local_path: file_update.file.local.path,
                            size: file_update.file.size as u64,
                        },
                    ))
                } else {
                    None
                }
            }
            _ => None,
        }
    }

    /// Extract text content from a MessageContent enum.
    /// Returns the text for text messages, the (base64-encoded) caption for
    /// document messages, or empty string for other content types. The base64
    /// caption is set by `send_document` (see `send_document`'s doc-comment);
    /// the adapter's `canonicalize` decodes it.
    fn extract_message_text(content: &tdlib_rs::enums::MessageContent) -> String {
        match content {
            tdlib_rs::enums::MessageContent::MessageText(msg) => msg.text.text.clone(),
            tdlib_rs::enums::MessageContent::MessageDocument(doc) => doc.caption.text.clone(),
            _ => String::new(),
        }
    }
}

#[async_trait]
impl TelegramClient for RealTelegramClient {
    async fn send_message(&self, chat_id: &str, text: &str) -> Result<SentMessage> {
        let chat_id_i64: i64 = chat_id
            .parse()
            .map_err(|_| TelegramError::InvalidChatId(chat_id.into()))?;

        let content = tdlib_rs::types::InputMessageText {
            text: tdlib_rs::types::FormattedText {
                text: text.into(),
                entities: vec![],
            },
            link_preview_options: None,
            clear_draft: false,
        };

        let result = tdlib_rs::functions::send_message(
            chat_id_i64,
            None, // topic_id
            None, // reply_to
            None, // options
            tdlib_rs::enums::InputMessageContent::InputMessageText(content),
            self.state.client_id,
        )
        .await;

        match result {
            Ok(tdlib_rs::enums::Message::Message(msg)) => {
                Ok(SentMessage::new(msg.id.to_string(), msg.date as i64))
            }
            Err(e) => Err(Self::classify_tdlib_error(e)),
        }
    }

    async fn send_document(
        &self,
        chat_id: &str,
        filename: &str,
        data: &[u8],
    ) -> Result<SentMessage> {
        use std::io::Write;

        let chat_id_i64: i64 = chat_id
            .parse()
            .map_err(|_| TelegramError::InvalidChatId(chat_id.into()))?;

        // Write data to a uniquely-named temp file to avoid collisions.
        let temp_path = unique_temp_path("octo_doc");
        {
            let mut file = std::fs::File::create(&temp_path).map_err(TelegramError::Io)?;
            file.write_all(data).map_err(TelegramError::Io)?;
        }

        let input_file = tdlib_rs::types::InputFileLocal {
            path: temp_path.to_string_lossy().into_owned(),
        };

        // Wire format: caption is the base64-encoded envelope. The receive path
        // returns the caption as `extract_message_text` for MessageDocument, so
        // the adapter can decode it without an extra round-trip.
        let encoded = crate::envelope::encode_envelope(data);
        let content = tdlib_rs::types::InputMessageDocument {
            document: tdlib_rs::enums::InputFile::Local(input_file),
            thumbnail: None,
            disable_content_type_detection: false,
            caption: Some(tdlib_rs::types::FormattedText {
                text: encoded,
                entities: vec![],
            }),
        };

        let result = tdlib_rs::functions::send_message(
            chat_id_i64,
            None,
            None,
            None,
            tdlib_rs::enums::InputMessageContent::InputMessageDocument(content),
            self.state.client_id,
        )
        .await;

        crate::cleanup::cleanup_temp_file(&temp_path);

        match result {
            Ok(tdlib_rs::enums::Message::Message(msg)) => {
                // `_filename` is reserved in case the wire format evolves to embed it
                // alongside the encoded envelope. Currently the encoded envelope is
                // the entire caption; filename is preserved here for API symmetry.
                let _ = filename;
                Ok(SentMessage::new(msg.id.to_string(), msg.date as i64))
            }
            Err(e) => Err(Self::classify_tdlib_error(e)),
        }
    }

    /// Download a file by its TDLib file_id (as a string).
    async fn download_file(&self, file_id_str: &str) -> Result<Vec<u8>> {
        let file_id: i32 = file_id_str.parse().map_err(|_| {
            TelegramError::InvalidChatId(format!("invalid file_id: {}", file_id_str))
        })?;

        crate::files::download_file_bytes(self.state.client_id, file_id)
            .await
            .map_err(|e| TelegramError::File(e.to_string()))
    }

    async fn receive_updates(&self) -> Result<Vec<TelegramUpdate>> {
        let mut pending = self.state.pending_updates.lock().unwrap();
        Ok(std::mem::take(&mut *pending))
    }

    async fn authenticate(&self) -> Result<()> {
        // Bot mode: caller provides bot token via RealTelegramClient::new()
        // User mode: caller uses RealTelegramClient::new_user(user_auth) instead.
        // This method is a no-op for bot mode.
        Ok(())
    }
}

impl RealTelegramClient {
    /// Map a TDLib error to a structured `TelegramError`, recognizing
    /// 429-equivalent error codes and FLOOD_WAIT_* messages.
    fn classify_tdlib_error(e: tdlib_rs::types::Error) -> TelegramError {
        // 429 = FLOOD_WAIT_X in TDLib; we expose RateLimited.
        if e.code == 429 {
            let secs = parse_flood_wait_secs(&e.message).unwrap_or(1);
            return TelegramError::RateLimited {
                retry_after_secs: secs,
            };
        }
        TelegramError::TdlibClient(e.message)
    }
}

/// Parse `FLOOD_WAIT_42` → `Some(42)`. Returns None on non-FLOOD_WAIT errors.
fn parse_flood_wait_secs(message: &str) -> Option<u64> {
    let rest = message.strip_prefix("FLOOD_WAIT_")?;
    rest.parse().ok()
}

impl Drop for RealTelegramClient {
    fn drop(&mut self) {
        self.state.running.store(false, Ordering::Release);
        let client_id = self.state.client_id;
        // Spawn a detached thread to close the TDLib client. We deliberately
        // do NOT panic on failure: if the runtime cannot be built (FD
        // exhaustion during process shutdown) the OS will reclaim the TDLib
        // resources at exit. Panicking during Drop causes std::process::abort
        // if multiple threads are dropping simultaneously.
        std::thread::spawn(move || {
            let rt_result = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build();
            if let Ok(rt) = rt_result {
                rt.block_on(async {
                    let _ = tdlib_rs::functions::close(client_id).await;
                });
            }
            // On failure, silently no-op. Process exit will reclaim the client.
        });
    }
}

/// Generate a unique temp file path under the system temp dir.
fn unique_temp_path(prefix: &str) -> std::path::PathBuf {
    use std::sync::atomic::{AtomicU64, Ordering};
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let id = COUNTER.fetch_add(1, Ordering::Relaxed);
    std::env::temp_dir().join(format!("{}_{}_{}", prefix, std::process::id(), id))
}
