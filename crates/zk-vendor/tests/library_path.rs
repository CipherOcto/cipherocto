//! M-4: the `stwo-sys` shared-library path is a code-execution boundary.
//!
//! `CIPHEROCTO_STWO_LIB` selected which object `dlopen`s into the
//! wallet/router process. Because `stwo_verify` is what decides whether a ZK
//! proof is accepted, an attacker who could set that variable got arbitrary
//! code execution *and* a verifier that rubber-stamps every proof.
//!
//! These tests exercise `validate_library_path` directly rather than going
//! through `loaded_library()`, whose `OnceLock` cache is process-global and
//! therefore cannot be re-initialised per test.

use std::path::PathBuf;

use zk_vendor::{validate_library_path, LibraryPathError};

fn tmpdir(label: &str) -> PathBuf {
    let base = std::env::temp_dir().join(format!("zk-vendor-m4-{label}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&base);
    std::fs::create_dir_all(&base).expect("create temp dir");
    base
}

#[test]
fn m4_relative_path_is_refused() {
    // A cwd-relative path lets the effective path depend on the process's
    // working directory, which a caller may control.
    let err = validate_library_path(std::path::Path::new("libstwo_sys.so"))
        .expect_err("a relative library path must be refused");
    assert_eq!(
        err,
        LibraryPathError::NotAbsolute(PathBuf::from("libstwo_sys.so")),
        "relative path must fail the absolute-path rule"
    );
}

#[test]
fn m4_missing_path_is_refused() {
    let err = validate_library_path(std::path::Path::new("/nonexistent/libstwo_sys_test.so"))
        .expect_err("a missing library must be refused, not silently ignored");
    assert!(
        matches!(err, LibraryPathError::Missing { .. }),
        "missing path must fail as Missing, got {err:?}"
    );
}

#[test]
fn m4_directory_is_refused() {
    // A directory is not a library; dlopen-ing one is not an error we want to
    // discover at verification time.
    let dir = tmpdir("dir");
    let err = validate_library_path(&dir).expect_err("a directory must be refused");
    assert!(
        matches!(err, LibraryPathError::NotRegularFile { .. }),
        "directory must fail as NotRegularFile, got {err:?}"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn m4_world_writable_library_is_refused() {
    // The exact attacker primitive: a file the attacker can rewrite between
    // the validation and the next process start.
    let dir = tmpdir("world-writable");
    let lib = dir.join("libstwo_sys.so");
    std::fs::write(&lib, b"not really a library").expect("write stub");
    std::fs::set_permissions(&lib, std::os::unix::fs::PermissionsExt::from_mode(0o666))
        .expect("chmod 0666");

    let err = validate_library_path(&lib).expect_err("world-writable library must be refused");
    assert!(
        matches!(err, LibraryPathError::WritableByOthers { .. }),
        "world-writable library must fail the permission rule, got {err:?}"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn m4_group_writable_library_is_refused() {
    let dir = tmpdir("group-writable");
    let lib = dir.join("libstwo_sys.so");
    std::fs::write(&lib, b"not really a library").expect("write stub");
    std::fs::set_permissions(&lib, std::os::unix::fs::PermissionsExt::from_mode(0o664))
        .expect("chmod 0664");

    let err = validate_library_path(&lib).expect_err("group-writable library must be refused");
    assert!(
        matches!(err, LibraryPathError::WritableByOthers { .. }),
        "group-writable library must fail the permission rule, got {err:?}"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

/// Positive control: an owner-only file at an absolute path is accepted.
///
/// This also pins the ownership rule in the common case — under `cargo test`
/// the test binary and the file we just created share a uid, so the
/// "owned by the running binary's owner" check must accept it. Without this
/// test the rules above could all pass while validation rejected everything.
#[test]
fn m4_owner_only_library_is_accepted() {
    let dir = tmpdir("owner-only");
    let lib = dir.join("libstwo_sys.so");
    std::fs::write(&lib, b"not really a library").expect("write stub");
    std::fs::set_permissions(&lib, std::os::unix::fs::PermissionsExt::from_mode(0o600))
        .expect("chmod 0600");

    validate_library_path(&lib).expect("an owner-only absolute path must be accepted");
    let _ = std::fs::remove_dir_all(&dir);
}

/// A symlink is followed, and the *resolved* file's ownership is what counts.
///
/// Without this, an attacker who controls a directory in the path could point
/// a benign-looking root-owned name at a library they own.
#[test]
fn m4_symlink_is_followed_to_the_resolved_target() {
    let dir = tmpdir("symlink");
    let target = dir.join("payload.so");
    std::fs::write(&target, b"attacker payload").expect("write payload");
    std::fs::set_permissions(&target, std::os::unix::fs::PermissionsExt::from_mode(0o666))
        .expect("chmod 0666");
    let link = dir.join("libstwo_sys.so");
    std::os::unix::fs::symlink(&target, &link).expect("create symlink");

    let err = validate_library_path(&link)
        .expect_err("a symlink to a world-writable payload must be refused");
    assert!(
        matches!(err, LibraryPathError::WritableByOthers { .. }),
        "validation must inspect the resolved target, not the link, got {err:?}"
    );
    let _ = std::fs::remove_dir_all(&dir);
}
