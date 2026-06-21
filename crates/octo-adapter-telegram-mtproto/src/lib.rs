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
pub mod envelope;
pub mod error;
pub mod lifecycle;
pub mod self_handle;
pub mod session;

#[cfg(feature = "real-network")]
pub mod real_client;

// Re-exports
pub use adapter::MtprotoTelegramAdapter;
pub use auth::{AuthStateKey, BotIdentity, MtprotoAuthAction, MtprotoAuthError, UserAuth};
pub use client::{
    MtprotoSentMessage, MtprotoTelegramClient, MtprotoTelegramUpdate, SelfUserInfo,
};
pub use config::MtprotoTelegramConfig;
pub use envelope::wire_encode;
pub use error::{redact_credentials, MtprotoTelegramError};
pub use lifecycle::AdapterLifecycle;
pub use self_handle::{MtprotoSelfHandle, MtprotoSelfIdentity};
pub use session::StoolapSession;

#[cfg(feature = "real-network")]
pub use real_client::RealTelegramMtprotoClient;
