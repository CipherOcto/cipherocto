//! Secret-handling helpers for the WhatsApp Chrome session binaries.
//!
//! Every binary in this crate reads a Chrome profile that is **logged in to
//! WhatsApp Web** and writes out the session material it finds: Noise private
//! keys, `document.cookie`, localStorage entries classified `crypto`, and
//! captured AES import keys. Those writes used to be bare `std::fs::write`,
//! which creates files `0644` under the process umask, into `/tmp/wa-observer`
//! and `/tmp/idb-diff` by default.
//!
//! On a multi-user host that makes every dump world-readable, so any local
//! account can copy a live session without touching the profile at all.
//!
//! **The permission rule itself is not defined here.** It lives in
//! `cipherocto-fs-perms`, shared with the wallet and the session-export path.
//! An earlier revision of this file carried its own `write_private_file`; that
//! was a parallel abstraction — the exact failure mode `cipherocto-fs-perms`
//! was created to prevent — so it was deleted rather than kept. This module is
//! only the JSON convenience wrapper on top of it.
//!
//! H-6 in `docs/audits/2026-10-04-red-team-security-audit.md`. This closes the
//! *permissions* half of that finding. The remaining half — encrypting dumps
//! under the session-vault key, a consent flag on every diagnostic binary, and
//! shredding after use — is the design-sized part (CD-5 D2/D3) and is **not**
//! addressed here. Note also that these binaries still print key material to
//! stdout, so operator terminals and CI logs remain a disclosure path that a
//! file mode cannot address.

use std::path::PathBuf;

/// Parent directory for session material, private from birth.
///
/// `$XDG_RUNTIME_DIR` is `0700` by convention and is the correct home for
/// anything that lives only as long as the session. `/tmp` is world-writable
/// and sticky — it protects the *directory entries* of other users, not the
/// contents of a file you create in it, which is why a `0644` file there is a
/// disclosure.
#[must_use]
pub fn default_private_dir() -> PathBuf {
    std::env::var_os("XDG_RUNTIME_DIR")
        .map(PathBuf::from)
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or_else(|| PathBuf::from("/tmp/octo-wa-session"))
}

/// Write a JSON dump so that only its owner can read it.
///
/// Thin wrapper: the parent directory is made `0700` and the file is created
/// `0600` by `cipherocto_fs_perms`, which is the single definition of that
/// rule. This exists only so the call sites read as intent rather than as
/// three filesystem calls.
///
/// # Errors
///
/// Propagates serialization and I/O errors.
pub fn write_private_json<T: serde::Serialize>(
    path: &std::path::Path,
    value: &T,
) -> anyhow::Result<()> {
    use std::io::Write;

    if let Some(parent) = path.parent() {
        if !parent.as_os_str().is_empty() {
            cipherocto_fs_perms::create_dir_private(parent)?;
        }
    }
    let mut f = cipherocto_fs_perms::create_private(path)?;
    f.write_all(&serde_json::to_vec_pretty(value)?)?;
    Ok(())
}

/// Write raw bytes so that only its owner can read it.
///
/// The non-JSON sibling of [`write_private_json`]; the permission rule is the
/// shared one, for the same reason.
///
/// # Errors
///
/// Propagates I/O errors.
pub fn write_private_file(path: &std::path::Path, contents: &[u8]) -> std::io::Result<()> {
    use std::io::Write;

    if let Some(parent) = path.parent() {
        if !parent.as_os_str().is_empty() {
            cipherocto_fs_perms::create_dir_private(parent)?;
        }
    }
    let mut f = cipherocto_fs_perms::create_private(path)?;
    f.write_all(contents)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch(label: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "wa-private-{label}-{}-{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        dir
    }

    #[test]
    fn h6_dumps_are_not_world_readable() {
        let dir = scratch("mode");
        let path = dir.join("nested").join("captured-import-keys.json");
        write_private_json(&path, &serde_json::json!({"secret": true})).expect("write");

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let f = std::fs::metadata(&path).expect("stat file");
            assert_eq!(
                f.permissions().mode() & 0o777,
                0o600,
                "a session dump must be 0600, not umask-derived 0644"
            );
            let d = std::fs::metadata(path.parent().expect("parent")).expect("stat dir");
            assert_eq!(
                d.permissions().mode() & 0o777,
                0o700,
                "the dump's parent directory must be 0700"
            );
        }
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn h6_an_existing_loose_file_is_tightened_not_trusted() {
        // `open` preserves the mode of a pre-existing file, so a dump left by
        // an older run at 0644 must be rewritten to 0600 rather than merely
        // reopened.
        let dir = scratch("existing");
        std::fs::create_dir_all(&dir).expect("mkdir");
        let path = dir.join("stale.json");
        std::fs::write(&path, b"old").expect("seed");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o644))
                .expect("chmod 0644");
        }

        write_private_file(&path, b"new").expect("rewrite");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let f = std::fs::metadata(&path).expect("stat");
            assert_eq!(f.permissions().mode() & 0o777, 0o600);
        }
        assert_eq!(std::fs::read_to_string(&path).expect("read"), "new");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn h6_default_dir_is_not_bare_tmp() {
        let d = default_private_dir();
        assert!(
            !d.starts_with("/tmp/wa-observer") && !d.starts_with("/tmp/idb-diff"),
            "the default must not be the predictable /tmp path from H-6"
        );
        assert!(
            d.is_absolute(),
            "the default private dir must be absolute, got {d:?}"
        );
    }
}
