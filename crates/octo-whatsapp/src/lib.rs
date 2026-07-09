//! `octo-whatsapp` - long-lived daemon for the WhatsApp adapter.
//!
//! See `docs/plans/2026-07-04-whatsapp-runtime-cli-mcp-design.md` for the
//! full design. Phase 1 (MVP) covers the daemon + unix socket + JSON-RPC +
//! the 12 method surfaces listed in §Rollout, plus CLI and MCP mirrors.
//!
//! This crate is excluded from the default workspace build. Build via
//! `cargo build -p octo-cli-meta --features whatsapp-cli`.

#![deny(unsafe_code)]
#![warn(clippy::await_holding_lock)]
#![deny(rust_2018_idioms)]
#![warn(missing_debug_implementations)]

pub mod actions;
pub mod adapter_trait;
pub use adapter_trait::OctoWhatsAppAdapter;
pub mod audit;
pub mod cli;
pub mod config;
pub mod daemon;
pub mod events;
pub mod events_buffer;
pub mod events_persister;
pub mod events_query;
pub mod events_router;
pub mod ipc;
pub mod jids;
pub mod limits;
pub mod mcp_server;
pub mod media_buffer;
pub mod observability;
pub mod onboarding;
pub mod rules;
pub mod security;
pub mod triggers;

#[cfg(any(test, feature = "test-helpers"))]
pub mod test_mock_adapter;
