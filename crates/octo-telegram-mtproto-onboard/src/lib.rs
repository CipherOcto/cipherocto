//! `octo-telegram-mtproto-onboard` — CLI binary library half.
//!
//! Mission 0850ab-c Phase B. See RFC-0850ab-c for the full
//! specification. The actual CLI entry point lives in
//! `main.rs`; this file re-exports the modules so they can be
//! unit-tested in isolation and (eventually) reused by the
//! `octo-cli-meta` meta-crate.

pub mod cli;
pub mod error;
pub mod logging;
pub mod stdin_io;
