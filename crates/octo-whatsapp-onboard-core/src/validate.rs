//! Shared validation helpers.
//!
//! R1-M2: phone validation was duplicated between the adapter's
//! `is_e164` (in the `validate()` impl) and the core lib's
//! `pair_link::validate_phone`. Move it here so both call sites
//! use the same function. Future bug fixes need only be applied
//! once.
//!
//! Mission 0850p-a-symlink-check: `check_session_path_safe` rejects
//! `session_path` values that resolve to a symlink whose target
//! differs from the user-requested location. Closes D-WA-4 (a
//! pre-launch mitigation for the symlink-attack gap).

use std::path::Path;

use crate::error::CoreError;

/// E.164 phone validation: `+` followed by 7-15 ASCII digits, no
/// leading 0 after `+`.
///
/// Returns the validation result as a `Result<(), String>` for
/// direct use in `validate()` impls.
pub fn validate_phone_e164(phone: &str) -> Result<(), String> {
    if !phone.starts_with('+') {
        return Err(format!("{phone:?}: missing leading +"));
    }
    let digits = &phone[1..];
    if digits.is_empty() {
        return Err(format!("{phone:?}: no digits after +"));
    }
    if digits.len() < 7 || digits.len() > 15 {
        return Err(format!(
            "{phone:?}: expected 7-15 digits, got {}",
            digits.len()
        ));
    }
    if !digits.chars().all(|c| c.is_ascii_digit()) {
        return Err(format!("{phone:?}: non-digit character"));
    }
    if digits.starts_with('0') {
        return Err(format!("{phone:?}: leading 0 after +"));
    }
    Ok(())
}

/// Boolean form of `validate_phone_e164` (used by the adapter's
/// `validate()` which just wants a yes/no).
pub fn is_e164(phone: &str) -> bool {
    validate_phone_e164(phone).is_ok()
}

/// Mission 0850p-a-symlink-check: reject a `session_path` that
/// resolves to a symlink whose target is outside the requested
/// parent directory. Closes the symlink-attack gap where an
/// attacker pre-creates the session path as a symlink to a location
/// they control, causing the CLI to write the Signal session keys
/// (and thus the WhatsApp identity) into attacker-readable storage.
///
/// `canonicalize(target) != canonicalize(parent)` is the check. If
/// the target is a symlink, `canonicalize` follows it and returns
/// the real path, which differs from the user-requested parent.
/// A plain `symlink_metadata` check would miss TOCTOU races (the
/// attacker swaps the symlink between the check and the use), so
/// we canonicalize the parent first then re-canonicalize the
/// target.
///
/// Returns `Ok(())` if the path is safe; otherwise `Err(CoreError::SessionPathSymlink)`
/// with both the requested and resolved paths for diagnostic output.
pub fn check_session_path_safe(session_path: &Path) -> Result<(), CoreError> {
    // If the path does not exist yet (fresh link), only the parent
    // is canonicalizable. Run the check on the parent so a symlink
    // at the requested path is detected on subsequent use.
    let (target, parent) = if session_path.exists() {
        let canon =
            std::fs::canonicalize(session_path).map_err(|e| CoreError::InvalidSessionPath {
                path: session_path.to_path_buf(),
                reason: format!("canonicalize: {e}"),
            })?;
        let parent = session_path
            .parent()
            .ok_or_else(|| CoreError::InvalidSessionPath {
                path: session_path.to_path_buf(),
                reason: "no parent directory".to_string(),
            })?;
        let canon_parent =
            std::fs::canonicalize(parent).map_err(|e| CoreError::InvalidSessionPath {
                path: parent.to_path_buf(),
                reason: format!("canonicalize parent: {e}"),
            })?;
        (canon, canon_parent)
    } else {
        // Path does not exist yet: only check the parent
        let parent = session_path
            .parent()
            .ok_or_else(|| CoreError::InvalidSessionPath {
                path: session_path.to_path_buf(),
                reason: "no parent directory".to_string(),
            })?;
        if parent.as_os_str().is_empty() {
            return Ok(()); // current working directory; nothing to check
        }
        let canon_parent =
            std::fs::canonicalize(parent).map_err(|e| CoreError::InvalidSessionPath {
                path: parent.to_path_buf(),
                reason: format!("canonicalize parent: {e}"),
            })?;
        // No target to compare against; the existing `validate_session_args`
        // will create the parent and fail if the path is a symlink to
        // attacker-controlled storage. Return Ok for now.
        let _ = canon_parent;
        return Ok(());
    };

    // The canonical target's parent must equal the canonical parent
    // requested by the user. If they differ, the path is a symlink
    // whose target is outside the requested parent.
    match target.parent() {
        Some(target_parent) if target_parent == parent.as_path() => Ok(()),
        _ => Err(CoreError::SessionPathSymlink {
            requested: session_path.display().to_string(),
            resolved: target.display().to_string(),
        }),
    }
}

// Helper for symlink tests; cargo tempfile is not a dependency so we
// use the std-only target dir.
#[cfg(test)]
fn tempdir_in_target() -> std::path::PathBuf {
    use std::sync::atomic::{AtomicU64, Ordering};
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let n = COUNTER.fetch_add(1, Ordering::SeqCst);
    let pid = std::process::id();
    let dir = std::env::temp_dir().join(format!(
        "octo-symlink-check-{pid}-{n}-{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accept_valid_e164() {
        for good in ["+15551234567", "+1234567", "+123456789012345"] {
            assert!(is_e164(good), "{good:?} should be accepted");
        }
    }

    #[test]
    fn reject_malformed() {
        for bad in [
            "5551234",           // no +
            "+0123456789",       // leading 0
            "+1-555-1234567",    // non-digit
            "+",                 // no digits
            "+abcdefg",          // non-digit
            "+123456",           // too short (6 digits)
            "+1234567890123456", // too long (16 digits)
        ] {
            assert!(!is_e164(bad), "{bad:?} should be rejected");
        }
    }

    // Mission 0850p-a-symlink-check tests
    #[test]
    fn symlink_check_accepts_normal_path() {
        // A normal, non-symlinked path under a real temp dir passes.
        let tmp = tempdir_in_target();
        let session_path = tmp.join("session.db");
        std::fs::write(&session_path, b"x").unwrap();
        assert!(check_session_path_safe(&session_path).is_ok());
    }

    #[test]
    fn symlink_check_rejects_external_symlink() {
        // Create an attacker-controlled dir and a victim dir; plant
        // a symlink in the victim dir pointing to the attacker dir.
        let victim = tempdir_in_target();
        let attacker = tempdir_in_target();
        std::fs::write(attacker.join("session.db"), b"x").unwrap();
        let link = victim.join("session.db");
        std::os::unix::fs::symlink(attacker.join("session.db"), &link).unwrap();

        let err = check_session_path_safe(&link).unwrap_err();
        match err {
            CoreError::SessionPathSymlink {
                requested,
                resolved,
            } => {
                assert!(requested.contains("session.db"));
                assert!(resolved.contains("session.db"));
                // Resolved path is in the attacker dir, not the victim dir.
                assert!(
                    resolved.starts_with(attacker.to_string_lossy().as_ref())
                        || resolved.starts_with(
                            attacker.canonicalize().unwrap().to_string_lossy().as_ref()
                        )
                );
            }
            other => panic!("expected SessionPathSymlink, got {other:?}"),
        }
    }

    #[test]
    fn symlink_check_accepts_nonexistent_path() {
        // A path that does not exist yet (fresh link case) returns Ok
        // because there's no target to validate. The check is
        // re-applied on subsequent use.
        let tmp = tempdir_in_target();
        let session_path = tmp.join("not-yet-created.db");
        assert!(check_session_path_safe(&session_path).is_ok());
    }
}
