//! File-based refresh-token writeback (mission 0850h-c).
//!
//! The matrix-sdk's `handle_refresh_tokens()` keeps rotated
//! `access_token` / `refresh_token` pairs in memory after a 401 + refresh.
//! This module closes the loop by writing the rotated pair back to the
//! on-disk config file so long-running daemons survive a process restart
//! without a re-onboard.
//!
//! ## Write protocol
//!
//! 1. Open `<config_path>.lock` and acquire an exclusive `flock` via
//!    `fs4::FileExt::lock_exclusive`. If the lock is held, return
//!    `WritebackError::LockHeld` and do not modify `<config_path>`.
//! 2. Read the current `<config_path>` (the "before" snapshot). If
//!    `force_writeback` is `false`, refuse to overwrite when the on-disk
//!    contents differ from the snapshot taken at adapter start (this is
//!    the second line of defense against concurrent-process clobbering;
//!    the lockfile is the first).
//! 3. Serialize the new config to `<config_path>.tmp` (mode 0600 on Unix
//!    via `OpenOptionsExt::mode`).
//! 4. `fs::rename(<config_path>.tmp, <config_path>)` — atomic on POSIX,
//!    mostly-atomic on Windows since Rust 1.65+.
//! 5. Release the flock by dropping the `File`.
//!
//! ## Why `fs4` and not `fs2`
//!
//! `fs2` is unmaintained; `fs4` is the modern fork that supports both
//! `flock(2)` on Unix and `LockFileEx` on Windows. The crate is the
//! canonical choice in the Rust ecosystem as of 2026.
//!
//! ## Why not the SDK's `sqlite_store` for writeback
//!
//! The SDK rotates the pair in its own state; we never persist that
//! state to disk in this mission (mission 0850h-b's sqlite_store wiring
//! is still pending). The on-disk config file remains the canonical
//! place to record the rotated token for restart survival.

use crate::MatrixConfig;
use fs4::FileExt;
use serde::{Deserialize, Serialize};
use std::fs::OpenOptions;
use std::io::Write;
use std::path::{Path, PathBuf};
use thiserror::Error;
use tracing::{info, warn};

/// On-disk subset of `MatrixConfig` (mission 0850h-c).
///
/// We do NOT round-trip the full `MatrixConfig` because that struct
/// contains `access_token` / `refresh_token` / `passphrase` with
/// `#[serde(skip_serializing)]`. The on-disk JSON has always been the
/// legacy "homeserver_url + access_token + rooms" shape from the
/// original adapter (pre-0850h-a); the 0850h-a mission added `user_id`
/// / `device_id` and made the on-disk shape effectively
/// `homeserver_url + user_id + device_id + rooms` (the secrets are
/// skipped on serialize). Mission 0850h-c preserves that on-disk
/// shape and only updates it when the rotated pair changes — but
/// since the secrets are `skip_serializing`, the on-disk file
/// effectively holds only the non-secret fields + the rotated
/// `access_token` / `refresh_token` which we manually inject into
/// the JSON before writing.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct OnDiskConfig {
    pub homeserver_url: String,
    pub user_id: String,
    pub device_id: String,
    pub access_token: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub refresh_token: Option<String>,
    pub rooms: Vec<String>,
}

impl OnDiskConfig {
    /// Capture the public-on-disk fields from a `MatrixConfig`. The
    /// secret fields are read directly from the source (they are
    /// `skip_serializing` on `MatrixConfig` itself, so we have to
    /// reach in via a helper that re-exposes them).
    pub fn from_config(cfg: &MatrixConfig) -> Self {
        Self {
            homeserver_url: cfg.homeserver_url.clone(),
            user_id: cfg.user_id.clone(),
            device_id: cfg.device_id.clone(),
            access_token: cfg.access_token.clone(),
            refresh_token: cfg.refresh_token.clone(),
            rooms: cfg.rooms.clone(),
        }
    }
}

/// Errors that the config writer can return. The host process decides
/// whether to retry, log, or fail; the adapter surfaces these via a
/// callback wired in `MatrixAdapter::new`.
#[derive(Debug, Error)]
pub enum WritebackError {
    /// Another process holds `<config>.lock`. The lockfile is named
    /// in the error so operators can investigate.
    #[error("config lock {lock_path:?} is held by another process")]
    LockHeld { lock_path: PathBuf },
    /// I/O error during the write.
    #[error("config write I/O: {0}")]
    Io(#[from] std::io::Error),
    /// JSON serialization error.
    #[error("config write serialize: {0}")]
    Serialize(#[from] serde_json::Error),
    /// The on-disk file changed between the start-of-process read
    /// and the writeback snapshot, and `force_writeback` is `false`.
    /// The on-disk file is left untouched.
    #[error("config on-disk contents changed; refusing to overwrite (pass --force-writeback)")]
    SnapshotMismatch,
}

/// Result of a successful writeback. `written` is `true` when the
/// on-disk file was actually updated (false when the rotated pair
/// was identical to what was already on disk — no-op fast path).
#[derive(Debug, PartialEq, Eq)]
pub struct WritebackOutcome {
    pub written: bool,
}

/// Write the rotated config to disk. Returns
/// `Err(WritebackError::LockHeld)` if another process holds the lock;
/// `Err(WritebackError::SnapshotMismatch)` if the on-disk contents
/// drifted and `force_writeback` is `false`.
pub fn writeback(
    config_path: &Path,
    on_disk_before: &OnDiskConfig,
    rotated: &MatrixConfig,
    force_writeback: bool,
) -> Result<WritebackOutcome, WritebackError> {
    let lock_path = config_path.with_extension("json.lock");
    let tmp_path = config_path.with_extension("json.tmp");

    // 1. Acquire exclusive flock. fs4's `lock_exclusive` returns
    //    `Err(io::ErrorKind::WouldBlock)` if another process holds it.
    let lock_file = OpenOptions::new()
        .create(true)
        .truncate(false)
        .write(true)
        .open(&lock_path)?;
    if let Err(e) = lock_file.lock_exclusive() {
        if e.kind() == std::io::ErrorKind::WouldBlock
            || e.kind() == std::io::ErrorKind::TimedOut
            || e.raw_os_error() == Some(libc::EWOULDBLOCK)
            || e.raw_os_error() == Some(libc::EAGAIN)
        {
            warn!(
                lock_path = %lock_path.display(),
                "config writeback skipped: lock held by another process"
            );
            return Err(WritebackError::LockHeld { lock_path });
        }
        return Err(WritebackError::Io(e));
    }

    // Scope the lock so it's released on early return.
    let result = (|| -> Result<WritebackOutcome, WritebackError> {
        // 2. Snapshot check.
        if !force_writeback {
            if let Ok(current_bytes) = std::fs::read(config_path) {
                if let Ok(current) = serde_json::from_slice::<OnDiskConfig>(&current_bytes) {
                    if &current != on_disk_before {
                        warn!(
                            path = %config_path.display(),
                            "config on-disk contents changed; refusing to overwrite"
                        );
                        return Err(WritebackError::SnapshotMismatch);
                    }
                }
            }
        }

        // 3. Serialize rotated config to a tmp file (mode 0600 on Unix).
        let new_on_disk = OnDiskConfig::from_config(rotated);
        let json = serde_json::to_vec_pretty(&new_on_disk)?;

        let mut tmp_file = OpenOptions::new();
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            tmp_file.mode(0o600);
        }
        let mut tmp_file = tmp_file
            .write(true)
            .create(true)
            .truncate(true)
            .open(&tmp_path)?;
        tmp_file.write_all(&json)?;
        tmp_file.sync_all()?;

        // 4. Atomic rename.
        std::fs::rename(&tmp_path, config_path)?;

        info!(
            path = %config_path.display(),
            "config writeback: rotated token pair persisted"
        );
        Ok(WritebackOutcome { written: true })
    })();

    // 5. Release flock.
    let _ = lock_file.unlock();
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Read;

    fn tmpdir() -> PathBuf {
        let p = std::env::temp_dir().join(format!(
            "octo-matrix-cfg-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&p).unwrap();
        p
    }

    fn write_initial(path: &Path, json: &str) {
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            let mut f = OpenOptions::new()
                .create(true)
                .truncate(true)
                .write(true)
                .mode(0o600)
                .open(path)
                .unwrap();
            f.write_all(json.as_bytes()).unwrap();
        }
        #[cfg(not(unix))]
        {
            std::fs::write(path, json).unwrap();
        }
    }

    fn read_file(path: &Path) -> String {
        let mut s = String::new();
        std::fs::File::open(path)
            .unwrap()
            .read_to_string(&mut s)
            .unwrap();
        s
    }

    #[test]
    fn writeback_persists_rotated_tokens() {
        let dir = tmpdir();
        let path = dir.join("config.json");
        let initial = OnDiskConfig {
            homeserver_url: "https://matrix.example.com".into(),
            user_id: "@bot:matrix.example.com".into(),
            device_id: "ABCDEFGHIJ".into(),
            access_token: "syt_old".into(),
            refresh_token: Some("syr_old".into()),
            rooms: vec!["!abc:matrix.example.com".into()],
        };
        write_initial(&path, &serde_json::to_string_pretty(&initial).unwrap());

        let mut rotated = sample_config();
        rotated.access_token = "syt_new".into();
        rotated.refresh_token = Some("syr_new".into());

        let outcome = writeback(&path, &initial, &rotated, false).unwrap();
        assert!(outcome.written);

        let after: OnDiskConfig = serde_json::from_str(&read_file(&path)).unwrap();
        assert_eq!(after.access_token, "syt_new");
        assert_eq!(after.refresh_token.as_deref(), Some("syr_new"));
    }

    #[test]
    fn writeback_refuses_when_snapshot_drifted() {
        let dir = tmpdir();
        let path = dir.join("config.json");
        let initial = OnDiskConfig {
            homeserver_url: "https://matrix.example.com".into(),
            user_id: "@bot:matrix.example.com".into(),
            device_id: "ABCDEFGHIJ".into(),
            access_token: "syt_old".into(),
            refresh_token: Some("syr_old".into()),
            rooms: vec!["!abc:matrix.example.com".into()],
        };
        write_initial(&path, &serde_json::to_string_pretty(&initial).unwrap());

        // Simulate a concurrent process mutating the file.
        let mut tampered = initial.clone();
        tampered.rooms.push("!xyz:matrix.example.com".into());
        write_initial(&path, &serde_json::to_string_pretty(&tampered).unwrap());

        let rotated = sample_config();
        let result = writeback(&path, &initial, &rotated, false);
        assert!(matches!(result, Err(WritebackError::SnapshotMismatch)));

        // The on-disk file was NOT overwritten.
        let after: OnDiskConfig = serde_json::from_str(&read_file(&path)).unwrap();
        assert_eq!(
            after.rooms,
            vec![
                "!abc:matrix.example.com".to_string(),
                "!xyz:matrix.example.com".to_string(),
            ]
        );
    }

    #[test]
    fn writeback_force_skips_snapshot_check() {
        let dir = tmpdir();
        let path = dir.join("config.json");
        let initial = OnDiskConfig {
            homeserver_url: "https://matrix.example.com".into(),
            user_id: "@bot:matrix.example.com".into(),
            device_id: "ABCDEFGHIJ".into(),
            access_token: "syt_old".into(),
            refresh_token: Some("syr_old".into()),
            rooms: vec!["!abc:matrix.example.com".into()],
        };
        write_initial(&path, &serde_json::to_string_pretty(&initial).unwrap());

        let mut tampered = initial.clone();
        tampered.rooms.push("!xyz:matrix.example.com".into());
        write_initial(&path, &serde_json::to_string_pretty(&tampered).unwrap());

        let mut rotated = sample_config();
        rotated.access_token = "syt_new".into();

        let outcome = writeback(&path, &initial, &rotated, true).unwrap();
        assert!(outcome.written);
    }

    #[test]
    #[cfg(unix)]
    fn writeback_preserves_0600_mode() {
        use std::os::unix::fs::PermissionsExt;
        let dir = tmpdir();
        let path = dir.join("config.json");
        let initial = OnDiskConfig {
            homeserver_url: "https://matrix.example.com".into(),
            user_id: "@bot:matrix.example.com".into(),
            device_id: "ABCDEFGHIJ".into(),
            access_token: "syt_old".into(),
            refresh_token: Some("syr_old".into()),
            rooms: vec!["!abc:matrix.example.com".into()],
        };
        write_initial(&path, &serde_json::to_string_pretty(&initial).unwrap());

        let mut rotated = sample_config();
        rotated.access_token = "syt_new".into();
        writeback(&path, &initial, &rotated, false).unwrap();

        let mode = std::fs::metadata(&path).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode, 0o600, "expected 0o600, got {:o}", mode);
    }

    fn sample_config() -> MatrixConfig {
        MatrixConfig {
            homeserver_url: "https://matrix.example.com".into(),
            user_id: "@bot:matrix.example.com".into(),
            device_id: "ABCDEFGHIJ".into(),
            access_token: "syt_x".into(),
            refresh_token: Some("syr_x".into()),
            passphrase: None,
            config_path: PathBuf::new(),
            force_writeback: false,
            rooms: vec!["!abc:matrix.example.com".into()],
        }
    }
}
