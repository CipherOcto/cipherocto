//! Filesystem permission primitives shared by the substrate's writers.
//!
//! One primitive, four callers: the sealed seed slot (`vault`), the
//! index (`identity_store`), the keystore (`keystore`), and the
//! `octo-wallet` binary's `init` seed export. They each write a
//! secret-bearing file, and each needs it to be `0o600` at every
//! instant it exists.
//!
//! The binary is the odd one out: it writes DIRECTLY, with no temp
//! path and no rename, so it has no rename to make durable and needs
//! only `create_private`. It is listed here anyway, because it is the
//! only writer of the RAW MASTER SEED - the other three write
//! ciphertext, the index, or the keystore.

use std::fs::File;
use std::io;
use std::path::Path;

/// Create or truncate `path` with mode `0o600` **from birth**.
///
/// `std::fs::File::create` applies the process umask, so the file is
/// born at `0o666 & !umask` - `0o664` under this repository's `0002`
/// umask, which is group- and world-readable. A caller that chmods
/// after the rename has therefore placed the file at a readable mode
/// in between, and if the process dies inside that window the
/// readable mode is what survives on disk.
///
/// That window is the exposure this primitive exists to close. For
/// the sealed seed ciphertext, the index, and the keystore it is
/// exactly what the store's own risk table names: the material
/// becomes readable by every local user, and the exposure is not
/// recoverable after the fact.
///
/// `OpenOptions::mode` is itself masked by the umask, so an unusually
/// restrictive umask that strips owner bits (`0600`) would still
/// produce a too-tight file. Callers therefore KEEP their post-rename
/// `set_permissions` arm: it is redundant on any normal umask and
/// correcting on a hostile one.
///
/// The Layer C CLI's `agent::write_token_file` already writes its
/// token this way, and its doc comment names this the
/// substrate-faithful contract. This is the same primitive on the
/// substrate side, so the two cannot drift apart silently.
///
/// `pub` rather than `pub(crate)` because the `octo-wallet` binary is
/// a SEPARATE crate target and cannot reach a `pub(crate)` item. It
/// is the only writer of the raw master seed, so it needs this
/// primitive; giving it a private inline `OpenOptions` instead would
/// be a second copy of the same rule, which is how the two halves
/// drift apart in the first place.
#[cfg(unix)]
pub fn create_private(path: &Path) -> io::Result<File> {
    use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
    let f = std::fs::OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .mode(0o600)
        .open(path)?;
    // `OpenOptionsExt::mode` applies ONLY at create time. If the path
    // already exists - a `.vault.tmp` left behind at a permissive mode
    // by a build predating this primitive - the truncate above reuses
    // the old inode and its old mode, and the file would stay
    // group-readable. Force the mode on the way out so the name
    // `create_private` is true unconditionally rather than only for
    // paths that did not exist. Cheap: one syscall on a path that is
    // about to be written and renamed anyway.
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600))?;
    Ok(f)
}

/// Non-Unix has no POSIX mode to request, so plain creation is the
/// whole contract. The callers' post-rename arms are themselves
/// `#[cfg(unix)]`, so behaviour off Unix is unchanged.
#[cfg(not(unix))]
pub fn create_private(path: &Path) -> io::Result<File> {
    std::fs::File::create(path)
}

/// Make the rename that published `path` durable across power loss.
///
/// `File::sync_all` on the temp file persists the DATA BLOCKS.
/// `rename` creates a DIRECTORY ENTRY, and a directory entry is only
/// durable once the parent directory itself has been synced. Without
/// this, `register` can return `Ok(())` having sealed the slot and
/// written the index, and a power cut can lose either or both
/// renames - independently and in either order. The state that
/// survives is then an index naming a DID whose sealed seed is gone,
/// which is the orphan shape the substrate has handling paths for,
/// arriving by a route those paths do not model.
///
/// The same repository already settled this: `octo-whatsapp`'s
/// atomic-write path syncs the parent directory and names the reason
/// in a comment. This is that call, on the substrate side.
///
/// HONEST LIMIT, and it is a real one: this is BEST-EFFORT. A
/// filesystem that refuses to open or sync a directory (some network
/// and FUSE mounts) is not treated as an error, because by this point
/// the rename has already succeeded and the bytes are correct -
/// returning `Err` would report a write that did in fact land, and
/// would make the store unusable on storage it can otherwise handle.
/// The guarantee is therefore "the entry is synced on filesystems
/// that support it", not "the entry is always synced". No test can
/// observe an `fsync`, so nothing here is pinned by a vector; that is
/// stated rather than papered over with a source-grep that would only
/// pin a spelling.
#[cfg(unix)]
pub fn sync_parent_dir(path: &Path) {
    if let Some(parent) = path.parent() {
        if let Ok(dir) = std::fs::File::open(parent) {
            let _ = dir.sync_all();
        }
    }
}

/// Windows has no directory handle to sync, so the durability
/// caveat is Unix-only and this is the whole contract off it.
#[cfg(not(unix))]
pub fn sync_parent_dir(_path: &Path) {}

#[cfg(test)]
mod tests {
    use super::create_private;
    use std::io::Write;

    /// Every secret-bearing writer creates its temp file at 0o600
    /// FROM BIRTH.
    ///
    /// Three files under the store root hold material the store's own
    /// risk table says must not be readable by other local users: the
    /// sealed seed slot, `store.json`, and the keystore. All three are
    /// written through a temp path and a rename. All three used to
    /// `File::create` that temp file - born at `0o666 & !umask`, which
    /// is 0o664 in this repository - and only chmod the FINAL path to
    /// 0600 after the rename. The window sat between the rename and
    /// the chmod, and it is widest for the sealed seed ciphertext,
    /// which is the one artifact the risk table is actually about.
    ///
    /// Two halves, because one would not do.
    ///
    /// The BEHAVIOURAL half checks the primitive with no chmod
    /// anywhere in sight, so it cannot pass by accident on the
    /// post-rename correction that the callers still carry.
    ///
    /// The SOURCE half is needed because the final mode was already
    /// 0600: both implementations agree on the end state and differ
    /// only on the path taken to it, so a test that reads the mode
    /// after the write cannot see the difference at all. HONEST LIMIT:
    /// this passes a rewrite that created the temp file readable and
    /// chmodded it BEFORE the rename. That rewrite is equally
    /// correct - its residual window is on a temp name no reader is
    /// told - so the limit does not hide a defect this vector should
    /// have caught.
    ///
    /// SECOND HONEST LIMIT, and this one is about the vector rather
    /// than the code. The source half matches the literal text
    /// `create_private(&tmp)`, so renaming the local `tmp` to
    /// anything else - behaviourally identical, still private from
    /// birth - fails this vector. Verified: renaming the local in
    /// `vault::put` fails exactly this one test. It is deliberately
    /// not loosened, because a wider pattern match is weaker at
    /// catching the real regression; but the failure mode to know
    /// about is a FALSE POSITIVE, so a red here means "the vector
    /// matched a spelling", not "the fix regressed". The behavioural
    /// half is the part that carries the property.
    #[cfg(unix)]
    #[test]
    fn tv_x_73_every_secret_writer_creates_its_temp_file_private() {
        use std::os::unix::fs::PermissionsExt;

        // Behavioural: the primitive is private with no correction.
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

        // Behavioural: a pre-existing permissive file is corrected,
        // not inherited. `OpenOptionsExt::mode` applies only at create
        // time, so without the post-open force a `.vault.tmp` left at
        // 0o664 by a build predating this primitive is truncated in
        // place and STAYS group-readable.
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

        // Source: the three temp-then-rename writers route the temp
        // file through it.
        let writers: &[(&str, &str)] = &[
            ("vault.rs", include_str!("vault.rs")),
            ("keystore.rs", include_str!("keystore.rs")),
            ("identity_store.rs", include_str!("identity_store.rs")),
        ];
        for (label, src) in writers {
            assert!(
                !src.contains("File::create(&tmp)"),
                "{label} creates its temp file with File::create, which applies the umask \
                 and leaves the secret readable from birth until a later chmod. Route it \
                 through fs_perms::create_private."
            );
            assert!(
                src.contains("create_private(&tmp)"),
                "{label} must create its temp file through fs_perms::create_private so the \
                 mode is 0o600 before the rename, not only after it."
            );
        }

        // Source: the binary, which writes DIRECTLY and is the only
        // writer of the raw master seed. It is a separate `[[bin]]`
        // crate target, so neither `cargo test --lib` nor the
        // substrate's own suite compiles it - the defect this half
        // pins is structurally invisible to every other check in the
        // crate, which is why it needs a source assertion rather than
        // a runtime one.
        let bin_src = include_str!("bin/octo-wallet.rs");
        assert!(
            !bin_src.contains("std::fs::write(&seed_out"),
            "bin/octo-wallet.rs writes the raw master seed with std::fs::write, which is \
             born at 0o666 & !umask and only corrected by the chmod that follows. Route it \
             through fs_perms::create_private."
        );
        assert!(
            bin_src.contains("create_private(&seed_out"),
            "bin/octo-wallet.rs must write the raw master seed through \
             fs_perms::create_private so it is 0o600 from birth."
        );
    }
}
