//! Re-export of the canonical filesystem permission primitives.
//!
//! The implementations moved to `cipherocto-fs-perms` (2026-10-05). They
//! started here because the wallet was their only caller; H-6 and H-7 added
//! two callers in crates that deliberately have no `octo-*` dependency, and
//! the choice was between an upward dep, a private re-implementation, or one
//! shared home. The re-export keeps every existing wallet call site and the
//! source assertions in the test module below working unchanged, while the
//! definition itself now exists exactly once.
//!
//! `pub use` rather than `pub(crate)` is unchanged from before: the
//! `octo-wallet` binary is a separate crate target and cannot reach a
//! `pub(crate)` item.

pub use cipherocto_fs_perms::{create_dir_private, create_private, sync_parent_dir};

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
