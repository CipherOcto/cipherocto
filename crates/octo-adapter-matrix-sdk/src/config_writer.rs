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
//! ## Why we do NOT unlink the lock file
//!
//! On Linux, `flock(2)` locks the **open file description**
//! (inode + fd), not the path. If we unlinked the lock path on
//! any path — happy, contention, or unexpected error — a third
//! process could `open("lock", O_CREAT)` after the unlink and
//! get a fresh inode with no flock holder, racing against the
//! process that still holds the original inode. That breaks the
//! whole point of the lockfile. The pre-R2 code (`f99df1a`)
//! left the lock file on disk forever, and we restored that
//! behaviour in R4 after R2-M14 / R3-M1 introduced the race.
//! The lock file is a zero-byte coordination point with no
//! secrets; its persistence is harmless.
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
    /// and the writeback snapshot, and the effective `force_writeback`
    /// is `false` (per-call argument `false` AND config-level setting
    /// `force_writeback: false` per R6-M1 OR semantics). The on-disk
    /// file is left untouched. R9-L1: previously the error message
    /// suggested a non-existent `--force-writeback` CLI flag; the
    /// actual mechanisms are the on-disk config field or the per-call
    /// `force_writeback` argument.
    #[error("config on-disk contents changed; refusing to overwrite (set `force_writeback: true` in the on-disk config, or pass `true` to the per-call `force_writeback` argument)")]
    SnapshotMismatch,
    /// R2-M14: the on-disk file was present at process start (the
    /// adapter constructed an `on_disk_before` from it) but is
    /// missing at writeback time. Silently re-writing under these
    /// conditions destroys the audit trail — a malicious or buggy
    /// deploy script could delete the file between startup and
    /// token rotation to force a fresh write. Surfacing this
    /// explicitly lets the adapter refuse, or the operator
    /// investigate.
    #[error("config file {path:?} disappeared between startup and writeback (refusing to silently rewrite)")]
    FileMissing { path: PathBuf },
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

    // 1. Acquire exclusive flock non-blocking. fs4's `try_lock_exclusive`
    //    returns `Err(io::ErrorKind::WouldBlock)` when another process
    //    holds the lock; `lock_exclusive` is BLOCKING and would
    //    deadlock the worker thread, so we MUST use the non-blocking
    //    variant here. (Mission 0850h-c acceptance criterion: a
    //    second writer surfaces `WritebackError::LockHeld`.)
    let mut lock_open = OpenOptions::new();
    #[cfg(unix)]
    {
        // R2-L1: set the mode explicitly. The lockfile contains no
        // secrets, but the previous `OpenOptions::new()` inherited
        // the umask, so on a `umask 022` box the lockfile was
        // world-readable. The actual config file at line ~245 sets
        // `.mode(0o600)` for the same reason — this just brings the
        // lockfile in line with that policy.
        use std::os::unix::fs::OpenOptionsExt;
        lock_open.mode(0o600);
    }
    let lock_file = lock_open
        .create(true)
        .truncate(false)
        .write(true)
        .open(&lock_path)?;
    if let Err(try_err) = lock_file.try_lock() {
        let e: std::io::Error = try_err.into();
        // R2-M3: on Windows, `LockFileEx` returns
        // `ERROR_LOCK_VIOLATION` (raw_os_error = 33) when the
        // region is already exclusively locked. fs4 surfaces this
        // as a generic `io::Error` without `WouldBlock`, so without
        // this check the lock-held case would fall through to
        // `WritebackError::Io` and defeat the non-blocking
        // guarantee.
        #[cfg(windows)]
        const ERROR_LOCK_VIOLATION: i32 = 33;
        #[cfg(windows)]
        let is_lock_violation = e.raw_os_error() == Some(ERROR_LOCK_VIOLATION);
        #[cfg(not(windows))]
        let is_lock_violation = false;
        if e.kind() == std::io::ErrorKind::WouldBlock
            || e.kind() == std::io::ErrorKind::TimedOut
            || e.raw_os_error() == Some(libc::EWOULDBLOCK)
            || e.raw_os_error() == Some(libc::EAGAIN)
            || is_lock_violation
        {
            warn!(
                lock_path = %lock_path.display(),
                "config writeback skipped: lock held by another process"
            );
            // R4-H1: do NOT unlink the lock file. `flock(2)` is
            // per-inode on Linux, so unlinking the path while
            // process A still holds the flock on the original
            // inode would let process C's `O_CREAT` allocate a
            // fresh inode with no flock holder, racing A's
            // writeback. Leaving the file on disk is harmless
            // (zero-byte, no secrets) and is the pre-R2 design.
            // `lock_file` drops at the end of this scope,
            // releasing the flock.
            return Err(WritebackError::LockHeld { lock_path });
        }
        // R4-H1: same reasoning on the unexpected-error path —
        // do NOT unlink. Just propagate the Io error and let
        // `lock_file` drop at scope exit.
        return Err(WritebackError::Io(e));
    }

    // Scope the lock so it's released on early return.
    let result = (|| -> Result<WritebackOutcome, WritebackError> {
        // 2. Snapshot check. R2-M14: a missing file at writeback time
        //    is now a hard error (was previously silently treated as
        //    "no drift, proceed"). The file existed at process start
        //    (the adapter built `on_disk_before` from it), so a
        //    disappearance between then and now is suspicious — a
        //    malicious or buggy deploy script could have removed the
        //    audit trail. We refuse rather than silently rewriting.
        if !force_writeback {
            match std::fs::read(config_path) {
                Ok(current_bytes) => {
                    if let Ok(current) = serde_json::from_slice::<OnDiskConfig>(&current_bytes) {
                        if &current != on_disk_before {
                            warn!(
                                path = %config_path.display(),
                                "config on-disk contents changed; refusing to overwrite"
                            );
                            return Err(WritebackError::SnapshotMismatch);
                        }
                    }
                    // current_bytes failed to deserialize — the file
                    // exists but is corrupt. Treat that as a snapshot
                    // drift (the on-disk shape is no longer what we
                    // expect). This used to be a silent skip in the
                    // pre-R2 code; surfacing it as SnapshotMismatch
                    // makes it visible to the operator.
                    else {
                        warn!(
                            path = %config_path.display(),
                            "config on-disk contents unparseable; refusing to overwrite"
                        );
                        return Err(WritebackError::SnapshotMismatch);
                    }
                }
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                    warn!(
                        path = %config_path.display(),
                        "config file disappeared between startup and writeback; refusing to rewrite"
                    );
                    return Err(WritebackError::FileMissing {
                        path: config_path.to_path_buf(),
                    });
                }
                Err(e) => return Err(WritebackError::Io(e)),
            }
        }

        // 3. Fast path: if the rotated pair equals what was already on
        //    disk (in-memory config matches the pre-writeback snapshot),
        //    skip the rename. This is the `written: false` branch the
        //    doc on `WritebackOutcome` promised.
        let new_on_disk = OnDiskConfig::from_config(rotated);
        if &new_on_disk == on_disk_before {
            info!(
                path = %config_path.display(),
                "config writeback: rotated pair unchanged; no-op"
            );
            return Ok(WritebackOutcome { written: false });
        }
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

    // 5. Release the flock. R4-H1: do NOT unlink the lock file
    //    here. The lockfile is a coordination point whose
    //    presence guarantees the next writer will flock the
    //    same inode; unlinking it after a successful writeback
    //    reintroduces the per-inode-vs-per-path race that the
    //    pre-R2 design (lock file persists) avoided. The
    //    file is zero-byte and contains no secrets; persistence
    //    is the design, not a leak.
    let _ = lock_file.unlock();
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Read;

    /// R2-M17: use `tempfile::TempDir` so the directory is auto-
    /// removed on test completion (and on panic) instead of
    /// accumulating in `/tmp` forever.
    fn tmpdir() -> tempfile::TempDir {
        tempfile::TempDir::new().expect("tempfile::TempDir::new() must succeed on the test box")
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
        let path = dir.path().join("config.json");
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
        let path = dir.path().join("config.json");
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
        let path = dir.path().join("config.json");
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
    fn writeback_returns_lock_held_when_already_locked() {
        // R1-M22: a second writer must surface `LockHeld` instead
        // of blocking forever on the flock. R1-H2 changed the
        // underlying call to `try_lock_exclusive`; this test
        // proves the error path is reachable.
        let dir = tmpdir();
        let path = dir.path().join("config.json");
        let lock_path = path.with_extension("json.lock");
        let initial = OnDiskConfig {
            homeserver_url: "https://matrix.example.com".into(),
            user_id: "@bot:matrix.example.com".into(),
            device_id: "ABCDEFGHIJ".into(),
            access_token: "syt_old".into(),
            refresh_token: Some("syr_old".into()),
            rooms: vec!["!abc:matrix.example.com".into()],
        };
        write_initial(&path, &serde_json::to_string_pretty(&initial).unwrap());

        // Hold the lock from a separate File handle.
        let holder = std::fs::OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(false)
            .open(&lock_path)
            .unwrap();
        holder.lock().unwrap();

        // Now try to writeback — should return LockHeld.
        let rotated = sample_config();
        let result = writeback(&path, &initial, &rotated, false);
        match result {
            Err(WritebackError::LockHeld { .. }) => {}
            other => panic!("expected LockHeld, got {:?}", other),
        }

        // R4-H1: the lock file persists. The holder's `lock_file`
        // and the test's own `holder` both have open fds on the
        // same inode, so the directory entry is still there too.
        // Best-effort cleanup at the end of the test for hygiene,
        // not correctness.
        holder.unlock().unwrap();
        let _ = std::fs::remove_file(&lock_path);
    }

    /// R4-H1: after a contention failure, the lock file MUST
    /// still be on disk — it's the per-inode coordination point
    /// for the next writer. Replaces the R3-M1 test that
    /// asserted the opposite (which documented the bug we just
    /// reverted).
    #[test]
    #[cfg(unix)]
    fn writeback_preserves_lock_file_after_contention_and_happy_path() {
        let dir = tmpdir();
        let path = dir.path().join("config.json");
        let lock_path = path.with_extension("json.lock");
        let initial = OnDiskConfig {
            homeserver_url: "https://matrix.example.com".into(),
            user_id: "@bot:matrix.example.com".into(),
            device_id: "ABCDEFGHIJ".into(),
            access_token: "syt_old".into(),
            refresh_token: Some("syr_old".into()),
            rooms: vec!["!abc:matrix.example.com".into()],
        };
        write_initial(&path, &serde_json::to_string_pretty(&initial).unwrap());

        // Hold the lock from a separate File handle so the inner
        // writeback hits the contention path.
        let holder = std::fs::OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(false)
            .open(&lock_path)
            .unwrap();
        holder.lock().unwrap();

        let rotated = sample_config();
        let _ = writeback(&path, &initial, &rotated, false);

        // Release the holder and re-attempt — the second
        // writeback should succeed.
        holder.unlock().unwrap();
        drop(holder);

        let mut rotated = sample_config();
        rotated.access_token = "syt_new".into();
        rotated.refresh_token = Some("syr_new".into());
        let outcome = writeback(&path, &initial, &rotated, false).unwrap();
        assert!(outcome.written);

        // R4-H1: after a successful writeback, the lock file
        // MUST still be on disk. The next writer's
        // `OpenOptions::create(true)` will flock the same
        // inode; unlinking here would break serialization.
        assert!(
            lock_path.exists(),
            "lock file should persist after successful writeback: {:?}",
            lock_path
        );

        // Test hygiene: clean up so the next test starts fresh.
        let _ = std::fs::remove_file(&lock_path);
    }

    #[test]
    fn writeback_no_op_when_rotated_equals_snapshot() {
        // R1-M21: the fast-path that was promised by the doc on
        // `WritebackOutcome::written`. When the rotated config
        // equals the pre-writeback snapshot, no file is touched
        // and `written: false` is returned.
        let dir = tmpdir();
        let path = dir.path().join("config.json");
        let initial = OnDiskConfig {
            homeserver_url: "https://matrix.example.com".into(),
            user_id: "@bot:matrix.example.com".into(),
            device_id: "ABCDEFGHIJ".into(),
            access_token: "syt_same".into(),
            refresh_token: Some("syr_same".into()),
            rooms: vec!["!abc:matrix.example.com".into()],
        };
        write_initial(&path, &serde_json::to_string_pretty(&initial).unwrap());
        let before_mtime = std::fs::metadata(&path).unwrap().modified().unwrap();

        // `rotated` equals `initial` field-for-field → fast path.
        let mut rotated = sample_config();
        rotated.access_token = "syt_same".into();
        rotated.refresh_token = Some("syr_same".into());
        let outcome = writeback(&path, &initial, &rotated, false).unwrap();
        assert!(!outcome.written, "expected written: false on no-op");

        // The file was not touched.
        let after_mtime = std::fs::metadata(&path).unwrap().modified().unwrap();
        assert_eq!(before_mtime, after_mtime);
    }

    /// R2-M14: the on-disk file is present at process start (the
    /// adapter built `on_disk_before` from it) but missing at
    /// writeback time. The pre-R2 code silently treated this as
    /// "no drift, proceed" and rewrote the file. The fix surfaces
    /// `WritebackError::FileMissing` so the operator can decide.
    #[test]
    fn writeback_refuses_when_file_missing_at_writeback_time() {
        let dir = tmpdir();
        let path = dir.path().join("config.json");
        let snapshot = OnDiskConfig {
            homeserver_url: "https://matrix.example.com".into(),
            user_id: "@bot:matrix.example.com".into(),
            device_id: "ABCDEFGHIJ".into(),
            access_token: "syt_orig".into(),
            refresh_token: Some("syr_orig".into()),
            rooms: vec!["!abc:matrix.example.com".into()],
        };
        // Note: do NOT call `write_initial` — the file is intentionally
        // absent. The snapshot was taken from a prior read at
        // process start; the file has since been removed (or never
        // written, e.g. a race with a deploy script).
        let mut rotated = sample_config();
        rotated.access_token = "syt_new".into();
        rotated.refresh_token = Some("syr_new".into());
        let result = writeback(&path, &snapshot, &rotated, false);
        assert!(
            matches!(result, Err(WritebackError::FileMissing { .. })),
            "expected FileMissing, got {:?}",
            result
        );
        // R4-H1: the lock file persists by design. The flock was
        // released (the early-return path drops `lock_file` before
        // returning), but the directory entry stays — it's the
        // per-inode coordination point for the next writer. The
        // R2-M14 assertion that it was cleaned up documented a
        // bug we just reverted.
        let lock_path = path.with_extension("json.lock");
        assert!(
            lock_path.exists(),
            "lock file should persist as flock coordination point: {:?}",
            lock_path
        );

        // Test hygiene: clean up so the next test starts fresh.
        let _ = std::fs::remove_file(&lock_path);
    }

    /// R2-M14: `force_writeback = true` bypasses the snapshot check
    /// (and therefore the FileMissing check). This is the escape
    /// hatch for the operator who has already decided they want a
    /// fresh write — the file gets re-created.
    #[test]
    fn writeback_force_rewrites_when_file_missing() {
        let dir = tmpdir();
        let path = dir.path().join("config.json");
        let snapshot = OnDiskConfig {
            homeserver_url: "https://matrix.example.com".into(),
            user_id: "@bot:matrix.example.com".into(),
            device_id: "ABCDEFGHIJ".into(),
            access_token: "syt_orig".into(),
            refresh_token: Some("syr_orig".into()),
            rooms: vec!["!abc:matrix.example.com".into()],
        };
        let mut rotated = sample_config();
        rotated.access_token = "syt_new".into();
        rotated.refresh_token = Some("syr_new".into());
        let outcome = writeback(&path, &snapshot, &rotated, true).unwrap();
        assert!(outcome.written, "force writeback should write the file");
        let written = std::fs::read_to_string(&path).unwrap();
        assert!(written.contains("syt_new"), "written={written}");
    }

    #[test]
    #[cfg(unix)]
    fn writeback_preserves_0600_mode() {
        use std::os::unix::fs::PermissionsExt;
        let dir = tmpdir();
        let path = dir.path().join("config.json");
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
            use_session_store: true,
            session_store_path: PathBuf::new(),
            rooms: vec!["!abc:matrix.example.com".into()],
        }
    }
}
