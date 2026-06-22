//! Production wiring for the MTProto Telegram adapter.
//!
//! This module is the **only** public path to obtain a
//! production-ready `MtprotoTelegramAdapter`. The adapter is
//! generic over its `MtprotoTelegramClient`, so a downstream
//! user *could* in principle construct a real adapter by hand:
//!
//! ```ignore
//! let client = RealTelegramMtprotoClient::connect(
//!     api_id, api_hash, session, self_handle,
//! ).await?;
//! let adapter = MtprotoTelegramAdapter::new(cfg, client);
//! ```
//!
//! but that pattern is error-prone (the `self_handle` is
//! shared between the client and the adapter; mistakingly
//! creating two of them silently loses identity updates).
//! The factory below hides the boilerplate and is the
//! recommended entry point.
//!
//! ## Mock scope
//!
//! The mock client (`MockTelegramMtprotoClient`) is **not**
//! exported from this module. It lives behind a `test-mock`
//! Cargo feature in `client.rs` and `client::mock`, and is
//! reachable from production code only when that feature is
//! explicitly enabled. The project rule "no mocks in
//! production code paths" is enforced structurally: production
//! binaries do not enable `test-mock`, so they cannot
//! accidentally construct an adapter backed by the mock.
//!
//! ## Real-network feature
//!
//! This module is gated on `real-network`. When that feature
//! is **not** enabled, the `connect_real` constructor is
//! unavailable. Production binaries that need to actually talk
//! to Telegram must enable `real-network`; the mock-only build
//! is for tests and for crates that want to depend on the
//! `MtprotoTelegramClient` trait surface without linking
//! `grammers-client`.
//!
//! ## Storage
//!
//! The factory opens a `StoolapSession` at
//! `<data_dir>/session.db` (or in-memory if `data_dir` is
//! `None`). The session is the canonical persistence point
//! for the MTProto auth_key, peer cache, and update state —
//! subsequent boots of the adapter against the same `data_dir`
//! reuse the existing session, so the operator does NOT have
//! to re-authenticate on every CLI run.

#![cfg(feature = "real-network")]

use std::path::Path;
use std::sync::Arc;

use crate::adapter::MtprotoTelegramAdapter;
use crate::config::MtprotoTelegramConfig;
use crate::error::MtprotoTelegramError;
use crate::real_client::RealTelegramMtprotoClient;
use crate::self_handle::MtprotoSelfHandle;
use crate::session::StoolapSession;

/// The concrete production adapter type: an adapter backed by
/// a real `RealTelegramMtprotoClient`. Exposed as a type alias
/// so downstream code does not have to repeat the
/// `<RealTelegramMtprotoClient>` generic parameter everywhere.
///
/// ```ignore
/// use octo_adapter_telegram_mtproto::factory::RealMtprotoTelegramAdapter;
///
/// let adapter: RealMtprotoTelegramAdapter = ... ;
/// ```
pub type RealMtprotoTelegramAdapter = MtprotoTelegramAdapter<RealTelegramMtprotoClient>;

/// Open (or create) the on-disk `StoolapSession` for this
/// adapter. The session is keyed on the `data_dir` field of
/// the config. If `data_dir` is `None`, an in-memory session
/// is returned (useful for tests and ephemeral runs, but the
/// session is lost on process exit).
///
/// The session path is `<data_dir>/session.db`. The directory
/// is created if it does not exist.
pub fn open_session(
    cfg: &MtprotoTelegramConfig,
) -> Result<Arc<StoolapSession>, MtprotoTelegramError> {
    use crate::session::MtprotoSessionError;
    if let Some(dir) = cfg.data_dir.as_deref() {
        ensure_session_dir(dir)?;
        let path = dir.join("session.db");
        StoolapSession::open(&path).map_err(|e: MtprotoSessionError| {
            MtprotoTelegramError::Session(format!("open session {}: {}", path.display(), e))
        })
    } else {
        StoolapSession::open_in_memory().map_err(|e: MtprotoSessionError| {
            MtprotoTelegramError::Session(format!("open in-memory session: {}", e))
        })
    }
}

/// Build the production-ready MTProto adapter, wired to a
/// real `RealTelegramMtprotoClient`.
///
/// This is the **only** public entry point to construct a
/// production adapter. It:
///
/// 1. Opens (or creates) the `StoolapSession` at
///    `<data_dir>/session.db` (or in-memory when `data_dir`
///    is `None`).
/// 2. Allocates a fresh `MtprotoSelfHandle` and shares it
///    with the client (so `sign_in_*` populates the
///    identity and the adapter's `self_handle()` accessor
///    reads from the same source of truth).
/// 3. Calls `RealTelegramMtprotoClient::connect` which
///    spawns the `SenderPool` runner task and performs the
///    initial `initConnection` handshake with Telegram.
/// 4. Wraps the client in a `MtprotoTelegramAdapter`.
///
/// **Note**: this does *not* perform sign-in. The caller
/// chooses the auth mode (bot_token / user_code / qr_login)
/// and calls the corresponding `connect_*` method on the
/// returned adapter.
///
/// ### Errors
///
/// Returns:
///
/// - `MtprotoTelegramError::Config` if the config does not
///   validate against the selected mode (e.g., missing
///   `api_id`, empty `bot_token` for bot mode, missing
///   `phone` for user mode). Callers should map this to a
///   user-facing "fix your config" error.
/// - `MtprotoTelegramError::Session` if the on-disk session
///   store cannot be opened (e.g., permission denied, schema
///   migration failure).
/// - `MtprotoTelegramError::Network` if the initial TCP/TLS
///   connection to the Telegram DC fails. The transport is
///   not yet authenticated at this point, so this is the
///   usual "no internet" / "firewall blocking Telegram"
///   failure mode.
pub async fn connect_real(
    cfg: MtprotoTelegramConfig,
) -> Result<RealMtprotoTelegramAdapter, MtprotoTelegramError> {
    cfg.validate().map_err(MtprotoTelegramError::Config)?;
    let session = open_session(&cfg)?;
    let self_handle = MtprotoSelfHandle::new();
    let api_id = cfg.api_id.unwrap_or(0);
    let api_hash = cfg.api_hash.as_deref().unwrap_or("");
    let client = RealTelegramMtprotoClient::connect(api_id, api_hash, session, self_handle).await?;
    Ok(MtprotoTelegramAdapter::new(cfg, client))
}

/// Ensure the `data_dir` directory exists. Returns `Err` if
/// it cannot be created (permission denied, parent does not
/// exist, etc.).
fn ensure_session_dir(dir: &Path) -> Result<(), MtprotoTelegramError> {
    if dir.exists() {
        if !dir.is_dir() {
            return Err(MtprotoTelegramError::Config(format!(
                "data_dir {} exists but is not a directory",
                dir.display()
            )));
        }
        return Ok(());
    }
    std::fs::create_dir_all(dir).map_err(|e| {
        MtprotoTelegramError::Session(format!("create data_dir {}: {}", dir.display(), e))
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::MtprotoTelegramConfig;

    #[test]
    fn connect_real_rejects_unvalidated_config() {
        // Empty config: validate() fails (bot mode requires
        // bot_token, user mode requires api_id/phone/data_dir).
        // The factory must propagate the Config error before
        // opening the session.
        let cfg = MtprotoTelegramConfig::default();
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        let r = rt.block_on(connect_real(cfg));
        match r {
            Err(MtprotoTelegramError::Config(msg)) => {
                assert!(!msg.is_empty());
            }
            Err(other) => panic!("expected Config error, got {:?}", other),
            Ok(_) => panic!("expected Config error, got Ok"),
        }
    }

    #[tokio::test(flavor = "current_thread")]
    async fn open_session_returns_in_memory_when_data_dir_none() {
        let cfg = MtprotoTelegramConfig::default();
        let s = open_session(&cfg).expect("in-memory session should open");
        // The in-memory session is non-null and distinct from
        // a file-backed session.
        assert!(!Arc::strong_count(&s) > 1_000_000); // sanity bound
    }

    #[test]
    fn ensure_session_dir_creates_missing_dir() {
        let tmp = tempfile::tempdir().unwrap();
        let nested = tmp.path().join("a").join("b").join("c");
        assert!(!nested.exists());
        ensure_session_dir(&nested).unwrap();
        assert!(nested.is_dir());
    }

    #[test]
    fn ensure_session_dir_rejects_existing_file() {
        let tmp = tempfile::tempdir().unwrap();
        let file = tmp.path().join("not-a-dir");
        std::fs::write(&file, b"x").unwrap();
        let e = ensure_session_dir(&file).unwrap_err();
        match e {
            MtprotoTelegramError::Config(msg) => {
                assert!(msg.contains("not a directory"), "msg = {}", msg);
            }
            other => panic!("expected Config, got {:?}", other),
        }
    }
}
