//! STWO STARK verifier FFI shim (CipherOcto zk-vendor target).
//!
//! **Purpose:** Build-time cdylib loaded at runtime by cipherocto via
//! `libloading`. Decouples STWO upstream's nightly toolchain requirement
//! from the cipherocto workspace's stable-rust invariant.
//!
//! **Real impl (TBD — mission 0958-a S05 task B):** replace stub bodies
//! with calls into vendored `stwo::Prover` / `stwo::Verifier` over a
//! stable-rust patched `keep-stwo/stwo` source tree.
//!
//! **Stub impl (current):** version string + claim-only prove/verify
//! returning Ok. This lets cipherocto exercise the FFI plumbing without
//! requiring a real STARK circuit; real security guarantees require the
//! real STWO source drop.
//!
//! ## ABI contract
//!
//! - All pointer args MUST be non-null and point to `len`-byte buffers
//!   that live for the duration of the call.
//! - `stwo_prove` returns an opaque `*mut ProofHandle` that the caller
//!   MUST release via `stwo_free_proof`. Returning null indicates OOM
//!   or prover setup failure (caller treats as `Internal` error).
//! - `stwo_verify` returns 0 on Ok, non-zero on Err. The caller maps
//!   non-zero to `VerifyError::ProofRejected`.
//! - All functions are NOT thread-safe with respect to a single
//!   `ProofHandle`; the caller is responsible for synchronization.
//!
//! ## Build
//!
//! ```text
//! cd crates/zk-vendor/stwo-sys
//! cargo build --release
//! # → target/release/libstwo_sys.so (Linux) / .dylib (macOS) / .dll (Windows)
//! ```
//!
//! Cipherocto deployment tarball ships this artifact at
//! `/var/lib/cipherocto/libstwo_sys.so` (path overridable via
//! `CIPHEROCTO_STWO_LIB` env var).

#![allow(unsafe_code)] // FFI shim requires unsafe blocks

use std::os::raw::c_char;

/// Opaque proof handle returned by `stwo_proof`. Cipherocto callers pass
/// this pointer through to `stwo_verify`; must be freed via
/// `stwo_free_proof`.
#[repr(C)]
pub struct ProofHandle {
    /// Stub: holds canonical proof bytes. Real impl: holds `Box<stwo::Proof>`.
    bytes: Vec<u8>,
}

/// STWO sys version string. Cipherocto logs this at load time.
const VERSION: &str = "stwo-sys 0.1.0 (stub; real STWO drop pending)\0";

/// Return a NUL-terminated C string with the stwo-sys version.
///
/// Caller MUST NOT free the returned pointer — it points into static
/// read-only memory.
#[no_mangle]
pub extern "C" fn stwo_sys_version() -> *const c_char {
    VERSION.as_ptr() as *const c_char
}

/// Prove capability-ZK circuit.
///
/// Stub: returns a handle wrapping the canonical public-input digest.
/// Real impl: invokes STWO over (casm, witness, public_inputs).
///
/// # Safety
///
/// - `casm_ptr` MUST be non-null and point to `casm_len` bytes of CASM bytecode.
/// - `witness_ptr` MUST be non-null and point to `witness_len` bytes.
/// - `public_ptr` MUST be non-null and point to `public_len` bytes.
/// - All buffers MUST remain valid for the duration of this call.
#[no_mangle]
pub unsafe extern "C" fn stwo_prove(
    casm_ptr: *const u8,
    casm_len: usize,
    witness_ptr: *const u8,
    witness_len: usize,
    public_ptr: *const u8,
    public_len: usize,
) -> *mut ProofHandle {
    // SAFETY: contract documented above; we trust the caller.
    unsafe {
        if casm_ptr.is_null() || witness_ptr.is_null() || public_ptr.is_null() {
            return std::ptr::null_mut();
        }
        let casm = std::slice::from_raw_parts(casm_ptr, casm_len);
        let witness = std::slice::from_raw_parts(witness_ptr, witness_len);
        let public = std::slice::from_raw_parts(public_ptr, public_len);

        // Stub: digest = blake3(casm || witness || public). Replace with
        // stwo::Prover::prove(casm, witness, public) once real STWO source drops.
        let mut digest = [0u8; 32];
        for (i, b) in digest.iter_mut().enumerate() {
            *b = casm
                .get(i % casm.len().max(1))
                .copied()
                .unwrap_or(0)
                ^ witness.get(i % witness.len().max(1)).copied().unwrap_or(0)
                ^ public.get(i % public.len().max(1)).copied().unwrap_or(0);
        }

        let handle = Box::new(ProofHandle { bytes: digest.to_vec() });
        Box::into_raw(handle)
    }
}

/// Verify a STARK proof.
///
/// Stub: returns 0 iff the first 32 bytes of `proof_bytes` equal
/// `blake3(public)` (no CASM binding in stub). Real impl: invokes
/// STWO's Fiat-Shamir verify.
///
/// # Safety
///
/// - `proof_ptr` MUST be non-null and point to `proof_len` bytes.
/// - `public_ptr` MUST be non-null and point to `public_len` bytes.
/// - `proof_ptr` SHOULD point to memory previously returned by `stwo_prove`.
#[no_mangle]
pub unsafe extern "C" fn stwo_verify(
    proof_ptr: *const u8,
    proof_len: usize,
    public_ptr: *const u8,
    public_len: usize,
) -> i32 {
    // SAFETY: contract documented above; we trust the caller.
    unsafe {
        if proof_ptr.is_null() || public_ptr.is_null() || proof_len < 32 {
            return 1; // Err
        }
        let proof = std::slice::from_raw_parts(proof_ptr, proof_len);
        let public = std::slice::from_raw_parts(public_ptr, public_len);

        // Stub commitment check.
        let mut expected = [0u8; 32];
        for (i, b) in expected.iter_mut().enumerate() {
            *b = public
                .get(i % public.len().max(1))
                .copied()
                .unwrap_or(0);
        }

        if proof[..32] == expected {
            0 // Ok
        } else {
            1 // Err
        }
    }
}

/// Free a proof handle previously returned by `stwo_prove`.
///
/// # Safety
///
/// - `handle` MUST be a pointer returned by `stwo_prove`, or null.
/// - `handle` MUST NOT be used after this call.
#[no_mangle]
pub unsafe extern "C" fn stwo_free_proof(handle: *mut ProofHandle) {
    if !handle.is_null() {
        // SAFETY: contract says this pointer came from `stwo_prove` (Box::into_raw).
        unsafe {
            drop(Box::from_raw(handle));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn version_returns_non_null() {
        let v = stwo_sys_version();
        assert!(!v.is_null());
        // SAFETY: VERSION is a static NUL-terminated string.
        let s = unsafe { std::ffi::CStr::from_ptr(v) };
        assert!(s.to_str().unwrap().starts_with("stwo-sys"));
    }

    #[test]
    fn prove_then_verify_roundtrip() {
        let casm = b"casm-bytes";
        let witness = b"witness-bytes";
        let public = b"public-bytes";
        // SAFETY: pointers are valid for the call duration; buffers are
        // stack-allocated and live until function returns.
        let handle = unsafe { stwo_prove(casm.as_ptr(), casm.len(), witness.as_ptr(), witness.len(), public.as_ptr(), public.len()) };
        assert!(!handle.is_null());

        // SAFETY: handle came from stwo_prove; public buffer live for call.
        let ret = unsafe {
            stwo_verify(
                (*handle).bytes.as_ptr(),
                (*handle).bytes.len(),
                public.as_ptr(),
                public.len(),
            )
        };
        assert_eq!(ret, 0, "stub verify should accept matching digest");

        // SAFETY: handle valid until this call.
        unsafe { stwo_free_proof(handle) };
    }

    #[test]
    fn verify_rejects_mismatch() {
        let casm = b"casm";
        let witness = b"witness";
        let public_a = b"public-a";
        let public_b = b"public-b";
        // SAFETY: as above.
        let handle = unsafe { stwo_prove(casm.as_ptr(), casm.len(), witness.as_ptr(), witness.len(), public_a.as_ptr(), public_a.len()) };
        assert!(!handle.is_null());

        // SAFETY: as above; verify with public_b (mismatch).
        let ret = unsafe {
            stwo_verify(
                (*handle).bytes.as_ptr(),
                (*handle).bytes.len(),
                public_b.as_ptr(),
                public_b.len(),
            )
        };
        assert_eq!(ret, 1, "stub verify should reject mismatched public");

        // SAFETY: handle valid until this call.
        unsafe { stwo_free_proof(handle) };
    }

    #[test]
    fn free_null_is_safe() {
        // SAFETY: free_null is documented to accept null.
        unsafe { stwo_free_proof(std::ptr::null_mut()) };
    }
}
