//! Session types — captured identity after successful auth.

use std::path::PathBuf;

/// Captured session after successful auth.
#[derive(Clone)]
pub struct TelegramSession {
    /// Bot username (e.g., "mybot") or user username (e.g., "johndoe").
    pub username: Option<String>,
    /// Numeric user ID from get_me().
    pub user_id: i64,
    /// "bot" or "user" — matches TelegramConfig::mode.
    pub mode: Option<String>,
    /// Path to TDLib database directory.
    pub data_dir: PathBuf,
    /// Ed25519 verifying key (base64, optional).
    pub verifying_key: Option<String>,
}

impl std::fmt::Debug for TelegramSession {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TelegramSession")
            .field("username", &self.username)
            .field("user_id", &self.user_id)
            .field("mode", &self.mode)
            .field("data_dir", &self.data_dir)
            .field(
                "verifying_key",
                &self.verifying_key.as_ref().map(|_| "[REDACTED]"),
            )
            .finish()
    }
}

/// Metadata sidecar written alongside the TDLib database for fast `session list`.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct SessionMeta {
    pub user_id: i64,
    pub username: Option<String>,
    pub mode: String,
    pub timestamp: i64,
}

impl SessionMeta {
    pub fn from_session(session: &TelegramSession) -> crate::error::Result<Self> {
        let mode = session.mode.clone().ok_or_else(|| {
            crate::error::OnboardError::BadConfig(
                "TelegramSession::mode must be set before writing sidecar".into(),
            )
        })?;
        Ok(Self {
            user_id: session.user_id,
            username: session.username.clone(),
            mode,
            timestamp: {
                let now = std::time::SystemTime::now();
                now.duration_since(std::time::UNIX_EPOCH)
                    .unwrap_or(std::time::Duration::ZERO)
                    .as_secs() as i64
            },
        })
    }

    /// Write the sidecar file alongside the TDLib database atomically.
    /// Uses tempfile + persist for atomicity, and sets 0600 permissions on Unix.
    pub fn write(&self, data_dir: &std::path::Path) -> crate::error::Result<()> {
        let path = data_dir.join("session_meta.json");
        let json = serde_json::to_string_pretty(self).map_err(|e| {
            crate::error::OnboardError::Generic(anyhow::anyhow!("serialize meta: {}", e))
        })?;
        let parent = path
            .parent()
            .filter(|p| !p.as_os_str().is_empty())
            .unwrap_or(std::path::Path::new("."));
        let mut tmp = tempfile::NamedTempFile::new_in(parent)
            .map_err(|e| crate::error::OnboardError::BadConfig(format!("create tmp: {}", e)))?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            if let Err(e) =
                std::fs::set_permissions(tmp.path(), std::fs::Permissions::from_mode(0o600))
            {
                eprintln!(
                    "WARNING: could not set 0600 on sidecar path={} error={}",
                    path.display(),
                    e
                );
            }
        }
        std::io::Write::write_all(&mut tmp, json.as_bytes())
            .map_err(|e| crate::error::OnboardError::BadConfig(format!("write meta: {}", e)))?;
        tmp.as_file()
            .sync_all()
            .map_err(|e| crate::error::OnboardError::BadConfig(format!("sync meta: {}", e)))?;
        tmp.persist(&path)
            .map_err(|e| crate::error::OnboardError::BadConfig(format!("persist meta: {}", e)))?;
        Ok(())
    }

    /// Read a sidecar file if it exists.
    pub fn read(data_dir: &std::path::Path) -> Option<Self> {
        let path = data_dir.join("session_meta.json");
        let bytes = std::fs::read(&path).ok()?;
        serde_json::from_slice(&bytes).ok()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_session() -> TelegramSession {
        TelegramSession {
            username: Some("testuser".into()),
            user_id: 12345,
            mode: Some("bot".into()),
            data_dir: PathBuf::from("/tmp/test-session"),
            verifying_key: None,
        }
    }

    #[test]
    fn session_meta_write_read_roundtrip() {
        let dir = tempfile::TempDir::new().unwrap();
        let session = sample_session();
        SessionMeta::from_session(&session)
            .unwrap()
            .write(dir.path())
            .unwrap();
        let read = SessionMeta::read(dir.path()).unwrap();
        assert_eq!(read.user_id, session.user_id);
        assert_eq!(read.mode, session.mode.unwrap());
        assert_eq!(read.username, session.username);
    }

    #[test]
    fn session_meta_from_session_errors_on_none_mode() {
        let mut session = sample_session();
        session.mode = None;
        assert!(SessionMeta::from_session(&session).is_err());
    }

    #[test]
    fn session_meta_read_missing_returns_none() {
        let dir = tempfile::TempDir::new().unwrap();
        assert!(SessionMeta::read(dir.path()).is_none());
    }

    #[test]
    fn session_meta_read_corrupted_returns_none_silently() {
        let dir = tempfile::TempDir::new().unwrap();
        std::fs::write(dir.path().join("session_meta.json"), "not json").unwrap();
        assert!(SessionMeta::read(dir.path()).is_none());
    }
}
