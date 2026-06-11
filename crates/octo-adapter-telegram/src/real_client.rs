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
use crate::config::TelegramConfig;
use crate::error::{Result, TelegramError};
use crate::self_handle::SelfHandle;
use async_trait::async_trait;
use std::path::PathBuf;
use parking_lot::Mutex as PlMutex;
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc, Mutex,
};
use tokio::sync::{mpsc, Notify};

/// The TDLib client ID typealias for clarity.
type ClientId = i32;

/// Shared state for the real TDLib client.
struct ClientState {
    /// The TDLib client identifier (used in Drop for cleanup).
    client_id: ClientId,
    /// Flag to control the receive loop. `false` ⇒ loop should exit.
    running: AtomicBool,
    /// Pending updates sent via mpsc channel (CONC-C1).
    pending_updates_tx: mpsc::Sender<TelegramUpdate>,
    /// CR-H2: counter of dropped updates due to channel full.
    dropped_updates: std::sync::atomic::AtomicU64,
    /// Receiver for pending updates (CONC-C1).
    pending_updates_rx: parking_lot::Mutex<Option<mpsc::Receiver<TelegramUpdate>>>,
    /// Notified when auth reaches Ready state.
    auth_ready: Notify,
    /// PERF-H4: notified when new TDLib updates arrive.
    update_notify: Notify,
    /// Auth has completed successfully.
    auth_done: AtomicBool,
    /// Last auth error (set when auth fails; drained by the constructor).
    auth_error: PlMutex<Option<String>>,
    /// User-mode auth config (None for bot mode).
    user_auth: Option<UserAuth>,
    /// Bot token for bot mode (None for user mode).
    bot_token: Option<String>,
    /// TDLib base data directory.
    data_dir: Option<PathBuf>,
    /// api_id for `set_tdlib_parameters` (bot + user modes).
    /// Required by TDLib's `set_tdlib_parameters` regardless of mode.
    /// C2: previously bot mode passed `0`, which is only valid on the
    /// test DC. Production callers must supply a real api_id from
    /// my.telegram.org via `TelegramConfig`.
    api_id: i32,
    /// api_hash for `set_tdlib_parameters` (bot + user modes).
    /// C2: previously bot mode passed `String::new()`, which is only
    /// valid on the test DC.
    api_hash: String,
    /// Tracks whether set_tdlib_parameters has been called for bot mode.
    bot_params_set: AtomicBool,
    /// Tracks whether set_tdlib_parameters has been called for user mode.
    user_params_set: AtomicBool,
    /// Set to true when AuthorizationState::Closed is received.
    closed: AtomicBool,
    /// JoinHandle for the long-lived tdlib_rs::receive() blocking thread (L15).
    /// Populated by receive_loop after spawn; joined in Drop.
    receive_thread: PlMutex<Option<std::thread::JoinHandle<()>>>,
    /// Channel for inbound verification codes (user mode).
    code_tx: mpsc::Sender<String>,
    /// Receiver end of the verification-code channel. The receive loop
    /// drains this on every `WaitCode` update and forwards the most recent
    /// code via `tdlib_rs::functions::check_authentication_code`. Held
    /// inside `Option<...>` so the receive loop can take ownership once
    /// auth completes (or the client is dropped) and the channel can
    /// close cleanly. `Arc<Mutex<...>>` because `RealTelegramClient` is
    /// `Clone` and the receive loop runs on a `tokio::spawn`'d task.
    code_rx: Arc<PlMutex<Option<mpsc::Receiver<String>>>>,
    /// Self handle — populated after a successful `get_me` call.
    self_handle: SelfHandle,
    /// Sender half of the shutdown channel. `Drop` pushes the `client_id`
    /// through this to ask the receive loop to call
    /// `tdlib_rs::functions::close` and exit. C3: this replaces a detached
    /// `std::thread` + nested `current_thread` runtime, which could race
    /// with the receive loop and leak the TDLib client. The receiver is
    /// held in `shutdown_rx` below and is moved into the receive loop at
    /// spawn time; the sender stays on `ClientState` so any `Drop` of a
    /// `RealTelegramClient` clone can signal the loop.
    shutdown_tx: mpsc::Sender<ClientId>,
    /// Receiver half of the shutdown channel. Moved into the receive loop
    /// by `new_internal`; stored as `Option<...>` so the constructor can
    /// `take()` it before spawning. After that it lives only on the
    /// receive loop task; `Drop` does not need it.
    shutdown_rx: Option<mpsc::Receiver<ClientId>>,
}

impl ClientState {
    #[allow(clippy::too_many_arguments)]
    fn new(
        client_id: ClientId,
        user_auth: Option<UserAuth>,
        bot_token: Option<String>,
        data_dir: Option<PathBuf>,
        api_id: i32,
        api_hash: String,
    ) -> Self {
        let (update_tx, update_rx) = mpsc::channel::<TelegramUpdate>(256);
        let (code_tx, code_rx) = mpsc::channel(8);
        let (shutdown_tx, shutdown_rx) = mpsc::channel(1);
        Self {
            client_id,
            running: AtomicBool::new(true),
            pending_updates_tx: update_tx,
            pending_updates_rx: parking_lot::Mutex::new(Some(update_rx)),
            dropped_updates: std::sync::atomic::AtomicU64::new(0),
            auth_ready: Notify::new(),
            update_notify: Notify::new(),
            auth_done: AtomicBool::new(false),
            auth_error: PlMutex::new(None),
            user_auth,
            bot_token,
            data_dir,
            api_id,
            api_hash,
            bot_params_set: AtomicBool::new(false),
            user_params_set: AtomicBool::new(false),
            closed: AtomicBool::new(false),
            receive_thread: PlMutex::new(None),
            code_tx,
            code_rx: Arc::new(PlMutex::new(Some(code_rx))),
            self_handle: SelfHandle::new(),
            shutdown_tx,
            shutdown_rx: Some(shutdown_rx),
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
    ///
    /// `config` is the validated `TelegramConfig`. Caller must call
    /// `config.validate()` first; this constructor does not re-validate.
    /// `config.bot_token` is the Telegram BotFather token.
    /// `config.data_dir` is the directory for TDLib's database and files.
    /// `config.api_id` and `config.api_hash` are passed through to
    /// `set_tdlib_parameters` (C2 — production bot mode requires real
    /// credentials from my.telegram.org, not the test DC).
    pub async fn new(config: &TelegramConfig) -> Result<Self> {
        let api_id = config.api_id.unwrap_or(0);
        let api_hash = config.api_hash.clone().unwrap_or_default();
        Self::new_internal(
            None,
            config.bot_token.clone(),
            config.data_dir.clone(),
            api_id,
            api_hash,
        )
        .await
    }

    /// Create a new RealTelegramClient for user mode with the given auth config.
    /// `data_dir` is the directory for TDLib's database and files.
    pub async fn new_user(user_auth: UserAuth, data_dir: Option<PathBuf>) -> Result<Self> {
        let api_id = user_auth.api_id;
        let api_hash = user_auth.api_hash.clone();
        Self::new_internal(Some(user_auth), None, data_dir, api_id, api_hash.to_string()).await
    }

    /// Internal constructor. `user_auth = None` for bot mode, `Some(...)` for user mode.
    /// `api_id` and `api_hash` are plumbed to `set_tdlib_parameters` regardless of mode.
    async fn new_internal(
        user_auth: Option<UserAuth>,
        bot_token: Option<String>,
        data_dir: Option<PathBuf>,
        api_id: i32,
        api_hash: String,
    ) -> Result<Self> {
        let client_id = tdlib_rs::create_client();

        // Create auth directories up-front so set_tdlib_parameters does not
        // fail with "database directory does not exist".
        if let Some(ref dir) = data_dir {
            create_auth_dirs(dir).map_err(TelegramError::Io)?;
        }

        let mut state = Arc::new(ClientState::new(
            client_id,
            user_auth.clone(),
            bot_token.clone(),
            data_dir.clone(),
            api_id,
            api_hash,
        ));

        // Spawn the receive loop. The shutdown receiver is moved in here
        // (and only here) — `Drop` keeps a `Sender<ClientId>` on the
        // shared state and `try_send`s the client_id to ask the loop to
        // close cleanly. C3.
        //
        // `Arc::get_mut` succeeds because we are the sole strong
        // reference to `state` at this point; the clone below is the
        // second strong ref, and the spawned task takes it.
        let shutdown_rx = Arc::get_mut(&mut state)
            .expect("state should have exactly one strong ref before spawn")
            .shutdown_rx
            .take()
            .expect("shutdown_rx is set in ClientState::new and consumed exactly once");
        let state_clone = state.clone();
        tokio::spawn(async move {
            Self::receive_loop(state_clone, shutdown_rx).await;
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
                .clone()
                .unwrap_or_else(|| "auth timeout".into());
            return Err(TelegramError::TdlibClient { code: 408, message: err_msg });
        }

        if !state.auth_done.load(Ordering::Acquire) {
            let err_msg = state
                .auth_error
                .lock()
                .clone()
                .unwrap_or_else(|| "auth failed".into());
            return Err(TelegramError::TdlibClient { code: 408, message: err_msg });
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
            .map_err(|_| TelegramError::TdlibClient { code: 500, message: "code channel closed".into() })
    }

    /// H8: clone the `SelfHandle` so the gateway can hand the same
    /// instance to the adapter. Cheap (Arc clone). The receive loop
    /// populates this from `get_me` on `Ready`; callers may receive an
    /// empty handle if the client is queried before auth completes.
    pub fn self_handle(&self) -> SelfHandle {
        self.state.self_handle.clone()
    }

    /// PERF-C1: returns the number of updates dropped since last poll due to
    /// channel capacity exhaustion. The counter is reset on each call.
    pub fn dropped_updates_count(&self) -> u64 {
        self.state.dropped_updates.swap(0, std::sync::atomic::Ordering::Relaxed)
    }

    /// The receive loop that processes TDLib updates.
    ///
    /// `shutdown_rx` is the receiver end of the shutdown channel owned by
    /// `ClientState`. `Drop` pushes the `client_id` through the matching
    /// sender; on receipt the loop calls `tdlib_rs::functions::close`
    /// from within this existing tokio runtime (no nested runtime, no
    /// detached thread — C3) and then exits.
    async fn receive_loop(state: Arc<ClientState>, mut shutdown_rx: mpsc::Receiver<ClientId>) {
        // L15: single long-lived blocking thread for tdlib_rs::receive.
        // Previously each iteration called spawn_blocking, creating a new
        // thread per update. The dedicated thread loops on tdlib_rs::receive
        // and pushes (update, client_id) pairs through an mpsc channel.
        let (update_tx, mut update_rx) = mpsc::channel::<(tdlib_rs::enums::Update, ClientId)>(256);
        let state_clone = Arc::clone(&state);
        let handle = std::thread::spawn(move || {
            while state_clone.running.load(Ordering::Acquire) {
                match tdlib_rs::receive() {
                    Some((update, client_id)) => {
                        if update_tx.blocking_send((update, client_id)).is_err() {
                            break;
                        }
                    }
                    None => // CR-L2: bounded by TDLib 2s internal timeout
                        std::thread::sleep(std::time::Duration::from_millis(10)),
                }
            }
        });
        // Store the JoinHandle so Drop can join the thread after shutdown.
        *state.receive_thread.lock() = Some(handle);

        // Async receive loop reads from the channel.
        loop {
            if !state.running.load(Ordering::Acquire) {
                break;
            }
            tokio::select! {
                biased;
                _ = shutdown_rx.recv() => {
                    let client_id = state.client_id;
                    if let Err(e) = // CR-M8: fire-and-forget close — error propagation not possible in Drop context
                    tdlib_rs::functions::close(client_id).await {
                        tracing::debug!(error = %e.message, "tdlib close on shutdown failed");
                    }
                    state.running.store(false, Ordering::Release);
                    break;
                }
                Some((update, _client_id)) = update_rx.recv() => {
                    if let Err(e) = Self::handle_update(&state, update).await {
                        tracing::debug!(error = %e, "tdlib update handler error");
                    }
                    // PERF-H4: 10ms sleep imposes ~100 msg/s ceiling. Adjust if throughput is insufficient.
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
                }
                else => {
                    // Channel closed — blocking thread exited.
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
            // PERF-H4: notify receive loop that updates are available
            state.update_notify.notify_one();
            // CONC-C1: unbounded mpsc send; if full, drop oldest
            if let Err(e) = state.pending_updates_tx.try_send(telegram_update) {
                // CR-H2: increment counter so receive_updates can report the loss
                state.dropped_updates.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                tracing::warn!("pending_updates channel full, dropping update");
                let _ = e;
            }
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
                        tracing::warn!(error = %crate::error::redact_credentials(&e.message), "get_me failed; SelfHandle left empty");
                        // SM-M3: one retry for transient get_me failure
                        if e.code >= 500 || e.message.contains("connection") {
                            tracing::debug!("get_me: retrying after transient failure");
                            match tdlib_rs::functions::get_me(state.client_id).await {
                                Ok(me) => {
                                    let tdlib_rs::enums::User::User(u) = me;
                                    state.self_handle.set_identity(u.id, u.usernames.map_or(String::new(), |un| un.active_usernames.first().cloned().unwrap_or_default()));
                                }
                                Err(e2) => {
                                    tracing::error!(error = %crate::error::redact_credentials(&e2.message), "get_me: retry also failed");
                                }
                            }
                        }
                    }
                }
                Ok(())
            }
            tdlib_rs::enums::AuthorizationState::Closed => {
                // L10: record the closed state so the constructor can check
                // it before waiting for `auth_ready`.
                state.closed.store(true, Ordering::Release);
                let mut err = state.auth_error.lock();
                if err.is_none() {
                    *err = Some("tdlib session closed".into());
                }
                state.auth_ready.notify_waiters();
                Err("tdlib session closed".into())
            }
            tdlib_rs::enums::AuthorizationState::WaitTdlibParameters => {
                if state.user_auth.is_none() {
                    // Bot mode
                    if !state.bot_params_set.swap(true, Ordering::Release) {
                        let base = state
                            .data_dir
                            .clone()
                            .unwrap_or_else(|| PathBuf::from("octo_telegram_bot"));
                        let db_dir = base.join("database");
                        let files_dir = base.join("files");
                        let _ = std::fs::create_dir_all(&db_dir);
                        let _ = std::fs::create_dir_all(&files_dir);
                        // C2: bot mode uses real api_id/api_hash from
                        // `TelegramConfig` and the production DC. Synthetic
                        // credentials (`api_id=0`, `api_hash=""`) and
                        // `use_test_dc=true` are only valid on the test DC.
                        // `config::validate()` rejects bot configs that lack
                        // these fields, so by the time we get here the
                        // caller has supplied real credentials.
                        let resp = tdlib_rs::functions::set_tdlib_parameters(
                            false,                                    // use_test_dc
                            db_dir.to_string_lossy().into_owned(),    // database_directory
                            files_dir.to_string_lossy().into_owned(), // files_directory
                            String::new(),                            // database_encryption_key
                            true,                                     // use_file_database
                            true,                                     // use_chat_info_database
                            true,                                     // use_message_database
                            false,                                    // use_secret_chats
                            state.api_id,
                            state.api_hash.clone(),
                            "en".into(),                      // language
                            "CipherOcto".into(),              // device_model
                            String::new(),                    // system_version
                            env!("CARGO_PKG_VERSION").into(), // app_version
                            state.client_id,
                        )
                        .await;
                        if let Err(e) = resp {
                            let msg = format!("set_tdlib_parameters: {}", e.message);
                            *state.auth_error.lock() = Some(msg.clone());
                            return Err(msg);
                        }
                    }
                } else if let Some(ref user_auth) = state.user_auth {
                    // L5: only set parameters once per session to avoid
                    // redundant TDLib calls on re-emitted WaitTdlibParameters.
                    if !state.user_params_set.swap(true, Ordering::Release) {
                        if let Err(e) = user_auth
                            .handle_authorization_state(
                                auth_state.clone(),
                                state.client_id,
                                state.data_dir.as_deref(),
                            )
                            .await
                        {
                            let msg = format!("user auth: {}", e);
                            *state.auth_error.lock() = Some(msg.clone());
                            return Err(msg);
                        }
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
                            *state.auth_error.lock() = Some(msg.clone());
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
                        *state.auth_error.lock() = Some(msg.clone());
                        return Err(msg);
                    }
                }
                Ok(())
            }
            tdlib_rs::enums::AuthorizationState::LoggingOut => {
                tracing::info!("auth: LoggingOut — stopping receive loop");
                // Signal the loop to stop, then return control
                state.running.store(false, Ordering::Release);
                Ok(())
            }
            tdlib_rs::enums::AuthorizationState::Closing => {
                tracing::debug!("auth: Closing — TDLib is shutting down");
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
                                    *state.auth_error.lock() = Some(msg.clone());
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
                // SM-M1: log unrecognized auth states so operators can audit TDLib binding changes
                tracing::trace!("handle_auth_state: unrecognized TDLib auth state");
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
        let mut guard = state.code_rx.lock();
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
    // SM-L4: sync-only — any future variant needing get_message would need async refactor
    fn convert_update(update: tdlib_rs::enums::Update) -> Option<TelegramUpdate> {
        match update {
            tdlib_rs::enums::Update::NewMessage(new_msg) => {
                // SM-H2: skip outgoing messages (sent by the bot/user themselves)
                // to prevent echo feedback through the adapter's receive path
                if new_msg.message.is_outgoing {
                    tracing::trace!(
                        chat_id = new_msg.message.chat_id,
                        "convert_update: skipping outgoing message"
                    );
                    return None;
                }

                // M7: map TDLib's `MessageSender` enum to our structured
                // `MessageSender` so the adapter's self-loop filter can do
                // a typed comparison instead of string parsing. TDLib
                // currently only emits `User` and `Chat`; `Hidden` and
                // `Unknown` are reserved for future variants and fall
                // through with an empty legacy string.
                let from = match &new_msg.message.sender_id {
                    tdlib_rs::enums::MessageSender::User(user_id) => {
                        crate::client::MessageSender::User(user_id.user_id)
                    }
                    tdlib_rs::enums::MessageSender::Chat(c) => {
                        crate::client::MessageSender::Chat(c.chat_id)
                    }
                };
                let from_legacy = match &from {
                    crate::client::MessageSender::User(id)
                    | crate::client::MessageSender::Chat(id) => id.to_string(),
                    crate::client::MessageSender::Hidden
                    | crate::client::MessageSender::Unknown => String::new(),
                };
                Some(TelegramUpdate::NewMessage(crate::client::NewMessage {
                    chat_id: new_msg.message.chat_id,
                    message: Self::extract_message_text(&new_msg.message.content),
                    from,
                    from_legacy,
                }))
            }
            tdlib_rs::enums::Update::MessageEdited(edited) => Some(TelegramUpdate::MessageEdited(
                crate::client::MessageEdited {
                    chat_id: edited.chat_id,
                    message_id: edited.message_id.to_string(),
                    // UpdateMessageEdited does NOT carry the new content field.
                    // To get the edited text the caller must call get_message.
                    new_text: String::new(),
                },
            )),
            tdlib_rs::enums::// SM-M3: Update::File only for completed downloads; partial progress dropped
            Update::File(file_update) => {
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
    /// caption is set by `send_envelope` (see `send_envelope`'s doc-comment);
    /// the adapter's `canonicalize` decodes it.
    // SM-L1: returns empty for ~120 service message variants
    fn extract_message_text(content: &tdlib_rs::enums::MessageContent) -> String {
        match content {
            tdlib_rs::enums::MessageContent::MessageText(msg) => msg.text.text.clone(),
            tdlib_rs::enums::MessageContent::MessageDocument(doc) => doc.caption.text.clone(),
            tdlib_rs::enums::MessageContent::MessagePhoto(photo) => photo.caption.text.clone(),
            tdlib_rs::enums::MessageContent::MessageVideo(video) => video.caption.text.clone(),
            tdlib_rs::enums::MessageContent::MessageAudio(audio) => audio.caption.text.clone(),
            tdlib_rs::enums::MessageContent::MessageAnimation(anim) => anim.caption.text.clone(),
            _ => String::new(),
        }
    }
}

#[async_trait]
impl TelegramClient for RealTelegramClient {
    async fn send_message(&self, chat_id: &str, text: &str) -> Result<SentMessage> {
        let chat_id_i64: i64 = crate::client::parse_chat_id(chat_id)
            .map_err(|e| TelegramError::InvalidChatId(format!("{}: {}", e, chat_id)))?;

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
                Ok(SentMessage::new(msg.id.to_string(), i64::from(msg.date)))
            }
            Err(e) => Err(Self::classify_tdlib_error(e)),
        }
    }

    async fn send_envelope(
        &self,
        chat_id: &str,
        encoded_envelope: &str,
        filename: &str,
        data: &[u8],
    ) -> Result<SentMessage> {
        use std::io::Write;

        let chat_id_i64: i64 = crate::client::parse_chat_id(chat_id)
            .map_err(|e| TelegramError::InvalidChatId(format!("{}: {}", e, chat_id)))?;

        // CR-C2: keep NamedTempFile alive until after send_message returns.
        // into_temp_path() would detach from RAII; instead keep tmp alive
        // and reference the path via tmp.path(). The file is cleaned up
        // when tmp drops at the end of this function or on ?/panic.
        let mut tmp = tempfile::NamedTempFile::new().map_err(TelegramError::Io)?;
        tmp.write_all(data).map_err(TelegramError::Io)?;
        let temp_path = tmp.path().to_path_buf();

        let input_file = tdlib_rs::types::InputFileLocal {
            path: temp_path.to_string_lossy().into_owned(),
        };

        // Wire format: caption is the base64-encoded envelope. The receive path
        // returns the caption as `extract_message_text` for MessageDocument, so
        // the adapter can decode it without an extra round-trip.
        let content = tdlib_rs::types::InputMessageDocument {
            document: tdlib_rs::enums::InputFile::Local(input_file),
            thumbnail: None,
            disable_content_type_detection: false,
            caption: Some(tdlib_rs::types::FormattedText {
                text: encoded_envelope.to_string(),
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
                Ok(SentMessage::new(msg.id.to_string(), i64::from(msg.date)))
            }
            Err(e) => Err(Self::classify_tdlib_error(e)),
        }
    }

    async fn send_file(&self, chat_id: &str, filename: &str, data: &[u8]) -> Result<SentMessage> {
        use std::io::Write;

        let chat_id_i64: i64 = crate::client::parse_chat_id(chat_id)
            .map_err(|e| TelegramError::InvalidChatId(format!("{}: {}", e, chat_id)))?;

        // CR-C2: keep NamedTempFile alive until after send_message returns.
        let mut tmp = tempfile::NamedTempFile::new().map_err(TelegramError::Io)?;
        tmp.write_all(data).map_err(TelegramError::Io)?;
        let temp_path = tmp.path().to_path_buf();

        let input_file = tdlib_rs::types::InputFileLocal {
            path: temp_path.to_string_lossy().into_owned(),
        };

        // H6: raw file upload — no caption. The receive path will see a
        // MessageDocument with an empty caption (`extract_message_text`
        // returns ""), and the adapter's `canonicalize` rejects non-envelope
        // payloads (correctly: this is a media upload, not a control message).
        let content = tdlib_rs::types::InputMessageDocument {
            document: tdlib_rs::enums::InputFile::Local(input_file),
            thumbnail: None,
            disable_content_type_detection: false,
            caption: None,
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
                // `_filename` is reserved in case the wire format evolves to embed
                // it alongside the file content. Currently the filename is preserved
                // here for API symmetry with `send_envelope`.
                let _ = filename;
                Ok(SentMessage::new(msg.id.to_string(), i64::from(msg.date)))
            }
            Err(e) => Err(Self::classify_tdlib_error(e)),
        }
    }

    /// Download a file by its TDLib file_id (as a string).
    async fn download_file(&self, file_id_str: &str) -> Result<Vec<u8>> {
        let file_id: i32 = file_id_str
            .parse()
            .map_err(|_| TelegramError::InvalidFileId(file_id_str.into()))?;

        crate::files::download_file_bytes(self.state.client_id, file_id)
            .await
            .map_err(|e| TelegramError::File(e.to_string()))
    }

    async fn receive_updates(&self) -> Result<Vec<TelegramUpdate>> {
        // CONC-C1: drain all available updates from mpsc channel
        // CR-H2: report dropped updates since last drain
        let dropped = self.state.dropped_updates.swap(0, std::sync::atomic::Ordering::Relaxed);
        if dropped > 0 {
            tracing::warn!(dropped, "receive_updates: {} updates were dropped since last poll", dropped);
        }
        // PERF-L1: pre-allocate for typical poll size
        let mut updates = Vec::with_capacity(8);
        let mut guard = self.state.pending_updates_rx.lock();
        // parking_lot::MutexGuard derefs to &Option, use as_deref_mut pattern
        if let Some(ref mut rx) = *guard {
            use mpsc::error::TryRecvError;
            loop {
                match rx.try_recv() {
                    Ok(msg) => updates.push(msg),
                    Err(TryRecvError::Empty) => break,
                    Err(TryRecvError::Disconnected) => break,
                }
            }
        }
        Ok(updates)
    }

    async fn authenticate(&self) -> Result<()> {
        // Bot mode: caller provides the validated `TelegramConfig` (with
        // bot_token + api_id + api_hash) via `RealTelegramClient::new(&config)`.
        // User mode: caller uses `RealTelegramClient::new_user(user_auth)` instead.
        // This method is a no-op for bot mode.
        Ok(())
    }

    /// Resolve a message by chat_id and message_id to its attached file_id.
    /// L1: uses TDLib's get_message to look up the message, then extracts
    /// the first file_id from common file-bearing message content types.
    async fn get_file_id_for_message(&self, chat_id: i64, message_id: i64) -> Result<String> {
        use tdlib_rs::enums::{Message, MessageContent};
        let msg = tdlib_rs::functions::get_message(chat_id, message_id, self.state.client_id)
            .await
            .map_err(Self::classify_tdlib_error)?;
        match msg {
            Message::Message(m) => {
                let file_id = match m.content {
                    MessageContent::MessageDocument(doc) => doc.document.document.id,
                    MessageContent::MessageVideo(video) => video.video.video.id,
                    MessageContent::MessageAudio(audio) => audio.audio.audio.id,
                    MessageContent::MessageAnimation(anim) => anim.animation.animation.id,
                    MessageContent::MessageVoiceNote(vn) => vn.voice_note.voice.id,
                    MessageContent::MessageVideoNote(vn) => vn.video_note.video.id,
                    MessageContent::MessageSticker(sticker) => sticker.sticker.sticker.id,
                    MessageContent::MessagePhoto(photo) => {
                        // Use the largest available photo size.
                        photo
                            .photo
                            .sizes
                            .last()
                            .map(|s| s.photo.id)
                            .ok_or_else(|| TelegramError::File("photo has no sizes".into()))?
                    }
                    _ => {
                        return Err(TelegramError::File(
                            "message content type has no extractable file".into(),
                        ))
                    }
                };
                Ok(file_id.to_string())
            }
        }
    }
}

impl RealTelegramClient {
    /// Map a TDLib error to a structured `TelegramError`, recognizing
    /// 429-equivalent error codes, FLOOD_WAIT_* messages, and transient
    /// (recoverable) errors so the adapter can retry them.
    fn classify_tdlib_error(e: tdlib_rs::types::Error) -> TelegramError {
        // 429 = FLOOD_WAIT_X in TDLib; we expose RateLimited.
        if e.code == 429 {
            let secs = parse_flood_wait_secs(&e.message).unwrap_or(1);
            return TelegramError::RateLimited {
                retry_after_secs: secs,
            };
        }
        // M6: 5xx-equivalent error codes and explicit connection-lost
        // strings are recoverable. `send_with_retry` treats them like
        // `RateLimited` and applies exponential backoff up to
        // `RetryConfig::max_retries`. Everything else falls through to the
        // catch-all `TdlibClient` variant, which surfaces as a fatal error.
        if (e.code >= 500 && e.code < 600)
            || e.message.contains("connection failed")
            || e.message.contains("connection closed")
        {
            return TelegramError::Transient(e.message);
        }
        TelegramError::TdlibClient { code: e.code as u16, message: e.message }
    }
}

/// Parse `FLOOD_WAIT_42` → `Some(42)`. Returns None on non-FLOOD_WAIT errors.
fn parse_flood_wait_secs(message: &str) -> Option<u64> {
    let rest = message.strip_prefix("FLOOD_WAIT_")?;
    rest.parse().ok()
}

/// Drain the latest verification code from a code_rx mutex.
pub fn drain_code_receiver(
    code_rx: &std::sync::Arc<PlMutex<Option<String>>>,
) -> Option<String> {
    code_rx.lock().take()
}

impl Drop for RealTelegramClient {
    /// Signal the receive loop to close the TDLib client and exit.
    ///
    /// C3: this used to spawn a detached `std::thread` with a fresh
    /// `current_thread` runtime to call `tdlib_rs::functions::close`. That
    /// was wrong on three counts: (a) it raced the receive loop — TDLib's
    /// `close` is not safe to call while `tdlib_rs::receive` is mid-call,
    /// (b) the thread could outlive the process and leak, and (c) a
    /// runtime-in-runtime on a non-tdjson thread may not have reached
    /// TDLib at all.
    ///
    /// The new design pushes the `client_id` through a `mpsc::Sender`
    /// field on `ClientState`. The receive loop `tokio::select!`s on the
    /// matching receiver alongside `tdlib_rs::receive`; on receipt it
    /// calls `close` from within the existing tokio runtime and exits.
    /// `Drop` uses `try_send` (non-blocking) and silently no-ops if the
    /// receive loop is already gone — the OS reclaims the TDLib client on
    /// process exit in that case. We do not panic on send failure:
    /// panicking in `Drop` is unsound under multi-threaded drop.
    fn drop(&mut self) {
        // CR-M4: running=false BEFORE shutdown signal so receive loop stops polling
        self.state.running.store(false, Ordering::Release);
        // CR-H1: retry shutdown signal in case the channel was full
        for _ in 0..3 {
            // CR-L3: try_send may fail if channel is full; retry loop handles it
            if self.state.shutdown_tx.try_send(self.state.client_id).is_ok() {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(100));
        }
        // L15: join the blocking thread after signaling close. Short timeout
        // in case the thread is stuck in a long tdlib_rs::receive() call.
        // CONC-M2: scope the mutex guard so it's dropped before thread spawn
        let handle = {
            let mut guard = self.state.receive_thread.lock();
            guard.take()
        };
        if let Some(handle) = handle {
            // OBS-C4 + CR-H1: bounded join with progressive timeouts.
            // The receive loop normally responds to shutdown within ~100ms.
            // First attempt: 2s timeout (normal case).
            // Second attempt: 5s timeout with retried shutdown signal.
            // If both time out, the thread is stuck — log error and detach.
            let (tx, rx) = std::sync::mpsc::channel();
            std::thread::spawn(move || { let _ = tx.send(handle.join()); });
            let timeout_1 = std::time::Duration::from_secs(2);
            let result_1 = rx.recv_timeout(timeout_1);
            match result_1 {
                Ok(Ok(())) => {
                    tracing::debug!("receive thread joined cleanly");
                }
                Ok(Err(panic_err)) => {
                    tracing::error!("receive thread panicked: {:?}", panic_err);
                }
                Err(_) => {
                    // First timeout — retry shutdown signal and wait again.
                    tracing::warn!("receive thread did not join within 2s, retrying shutdown");
                    for _ in 0..3 {
                        // CR-L3: try_send may fail if channel is full; retry loop handles it
            if self.state.shutdown_tx.try_send(self.state.client_id).is_ok() {
                            break;
                        }
                        std::thread::sleep(std::time::Duration::from_millis(200));
                    }
                    let timeout_2 = std::time::Duration::from_secs(5);
                    match rx.recv_timeout(timeout_2) {
                        Ok(Ok(())) => tracing::debug!("receive thread joined on retry"),
                        Ok(Err(panic_err)) => tracing::error!(
                            "receive thread panicked on retry: {:?}", panic_err
                        ),
                        Err(_) => tracing::error!(
                            "receive thread did not join within 2+5s, DETACHING — TDLib client MAY LEAK"
                        ),
                    }
                }
            }
        }
    }
}

