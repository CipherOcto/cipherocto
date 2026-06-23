//! `octo-telegram-mtproto-onboard-core` — library half of
//! `octo-telegram-mtproto-onboard`.
//!
//! Mission 0850ab-c (Phase B): authenticate a CipherOcto operator
//! against Telegram via the **pure-Rust** MTProto adapter
//! (`octo-adapter-telegram-mtproto`, grammers-based) in three
//! modes:
//!
//! * `bot_token`   — direct bot token, no interactive prompts.
//! * `user_code`   — phone + SMS code (+ optional 2FA password).
//! * `qr_login`    — QR login: operator scans a `tg://login?token=...`
//!   link from another already-logged-in device. No phone, no SMS.
//!
//! All three modes drive a `MtprotoTelegramAdapter` to completion,
//! verify the resulting `MtprotoSelfHandle`, and (on success) write a
//! JSON config file matching the `MtprotoTelegramConfig` schema
//! consumed by the adapter on subsequent boots.
//!
//! ## Production wiring
//!
//! Production callers obtain an adapter via [`connect::connect`],
//! which delegates to `octo-adapter-telegram-mtproto::factory::connect_real`
//! — the adapter is wired to a real `RealTelegramMtprotoClient`
//! that drives an actual Telegram connection. The mock client
//! (`MockTelegramMtprotoClient`) is reserved for unit tests and
//! is not reachable from production code paths.

pub mod auth;
pub mod bot_token;
#[cfg(feature = "real-network")]
pub mod connect;
pub mod error;
pub mod output;
pub mod qr_link;
pub mod qr_login;
pub mod session;
pub mod user_code;

#[cfg(test)]
pub(crate) mod test_helpers;

pub use auth::auth_state_name;
pub use error::OnboardError;
pub use output::OnboardOutput;
pub use session::SessionRecord;

#[cfg(feature = "real-network")]
pub use connect::{connect as connect_adapter, RealMtprotoTelegramAdapter};
