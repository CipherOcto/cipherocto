//! CipherOcto zk-vendor: vendored STWO source with stable-rust patches +
//! runtime FFI loader for the `stwo-sys` cdylib.
//!
//! **Crypto home:** this crate is the cargo home for STWO (a STARK prover
//! that depends on nightly-only `curve25519-dalek` SIMD). Extracted from the
//! stoolap fork per [[stoolap-general-purpose-db]] (2026-07-22). Stable
//! rust only — MSRV pinned in `rust-toolchain.toml`.
//!
//! ## Loading strategy
//!
//! STWO is NOT compiled into the cipherocto workspace directly (its upstream
//! requires nightly toolchain). Instead, the STWO FFI shim is built as a
//! separate cargo project at `crates/zk-vendor/stwo-sys/` (excluded from
//! workspace via root `Cargo.toml`) producing `libstwo_sys.so`. Cipherocto
//! loads this artifact at runtime via `libloading`.
//!
//! If the library is missing, zk-vendor falls back to the stub commitment
//! check + logs a warning. This lets dev workflows run without the nightly
//! toolchain installed; production deployments ship the `.so` alongside
//! the binary.
//!
//! ## Vendor state (2026-07-22)
//!
//! STUB. Real STWO source drop deferred to mission 0958-a S05 task B. When
//! the source lands, the stwo-sys FFI bodies (currently XOR-digest stubs)
//! get replaced with calls into vendored `keep-stwo/stwo`.

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

/// Marker for whether STWO source is vendored.
///
/// - `Stub`: SHA-based stub in `zk-verifier` (current state, 2026-07-22).
/// - `Vendored`: real STWO source drop in `stwo-sys` cdylib (pending
///   mission 0958-a S05 task B).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VendorState {
    Stub,
    Vendored,
}

/// Returns the current vendor state.
///
/// **Note:** `vendor_state()` reports `Stub` until the real STWO source
/// drop lands in `stwo-sys/`. Even when `sttwo_sys.so` is present and
/// successfully loaded, the underlying STWO logic is still stub (XOR
/// digest), so `vendor_state() == Stub` is the accurate reflection.
#[must_use]
pub const fn vendor_state() -> VendorState {
    VendorState::Stub
}

/// BLAKE3 marker hash (deterministic; NOT a STARK proof).
///
/// Reserved for future stub-removal migration: when `VendorState::Vendored`
/// ships, callers should switch to `stwo::verify(...)` directly. This stub
/// hash exists for binary verification shape only.
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
pub struct StwoSys {
    _lib: Library,
    // Function pointers are valid for the lifetime of `_lib`.
    #[allow(dead_code)] // surfaced via API in follow-up commit (prover helper).
    prove: libloading::Symbol<'static, ProveFn>,
    verify: libloading::Symbol<'static, VerifyFn>,
    #[allow(dead_code)] // surfaced via API in follow-up commit (proof release).
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
        public: &[u8],
        witness: &[u8],
    ) -> Result<ProofBytes, VendorError> {
        // SAFETY: `casm`, `public`, `witness` are valid slices for the FFI
        // call duration; pointers come from non-null slice references;
        // lengths match the FFI signature. The returned `ProofHandle`
        // pointer is owned by the library; we wrap it in `ProofBytes`
        // which `Drop` releases via `stwo_free_proof`.
        let handle = unsafe {
            (self.prove)(
                casm.as_ptr(),
                casm.len(),
                public.as_ptr(),
                public.len(),
                witness.as_ptr(),
                witness.len(),
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
    fn vendor_state_is_stub_for_now() {
        assert_eq!(vendor_state(), VendorState::Stub);
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
        // Save and restore the env var so subsequent tests see the same
        // starting state (other tests like `default_path_is_lib_dir`
        // assume no override).
        let prev = std::env::var_os("CIPHEROCTO_STWO_LIB");
        // Point at a missing path; CI/dev typically won't have the lib
        // installed. If the test environment DOES have
        // /var/lib/cipherocto/libstwo_sys.so (rare), this test still
        // returns Some — but the rest of the suite doesn't depend on it.
        let p = prev.clone().map_or_else(
            || std::path::PathBuf::from("/nonexistent/libstwo_sys_test.so"),
            std::path::PathBuf::from,
        );
        std::env::set_var("CIPHEROCTO_STWO_LIB", &p);
        let result = loaded_library();
        match prev {
            Some(v) => std::env::set_var("CIPHEROCTO_STWO_LIB", v),
            None => std::env::remove_var("CIPHEROCTO_STWO_LIB"),
        }
        assert!(result.is_none(), "expected None when lib missing");
    }
}
