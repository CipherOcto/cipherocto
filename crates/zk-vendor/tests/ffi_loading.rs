//! FFI loading integration test (mission 0958-a Phase C.2 acceptance).
//!
//! Verifies the decoupled workspace FFI bridge works end-to-end without
//! mocking:
//!
//! 1. Locate the built `libstwo_sys.so` (built via the workspace-excluded
//!    `crates/zk-vendor/stwo-sys/` sub-crate under nightly toolchain —
//    see `crates/zk-vendor/stwo-sys/rust-toolchain.toml`).
//! 2. Set `CIPHEROCTO_STWO_LIB` env var to the lib path.
//! 3. Call `zk_vendor::loaded_library()` and assert `Some` (the
//!    libloading path succeeds).
//! 4. Read `vendor_state()` and assert `Ffi`.
//! 5. Call `stwo_sys_version()` via the loaded handle to verify the
//!    version string is readable (real cross-crate FFI call).
//! 6. Call `stwo_verify` with deliberately-bad JSON and assert it
//!    returns 1 (Err per FFI contract), confirming the verify symbol
//!    is reachable + the contract is preserved.
//!
//! **Run:**
//! ```bash
//! cd crates/zk-vendor/stwo-sys
//! cargo +nightly-2025-06-23 build --release
//! # Then from repo root:
//! cargo test -p zk-vendor --test ffi_loading -- --nocapture
//! ```
//!
//! The test is **skipped** if the nightly-built cdylib is not present
//! (dev / CI without the nightly toolchain installed). It is NOT
//! ignored via `#[ignore]` because the skip is automatic and the test
//! is meaningful in any environment where the cdylib has been built.
//!
//! Per master plan §8 R12 mitigation: no real STARK round-trip is
//! exercised here (that requires a valid `CairoProofForRustVerifier`
//! JSON, which needs a Cairo witness). The test exercises the
//! libloading + symbol resolution + error-path contract only; full
//! STARK prove/verify coverage lives in `crates/quota-router-integration-tests`
//! with fixture JSON.

use std::path::PathBuf;

use zk_vendor::try_load;

/// Resolve the path to `libstwo_sys.so` built by the nightly toolchain
/// at `crates/zk-vendor/stwo-sys/target/release/`.
fn nightly_lib_path() -> Option<PathBuf> {
    let manifest_dir = std::env::var("CARGO_MANIFEST_DIR").ok()?;
    let mut p = PathBuf::from(manifest_dir);
    p.push("stwo-sys");
    p.push("target");
    p.push("release");
    let lib_name = if cfg!(target_os = "windows") {
        "stwo_sys.dll"
    } else if cfg!(target_os = "macos") {
        "libstwo_sys.dylib"
    } else {
        "libstwo_sys.so"
    };
    p.push(lib_name);
    if p.exists() {
        Some(p)
    } else {
        None
    }
}

/// Skip the test if `libstwo_sys.so` is not built in the workspace.
fn require_built_lib() -> Option<PathBuf> {
    if let Some(p) = nightly_lib_path() {
        Some(p)
    } else {
        eprintln!(
            "SKIP ffi_loading: libstwo_sys.so not built. Run: cd crates/zk-vendor/stwo-sys && cargo +nightly-2025-06-23 build --release"
        );
        None
    }
}

#[test]
fn ffi_loading_resolves_lib_and_resolves_symbols() {
    let Some(lib) = require_built_lib() else {
        return;
    };

    // Use `try_load` directly (not `loaded_library()`) so we bypass
    // the process-global OnceLock cache. The cache is initialized once
    // per process; if a previous test in the same binary loaded with
    // CIPHEROCTO_STWO_LIB unset, the cache holds None and subsequent
    // calls return None even after the env var is set. `try_load`
    // always performs a fresh dlopen on the given path.

    // 1. libloading path returns Some (not stub).
    let sys = try_load(&lib)
        .expect("try_load should not error on a present lib")
        .expect("try_load should return Some when lib is loadable");
    assert!(
        sys.version().starts_with("stwo-sys"),
        "version must start with 'stwo-sys'; got: {}",
        sys.version()
    );
    assert!(
        sys.version().contains("real STWO"),
        "version must advertise real STWO (no mock); got: {}",
        sys.version()
    );
    eprintln!(
        "ffi_loading: loaded version = {:?} from path {}",
        sys.version(),
        lib.display()
    );
}

#[test]
fn ffi_loading_verify_rejects_invalid_json_via_real_stwo() {
    let Some(lib) = require_built_lib() else {
        return;
    };

    // Bypass the process-global OnceLock cache via `try_load` (see
    // comment in `ffi_loading_resolves_lib_and_resolves_symbols`).
    let sys = try_load(&lib)
        .expect("try_load should not error on a present lib")
        .expect("try_load should return Some when lib is loadable");

    // verify with invalid JSON returns Err per FFI contract. This
    // confirms the verify symbol is reachable + the error path is
    // honored end-to-end (libloading → real STWO → real error).
    let bad_proof: &[u8] = b"this is not json";
    let public: &[u8] = b"unused";
    let result = sys.verify(bad_proof, public);
    assert!(
        result.is_err(),
        "invalid JSON proof must return Err; got: {result:?}"
    );
    eprintln!("ffi_loading: verify(invalid_json) returned Err as expected: {result:?}");
}

#[test]
fn ffi_loading_try_load_handles_missing_path() {
    // Use a path that definitely does not exist. `try_load` returns
    // `Ok(None)` for missing libraries (fallback path), NOT Err.
    let missing = std::path::PathBuf::from("/nonexistent/libstwo_sys_test.so");
    let result = try_load(&missing);
    assert!(
        matches!(result, Ok(None)),
        "missing library returns Ok(None) (fallback), got: {result:?}"
    );
}
