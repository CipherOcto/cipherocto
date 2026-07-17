//! On-disk session record.
//!
//! After a successful `connect_*` call, the adapter persists
//! its `MtprotoSelfHandle` to `<data_dir>/session.json` (via the
//! `octo-network` `Keyring` adapter, which routes the file to a
//! platform-appropriate location).
//!
//! The CLI's `whoami` mode reads this file and prints the
//! cached `self_id` / `username` so operators can confirm that a
//! previous onboarding is still valid without re-authenticating.

use std::path::{Path, PathBuf};

use octo_adapter_telegram_mtproto::MtprotoSelfIdentity;
use serde::{Deserialize, Serialize};

use crate::error::OnboardError;

/// Schema version of the on-disk session file. Bump on
/// backward-incompatible changes. The CLI refuses to read a
/// session file with a different version and forces a fresh
/// onboarding.
pub const SESSION_SCHEMA_VERSION: u32 = 1;

/// Filename inside the data dir. Constant so the `whoami` reader
/// and the `connect_*` writers always agree.
pub const SESSION_FILENAME: &str = "session.json";

/// On-disk session record.
///
/// R2-ARCH-6: marked `#[non_exhaustive]` for the same
/// forward-compatibility reason as [`crate::output::OnboardOutput`]
/// (a future `SessionRecord { ..., device_model: ... }`
/// shouldn't break every external consumer). Construction
/// inside the workspace still works; the `from_identity`
/// constructor is the supported external surface.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[non_exhaustive]
pub struct SessionRecord {
    /// Schema version. See [`SESSION_SCHEMA_VERSION`].
    pub schema_version: u32,
    /// Telegram user-id (or bot-id) of the authenticated
    /// principal. Mirrors `MtprotoSelfIdentity::user_id`.
    pub user_id: i64,
    /// Optional `@username` (without the leading `@`). Mirrors
    /// `MtprotoSelfIdentity::username`.
    pub username: Option<String>,
    /// Unix epoch (seconds) of when the session was last
    /// refreshed (i.e. when the `connect_*` call completed
    /// successfully). Used for staleness hints in `whoami` mode.
    pub refreshed_at_unix: i64,
    /// The mode that produced this session. Mirrors
    /// [`crate::output::OnboardMode`].
    pub mode: String,
}

/// Write `data` to `tmp` with `0o600` perms on Unix
/// (R2-SEC-10) and `sync_all` (R2-OPS-9). Extracted as a
/// helper so the test suite can drive the perms check
/// without going through the full `write_to` flow.
fn write_session_tmp(tmp: &Path, data: &[u8]) -> Result<(), OnboardError> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        let mut opts = std::fs::OpenOptions::new();
        opts.write(true).create(true).truncate(true).mode(0o600);
        let mut f = opts.open(tmp)?;
        use std::io::Write;
        f.write_all(data)?;
        f.sync_all()?;
    }
    #[cfg(not(unix))]
    {
        use std::io::Write;
        let mut f = std::fs::OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .open(tmp)?;
        f.write_all(data)?;
        f.sync_all()?;
    }
    Ok(())
}

impl SessionRecord {
    /// Build a session record from a freshly-resolved self-handle
    /// identity and the mode that produced it.
    pub fn from_identity(
        identity: &MtprotoSelfIdentity,
        mode: &str,
        refreshed_at_unix: i64,
    ) -> Self {
        Self {
            schema_version: SESSION_SCHEMA_VERSION,
            user_id: identity.user_id,
            username: identity.username.clone(),
            refreshed_at_unix,
            mode: mode.to_string(),
        }
    }

    /// Persist to `<data_dir>/session.json` as pretty-printed
    /// JSON. The parent directory is created if it does not
    /// exist.
    ///
    /// R2-OPS-9 / R2-IE-20: the prior version opened the
    /// tmp file with `OpenOptions::new().write(true)
    /// .create(true).truncate(true)`, wrote the body, and
    /// renamed — but never called `sync_all()`. The
    /// `config.json` write in `main.rs` does call `sync_all`
    /// (via `atomic_write_with_mode`), so a crash between
    /// the rename and the OS flushing dirty pages could
    /// leave an empty `session.json`. The fix matches the
    /// `config.json` pattern: `sync_all()` on the file
    /// before rename, plus `sync_all()` on the parent
    /// directory after rename so the rename itself is
    /// durable.
    ///
    /// R2-SEC-10: on Unix, the session file is set to
    /// `0o600` (operator-only). The file doesn't carry
    /// secrets (just `user_id`, `username`, `mode`, and
    /// `refreshed_at_unix`), but it identifies the
    /// authenticated principal and so is treated as
    /// operator-private for consistency with `config.json`.
    pub fn write_to(&self, data_dir: &Path) -> Result<PathBuf, OnboardError> {
        if !data_dir.exists() {
            std::fs::create_dir_all(data_dir)?;
        }
        let path = data_dir.join(SESSION_FILENAME);
        let body = serde_json::to_string_pretty(self)?;
        // Atomic-ish write: stage to a temp file, fsync it,
        // rename over the target, then fsync the parent
        // directory so the rename itself is durable on
        // crash.
        let tmp = data_dir.join(format!("{}.tmp", SESSION_FILENAME));
        write_session_tmp(&tmp, body.as_bytes())?;
        std::fs::rename(&tmp, &path)?;
        // R2-IE-20: fsync the parent directory so the
        // rename is durable. On non-Unix platforms
        // `File::open` on a directory succeeds but
        // `sync_all` is a no-op (Windows uses FlushFileBuffers
        // which is only meaningful for files); we still call
        // it so the cross-platform code path is uniform and
        // the behaviour on Linux/macOS is correct.
        if let Some(parent) = path.parent() {
            if let Ok(dir) = std::fs::File::open(parent) {
                let _ = dir.sync_all();
            }
        }
        Ok(path)
    }

    /// Read from `<data_dir>/session.json`. Returns
    /// `OnboardError::NoSessionFile` if the file does not
    /// exist or its schema version is unsupported.
    ///
    /// ARCH-1/OPS-3 (R26): the prior implementation
    /// reused `OnboardError::NotReady { last_state:
    /// "no session file at ..." }` for this case, which
    /// conflated "auth flow in flight" with "never
    /// onboarded". The CLI's `whoami` mode needs the
    /// latter to render a "no session found; run one of
    /// bot-token / user-code / qr-login first" hint.
    pub fn read_from(data_dir: &Path) -> Result<Self, OnboardError> {
        let path = data_dir.join(SESSION_FILENAME);
        if !path.exists() {
            return Err(OnboardError::NoSessionFile(format!(
                "no session file at {}",
                path.display()
            )));
        }
        let body = std::fs::read_to_string(&path)?;
        let rec: Self = serde_json::from_str(&body)?;
        if rec.schema_version != SESSION_SCHEMA_VERSION {
            return Err(OnboardError::NoSessionFile(format!(
                "session schema_version {} (expected {})",
                rec.schema_version, SESSION_SCHEMA_VERSION
            )));
        }
        Ok(rec)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::output::OnboardMode;
    use octo_adapter_telegram_mtproto::MtprotoSelfIdentity;
    use tempfile::tempdir;

    #[test]
    fn write_then_read_round_trips() {
        let tmp = tempdir().unwrap();
        let id = MtprotoSelfIdentity {
            user_id: 12345,
            username: Some("test_bot".into()),
        };
        let rec = SessionRecord::from_identity(&id, "bot_token", 1_700_000_000);
        let path = rec.write_to(tmp.path()).unwrap();
        assert!(path.exists());
        let read = SessionRecord::read_from(tmp.path()).unwrap();
        assert_eq!(read, rec);
    }

    #[test]
    fn read_missing_file_returns_no_session_file() {
        let tmp = tempdir().unwrap();
        let err = SessionRecord::read_from(tmp.path()).unwrap_err();
        // ARCH-1 (R26): distinct from `Lifecycle` (the
        // "auth flow in flight" case) so the CLI can
        // render mode-specific hints in whoami.
        assert_eq!(err.kind(), "no_session_file");
    }

    #[test]
    fn read_unsupported_schema_returns_no_session_file() {
        let tmp = tempdir().unwrap();
        std::fs::create_dir_all(tmp.path()).unwrap();
        std::fs::write(
            tmp.path().join(SESSION_FILENAME),
            r#"{"schema_version": 999, "user_id": 1, "username": null, "refreshed_at_unix": 0, "mode": "bot_token"}"#,
        )
        .unwrap();
        let err = SessionRecord::read_from(tmp.path()).unwrap_err();
        assert_eq!(err.kind(), "no_session_file");
    }

    #[test]
    fn write_creates_parent_dir() {
        let tmp = tempdir().unwrap();
        let nested = tmp.path().join("nested").join("subdir");
        let id = MtprotoSelfIdentity {
            user_id: 1,
            username: None,
        };
        let rec = SessionRecord::from_identity(&id, "qr_login", 0);
        rec.write_to(&nested).unwrap();
        assert!(nested.join(SESSION_FILENAME).exists());
    }

    #[test]
    fn mode_field_serializes_to_onboard_mode_string() {
        // Round-trip: feed the `OnboardMode` string into the
        // session record and confirm it matches what the CLI
        // emits in `--output`.
        let id = MtprotoSelfIdentity::default();
        let rec = SessionRecord::from_identity(&id, "bot_token", 0);
        assert_eq!(rec.mode, "bot_token");
        // exercise the snake_case mapping used by the CLI
        let json = serde_json::to_string(&OnboardMode::QrLogin).unwrap();
        assert_eq!(json, "\"qr_login\"");
    }

    /// R2-SEC-10: the session file (which identifies the
    /// authenticated principal) must NOT be world-readable.
    /// On Unix, `SessionRecord::write_to` opens the tmp
    /// file with `0o600` (operator-only). The session file
    /// doesn't carry secrets (just `user_id`, `username`,
    /// `mode`, `refreshed_at_unix`) but it's still treated
    /// as operator-private for consistency with
    /// `config.json`.
    #[cfg(unix)]
    #[test]
    fn session_file_is_0o600_on_unix() {
        use std::os::unix::fs::PermissionsExt;
        let tmp = tempdir().unwrap();
        let id = MtprotoSelfIdentity {
            user_id: 42,
            username: Some("test".into()),
        };
        let rec = SessionRecord::from_identity(&id, "bot_token", 0);
        rec.write_to(tmp.path()).unwrap();
        let mode = std::fs::metadata(tmp.path().join(SESSION_FILENAME))
            .unwrap()
            .permissions()
            .mode()
            & 0o777;
        assert_eq!(
            mode, 0o600,
            "session.json perms should be 0o600 (got {:#o})",
            mode
        );
    }

    /// R2-OPS-9: the tmp file must NOT be left on disk
    /// after the rename. The previous round's `write_to`
    /// used `std::fs::write` (no explicit close) which
    /// could leak the tmp file if the rename failed; the
    /// new helper uses `OpenOptions::create` + `truncate`
    /// + `sync_all` + rename, which leaves no residue.
    #[test]
    fn write_to_leaves_no_tmp_file() {
        let tmp = tempdir().unwrap();
        let id = MtprotoSelfIdentity {
            user_id: 1,
            username: None,
        };
        let rec = SessionRecord::from_identity(&id, "qr_login", 0);
        rec.write_to(tmp.path()).unwrap();
        assert!(tmp.path().join(SESSION_FILENAME).exists());
        assert!(
            !tmp.path()
                .join(format!("{}.tmp", SESSION_FILENAME))
                .exists(),
            "tmp file must be renamed away"
        );
    }
}
