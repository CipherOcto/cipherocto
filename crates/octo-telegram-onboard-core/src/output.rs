//! Config writer — JSON output with 0600 file mode on Unix.
//!
//! Writes a `TelegramConfig`-compatible JSON file that the adapter
//! can load without modification. Also writes a `session_meta.json`
//! sidecar for fast `session list`.

use crate::error::{OnboardError, Result};
use crate::session::TelegramSession;
use std::path::{Path, PathBuf};

/// Resolve the default output path via `dirs::config_dir()`.
pub fn default_config_path() -> Result<PathBuf> {
    default_config_path_opt()
        .ok_or_else(|| OnboardError::BadConfig("could not determine config directory".into()))
}

/// Resolve the default output path, returning `None` if `dirs::config_dir()` is unavailable.
pub fn default_config_path_opt() -> Option<PathBuf> {
    let mut base = dirs::config_dir()?;
    base.push("octo");
    base.push("telegram.json");
    Some(base)
}

/// Build the TelegramConfig-compatible JSON from a session.
pub fn build_config_json(session: &TelegramSession) -> serde_json::Value {
    let mut map = serde_json::Map::new();

    if let Some(ref mode) = session.mode {
        map.insert("mode".into(), serde_json::Value::String(mode.clone()));
    }
    // bot_token and api_hash are included by the CLI caller (they come from
    // the Credentials, not the session). The output module only handles
    // the fields derived from the session. The caller is responsible for
    // inserting bot_token, api_id, api_hash, phone, etc.

    map.insert(
        "data_dir".into(),
        serde_json::Value::String(session.data_dir.to_string_lossy().into_owned()),
    );
    map.insert("groups".into(), serde_json::Value::Array(vec![]));
    map.insert(
        "features".into(),
        serde_json::json!({ "e2e_chats": false, "voice_video": false }),
    );
    if let Some(ref key) = session.verifying_key {
        map.insert(
            "verifying_key".into(),
            serde_json::Value::String(key.clone()),
        );
    } else {
        map.insert("verifying_key".into(), serde_json::Value::Null);
    }

    serde_json::Value::Object(map)
}

/// Write the config JSON to the chosen sink.
pub fn write_config(
    out: Option<&Path>,
    stdout: bool,
    force: bool,
    json: &serde_json::Value,
) -> Result<()> {
    if stdout {
        let text =
            serde_json::to_string_pretty(json).map_err(|e| OnboardError::Generic(e.into()))?;
        println!("{}", text);
        return Ok(());
    }

    let path = match out {
        Some(p) => p.to_path_buf(),
        None => default_config_path()?,
    };

    if path.exists() && !force {
        return Err(OnboardError::BadConfig(format!(
            "refusing to overwrite existing file: {} (pass --force to override)",
            path.display()
        )));
    }

    write_atomic(&path, json)
}

/// Atomic write with 0600 permissions on Unix.
fn write_atomic(path: &Path, json: &serde_json::Value) -> Result<()> {
    if let Some(parent) = path.parent() {
        if !parent.as_os_str().is_empty() {
            std::fs::create_dir_all(parent).map_err(|e| {
                OnboardError::BadConfig(format!("create_dir_all({}): {}", parent.display(), e))
            })?;
        }
    }

    let text = serde_json::to_string_pretty(json).map_err(|e| OnboardError::Generic(e.into()))?;

    use std::io::Write;
    let mut tmp = tempfile::NamedTempFile::new_in(
        path.parent()
            .filter(|p| !p.as_os_str().is_empty())
            .unwrap_or_else(|| Path::new(".")),
    )
    .map_err(|e| OnboardError::BadConfig(format!("create tmp: {}", e)))?;

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let perms = std::fs::Permissions::from_mode(0o600);
        std::fs::set_permissions(tmp.path(), perms)
            .map_err(|e| OnboardError::BadConfig(format!("set_permissions: {}", e)))?;
    }

    tmp.write_all(text.as_bytes())
        .and_then(|_| tmp.write_all(b"\n"))
        .map_err(|e| OnboardError::BadConfig(format!("write tmp: {}", e)))?;
    tmp.as_file()
        .sync_all()
        .map_err(|e| OnboardError::BadConfig(format!("sync tmp: {}", e)))?;

    #[cfg(not(unix))]
    if path.exists() {
        std::fs::remove_file(path).ok();
    }

    tmp.persist(path).map_err(|e| {
        OnboardError::BadConfig(format!("persist tmp to {}: {}", path.display(), e))
    })?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::session::TelegramSession;
    use std::path::PathBuf;

    fn sample_session() -> TelegramSession {
        TelegramSession {
            username: Some("mybot".into()),
            user_id: 123456789,
            mode: Some("bot".into()),
            data_dir: PathBuf::from("/tmp/test-tg"),
            verifying_key: None,
        }
    }

    #[test]
    fn build_config_json_has_required_fields() {
        let session = sample_session();
        let json = build_config_json(&session);
        assert_eq!(json["mode"], "bot");
        assert_eq!(json["data_dir"], "/tmp/test-tg");
        assert_eq!(json["groups"], serde_json::json!([]));
        assert_eq!(json["features"]["e2e_chats"], false);
        assert_eq!(json["verifying_key"], serde_json::Value::Null);
    }

    #[test]
    fn build_config_json_includes_verifying_key_when_set() {
        let mut session = sample_session();
        session.verifying_key = Some("dGVzdGtleQ==".into());
        let json = build_config_json(&session);
        assert_eq!(json["verifying_key"], "dGVzdGtleQ==");
    }

    #[test]
    fn write_atomic_creates_file_with_0600() {
        let dir = tempfile::TempDir::new().unwrap();
        let path = dir.path().join("telegram.json");
        let json = serde_json::json!({"mode": "bot"});
        write_atomic(&path, &json).unwrap();
        let text = std::fs::read_to_string(&path).unwrap();
        assert!(text.contains("\"mode\""));
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let perms = std::fs::metadata(&path).unwrap().permissions().mode() & 0o777;
            assert_eq!(perms, 0o600);
        }
    }

    #[test]
    fn refuse_overwrite_without_force() {
        let dir = tempfile::TempDir::new().unwrap();
        let path = dir.path().join("telegram.json");
        std::fs::write(&path, "{}").unwrap();
        let json = serde_json::json!({"mode": "bot"});
        let err = write_config(Some(&path), false, false, &json).unwrap_err();
        assert!(
            matches!(err, OnboardError::BadConfig(ref msg) if msg.contains("refusing to overwrite"))
        );
    }

    #[test]
    fn force_overwrites_existing() {
        let dir = tempfile::TempDir::new().unwrap();
        let path = dir.path().join("telegram.json");
        std::fs::write(&path, "{}").unwrap();
        let json = serde_json::json!({"mode": "bot"});
        write_config(Some(&path), false, true, &json).unwrap();
        let text = std::fs::read_to_string(&path).unwrap();
        assert!(text.contains("\"mode\""));
    }
}
