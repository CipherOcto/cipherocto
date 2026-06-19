//! `session_meta.json` sidecar writing.
//!
//! R5-M2: sidecar is **required**, not an optimization. Written
//! immediately after `wait_for_connected` returns `Ok`, **before** the
//! config JSON write. If the sidecar write fails, the link fails with
//! `CoreError::Adapter` — the operator should not get a "linked" exit
//! 0 if the metadata is missing, because `session list` would then
//! have to fall back to 5s bot startup per session.
//!
//! R2-M1: `linked_at` is formatted via `crate::time::format_rfc3339_secs`
//! in the 20-char no-subsec format `YYYY-MM-DDTHH:MM:SSZ`.
//!
//! R3-H1: pinned via a regex test `^\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}Z$`.

use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::error::{CoreError, Result};
use crate::output::WhatsAppSession;
use crate::time::now_as_rfc3339_secs;

/// On-disk sidecar shape.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionMeta {
    /// Bot's own phone number (digits-only, no `+` prefix).
    /// `None` if the device snapshot wasn't yet persisted.
    pub self_phone: Option<String>,
    /// RFC 3339 UTC `YYYY-MM-DDTHH:MM:SSZ`.
    pub linked_at: String,
    /// `"qr-link"` or `"pair-link"`.
    pub mode: String,
    pub groups: Vec<String>,
}

impl SessionMeta {
    /// Build the sidecar JSON for a `qr-link` success.
    pub fn for_qr_link(session: &WhatsAppSession) -> Self {
        Self {
            self_phone: session.self_phone.clone(),
            linked_at: now_as_rfc3339_secs(),
            mode: "qr-link".to_string(),
            groups: session.groups.clone(),
        }
    }

    /// Build the sidecar JSON for a `pair-link` success.
    pub fn for_pair_link(session: &WhatsAppSession) -> Self {
        Self {
            self_phone: session.self_phone.clone(),
            linked_at: now_as_rfc3339_secs(),
            mode: "pair-link".to_string(),
            groups: session.groups.clone(),
        }
    }
}

/// Write the `session_meta.json` sidecar next to the stoolap DB.
///
/// Atomic write via `tempfile::NamedTempFile` + `persist`. Mode 0600
/// on Unix (contains the resolved phone number; not a secret but
/// still PII).
///
/// R5-M2: if the write fails, the link fails with `CoreError::Adapter`.
pub fn write_sidecar(
    session_path: &Path,
    session: &WhatsAppSession,
    mode: SidecarMode,
) -> Result<()> {
    let sidecar = match mode {
        SidecarMode::QrLink => SessionMeta::for_qr_link(session),
        SidecarMode::PairLink => SessionMeta::for_pair_link(session),
    };

    let parent = session_path
        .parent()
        .ok_or_else(|| CoreError::InvalidSessionPath {
            path: session_path.to_path_buf(),
            reason: "session_path has no parent directory".to_string(),
        })?;

    std::fs::create_dir_all(parent).map_err(|e| {
        CoreError::Adapter(anyhow::anyhow!(
            "create_dir_all({}): {}",
            parent.display(),
            e
        ))
    })?;

    let sidecar_path = sidecar_path_for(session_path);
    let json = serde_json::to_string_pretty(&sidecar)
        .map_err(|e| CoreError::Adapter(anyhow::anyhow!("serialize sidecar: {e}")))?;

    write_atomic(&sidecar_path, json.as_bytes()).map_err(|e| {
        CoreError::Adapter(anyhow::anyhow!(
            "write sidecar {}: {}",
            sidecar_path.display(),
            e
        ))
    })?;

    Ok(())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SidecarMode {
    QrLink,
    PairLink,
}

/// Compute the sidecar path: `<session_path>.meta.json` next to the
/// stoolap DB. e.g., `default.session.db` -> `default.session.db.meta.json`.
fn sidecar_path_for(session_path: &Path) -> std::path::PathBuf {
    let mut p = session_path.as_os_str().to_owned();
    p.push(".meta.json");
    std::path::PathBuf::from(p)
}

fn write_atomic(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    use std::io::Write;
    let mut tmp = tempfile::NamedTempFile::new_in(
        path.parent()
            .filter(|p| !p.as_os_str().is_empty())
            .unwrap_or_else(|| std::path::Path::new(".")),
    )?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let perms = std::fs::Permissions::from_mode(0o600);
        std::fs::set_permissions(tmp.path(), perms)?;
    }
    tmp.write_all(bytes)?;
    tmp.write_all(b"\n")?;
    tmp.as_file().sync_all()?;
    tmp.persist(path)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, format!("persist: {e}")))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn sample_session() -> WhatsAppSession {
        WhatsAppSession {
            self_phone: Some("15551234567".to_string()),
            session_path: PathBuf::from("/tmp/octo-whatsapp-test/session.db"),
            groups: vec!["120363012345678901@g.us".to_string()],
            pair_phone: None,
        }
    }

    #[test]
    fn sidecar_path_for_appends_meta_json() {
        let p = sidecar_path_for(Path::new("/tmp/session.db"));
        assert_eq!(p, PathBuf::from("/tmp/session.db.meta.json"));
    }

    #[test]
    fn write_sidecar_creates_file_with_0600_mode() {
        let tmp = tempfile::TempDir::new().unwrap();
        let session_path = tmp.path().join("test.session.db");
        std::fs::write(&session_path, b"").unwrap();

        let session = sample_session();
        write_sidecar(&session_path, &session, SidecarMode::QrLink).unwrap();

        let _sidecar_path = session_path.with_extension("session.db.meta.json");
        // The sidecar path is "<session_path>.meta.json", e.g.,
        // "test.session.db.meta.json"
        let sidecar_path = tmp.path().join("test.session.db.meta.json");
        assert!(
            sidecar_path.exists(),
            "sidecar should exist at {sidecar_path:?}"
        );

        let bytes = std::fs::read(&sidecar_path).unwrap();
        let text = std::fs::read_to_string(&sidecar_path).unwrap();
        assert!(!bytes.is_empty());

        // R3-M1: linked_at format is RFC 3339 UTC no-subsec
        let parsed: SessionMeta = serde_json::from_str(&text).unwrap();
        assert_eq!(parsed.mode, "qr-link");
        assert_eq!(parsed.self_phone.as_deref(), Some("15551234567"));
        assert_eq!(parsed.groups, vec!["120363012345678901@g.us".to_string()]);
        // regex pin: YYYY-MM-DDTHH:MM:SSZ (20 chars)
        let re = regex_lite_match();
        assert!(
            re.is_match(&parsed.linked_at),
            "linked_at {:?} doesn't match YYYY-MM-DDTHH:MM:SSZ",
            parsed.linked_at
        );

        // R2-M1: 0600 mode on Unix
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let perms = std::fs::metadata(&sidecar_path).unwrap().permissions();
            assert_eq!(perms.mode() & 0o777, 0o600);
        }
    }

    /// Tiny regex matcher (avoids a `regex` crate dep just for the
    /// one YYYY-MM-DDTHH:MM:SSZ test).
    fn regex_lite_match() -> LinkedAtPattern {
        LinkedAtPattern
    }

    struct LinkedAtPattern;
    impl LinkedAtPattern {
        fn is_match(&self, s: &str) -> bool {
            if s.len() != 20 {
                return false;
            }
            let b = s.as_bytes();
            // YYYY-MM-DDTHH:MM:SSZ
            b[4] == b'-'
                && b[7] == b'-'
                && b[10] == b'T'
                && b[13] == b':'
                && b[16] == b':'
                && b[19] == b'Z'
                && b[0..4].iter().all(u8::is_ascii_digit)
                && b[5..7].iter().all(u8::is_ascii_digit)
                && b[8..10].iter().all(u8::is_ascii_digit)
                && b[11..13].iter().all(u8::is_ascii_digit)
                && b[14..16].iter().all(u8::is_ascii_digit)
                && b[17..19].iter().all(u8::is_ascii_digit)
        }
    }

    #[test]
    fn write_sidecar_pair_link_mode() {
        let tmp = tempfile::TempDir::new().unwrap();
        let session_path = tmp.path().join("test.session.db");
        std::fs::write(&session_path, b"").unwrap();

        let session = sample_session();
        write_sidecar(&session_path, &session, SidecarMode::PairLink).unwrap();

        let sidecar_path = tmp.path().join("test.session.db.meta.json");
        let parsed: SessionMeta =
            serde_json::from_str(&std::fs::read_to_string(&sidecar_path).unwrap()).unwrap();
        assert_eq!(parsed.mode, "pair-link");
    }

    #[test]
    fn write_sidecar_failure_returns_adapter_error() {
        // R5-M2: sidecar write failure returns CoreError::Adapter.
        // Use a path whose parent doesn't exist and can't be created.
        let bad_path = PathBuf::from("/nonexistent/octowhatsapp/that/cannot/be/created/session.db");
        let session = sample_session();
        let result = write_sidecar(&bad_path, &session, SidecarMode::QrLink);
        match result {
            Err(CoreError::Adapter(_)) => {} // expected
            Err(other) => panic!("expected CoreError::Adapter, got {other:?}"),
            Ok(()) => panic!("expected error"),
        }
    }
}
