//! Filesystem permission primitives shared by the substrate's writers.
//!
//! One primitive, three callers: the sealed seed slot (`vault`), the
//! index (`identity_store`), and the keystore (`keystore`). They each
//! write a secret-bearing file through a temp path and a rename, and
//! each needs the file to be `0o600` at every instant it exists.

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
#[cfg(unix)]
pub(crate) fn create_private(path: &Path) -> io::Result<File> {
    use std::os::unix::fs::OpenOptionsExt;
    std::fs::OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .mode(0o600)
        .open(path)
}

/// Non-Unix has no POSIX mode to request, so plain creation is the
/// whole contract. The callers' post-rename arms are themselves
/// `#[cfg(unix)]`, so behaviour off Unix is unchanged.
#[cfg(not(unix))]
pub(crate) fn create_private(path: &Path) -> io::Result<File> {
    std::fs::File::create(path)
}

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

        // Source: all three writers route the temp file through it.
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
    }
}
