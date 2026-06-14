//! `octo-telegram-onboard-core` — library half of `octo-telegram-onboard`.
//!
//! Mission 0850ab-a: authenticate a CipherOcto operator against Telegram
//! via TDLib in three modes (bot-setup, qr-link, user-login), verify
//! sessions (whoami), and write a JSON config file matching the
//! `TelegramConfig` schema consumed by `octo-adapter-telegram`.
//!
//! Bot mode: direct TDLib calls (no `UserAuth`).
//! User mode: uses adapter's `UserAuth::decide_key` for state decisions.
//! QR mode: TDLib generates a `tg://login?token=...` link, the caller
//!   renders it as a QR code and the user scans it from another
//!   already-logged-in device (modern Telegram login UX, no phone+code).

pub mod auth;
pub mod error;
pub mod keys;
pub mod output;
pub mod qr_link;
pub mod session;

pub use auth::drive_qr_auth;
