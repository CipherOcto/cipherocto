//! Real `grammers_client::Client`-backed impl of
//! `MtprotoTelegramClient`. Gated behind `--features
//! real-network`.
//!
//! ## Status: Phase 1 stub
//!
//! The `connect` path wires up the `SenderPool` and a real
//! `grammers_client::Client`, but the per-RPC implementations
//! (`send_message`, `send_document`, etc.) are stubbed out
//! because Phase 1 is bot-mode auth + basic adapter plumbing
//! only. Peer resolution (which needs an `InputPeer` carrying
//! `access_hash`) and the user-mode auth flow are Phase 2
//! work tracked under sub-mission `0850ab-c-user`.
//!
//! ## Storage
//!
//! The `StoolapSession` is the canonical session store. The
//! `SenderPool` reads/writes via the `grammers_session::Session`
//! trait which our `StoolapSession` impls. The
//! `RealTelegramMtprotoClient` additionally holds a typed
//! `Arc<StoolapSession>` so `sign_out` can call
//! `StoolapSession::reset()` to wipe the on-disk store.

#![cfg(feature = "real-network")]

use std::sync::Arc;

use async_trait::async_trait;
use grammers_client::sender::SenderPool;
use grammers_client::Client as GrammersClient;
use tokio::task::JoinHandle;
use tracing::{error, warn};

use crate::client::{
    MtprotoSentMessage, MtprotoTelegramClient, MtprotoTelegramUpdate, SelfUserInfo,
};
use crate::error::MtprotoTelegramError;
use crate::self_handle::MtprotoSelfHandle;
use crate::session::StoolapSession;

/// Wrapper around `grammers_client::Client` that implements
/// `MtprotoTelegramClient`. Constructed via
/// `RealTelegramMtprotoClient::connect`.
pub struct RealTelegramMtprotoClient {
    #[allow(dead_code)]
    client: Arc<GrammersClient>,
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
        let SenderPool { runner, handle: _handle, .. } =
            SenderPool::new(session.clone(), api_id);
        let client = Arc::new(GrammersClient::new(_handle));
        let runner_task = tokio::spawn(runner.run());
        Ok(Arc::new(Self {
            client,
            runner: parking_lot::Mutex::new(Some(runner_task)),
            session,
            self_handle,
        }))
    }

    /// Read-only accessor for the underlying grammers
    /// client. Used by the `MtprotoTelegramAdapter` when it
    /// needs access to RPCs that are not modelled on the
    /// `MtprotoTelegramClient` trait (e.g., `iter_dialogs`
    /// for group discovery).
    #[allow(dead_code)]
    pub fn grammers_client(&self) -> &GrammersClient {
        &self.client
    }
}

#[async_trait]
impl MtprotoTelegramClient for RealTelegramMtprotoClient {
    async fn send_message(
        &self,
        _chat_id: i64,
        _text: &str,
    ) -> Result<MtprotoSentMessage, MtprotoTelegramError> {
        // Phase 1 stub. Real impl needs to resolve the chat to
        // an `InputPeer` (carrying `access_hash`) before calling
        // `Client::send_message`.
        Err(MtprotoTelegramError::NotReady(
            "RealTelegramMtprotoClient::send_message: peer resolution not yet implemented (Phase 1 stub)".into(),
        ))
    }

    async fn send_document(
        &self,
        _chat_id: i64,
        _caption: &str,
        _filename: &str,
        _data: &[u8],
    ) -> Result<MtprotoSentMessage, MtprotoTelegramError> {
        Err(MtprotoTelegramError::NotReady(
            "RealTelegramMtprotoClient::send_document: peer resolution not yet implemented (Phase 1 stub)".into(),
        ))
    }

    async fn download_file(
        &self,
        _file_id: &str,
    ) -> Result<Vec<u8>, MtprotoTelegramError> {
        Err(MtprotoTelegramError::NotReady(
            "RealTelegramMtprotoClient::download_file: not yet implemented".into(),
        ))
    }

    async fn receive_updates(
        &self,
    ) -> Result<Vec<MtprotoTelegramUpdate>, MtprotoTelegramError> {
        // Phase 1 stub: real impl drains the SenderPool's
        // update channel and converts via `convert_update`.
        Ok(Vec::new())
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
                self.self_handle.set_identity(info.user_id, info.username.clone());
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
        _api_hash: &str,
        _phone: &str,
    ) -> Result<(), MtprotoTelegramError> {
        // Phase 1: bot mode only. The user-mode flow is
        // Phase 2 (sub-mission 0850ab-c-user).
        Err(MtprotoTelegramError::NotReady(
            "user-mode auth (request_login_code) is Phase 2 — not implemented in Phase 1".into(),
        ))
    }

    async fn submit_code(
        &self,
        _code: &str,
    ) -> Result<SelfUserInfo, MtprotoTelegramError> {
        Err(MtprotoTelegramError::NotReady(
            "user-mode auth (submit_code) is Phase 2 — not implemented in Phase 1".into(),
        ))
    }

    async fn submit_password(
        &self,
        _password: &str,
    ) -> Result<SelfUserInfo, MtprotoTelegramError> {
        Err(MtprotoTelegramError::NotReady(
            "user-mode auth (submit_password) is Phase 2 — not implemented in Phase 1".into(),
        ))
    }

    async fn sign_out(&self) -> Result<(), MtprotoTelegramError> {
        // 1. Call Telegram's auth.logOut to invalidate the
        // server-side session.
        if let Err(e) = self.client.sign_out().await {
            warn!(error = %e, "auth.logOut RPC failed; continuing to wipe local state");
        }
        // 2. Wipe the local session store (DD6:
        // mtproto_dc_option rows including auth_key;
        // mtproto_peer_info including self_user).
        if let Err(e) = self.session.reset() {
            error!(error = %e, "StoolapSession::reset failed; signing out left on-disk artifacts");
            return Err(MtprotoTelegramError::Session(format!(
                "session reset: {}",
                e
            )));
        }
        // 3. Clear the cached self-handle.
        self.self_handle.clear();
        Ok(())
    }

    async fn get_file_id_for_message(
        &self,
        _chat_id: i64,
        _message_id: i64,
    ) -> Result<String, MtprotoTelegramError> {
        // Real impl: use grammers' get_messages_by_id to
        // resolve the message, then return its
        // document.file_id. Stubbed in Phase 1.
        Err(MtprotoTelegramError::NotReady(
            "get_file_id_for_message: not yet implemented (Phase 1 stub)".into(),
        ))
    }
}