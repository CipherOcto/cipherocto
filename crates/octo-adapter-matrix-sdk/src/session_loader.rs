//! Session loader for the Matrix adapter (mission 0850h-d).
//!
//! Decides whether to load a session from the multi-account stoolap
//! store (`octo-matrix-session-store`) or from the legacy single-file
//! JSON config (missions 0850h-a / 0850h-c), based on
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
//! The loader is `async` (R1-M23): it does NOT build a runtime
//! internally. The caller is expected to be running inside an
//! existing tokio runtime (e.g., `MatrixAdapter::new`'s
//! `runtime.block_on(...)` call). This avoids the prior
//! "runtime-inside-runtime" anti-pattern where the loader built a
//! fresh `current_thread` runtime per call.
//!
//! The loader does NOT itself create a `Client`; it returns a
//! `LoadedSession` and the caller (typically `MatrixAdapter::new`)
//! does the SDK wiring. Keeping the loader pure-data makes it easy
//! to test.

use crate::config_writer::OnDiskConfig;
use crate::MatrixConfig;
use octo_matrix_session_store::{
    default_store_path, SessionRow, SessionStore, SessionStoreError, StoolapSessionStore,
};
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
    /// R2-M13: I/O error from the store. The inner `SessionStoreError`
    /// carries the structured variant (e.g., `Stoolap`,
    /// `AlreadyExists`, `NotFound`) so the operator / host can
    /// branch on the cause (e.g., a `NotFound` here would be a bug
    /// in the host — the loader already pre-checks for that case;
    /// an `AlreadyExists` would be unexpected on a read path; a
    /// `Stoolap` corruption should suggest a wipe-and-restore). The
    /// previous shape `Store(String)` collapsed all of these into
    /// a single opaque string.
    #[error("store error: {0}")]
    Store(#[source] SessionStoreError),
    /// R19-L1: I/O error reading the legacy config file (e.g.,
    /// file not found, permission denied). The inner `io::Error`
    /// carries the typed kind (`ErrorKind::NotFound`,
    /// `ErrorKind::PermissionDenied`, etc.) so the host can
    /// branch on the cause (e.g., a `NotFound` suggests the
    /// operator set the wrong path; a `PermissionDenied` suggests
    /// a 0600 mode mismatch). The previous shape `File(String)`
    /// collapsed I/O and parse errors into a single opaque
    /// string, asymmetric with the typed `Store` variant above.
    #[error("config file I/O error reading {path:?}: {source}")]
    FileIo {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    /// R19-L1: JSON parse error from the legacy config file.
    /// The inner `serde_json::Error` carries the line/column
    /// pointer (e.g., `expected value at line 5 column 12`) so
    /// the host can surface the operator to the exact location
    /// in the malformed file. The previous shape `File(String)`
    /// collapsed I/O and parse errors into a single opaque
    /// string, asymmetric with the typed `Store` variant above.
    #[error("config file JSON parse error: {0}")]
    FileParse(#[source] serde_json::Error),
}

/// A loaded session, agnostic of source. The caller passes
/// `(user_id, device_id, access_token, refresh_token, homeserver_url)`
/// to `Client::restore_session` to wire the SDK.
#[derive(Clone, PartialEq, Eq)]
pub struct LoadedSession {
    pub user_id: String,
    pub device_id: String,
    pub homeserver_url: String,
    pub access_token: String,
    pub refresh_token: Option<String>,
}

/// R23-L1: hand-rolled `Debug` for `LoadedSession`. The
/// auto-derived form would print `access_token` and
/// `refresh_token` in plain text, so any `dbg!(loaded)` or
/// `tracing::debug!(?loaded)` would leak the loaded tokens to
/// stderr. The redacted form matches `MatrixConfig::Debug`
/// (3-tier `redact_token` from the adapter's `crate::redact_token`)
/// so the four session-bearing data structures
/// (`MatrixConfig`, `LoadedSession`, `OnboardConfig`,
/// `SessionRow`) all produce consistent redacted Debug output.
impl std::fmt::Debug for LoadedSession {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("LoadedSession")
            .field("user_id", &self.user_id)
            .field("device_id", &self.device_id)
            .field("homeserver_url", &self.homeserver_url)
            .field("access_token", &crate::redact_token(&self.access_token))
            .field(
                "refresh_token",
                &self.refresh_token.as_deref().map(crate::redact_token),
            )
            .finish()
    }
}

/// Load a session according to `MatrixConfig`.
///
/// R1-M23: this is `async` (was sync). The caller MUST be running
/// inside a tokio runtime. `MatrixAdapter::new` builds a runtime
/// and uses `runtime.block_on(load(&config))` to drive the loader
/// — this eliminates the prior per-call `current_thread` runtime
/// build that the loader used to do internally.
///
/// The `config` argument carries the host process's intent: which
/// source to use, where the store lives, which `(user_id, device_id)`
/// to look up. The `user_id` / `device_id` are taken from the
/// `MatrixConfig` itself (so the host only needs to populate
/// `user_id` + `device_id` once for both store and file sources).
pub async fn load(config: &MatrixConfig) -> Result<LoadedSession, LoadError> {
    if config.use_session_store {
        load_from_store(config).await
    } else {
        load_from_file(config)
    }
}

async fn load_from_store(config: &MatrixConfig) -> Result<LoadedSession, LoadError> {
    let store_path: PathBuf = if config.session_store_path.as_os_str().is_empty() {
        default_store_path().map_err(|e| match e {
            SessionStoreError::NoDefaultPath => LoadError::NoDefaultPath,
            other => LoadError::Store(other),
        })?
    } else {
        config.session_store_path.clone()
    };
    let store = StoolapSessionStore::new(&store_path).map_err(LoadError::Store)?;
    let session: SessionRow = store
        .get_session(&config.user_id, &config.device_id)
        .await
        .map_err(LoadError::Store)?
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
    let bytes = std::fs::read(&config.config_path).map_err(|source| LoadError::FileIo {
        path: config.config_path.clone(),
        source,
    })?;
    let on_disk: OnDiskConfig = serde_json::from_slice(&bytes).map_err(LoadError::FileParse)?;
    Ok(LoadedSession {
        user_id: on_disk.user_id,
        device_id: on_disk.device_id,
        homeserver_url: on_disk.homeserver_url,
        access_token: on_disk.access_token,
        refresh_token: on_disk.refresh_token,
    })
}

// R1-M23: removed `futures_block_on` helper — the loader is now
// `async` and uses the caller's runtime. The `MatrixAdapter::new`
// path drives the loader via `runtime.block_on(load(&config))`.

#[cfg(test)]
mod tests {
    use super::*;
    use octo_matrix_session_store::{LoginType, StoolapSessionStore};
    use std::io::Write;
    use std::os::unix::fs::OpenOptionsExt;

    /// R2-M17: use `tempfile::TempDir` so the directory is auto-
    /// removed on test completion (and on panic) instead of
    /// accumulating in `/tmp` forever. The previous `tmpdir()`
    /// helper built a nanosecond-named path under `std::env::temp_dir()`
    /// and never cleaned up.
    fn tmpdir() -> tempfile::TempDir {
        tempfile::TempDir::new().expect("tempfile::TempDir::new() must succeed on the test box")
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

    /// R1-M23: a per-test current-thread runtime. The loader is
    /// async now; tests that want to call it need a runtime. A
    /// fresh `current_thread` runtime per test is fine because
    /// tests are the only place we still build a runtime for the
    /// loader (production callers own a `multi_thread` runtime).
    fn block_on<F: std::future::Future>(future: F) -> F::Output {
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("build current-thread tokio runtime for loader test");
        rt.block_on(future)
    }

    #[test]
    fn load_from_store_returns_session_row() {
        let dir = tmpdir();
        let store_path = dir.path().join("sessions.db");
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
        block_on(async { store.add_session(&row, false).await }).unwrap();

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
        let loaded = block_on(load(&cfg)).expect("load from store");
        assert_eq!(loaded.user_id, "@bot:matrix.example.com");
        assert_eq!(loaded.access_token, "syt_xxx");
        assert_eq!(loaded.refresh_token.as_deref(), Some("syr_xxx"));
    }

    #[test]
    fn load_from_store_returns_not_in_store_for_missing_row() {
        let dir = tmpdir();
        let store_path = dir.path().join("sessions.db");
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
        let err = block_on(load(&cfg)).unwrap_err();
        assert!(matches!(err, LoadError::NotInStore { .. }));
    }

    #[test]
    fn load_from_file_returns_on_disk_config() {
        let dir = tmpdir();
        let cfg_path = dir.path().join("config.json");
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
        let loaded = block_on(load(&cfg)).expect("load from file");
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
        let err = block_on(load(&cfg)).unwrap_err();
        assert!(matches!(err, LoadError::NoSource));
    }

    /// R19-L1: a missing `config_path` for the file-based source
    /// surfaces as `LoadError::FileIo` (typed `io::Error`, not
    /// opaque `String`). The variant's `source` is the underlying
    /// `io::Error` so the host can branch on the kind
    /// (`ErrorKind::NotFound`, etc.) without substring-matching.
    #[test]
    fn load_from_file_missing_returns_file_io_error() {
        let dir = tmpdir();
        let cfg_path = dir.path().join("does-not-exist.json");
        let cfg = MatrixConfig {
            homeserver_url: "https://matrix.example.com".to_string(),
            user_id: "@bot:matrix.example.com".to_string(),
            device_id: "ABCDEFGHIJ".to_string(),
            access_token: String::new(),
            refresh_token: None,
            passphrase: None,
            config_path: cfg_path.clone(),
            force_writeback: false,
            use_session_store: false,
            session_store_path: PathBuf::new(),
            rooms: vec![],
        };
        let err = block_on(load(&cfg)).unwrap_err();
        match err {
            LoadError::FileIo { path, source } => {
                assert_eq!(path, cfg_path);
                assert_eq!(source.kind(), std::io::ErrorKind::NotFound);
            }
            other => panic!("expected LoadError::FileIo, got {other:?}"),
        }
    }

    /// R19-L1: a malformed JSON in `config_path` surfaces as
    /// `LoadError::FileParse` (typed `serde_json::Error`, not
    /// opaque `String`). The variant's `source` carries the
    /// serde error's line/column pointer so the host can surface
    /// the operator to the exact location.
    #[test]
    fn load_from_file_malformed_returns_file_parse_error() {
        let dir = tmpdir();
        let cfg_path = dir.path().join("malformed.json");
        write_file(&cfg_path, "{ this is not valid json");
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
        let err = block_on(load(&cfg)).unwrap_err();
        assert!(
            matches!(err, LoadError::FileParse(_)),
            "expected LoadError::FileParse, got {err:?}"
        );
    }
}
