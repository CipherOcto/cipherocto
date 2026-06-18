//! Config writer (atomic, 0600, --stdout, --force).
//!
//! R3-M3: `pub fn write(args, session)` calls
//! `session.to_disk_json()` (from the core lib) and then a binary-
//! private `write_atomic()` helper. R5-M1: layer ordering.

use std::path::{Path, PathBuf};

use octo_whatsapp_onboard_core::WhatsAppSession;

use crate::cli::OutputArgs;
use crate::error::OnboardError;
type Result<T> = std::result::Result<T, OnboardError>;

/// Write the captured session to the chosen sink.
pub fn write(args: &OutputArgs, session: &WhatsAppSession) -> Result<()> {
    let json = session.to_disk_json();

    if args.stdout {
        let text = serde_json::to_string_pretty(&json)
            .map_err(|e| OnboardError::Generic(anyhow::anyhow!("serialize: {e}")))?;
        println!("{text}");
        return Ok(());
    }

    let path = match &args.out {
        Some(p) => p.clone(),
        None => default_path()?,
    };

    if path.exists() && !args.force {
        return Err(OnboardError::BadConfig(format!(
            "refusing to overwrite existing file: {} (pass --force to override)",
            path.display()
        )));
    }

    write_atomic(&path, &json)
}

/// Default output path: `~/.config/octo/whatsapp.json` via `dirs::config_dir()`.
pub fn default_path() -> Result<PathBuf> {
    let mut base = dirs::config_dir()
        .ok_or_else(|| OnboardError::BadConfig("could not determine config directory".into()))?;
    base.push("octo");
    base.push("whatsapp.json");
    Ok(base)
}

fn write_atomic(path: &Path, json: &serde_json::Value) -> Result<()> {
    use std::io::Write;
    if let Some(parent) = path.parent() {
        if !parent.as_os_str().is_empty() {
            std::fs::create_dir_all(parent).map_err(|e| {
                OnboardError::BadConfig(format!("create_dir_all({}): {}", parent.display(), e))
            })?;
        }
    }

    let text = serde_json::to_string_pretty(json)
        .map_err(|e| OnboardError::Generic(anyhow::anyhow!("serialize: {e}")))?;

    let mut tmp = tempfile::NamedTempFile::new_in(
        path.parent()
            .filter(|p| !p.as_os_str().is_empty())
            .unwrap_or_else(|| Path::new(".")),
    )
    .map_err(|e| OnboardError::BadConfig(format!("create tmp: {e}")))?;

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let perms = std::fs::Permissions::from_mode(0o600);
        std::fs::set_permissions(tmp.path(), perms)
            .map_err(|e| OnboardError::BadConfig(format!("set_permissions: {e}")))?;
    }

    tmp.write_all(text.as_bytes())
        .and_then(|_| tmp.write_all(b"\n"))
        .map_err(|e| OnboardError::BadConfig(format!("write tmp: {e}")))?;
    tmp.as_file()
        .sync_all()
        .map_err(|e| OnboardError::BadConfig(format!("sync tmp: {e}")))?;

    // On Windows we may need to remove the destination first.
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
    use crate::cli::OutputArgs;
    use std::path::PathBuf;

    fn sample_session() -> WhatsAppSession {
        WhatsAppSession {
            self_phone: Some("15551234567".to_string()),
            session_path: PathBuf::from("/tmp/test.session.db"),
            groups: vec!["120363012345678901@g.us".to_string()],
            pair_phone: None,
        }
    }

    fn args_out(path: PathBuf) -> OutputArgs {
        OutputArgs {
            out: Some(path),
            stdout: false,
            force: false,
        }
    }

    #[test]
    fn write_to_temp_file_creates_parent_dirs_and_emits_config() {
        // The on-disk JSON is a WhatsAppConfig, which does NOT have
        // a `self_phone` field (the phone is the adapter's runtime
        // state, not config). The config has `session_path` and
        // `groups` only (plus optional `pair_phone` for pair-link).
        let dir = tempfile::TempDir::new().unwrap();
        let path = dir.path().join("subdir/whatsapp.json");
        write(&args_out(path.clone()), &sample_session()).unwrap();
        let text = std::fs::read_to_string(&path).unwrap();
        assert!(text.contains("session_path"));
        assert!(text.contains("120363012345678901@g.us"));
    }

    #[test]
    fn refuse_overwrite_without_force() {
        let dir = tempfile::TempDir::new().unwrap();
        let path = dir.path().join("whatsapp.json");
        std::fs::write(&path, "{}").unwrap();
        let err = write(&args_out(path.clone()), &sample_session()).unwrap_err();
        match err {
            OnboardError::BadConfig(msg) => assert!(msg.contains("refusing to overwrite")),
            other => panic!("expected BadConfig, got {other:?}"),
        }
    }

    #[test]
    fn force_overwrites_existing() {
        let dir = tempfile::TempDir::new().unwrap();
        let path = dir.path().join("whatsapp.json");
        std::fs::write(&path, "{}").unwrap();
        let mut args = args_out(path.clone());
        args.force = true;
        write(&args, &sample_session()).unwrap();
        let text = std::fs::read_to_string(&path).unwrap();
        assert!(text.contains("session_path"));
    }

    #[cfg(unix)]
    #[test]
    fn file_mode_is_0600() {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::TempDir::new().unwrap();
        let path = dir.path().join("whatsapp.json");
        write(&args_out(path.clone()), &sample_session()).unwrap();
        let perms = std::fs::metadata(&path).unwrap().permissions().mode() & 0o777;
        assert_eq!(perms, 0o600, "expected 0600, got {:o}", perms);
    }
}
