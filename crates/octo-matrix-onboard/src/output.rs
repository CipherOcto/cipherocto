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
/// The on-disk JSON is built by `Session::to_disk_json` (R1-L1) so
/// the `access_token` field is preserved (the adapter's
/// `MatrixConfig` marks it `#[serde(skip_serializing)]` to prevent
/// the adapter from rewriting it back). The on-disk config MUST
/// keep the real token (the adapter needs it on next start); token
/// redaction is the logging layer's job (see
/// `crate::logging::RedactingFormat`), not the writer's.
pub fn write(args: &OutputArgs, session: &Session) -> Result<()> {
    let json = session.to_disk_json();

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

    // R1-L5: use a `tempfile::NamedTempFile` instead of the
    // previous `path.with_extension("json.tmp")` sibling. The
    // previous approach produced a `matrix.json.tmp.tmp` for
    // `--out matrix.json.tmp` (cosmetic, not a correctness
    // issue) and, more importantly, leaked the tmp file on
    // crashes between the open and the rename (the file wasn't
    // cleaned up on drop). `NamedTempFile` ties the tmp file's
    // lifetime to a handle that auto-cleans on drop.
    use std::io::Write;
    let mut tmp = tempfile::NamedTempFile::new_in(
        path.parent()
            .filter(|p| !p.as_os_str().is_empty())
            .unwrap_or_else(|| std::path::Path::new(".")),
    )
    .map_err(|e| OnboardError::BadConfig(format!("create tmp: {}", e)))?;

    // Match the previous 0600 mode on Unix. The temp file is
    // created in `path.parent()` so the rename stays on the
    // same filesystem (preserving atomicity on POSIX).
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
    use tempfile::TempDir;

    fn sample_session() -> Session {
        Session::new(
            "https://matrix.example.com".into(),
            "@bot:matrix.example.com".into(),
            "ABCDEFGHIJ".into(),
            "syt_abcdefgh_long".into(),
            Some("syr_xyz_123".into()),
        )
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

    #[test]
    fn write_with_relative_bare_filename_succeeds() {
        // R1-M9: `Path::parent()` of a bare filename like `matrix.json`
        // returns `Some("")` on Linux. The `if let Some(parent) = path.parent()`
        // guard in `write_atomic` means we skip `create_dir_all("")`
        // (which would otherwise return `InvalidInput`). This test
        // pins down the contract: a bare-filename path under a
        // tempdir's cwd writes successfully, and the file lands in
        // that cwd.
        let dir = TempDir::new().unwrap();
        let cwd = dir.path().to_path_buf();
        let args = OutputArgs {
            out: Some(PathBuf::from("matrix.json")),
            stdout: false,
            force: false,
        };
        // Run the write with the tempdir as cwd so the bare
        // `matrix.json` lands inside it (and gets cleaned up with
        // the tempdir).
        let prev_cwd = std::env::current_dir().unwrap();
        std::env::set_current_dir(&cwd).unwrap();
        let result = write(&args, &sample_session());
        std::env::set_current_dir(&prev_cwd).unwrap();
        result.expect("bare-filename path should write successfully");
        let text = std::fs::read_to_string(cwd.join("matrix.json")).unwrap();
        assert!(text.contains("syt_abcdefgh_long"));
    }
}
