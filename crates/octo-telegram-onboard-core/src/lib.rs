//! `octo-telegram-onboard-core` — library half of `octo-telegram-onboard`.
//!
//! Mission 0850ab-a: authenticate a CipherOcto operator against Telegram
//! via TDLib in two modes (bot-setup, user-login), verify sessions (whoami),
//! and write a JSON config file matching the `TelegramConfig` schema
//! consumed by `octo-adapter-telegram`.
//!
//! Bot mode: direct TDLib calls (no `UserAuth`).
//! User mode: uses adapter's `UserAuth::decide_key` for state decisions.

pub mod auth;
pub mod error;
pub mod keys;
pub mod output;
pub mod session;
