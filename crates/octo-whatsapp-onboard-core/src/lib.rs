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
pub mod multi_account;
pub mod output;
pub mod pair_link;
pub mod qr_link;
pub mod session;
pub mod sidecar;
pub mod time;
pub mod validate;

pub use error::{CoreError, Result};
pub use multi_account::{AccountEntry, MultiAccountStore};
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

/// Shared validation for session link args (parent dir creation +
/// symlink-attack check). The adapter's `WhatsAppConfig::validate()`
/// handles field-shape checks (ws_url format, groups non-empty,
/// pair_phone E.164).
///
/// Mission 0850p-a-symlink-check: also rejects session paths that
/// resolve to a symlink whose target is outside the user-requested
/// parent (D-WA-4 mitigation).
pub fn validate_session_args(session_path: &std::path::Path) -> Result<()> {
    if let Some(parent) = session_path.parent() {
        if !parent.as_os_str().is_empty() && !parent.exists() {
            std::fs::create_dir_all(parent).map_err(|e| CoreError::InvalidSessionPath {
                path: parent.to_path_buf(),
                reason: format!("cannot create parent directory: {e}"),
            })?;
        }
    }
    // Mission 0850p-a-symlink-check: detect symlink-attack attempts
    // before any session DB is opened. The check is a no-op for
    // paths that do not exist yet (fresh link).
    crate::validate::check_session_path_safe(session_path)?;
    Ok(())
}
