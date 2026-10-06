//! Filesystem permission primitives shared by every secret-bearing writer.
//!
//! **Why this is its own crate.** These primitives started life inside
//! `octo-wallet::fs_perms` with four call sites, all in the wallet. Two more
//! call sites then turned up outside it: the WhatsApp CDP session dumps (H-6)
//! and the multi-account session export (H-7). A wallet-local module cannot
//! serve those without either an upward dependency from a crate that
//! deliberately has none, or a private re-implementation — and a private
//! re-implementation is precisely how the same rule drifts into two places and
//! only one of them gets fixed when the umask assumption changes.
//!
//! So the primitive is promoted here, and the wallet re-exports it. There is
//! one definition, reachable from every writer, with no upward dependencies.
//!
//! **The original warning, preserved because it is still live.** The wallet
//! module doc recorded that when these primitives covered three secret-bearing
//! files, the module doc enumerated exactly those files, and a later review
//! took the enumeration as the set. The slots *directory* was therefore never
//! converted. Deleting this crate's module docs is how that mistake gets
//! repeated. When a caller is added, the caller is what the docs have to keep
//! describing; the docs are not evidence about which callers exist.
//!
//! Layering: this crate has NO dependencies, in either direction. It is a
//! substrate primitive, not a helper owned by any one consumer.

use std::fs::File;
use std::io;
use std::path::Path;

/// Create or truncate `path` with mode `0o600` **from birth**.
///
/// `std::fs::File::create` applies the process umask, so the file is born at
/// `0o666 & !umask` — `0o664` under this repository's `0002` umask, which is
/// group- and world-readable. A caller that chmods after the fact has
/// therefore placed the file at a readable mode in between, and if the process
/// dies inside that window the readable mode is what survives on disk.
///
/// That window is the exposure this primitive exists to close.
///
/// `OpenOptions::mode` is itself masked by the umask, so an unusually
/// restrictive umask that strips owner bits would still produce a too-tight
/// file. Callers therefore KEEP their post-rename `set_permissions` arm: it is
/// redundant on any normal umask and correcting on a hostile one.
///
/// # Errors
///
/// Propagates any I/O error from opening the file or applying its mode.
#[cfg(unix)]
pub fn create_private(path: &Path) -> io::Result<File> {
    use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
    let f = std::fs::OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .mode(0o600)
        .open(path)?;
    // `OpenOptionsExt::mode` applies ONLY at create time. If the path already
    // exists — a `.tmp` left behind at a permissive mode by a build predating
    // this primitive — the truncate above reuses the old inode and its old
    // mode, and the file would stay group-readable. Force the mode on the way
    // out so the name `create_private` is true unconditionally rather than
    // only for paths that did not exist.
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600))?;
    Ok(f)
}

/// Non-Unix has no POSIX mode to request, so plain creation is the whole
/// contract. The callers' post-rename arms are themselves `#[cfg(unix)]`, so
/// behaviour off Unix is unchanged.
#[cfg(not(unix))]
pub fn create_private(path: &Path) -> io::Result<File> {
    std::fs::File::create(path)
}

/// Create `path` and its missing parents, private **from birth**.
///
/// The directory twin of [`create_private`]. `std::fs::create_dir_all` applies
/// the process umask to every component it creates — measured under this
/// repository's `0002` umask, a directory is born `0o775`, group- and
/// world-traversable. A later `chmod` narrows it, but in between, the
/// directory listing is readable by any local user.
#[cfg(unix)]
pub fn create_dir_private(path: &Path) -> io::Result<()> {
    use std::os::unix::fs::PermissionsExt;
    // `create_dir_all` succeeds if the directory ALREADY exists, and in that
    // case it does nothing at all — no mode is applied. So the force below is
    // not redundant: it is what makes the name true for a directory left
    // permissive by an older build.
    std::fs::create_dir_all(path)?;
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o700))?;
    Ok(())
}

/// Non-Unix fallback: plain creation. Mirrors [`create_private`]'s split.
#[cfg(not(unix))]
pub fn create_dir_private(path: &Path) -> io::Result<()> {
    std::fs::create_dir_all(path)
}

/// Make the rename that published `path` durable across power loss.
///
/// `File::sync_all` on the temp file persists the DATA BLOCKS. `rename` creates
/// a DIRECTORY ENTRY, and a directory entry is only durable once the parent
/// directory itself has been synced.
///
/// HONEST LIMIT, and it is a real one: this is BEST-EFFORT. A filesystem that
/// refuses to open or sync a directory (some network and FUSE mounts) is not
/// treated as an error, because by this point the rename has already succeeded
/// and the bytes are correct. The guarantee is therefore "the entry is synced
/// on filesystems that support it", not "the entry is always synced". No test
/// can observe an `fsync`, so nothing here is pinned by a vector; that is
/// stated rather than papered over with a source-grep that would only pin a
/// spelling.
#[cfg(unix)]
pub fn sync_parent_dir(path: &Path) {
    if let Some(parent) = path.parent() {
        if let Ok(dir) = std::fs::File::open(parent) {
            let _ = dir.sync_all();
        }
    }
}

/// Windows has no directory handle to sync, so the durability caveat is
/// Unix-only and this is the whole contract off it.
#[cfg(not(unix))]
pub fn sync_parent_dir(_path: &Path) {}

#[cfg(test)]
mod tests {
    use super::{create_dir_private, create_private};
    use std::io::Write;

    /// The primitive must yield `0o600` with no chmod anywhere in the call,
    /// and must correct rather than inherit a pre-existing permissive file.
    #[cfg(unix)]
    #[test]
    fn tv_fs_perms_create_private_is_private_from_birth() {
        use std::os::unix::fs::PermissionsExt;

        let dir = tempfile::tempdir().expect("tempdir");
        let probe = dir.path().join("probe.tmp");
        let mut f = create_private(&probe).expect("create_private");
        f.write_all(b"sealed").expect("write");
        drop(f);
        let mode = std::fs::metadata(&probe)
            .expect("metadata")
            .permissions()
            .mode()
            & 0o777;
        assert_eq!(
            mode, 0o600,
            "create_private must yield 0o600 with no chmod in the call, got {mode:o}"
        );

        // `OpenOptionsExt::mode` applies only at create time, so without the
        // post-open force a file left at 0o664 by an older build is truncated
        // in place and STAYS group-readable.
        let stale = dir.path().join("stale.tmp");
        std::fs::write(&stale, b"left behind").expect("seed stale file");
        std::fs::set_permissions(&stale, std::fs::Permissions::from_mode(0o664))
            .expect("widen stale file");
        let mut f = create_private(&stale).expect("create_private over stale");
        f.write_all(b"resealed").expect("write");
        drop(f);
        let stale_mode = std::fs::metadata(&stale)
            .expect("metadata")
            .permissions()
            .mode()
            & 0o777;
        assert_eq!(
            stale_mode, 0o600,
            "create_private must correct a pre-existing permissive file rather than \
             inherit its mode, got {stale_mode:o}"
        );
    }

    #[cfg(unix)]
    #[test]
    fn tv_fs_perms_create_dir_private_covers_existing_and_new() {
        use std::os::unix::fs::PermissionsExt;

        let dir = tempfile::tempdir().expect("tempdir");
        let fresh = dir.path().join("fresh");
        create_dir_private(&fresh).expect("create_dir_private");
        assert_eq!(
            std::fs::metadata(&fresh)
                .expect("metadata")
                .permissions()
                .mode()
                & 0o777,
            0o700,
            "a newly created parent must be 0700"
        );

        // `create_dir_all` is a no-op when the path exists, so the mode of a
        // directory left permissive by an older build is only corrected by the
        // explicit force.
        let existing = dir.path().join("existing");
        std::fs::create_dir_all(&existing).expect("seed");
        std::fs::set_permissions(&existing, std::fs::Permissions::from_mode(0o777)).expect("widen");
        create_dir_private(&existing).expect("create_dir_private over existing");
        assert_eq!(
            std::fs::metadata(&existing)
                .expect("metadata")
                .permissions()
                .mode()
                & 0o777,
            0o700,
            "an existing permissive directory must be tightened, not left alone"
        );
    }
}
