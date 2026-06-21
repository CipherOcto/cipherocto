//! octo-adapter-telegram-mtproto — Telegram platform adapter for CipherOcto DOT.
//!
//! Pure-Rust MTProto transport via the `grammers` family of crates
//! (RFC-0850ab-c). Co-exists with `octo-adapter-telegram` (TDLib-based);
//! users select at config time via `octo.telegram.adapter = mtproto |
//! tdlib`. No TDLib, no C/C++ toolchain.
//!
//! ## Architecture
//!
//! Four layers, each independently testable:
//!
//! 1. `session` — StoolapSession: `grammers_session::Session` impl
//!    backed by CipherOcto's stoolap fork on `feat/blockchain-sql`.
//!    Persists `DcOption` (per-DC config + auth_key), `PeerInfo`
//!    (cached peer info), `UpdatesState` (gapless update
//!    catch-up), `ChannelState` (per-channel update state), and
//!    `home_dc_id`.
//! 2. `client` — `TelegramMtprotoClient` trait with two impls:
//!    a pure-Rust mock (always available) and a `grammers_client`-
//!    backed real client (gated behind `--features real-network`).
//!    The trait uses only std types — no grammers types leak
//!    through the boundary — so the `PlatformAdapter` impl is
//!    unit-testable without a real Telegram DC and without the
//!    grammers-client dependency at all.
//! 3. `envelope` — DOT wire-format codec (shared with `octo-network`).
//!    Text-only Telegram transport: payloads are emitted as
//!    `DOT/1/{b64}` (RFC-0850 §3 wire format). Oversize payloads
//!    route to `DOT/2/{msg_id}` via the `upload_media` /
//!    `download_media` methods.
//! 4. `adapter` — `PlatformAdapter` impl that maps between the
//!    `TelegramMtprotoClient` trait and the DOT contract.

#![cfg_attr(docsrs, feature(doc_cfg))]

// Public modules
pub mod adapter;
pub mod auth;
pub mod client;
pub mod config;
pub mod coordinator_admin;
pub mod envelope;
pub mod error;
pub mod lifecycle;
pub mod self_handle;
pub mod session;
// `transport` is unconditional so `MtprotoTelegramConfig` can
// reference the `Transport` enum from the default build. The
// `BotApiClient` and method implementations that actually use
// the `BotApiHttp` variant live in `http_fallback` (gated).
pub mod transport;

#[cfg(feature = "real-network")]
pub mod real_client;

// Bot-API HTTP fallback transport (Phase 3 / sub-mission
// 0850ab-c-http). Gated on the `bot-api` feature so the
// default build (pure mock + MTProto) does not pull in
// reqwest / rustls. The `Transport` enum lives in the
// unconditional `transport` module; the typed response
// structs (`BotMessage`, `BotUpdate`, `BotUser`) and the
// `BotApiClient` live here.
#[cfg(feature = "bot-api")]
pub mod http_fallback;

// Re-exports
pub use adapter::MtprotoTelegramAdapter;
pub use auth::{
    next_user_auth_state, next_user_auth_state_server, AuthMode, AuthStateKey, BotIdentity,
    MtprotoAuthAction, MtprotoAuthError, UserAuth, UserAuthAction, UserAuthServerEvent,
};
pub use client::{
    GroupInfo, MtprotoSentMessage, MtprotoTelegramClient, MtprotoTelegramUpdate, QrLoginHandle,
    SelfUserInfo,
};
pub use config::MtprotoTelegramConfig;
pub use envelope::wire_encode;
pub use error::{redact_credentials, MtprotoTelegramError};
pub use lifecycle::{AdapterLifecycle, BotAuthLifecycle, UserAuthLifecycle};
pub use self_handle::{MtprotoSelfHandle, MtprotoSelfIdentity};
pub use session::StoolapSession;
pub use transport::Transport;

#[cfg(feature = "real-network")]
pub use real_client::RealTelegramMtprotoClient;

// Phase 3 (sub-mission 0850ab-c-http): Bot-API HTTP fallback
// transport. Gated on the `bot-api` feature. The `Transport`
// enum is re-exported unconditionally above; the
// `BotApiClient` and the typed Bot API response structs
// (`BotMessage`, `BotUpdate`, `BotUser`, etc.) are re-exported
// here for the adapter's `connect_with_transport` signature
// and for the example binary.
#[cfg(feature = "bot-api")]
pub use http_fallback::{
    BotApiClient, BotApiConfig, BotApiErrorParameters, BotChat, BotDocument, BotMessage, BotUpdate,
    BotUser, DEFAULT_BOT_API_BASE_URL, MAX_LONG_POLL_SECS, MAX_MESSAGE_CHARS, MAX_UPLOAD_BYTES,
};
