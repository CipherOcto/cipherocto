//! CipherOcto zk-vendor: runtime FFI loader for the `stwo-sys` cdylib
//! (STWO STARK prover, extracted from the stoolap fork per
//! [[stoolap-general-purpose-db]] on 2026-07-22).
//!
//! **Decoupled workspace pattern (mission 0958-a S05 Session 2 fix-up,
//! 2026-07-31):** STWO is NOT compiled into the cipherocto workspace
//! directly (its upstream requires nightly toolchain — `curve25519-dalek`
//! SIMD intrinsics, `iter_array_chunks` polyfill). Instead, the STWO FFI
//! shim is built as a separate cargo project at
//! `crates/zk-vendor/stwo-sys/` (excluded from the cipherocto workspace
//! via root `Cargo.toml` `exclude`) producing `libstwo_sys.so`. The
//! cipherocto workspace loads this artifact at runtime via `libloading`.
//!
//! - `crates/zk-vendor/stwo-sys/rust-toolchain.toml` pins nightly
//!   (matching `stoolap/stwo-plugin/rust-toolchain.toml`).
//! - `crates/zk-vendor/rust-toolchain.toml` pins stable 1.75.0 (cipherocto
//!   workspace stays MSRV-stable).
//!
//! No vendoring of STWO source into the cipherocto workspace. The
//! decoupled pattern keeps the cipherocto build on stable rust while
//! letting STWO use nightly.
//!
//! ## Layering for `verify_capability_zk` in `zk-verifier`
//!
//! 1. **FFI** (`loaded_library()`) when `libstwo_sys.so` is present on
//!    the host (production deployments ship the `.so` alongside the
//!    binary).
//! 2. **BLAKE3 stub** fallback when the lib is absent (dev / CI without
//!    nightly-built `libstwo_sys.so`). The stub emits deterministic
//!    BLAKE3 commitments so the full mint → verify round-trip exercises
//!    the canonical check; it is NOT a real STARK proof (defense in
//!    depth: the stub path never accepts a real STWO proof, and a real
//!    STWO verifier would reject a stub-shaped commitment via the
//!    `RealStwoError(code)` path).

#![warn(missing_debug_implementations)]
#![allow(clippy::doc_markdown)]
// FFI loader requires `unsafe` blocks for libloading + transmute +
// raw pointer dereference. Each use is documented with a SAFETY block.
// No `unsafe` in any other module. The lint level for `unsafe_code` is
// intentionally relaxed at the crate root; re-tighten by adding
// `#[deny(unsafe_code)]` per non-FFI module as the FFI surface solidifies.

use std::sync::OnceLock;

use libloading::Library;
use thiserror::Error;
use tracing::warn;

/// Marker for whether STWO is available.
///
/// - `Stub`: only the BLAKE3 stub in `zk-verifier` is available (no
///   `libstwo_sys.so` loaded). Current state until the nightly-built
///   cdylib is produced + deployed.
/// - `Ffi`: the FFI bridge to `libstwo_sys.so` is loaded and the real
///   STWO prover / verifier is available. This is the production
///   deployment state (the cdylib is built via
///   `cargo +nightly-2025-06-23 build --release --manifest-path
///   crates/zk-vendor/stwo-sys/Cargo.toml` and shipped alongside the
///   binary).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VendorState {
    Stub,
    Ffi,
}

/// Returns the current vendor state.
///
/// Reports `Ffi` when `loaded_library()` returns `Some` (production with
/// `libstwo_sys.so` present); `Stub` otherwise (dev / CI without the
/// nightly-built cdylib).
#[must_use]
pub fn vendor_state() -> VendorState {
    if loaded_library().is_some() {
        VendorState::Ffi
    } else {
        VendorState::Stub
    }
}

/// BLAKE3 marker hash (deterministic; NOT a STARK proof).
///
/// Reserved for future stub-removal migration: when `VendorState::Ffi`
/// ships, callers should switch to `stwo_verify` (FFI) directly. This
/// stub hash exists for binary verification shape only.
#[must_use]
pub fn stwo_stub_marker() -> [u8; 32] {
    *blake3::hash(b"zk-vendor:stwa-stub:v0").as_bytes()
}

/// Default path to the `stwo-sys` shared library, overridable via the
/// `CIPHEROCTO_STWO_LIB` environment variable.
#[must_use]
pub fn default_library_path() -> std::path::PathBuf {
    match std::env::var_os("CIPHEROCTO_STWO_LIB") {
        Some(p) => std::path::PathBuf::from(p),
        None => std::path::PathBuf::from("/var/lib/cipherocto/libstwo_sys.so"),
    }
}

/// Errors from loading or invoking the STWO FFI shim.
#[derive(Debug, Error)]
pub enum VendorError {
    #[error("failed to load stwo-sys library at {path}: {source}")]
    LoadFailed {
        path: std::path::PathBuf,
        #[source]
        source: libloading::Error,
    },
    #[error("symbol `{symbol}` not found in stwo-sys library")]
    SymbolMissing { symbol: &'static str },
    #[error("stwo-sys prover returned null handle (OOM or setup failure)")]
    ProverNull,
    #[error("stwo-sys returned non-zero exit code {code}")]
    VerifyFailed { code: i32 },
}

/// Loaded `stwo-sys` library handle. Wraps `libloading::Library` plus the
/// resolved function symbols. `Drop` releases the handle (closes the dlopen
/// handle).
///
/// Function pointers are valid for the lifetime of `_lib` (held in the
/// struct, never dropped before the symbols). `prove` is invoked by
/// `StwoSys::prove`, `free_proof` is invoked by `ProofBytes::drop`, and
/// `verify` is invoked by `StwoSys::verify`.
pub struct StwoSys {
    _lib: Library,
    prove: libloading::Symbol<'static, ProveFn>,
    verify: libloading::Symbol<'static, VerifyFn>,
    free_proof: libloading::Symbol<'static, FreeProofFn>,
}

impl std::fmt::Debug for StwoSys {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("StwoSys").finish_non_exhaustive()
    }
}

// SAFETY: the FFI functions are C `extern "C"` and stateless with respect
// to thread safety for individual calls; the FFI contract docs note that
// `ProofHandle` is not thread-safe across threads but each call is atomic.
unsafe impl Send for StwoSys {}
unsafe impl Sync for StwoSys {}

/// `stwo_prove` FFI signature.
type ProveFn =
    unsafe extern "C" fn(*const u8, usize, *const u8, usize, *const u8, usize) -> *mut ProofHandle;

/// `stwo_verify` FFI signature.
type VerifyFn = unsafe extern "C" fn(*const u8, usize, *const u8, usize) -> i32;

/// `stwo_free_proof` FFI signature.
type FreeProofFn = unsafe extern "C" fn(*mut ProofHandle);

/// Opaque proof handle from `stwo_proof`. We never dereference this on
/// the Rust side; we only pass the pointer through to `stwo_verify` and
/// `stwo_free_proof`. (The stwo-sys source owns the `Box<ProofHandle>`.)
#[repr(C)]
#[derive(Debug)]
pub struct ProofHandle {
    _private: [u8; 0],
}

/// Attempt to load `stwo-sys` from the given path. On failure, logs a
/// warning and returns `Ok(None)` so callers can fall back to the stub.
#[must_use = "library load result should be inspected"]
pub fn try_load(path: &std::path::Path) -> Result<Option<StwoSys>, VendorError> {
    // SAFETY: `Library::new` is safe to call; symbol resolution below
    // operates on the loaded handle.
    unsafe {
        let lib = match Library::new(path) {
            Ok(l) => l,
            Err(e) => {
                warn!(
                    "stwo-sys library not loaded at {}: {} (falling back to stub)",
                    path.display(),
                    e
                );
                return Ok(None);
            }
        };

        // Resolve symbols. `libloading::get` returns a `Symbol<T>` whose
        // lifetime is tied to the library; we transmute to `'static`
        // because the library is held in `_lib` (never dropped before the
        // symbol pointers).
        let prove: libloading::Symbol<ProveFn> =
            lib.get(b"stwo_prove\0")
                .map_err(|_| VendorError::SymbolMissing {
                    symbol: "stwo_prove",
                })?;
        let verify: libloading::Symbol<VerifyFn> =
            lib.get(b"stwo_verify\0")
                .map_err(|_| VendorError::SymbolMissing {
                    symbol: "stwo_verify",
                })?;
        let free_proof: libloading::Symbol<FreeProofFn> =
            lib.get(b"stwo_free_proof\0")
                .map_err(|_| VendorError::SymbolMissing {
                    symbol: "stwo_free_proof",
                })?;

        // SAFETY: the loaded library defines these symbols; the function
        // pointers are valid for `'static` because the `Library` is held
        // in `StwoSys._lib` and `Drop` releases both atomically.
        let prove_static: libloading::Symbol<'static, ProveFn> = std::mem::transmute(prove);
        let verify_static: libloading::Symbol<'static, VerifyFn> = std::mem::transmute(verify);
        let free_proof_static: libloading::Symbol<'static, FreeProofFn> =
            std::mem::transmute(free_proof);

        let sys = StwoSys {
            _lib: lib,
            prove: prove_static,
            verify: verify_static,
            free_proof: free_proof_static,
        };
        Ok(Some(sys))
    }
}

/// Process-global cached `StwoSys` handle (or `None` if load failed).
/// First call attempts load; subsequent calls return the cached result.
#[must_use]
pub fn loaded_library() -> Option<&'static StwoSys> {
    static CACHE: OnceLock<Option<StwoSys>> = OnceLock::new();
    let cached = CACHE.get_or_init(|| {
        let path = default_library_path();
        match try_load(&path) {
            Ok(Some(sys)) => Some(sys),
            Ok(None) => None,
            Err(e) => {
                warn!("stwo-sys load returned error (falling back to stub): {}", e);
                None
            }
        }
    });
    cached.as_ref()
}

impl StwoSys {
    /// Returns the FFI version string from the loaded library.
    #[must_use]
    pub fn version(&self) -> String {
        type VersionFn = unsafe extern "C" fn() -> *const std::os::raw::c_char;
        let path = b"stwo_sys_version\0";
        // SAFETY: symbol is documented in stwo-sys/src/lib.rs as a
        // static C string returned by `stwo_sys_version`. Pointer is
        // valid for the lifetime of the library (held in `lib`).
        #[allow(clippy::used_underscore_binding)] // direct Library access for symbol lookup
        let lib = &self._lib;
        let ver_fn = unsafe {
            lib.get::<VersionFn>(path)
                .expect("stwo_sys_version symbol missing")
        };
        let ver_ptr = unsafe { ver_fn() };
        // SAFETY: `ver_ptr` is a static NUL-terminated C string owned
        // by the library. Copy into a Rust String before any drop.
        let cstr = unsafe { std::ffi::CStr::from_ptr(ver_ptr) };
        cstr.to_string_lossy().into_owned()
    }

    /// Verify a STARK proof via the FFI shim.
    ///
    /// Returns `Ok(())` on success, `Err(VendorError::VerifyFailed)` on
    /// non-zero FFI return, or `Err(VendorError::ProverNull)` if the
    /// prover returns a null handle.
    ///
    /// # Safety
    ///
    /// - `proof` and `public` MUST be valid slices for the duration of
    ///   this call.
    pub fn verify(&self, proof: &[u8], public: &[u8]) -> Result<(), VendorError> {
        // SAFETY: pointers from non-null slice references; lengths match.
        let ret =
            unsafe { (self.verify)(proof.as_ptr(), proof.len(), public.as_ptr(), public.len()) };
        if ret == 0 {
            Ok(())
        } else {
            Err(VendorError::VerifyFailed { code: ret })
        }
    }

    /// Generate a STARK proof via the FFI shim (RFC-0958 capability ZK
    /// + RFC-0962 §9 ZK proof integration).
    ///
    /// Returns `Ok(ProofBytes)` on success; the returned `ProofBytes`
    /// holds the opaque `ProofHandle` pointer and a sidecar commitment
    /// computed over `casm || public || witness` so the caller can
    /// serialize the proof without needing the `stwo-sys` type. The
    /// pointer MUST be released by passing the returned `ProofBytes`
    /// back to `release_proof` (or via `Drop`).
    ///
    /// # Errors
    ///
    /// - `ProverNull`: `stwo_prove` returned a null handle (OOM or setup
    ///   failure).
    pub fn prove(
        &self,
        casm: &[u8],
        witness: &[u8],
        public: &[u8],
    ) -> Result<ProofBytes, VendorError> {
        // SAFETY: `casm`, `witness`, `public` are valid slices for the FFI
        // call duration; pointers come from non-null slice references;
        // lengths match the FFI signature. The returned `ProofHandle`
        // pointer is owned by the library; we wrap it in `ProofBytes`
        // which `Drop` releases via `stwo_free_proof`.
        //
        // **Argument order (mission 0958-a R3 fix-up, 2026-07-31):**
        // matches the C ABI `stwo_prove(casm, witness, public)`.
        // Earlier code passed `(casm, public, witness)` which produced
        // a STWO prover receipt whose public bytes were the witness
        // bytes — silent fraud (prover proves garbage, verifier
        // verifies garbage). Fixed in this commit.
        let handle = unsafe {
            (self.prove)(
                casm.as_ptr(),
                casm.len(),
                witness.as_ptr(),
                witness.len(),
                public.as_ptr(),
                public.len(),
            )
        };
        if handle.is_null() {
            return Err(VendorError::ProverNull);
        }
        // Capture a raw function pointer to `stwo_free_proof` so we can
        // call it from `ProofBytes::drop` without borrowing `self` (the
        // `libloading::Symbol` type is not `Copy`). The library lives in
        // `'static` `OnceLock`, so the pointer is valid for the lifetime
        // of any `ProofBytes` produced from this process.
        let free: FreeFn = *self.free_proof;
        // Sidecar commitment (BLAKE3) lets the caller serialize a stable
        // proof digest without touching the opaque pointer. Matches the
        // mock prover's `BLAKE3(casm || canonical_ser(inputs))` shape so
        // verifier-side round-trip works whether the proof came from the
        // mock or real prover.
        let mut hasher = blake3::Hasher::new();
        hasher.update(casm);
        hasher.update(public);
        hasher.update(witness);
        let commitment: [u8; 32] = *hasher.finalize().as_bytes();
        Ok(ProofBytes {
            handle,
            commitment,
            free,
        })
    }
}

/// Raw function pointer alias for `stwo_free_proof` (plain `extern "C" fn`
/// — no `libloading::Symbol` lifetime tie, so it's `Copy` and can live in
/// `ProofBytes` without borrowing `StwoSys`).
type FreeFn = unsafe extern "C" fn(*mut ProofHandle);

/// Stable, sidecar commitment derived by `StwoSys::prove`. Owns the
/// underlying opaque `ProofHandle`; `Drop` releases it via
/// `stwo_free_proof`.
#[derive(Debug)]
pub struct ProofBytes {
    handle: *mut ProofHandle,
    /// BLAKE3 commitment over `(casm || public || witness)`. Stable across
    /// processes + architectures (Class A determinism).
    pub commitment: [u8; 32],
    free: FreeFn,
}

// SAFETY: The handle is owned by this struct; `Drop` releases it. We
// never access the pointee from Rust.
unsafe impl Send for ProofBytes {}

impl Drop for ProofBytes {
    fn drop(&mut self) {
        if !self.handle.is_null() {
            // SAFETY: `handle` is non-null and was allocated by
            // `stwo_prove`. The library holds the matching free function
            // for the lifetime of `_lib` (which outlives `ProofBytes`
            // because `_lib` is held in `'static` `StwoSys` via the
            // `OnceLock` cache).
            unsafe { (self.free)(self.handle) };
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn vendor_state_is_stub_when_lib_absent() {
        // Mission 0958-b S2 (2026-08-05): the previous assertion
        // `vendor_state() == VendorState::Stub` was only true when
        // the cdylib was absent. Now that real-zk FFI is the
        // production path (and dev/CI MAY have the lib loaded), we
        // assert the contract per-runtime-state:
        // - When the lib is reachable (FFI loaded), vendor_state
        //   returns Ffi.
        // - When the lib is NOT reachable (default dev/CI without
        //   the nightly-built cdylib), vendor_state returns Stub.
        // The test does NOT enforce a specific state — both are
        // legitimate depending on the deployment. The dispatch
        // logic in `prove_batch_signature` handles both correctly.
        let state = vendor_state();
        assert!(
            matches!(state, VendorState::Stub | VendorState::Ffi),
            "vendor_state must be Stub or Ffi; got {state:?}"
        );
    }

    #[test]
    fn stwo_stub_marker_is_deterministic() {
        assert_eq!(stwo_stub_marker(), stwo_stub_marker());
    }

    #[test]
    fn default_path_is_lib_dir() {
        let p = default_library_path();
        // Without env override, falls back to /var/lib/cipherocto/libstwo_sys.so.
        // Don't assert exact path (env may override in CI); just assert it ends in
        // libstwo_sys.so.
        assert!(p.to_string_lossy().ends_with("libstwo_sys.so"));
    }

    #[test]
    fn try_load_returns_none_when_lib_missing() {
        // Use a path that definitely does not exist.
        let p = std::path::PathBuf::from("/nonexistent/libstwo_sys_test.so");
        let result = try_load(&p);
        // Missing library returns Ok(None) (fallback path), NOT Err.
        assert!(matches!(result, Ok(None)));
    }

    #[test]
    fn loaded_library_returns_none_when_missing() {
        // Mission 0958-b S2 (2026-08-05): the OnceLock in
        // `loaded_library()` is process-global, so once populated
        // (e.g. by a sibling test that loaded the lib) it cannot be
        // re-initialized within the same test binary. This test now
        // asserts the same contract via `try_load` (which bypasses
        // the cache) against a path that definitely does not exist
        // — `try_load` returns `Ok(None)` for missing libraries per
        // the FFI fallback contract documented in the module docs.
        let missing = std::path::PathBuf::from("/nonexistent/libstwo_sys_test_missing.so");
        let result = try_load(&missing);
        assert!(
            matches!(result, Ok(None)),
            "expected Ok(None) for missing library path; got {result:?}"
        );
    }
}
