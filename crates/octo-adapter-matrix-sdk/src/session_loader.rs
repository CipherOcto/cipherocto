//! Session loader for the Matrix adapter (mission 0850h-d).
//!
//! Decides whether to load a session from the multi-account stoolap
//! store (`octo-session-store`) or from the legacy single-file JSON
//! config (missions 0850h-a / 0850h-c), based on
//! `MatrixConfig.use_session_store` and `MatrixConfig.session_store_path`.
//!
//! ## Behavior
//!
//! - `use_session_store = true` (the default after this mission
//!   lands) AND `session_store_path` is non-empty: open the store at
//!   that path and look up the session by `(user_id, device_id)`.
//! - `use_session_store = true` AND `session_store_path` is empty:
//!   open the store at the per-platform default location
//!   (`$XDG_DATA_HOME/cipherocto/sessions.db` on Linux).
//! - `use_session_store = false`: read the legacy JSON file at
//!   `config_path` (the 0850h-a / 0850h-c path). When `config_path`
//!   is also empty, return `LoadError::NoSource` (an in-process
//!   config with no backing store is an unrecoverable configuration
//!   error).
//!
//! The loader does NOT itself create a `Client`; it returns a
//! `SessionRow` (or `OnDiskConfig`) and the caller (typically
//! `MatrixAdapter::new`) does the SDK wiring. Keeping the loader
//! pure-data makes it easy to test.

use crate::config_writer::OnDiskConfig;
use crate::MatrixConfig;
use octo_session_store::{default_store_path, SessionRow, SessionStore, StoolapSessionStore};
use std::path::PathBuf;
use thiserror::Error;

/// Error surfaced by `load()`. Wraps the underlying store / file I/O
/// errors and adds a few loader-specific cases (no source, both
/// sources configured, session not in store).
#[derive(Debug, Error)]
pub enum LoadError {
    /// Neither the store nor the file is configured. The host
    /// process should set one of them before constructing the
    /// adapter.
    #[error("no session source configured: set MatrixConfig.use_session_store=true with session_store_path, or use_session_store=false with config_path")]
    NoSource,
    /// `use_session_store = true` and `session_store_path` is empty:
    /// resolved to the per-platform default, but the default
    /// location could not be derived (no `$XDG_DATA_HOME` on
    /// unusual platforms).
    #[error("could not resolve default session store path")]
    NoDefaultPath,
    /// The store is configured but the `(user_id, device_id)` row is
    /// not present.
    #[error("session not found in store: user_id={user_id}, device_id={device_id}")]
    NotInStore { user_id: String, device_id: String },
    /// I/O error from the store.
    #[error("store error: {0}")]
    Store(String),
    /// I/O error from the legacy file path.
    #[error("config file error: {0}")]
    File(String),
}

/// A loaded session, agnostic of source. The caller passes
/// `(user_id, device_id, access_token, refresh_token, homeserver_url)`
/// to `Client::restore_session` to wire the SDK.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LoadedSession {
    pub user_id: String,
    pub device_id: String,
    pub homeserver_url: String,
    pub access_token: String,
    pub refresh_token: Option<String>,
}

/// Load a session according to `MatrixConfig`.
///
/// The `config` argument carries the host process's intent: which
/// source to use, where the store lives, which `(user_id, device_id)`
/// to look up. The `user_id` / `device_id` are taken from the
/// `MatrixConfig` itself (so the host only needs to populate
/// `user_id` + `device_id` once for both store and file sources).
pub fn load(config: &MatrixConfig) -> Result<LoadedSession, LoadError> {
    if config.use_session_store {
        load_from_store(config)
    } else {
        load_from_file(config)
    }
}

fn load_from_store(config: &MatrixConfig) -> Result<LoadedSession, LoadError> {
    let store_path: PathBuf = if config.session_store_path.as_os_str().is_empty() {
        default_store_path()
    } else {
        config.session_store_path.clone()
    };
    if store_path.as_os_str().is_empty() {
        return Err(LoadError::NoDefaultPath);
    }
    let store =
        StoolapSessionStore::new(&store_path).map_err(|e| LoadError::Store(e.to_string()))?;
    // Use the runtime to drive the async trait method. The adapter
    // holds a tokio runtime via `MatrixAdapter::new`; for the loader
    // we block on a fresh runtime when called outside one (e.g.,
    // from `MatrixAdapter::from_config_bytes`).
    let session: SessionRow =
        futures_block_on(async { store.get_session(&config.user_id, &config.device_id).await })
            .map_err(|e| LoadError::Store(e.to_string()))?
            .ok_or_else(|| LoadError::NotInStore {
                user_id: config.user_id.clone(),
                device_id: config.device_id.clone(),
            })?;
    Ok(LoadedSession {
        user_id: session.user_id,
        device_id: session.device_id,
        homeserver_url: session.homeserver_url,
        access_token: session.access_token,
        refresh_token: session.refresh_token,
    })
}

fn load_from_file(config: &MatrixConfig) -> Result<LoadedSession, LoadError> {
    if config.config_path.as_os_str().is_empty() {
        // No file path. Fall back to the in-process config's own
        // fields (this is the in-memory / cdylib-loaded mode used by
        // the existing tests and by hosts that build a MatrixConfig
        // from a config blob without persisting it).
        if config.access_token.is_empty() {
            return Err(LoadError::NoSource);
        }
        return Ok(LoadedSession {
            user_id: config.user_id.clone(),
            device_id: config.device_id.clone(),
            homeserver_url: config.homeserver_url.clone(),
            access_token: config.access_token.clone(),
            refresh_token: config.refresh_token.clone(),
        });
    }
    let bytes = std::fs::read(&config.config_path).map_err(|e| LoadError::File(e.to_string()))?;
    let on_disk: OnDiskConfig =
        serde_json::from_slice(&bytes).map_err(|e| LoadError::File(e.to_string()))?;
    Ok(LoadedSession {
        user_id: on_disk.user_id,
        device_id: on_disk.device_id,
        homeserver_url: on_disk.homeserver_url,
        access_token: on_disk.access_token,
        refresh_token: on_disk.refresh_token,
    })
}

/// Run a future to completion on a fresh single-threaded tokio
/// runtime. Used by the synchronous `load` entry point when the
/// caller doesn't already have a runtime (the `MatrixAdapter::new`
/// path runs inside an existing runtime; the `from_config_bytes`
/// path doesn't).
fn futures_block_on<F: std::future::Future>(future: F) -> F::Output {
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("build current-thread tokio runtime for session loader");
    rt.block_on(future)
}

#[cfg(test)]
mod tests {
    use super::*;
    use octo_session_store::{LoginType, StoolapSessionStore};
    use std::io::Write;
    use std::os::unix::fs::OpenOptionsExt;

    fn tmpdir() -> PathBuf {
        let p = std::env::temp_dir().join(format!(
            "octo-matrix-loader-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&p).unwrap();
        p
    }

    fn write_file(path: &std::path::Path, contents: &str) {
        let mut f = std::fs::OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .mode(0o600)
            .open(path)
            .unwrap();
        f.write_all(contents.as_bytes()).unwrap();
    }

    #[test]
    fn load_from_store_returns_session_row() {
        let dir = tmpdir();
        let store_path = dir.join("sessions.db");
        let store = StoolapSessionStore::new(&store_path).unwrap();
        let row = SessionRow {
            user_id: "@bot:matrix.example.com".to_string(),
            device_id: "ABCDEFGHIJ".to_string(),
            homeserver_url: "https://matrix.example.com".to_string(),
            access_token: "syt_xxx".to_string(),
            refresh_token: Some("syr_xxx".to_string()),
            login_type: LoginType::Password,
            login_timestamp: 0,
            last_used: 0,
            position: 0,
            display_name: None,
            avatar_url: None,
        };
        futures_block_on(async { store.add_session(&row, false).await }).unwrap();

        let cfg = MatrixConfig {
            homeserver_url: "https://matrix.example.com".to_string(),
            user_id: "@bot:matrix.example.com".to_string(),
            device_id: "ABCDEFGHIJ".to_string(),
            access_token: String::new(),
            refresh_token: None,
            passphrase: None,
            config_path: PathBuf::new(),
            force_writeback: false,
            use_session_store: true,
            session_store_path: store_path,
            rooms: vec![],
        };
        let loaded = load(&cfg).expect("load from store");
        assert_eq!(loaded.user_id, "@bot:matrix.example.com");
        assert_eq!(loaded.access_token, "syt_xxx");
        assert_eq!(loaded.refresh_token.as_deref(), Some("syr_xxx"));
    }

    #[test]
    fn load_from_store_returns_not_in_store_for_missing_row() {
        let dir = tmpdir();
        let store_path = dir.join("sessions.db");
        // Just open + init the store; don't add any rows.
        let _ = StoolapSessionStore::new(&store_path).unwrap();
        let cfg = MatrixConfig {
            homeserver_url: "https://matrix.example.com".to_string(),
            user_id: "@missing:matrix.example.com".to_string(),
            device_id: "ZZZZZZZZZZ".to_string(),
            access_token: String::new(),
            refresh_token: None,
            passphrase: None,
            config_path: PathBuf::new(),
            force_writeback: false,
            use_session_store: true,
            session_store_path: store_path,
            rooms: vec![],
        };
        let err = load(&cfg).unwrap_err();
        assert!(matches!(err, LoadError::NotInStore { .. }));
    }

    #[test]
    fn load_from_file_returns_on_disk_config() {
        let dir = tmpdir();
        let cfg_path = dir.join("config.json");
        write_file(
            &cfg_path,
            r#"{
                "homeserver_url": "https://matrix.example.com",
                "user_id": "@bot:matrix.example.com",
                "device_id": "ABCDEFGHIJ",
                "access_token": "syt_legacy",
                "refresh_token": "syr_legacy",
                "rooms": ["!abc:matrix.example.com"]
            }"#,
        );
        let cfg = MatrixConfig {
            homeserver_url: "https://matrix.example.com".to_string(),
            user_id: "@bot:matrix.example.com".to_string(),
            device_id: "ABCDEFGHIJ".to_string(),
            access_token: String::new(),
            refresh_token: None,
            passphrase: None,
            config_path: cfg_path,
            force_writeback: false,
            use_session_store: false,
            session_store_path: PathBuf::new(),
            rooms: vec![],
        };
        let loaded = load(&cfg).expect("load from file");
        assert_eq!(loaded.user_id, "@bot:matrix.example.com");
        assert_eq!(loaded.access_token, "syt_legacy");
        assert_eq!(loaded.refresh_token.as_deref(), Some("syr_legacy"));
    }

    #[test]
    fn load_with_no_source_returns_no_source() {
        let cfg = MatrixConfig {
            homeserver_url: "https://matrix.example.com".to_string(),
            user_id: "@bot:matrix.example.com".to_string(),
            device_id: "ABCDEFGHIJ".to_string(),
            access_token: String::new(),
            refresh_token: None,
            passphrase: None,
            config_path: PathBuf::new(),
            force_writeback: false,
            use_session_store: false,
            session_store_path: PathBuf::new(),
            rooms: vec![],
        };
        let err = load(&cfg).unwrap_err();
        assert!(matches!(err, LoadError::NoSource));
    }
}
