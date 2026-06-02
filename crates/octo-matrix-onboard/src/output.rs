//! Config writer — JSON output with 0600 file mode on Unix.
//!
//! Mission 0850h-a §Acceptance Criteria:
//! - JSON to `--out` (default `~/.config/octo/matrix.json` on Unix,
//!   `%APPDATA%\octo\matrix.json` on Windows — detected via
//!   `dirs::config_dir()`).
//! - Or `--stdout`.
//! - Refuses to overwrite existing file unless `--force` set.
//! - Output file mode `0600` on Unix; documented Windows caveat.
//!
//! The binary emits the JSON manually (rather than
//! `serde_json::to_string(&Session)`) so the `access_token` field
//! is included in the on-disk config — the adapter's `MatrixConfig`
//! marks that field `#[serde(skip_serializing)]` to prevent the
//! adapter from rewriting it.

use crate::cli::OutputArgs;
use crate::error::{OnboardError, Result};
use octo_matrix_onboard_core::Session;
use std::path::{Path, PathBuf};

#[cfg(unix)]
use std::os::unix::fs::OpenOptionsExt;

/// Resolve the default output path via `dirs::config_dir()`.
pub fn default_path() -> Result<PathBuf> {
    let mut base = dirs::config_dir()
        .ok_or_else(|| OnboardError::BadConfig("could not determine config directory".into()))?;
    base.push("octo");
    base.push("matrix.json");
    Ok(base)
}

/// Write the captured session to the chosen sink.
///
/// At this point the on-disk JSON is built manually so the
/// `access_token` field is preserved (the adapter's `MatrixConfig`
/// marks it `#[serde(skip_serializing)]` to prevent the adapter from
/// rewriting it back). `logging::redact_json` is applied to the
/// in-memory copy **only for log messages** — the on-disk config
/// MUST keep the real token (the adapter needs it on next start).
pub fn write(args: &OutputArgs, session: &Session) -> Result<()> {
    let json = serde_json::json!({
        "homeserver_url": session.homeserver_url,
        "user_id": session.user_id,
        "device_id": session.device_id,
        "access_token": session.access_token,
        "refresh_token": session.refresh_token,
        "rooms": Vec::<String>::new(),
    });

    if args.stdout {
        let text =
            serde_json::to_string_pretty(&json).map_err(|e| OnboardError::Generic(e.into()))?;
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

fn write_atomic(path: &Path, json: &serde_json::Value) -> Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| {
            OnboardError::BadConfig(format!("create_dir_all({}): {}", parent.display(), e))
        })?;
    }

    let text = serde_json::to_string_pretty(json).map_err(|e| OnboardError::Generic(e.into()))?;

    // Write to a sibling tmp file, then rename onto the target.
    // Atomic on POSIX; on Windows, rename-over-existing fails — we
    // delete first when --force is set (caller handles that).
    let tmp = path.with_extension("json.tmp");
    {
        let mut opts = std::fs::OpenOptions::new();
        opts.write(true).create(true).truncate(true);
        #[cfg(unix)]
        {
            opts.mode(0o600);
        }
        let mut f = opts
            .open(&tmp)
            .map_err(|e| OnboardError::BadConfig(format!("open({}): {}", tmp.display(), e)))?;
        use std::io::Write;
        f.write_all(text.as_bytes())
            .and_then(|_| f.write_all(b"\n"))
            .map_err(|e| OnboardError::BadConfig(format!("write({}): {}", tmp.display(), e)))?;
        f.sync_all()
            .map_err(|e| OnboardError::BadConfig(format!("sync({}): {}", tmp.display(), e)))?;
    }

    // On Windows we may need to remove the destination first.
    #[cfg(not(unix))]
    if path.exists() {
        std::fs::remove_file(path).ok();
    }

    std::fs::rename(&tmp, path).map_err(|e| {
        OnboardError::BadConfig(format!(
            "rename({} -> {}): {}",
            tmp.display(),
            path.display(),
            e
        ))
    })?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn sample_session() -> Session {
        Session {
            homeserver_url: "https://matrix.example.com".into(),
            user_id: "@bot:matrix.example.com".into(),
            device_id: "ABCDEFGHIJ".into(),
            access_token: "syt_abcdefgh_long".into(),
            refresh_token: Some("syr_xyz_123".into()),
        }
    }

    #[test]
    fn write_to_temp_file_creates_parent_dirs_and_preserves_token() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("subdir/matrix.json");
        let args = OutputArgs {
            out: Some(path.clone()),
            stdout: false,
            force: false,
        };
        write(&args, &sample_session()).unwrap();
        let text = std::fs::read_to_string(&path).unwrap();
        assert!(
            text.contains("syt_abcdefgh_long"),
            "access_token must be preserved on disk"
        );
        assert!(
            text.contains("syr_xyz_123"),
            "refresh_token must be preserved on disk"
        );
    }

    #[test]
    fn refuse_overwrite_without_force() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("matrix.json");
        std::fs::write(&path, "{}").unwrap();
        let args = OutputArgs {
            out: Some(path.clone()),
            stdout: false,
            force: false,
        };
        let err = write(&args, &sample_session()).unwrap_err();
        match err {
            OnboardError::BadConfig(msg) => {
                assert!(msg.contains("refusing to overwrite"));
            }
            other => panic!("expected BadConfig, got {:?}", other),
        }
    }

    #[test]
    fn force_overwrites_existing() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("matrix.json");
        std::fs::write(&path, "{}").unwrap();
        let args = OutputArgs {
            out: Some(path.clone()),
            stdout: false,
            force: true,
        };
        write(&args, &sample_session()).unwrap();
        let text = std::fs::read_to_string(&path).unwrap();
        assert!(text.contains("syt_abcdefgh_long"));
    }

    #[cfg(unix)]
    #[test]
    fn file_mode_is_0600() {
        use std::os::unix::fs::PermissionsExt;
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("matrix.json");
        let args = OutputArgs {
            out: Some(path.clone()),
            stdout: false,
            force: false,
        };
        write(&args, &sample_session()).unwrap();
        let perms = std::fs::metadata(&path).unwrap().permissions().mode() & 0o777;
        assert_eq!(perms, 0o600, "expected 0600, got {:o}", perms);
    }
}
