//! octo-adapter-telegram — Telegram platform adapter for CipherOcto DOT (RFC-0850 §8.1).
//!
//! TDLib-backed implementation per mission 0850ab.
//! See `missions/claimed/0850ab-dot-telegram-tdlib-adapter.md` for full spec.
//!
//! Architecture: three layers, each independently testable:
//! 1. `client` — TDLib `Client` wrapper behind `TelegramClient` trait (default = mock)
//! 2. `envelope` — DOT envelope pack/unpack (preserved 0850f wire format)
//! 3. `adapter` — `PlatformAdapter` impl (preserved contract)

#![cfg_attr(docsrs, feature(doc_cfg))]

// Public modules
pub mod adapter;
pub mod auth;
pub mod cleanup;
pub mod client;
pub mod config;
pub mod envelope;
pub mod error;
#[cfg(feature = "real-tdlib")]
pub mod files;
#[cfg(feature = "real-tdlib")]
pub mod groups;
#[cfg(feature = "real-tdlib")]
pub mod real_client;
pub mod self_handle;

// Mock client — a test helper that is always available. Named with the
// `Mock` prefix to make its purpose obvious; production code should not
// depend on this. Feature-gating the mock would break `tests/*.rs`
// integration tests because Cargo does not auto-enable custom features
// when building test binaries.
pub mod mock;
pub use mock::{FailureSpec, MockTelegramClient};

// Re-exports
pub use adapter::TelegramAdapter;
pub use auth::{AuthAction, AuthError, AuthMode, AuthStateKey, BotIdentity, UserAuth};
pub use client::TelegramClient;
pub use config::TelegramConfig;
pub use error::{redact_credentials, TelegramError};
pub use self_handle::{SelfHandle, SelfIdentity};
// FileError is always available; the Tdlib variant is gated to real-tdlib.
pub use error::FileError;
#[cfg(feature = "real-tdlib")]
pub use files::{FileMetadata, FileProgress};
#[cfg(feature = "real-tdlib")]
pub use groups::{ChatInfo, ChatResolver, ChatType, GroupError, MonitoredGroups};
#[cfg(feature = "real-tdlib")]
pub use real_client::{drain_code_receiver, RealTelegramClient};
