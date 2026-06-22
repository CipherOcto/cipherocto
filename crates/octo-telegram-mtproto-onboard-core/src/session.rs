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
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
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
    pub fn write_to(&self, data_dir: &Path) -> Result<PathBuf, OnboardError> {
        if !data_dir.exists() {
            std::fs::create_dir_all(data_dir)?;
        }
        let path = data_dir.join(SESSION_FILENAME);
        let body = serde_json::to_string_pretty(self)?;
        // Atomic-ish write: stage to a temp file, then rename.
        // Avoids leaving a half-written session.json if the
        // process is killed mid-write.
        let tmp = data_dir.join(format!("{}.tmp", SESSION_FILENAME));
        std::fs::write(&tmp, body)?;
        std::fs::rename(&tmp, &path)?;
        Ok(path)
    }

    /// Read from `<data_dir>/session.json`. Returns
    /// `OnboardError::NotReady` (repurposed) if the file does
    /// not exist or its schema version is unsupported.
    pub fn read_from(data_dir: &Path) -> Result<Self, OnboardError> {
        let path = data_dir.join(SESSION_FILENAME);
        if !path.exists() {
            return Err(OnboardError::NotReady {
                last_state: format!("no session file at {}", path.display()),
            });
        }
        let body = std::fs::read_to_string(&path)?;
        let rec: Self = serde_json::from_str(&body)?;
        if rec.schema_version != SESSION_SCHEMA_VERSION {
            return Err(OnboardError::NotReady {
                last_state: format!(
                    "session schema_version {} (expected {})",
                    rec.schema_version, SESSION_SCHEMA_VERSION
                ),
            });
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
    fn read_missing_file_returns_not_ready() {
        let tmp = tempdir().unwrap();
        let err = SessionRecord::read_from(tmp.path()).unwrap_err();
        assert_eq!(err.kind(), "not_ready");
    }

    #[test]
    fn read_unsupported_schema_returns_not_ready() {
        let tmp = tempdir().unwrap();
        std::fs::create_dir_all(tmp.path()).unwrap();
        std::fs::write(
            tmp.path().join(SESSION_FILENAME),
            r#"{"schema_version": 999, "user_id": 1, "username": null, "refreshed_at_unix": 0, "mode": "bot_token"}"#,
        )
        .unwrap();
        let err = SessionRecord::read_from(tmp.path()).unwrap_err();
        assert_eq!(err.kind(), "not_ready");
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
}
