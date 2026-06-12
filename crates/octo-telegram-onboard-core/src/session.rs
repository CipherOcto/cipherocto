//! Session types — captured identity after successful auth.

use std::path::PathBuf;

/// Captured session after successful auth.
#[derive(Debug, Clone)]
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

/// Session info for session list/verify.
#[derive(Debug, Clone)]
pub struct SessionInfo {
    pub data_dir: PathBuf,
    pub user_id: Option<i64>,
    pub username: Option<String>,
    pub mode: Option<String>,
    pub is_valid: bool,
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
    pub fn from_session(session: &TelegramSession) -> Self {
        debug_assert!(
            session.mode.is_some(),
            "SessionMeta::from_session called without mode set"
        );
        Self {
            user_id: session.user_id,
            username: session.username.clone(),
            mode: session.mode.clone().unwrap_or_else(|| "bot".into()),
            timestamp: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs() as i64,
        }
    }

    /// Write the sidecar file alongside the TDLib database.
    pub fn write(&self, data_dir: &std::path::Path) -> crate::error::Result<()> {
        let path = data_dir.join("session_meta.json");
        let json = serde_json::to_string_pretty(self).map_err(|e| {
            crate::error::OnboardError::Generic(anyhow::anyhow!("serialize meta: {}", e))
        })?;
        std::fs::write(&path, &json)
            .map_err(|e| crate::error::OnboardError::BadConfig(format!("write meta: {}", e)))?;
        // Set 0600 permissions on Unix for defense-in-depth (parent dir is 0700,
        // but macOS/Windows ancestors may not be).
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let _ = std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600));
        }
        Ok(())
    }

    /// Read a sidecar file if it exists.
    pub fn read(data_dir: &std::path::Path) -> Option<Self> {
        let path = data_dir.join("session_meta.json");
        let bytes = std::fs::read(&path).ok()?;
        serde_json::from_slice(&bytes).ok()
    }
}
