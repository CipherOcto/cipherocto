//! `octo-whatsapp-onboard-core` — library half of `octo-whatsapp-onboard`.
//!
//! Mission 0850p-a: authenticate a CipherOcto operator against
//! WhatsApp Web via the `whatsapp-rust` protocol crate in two modes
//! (`qr-link`, `pair-link`), and write a JSON config file matching
//! the `WhatsAppConfig` schema consumed by `octo-adapter-whatsapp`.
//!
//! The binary crate (`octo-whatsapp-onboard`) imports this lib to
//! drive the actual flows; the integration test also imports it
//! directly so it can call the same auth code without spawning a
//! subprocess.

pub mod error;
pub mod output;
pub mod pair_link;
pub mod qr_link;
pub mod session;
pub mod sidecar;
pub mod time;

pub use error::{CoreError, Result};
pub use output::{PairLinkArgs, QrLinkArgs, SessionInfo, WhatsAppSession};
pub use sidecar::SidecarMode;

// R6-H2: also expose `wait_for_health` (R7-H1 reuses the
// `POLL_INTERVAL_MS` and `POST_CONNECT_GRACE_MS` constants from
// `session`).
pub use session::{
    wait_for_connected, wait_for_health, POLL_INTERVAL_MS, POST_CONNECT_GRACE_MS,
    SESSION_LIST_HEALTH_TIMEOUT_SECS, WHOAMI_TIMEOUT_SECS,
};

/// Re-export the adapter types for downstream consumers (the
/// binary's `cli.rs` and integration tests).
pub use octo_adapter_whatsapp::{WhatsAppConfig, WhatsAppWebAdapter};
