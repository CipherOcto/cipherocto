//! STWO STARK verifier FFI shim (CipherOcto zk-vendor target).
//!
//! **Purpose:** Build-time cdylib loaded at runtime by cipherocto via
//! `libloading`. Decouples STWO upstream's nightly toolchain requirement
//! from the cipherocto workspace's stable-rust invariant.
//!
//! **Pattern:** Mirrors stoolap's `stwo-plugin/` crate
//! (`/home/mmacedoeu/_w/databases/stoolap/stwo-plugin/`):
//! - Separate cargo project (excluded from cipherocto workspace).
//! - Own `rust-toolchain.toml` pinning nightly.
//! - Builds as cdylib; cipherocto loads via libloading from
//!   `/var/lib/cipherocto/libstwo_sys.so` (overridable via
//!   `CIPHEROCTO_STWO_LIB`).
//!
//! **Implementation:** Real STWO verify via
//! `cairo_air::verifier::verify_cairo::<Blake2sMerkleChannel>` after
//! parsing `proof_bytes` as JSON-encoded `CairoProofForRustVerifier`.
//! Matches stoolap's `verify.rs::verify_proof_internal` signature.
//!
//! ## ABI contract
//!
//! - All pointer args MUST be non-null and point to `len`-byte buffers
//!   that live for the duration of the call.
//! - `stwo_prove` returns an opaque `*mut ProofHandle` that the caller
//!   MUST release via `stwo_free_proof`. Returning null indicates OOM,
//!   prover setup failure, or upstream STWO error (caller maps to
//!   `Internal`).
//! - `stwo_verify` returns 0 on Ok, non-zero on Err. The caller maps
//!   non-zero to `VerifyError::RealStwoError(code)`.
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
//!
//! `scripts/build-stwo-sys.sh` automates nightly build + staging.

#![allow(unsafe_code)] // FFI shim requires unsafe blocks

use cairo_air::CairoProofForRustVerifier;
use stwo::core::vcs_lifted::blake2_merkle::{Blake2sMerkleChannel, Blake2sMerkleHasher};
use stwo_cairo_adapter::ProverInput;
use stwo_cairo_prover::prover::{prove_cairo, ChannelHash, ProverParameters};
use cairo_air::PreProcessedTraceVariant;
use stwo::core::fri::FriConfig;
use stwo::core::pcs::PcsConfig;

use std::os::raw::c_char;

/// Opaque proof handle returned by `stwo_prove`. We hold the JSON-encoded
/// proof bytes (cipherocto consumes JSON-encoded
/// `CairoProofForRustVerifier`); FFI caller treats this as opaque pointer.
#[repr(C)]
pub struct ProofHandle {
    bytes: Vec<u8>,
}

/// STWO sys version string. Cipherocto logs this at load time.
const VERSION: &str = "stwo-sys 0.2.0 (real STWO; cipherocto zk-vendor)\0";

/// Return a NUL-terminated C string with the stwo-sys version.
///
/// Caller MUST NOT free the returned pointer — it points into static
/// read-only memory.
#[no_mangle]
pub extern "C" fn stwo_sys_version() -> *const c_char {
    VERSION.as_ptr() as *const c_char
}

/// Build the default STWO ProverParameters. Mirrors
/// `stoolap/stwo-plugin/src/verify.rs::create_default_prover_params`.
fn default_prover_params() -> ProverParameters {
    ProverParameters {
        channel_hash: ChannelHash::Blake2s,
        channel_salt: 0,
        pcs_config: PcsConfig {
            pow_bits: 26,
            fri_config: FriConfig {
                log_last_layer_degree_bound: 0,
                log_blowup_factor: 1,
                n_queries: 70,
                line_fold_step: 1,
            },
            lifting_log_size: None,
        },
        preprocessed_trace: PreProcessedTraceVariant::Canonical,
        store_polynomials_coefficients: false,
        include_all_preprocessed_columns: false,
    }
}

/// Prove capability-ZK circuit.
///
/// Real impl: parses `witness` as JSON `ProverInput` (per
/// `stwo_cairo_adapter::ProverInput`), invokes
/// `stwo_cairo_prover::prove_cairo::<Blake2sMerkleChannel>`, returns
/// the proof JSON-encoded as `CairoProofForRustVerifier<Blake2sMerkleHasher>`.
///
/// `casm` and `public` are accepted for ABI parity with the stub; the
/// real STWO proof is bound to the witness + ProverInput alone (CASM hash
/// + public inputs are checked by the cipherocto wrapper BEFORE calling
/// the FFI).
///
/// # Safety
///
/// - `casm_ptr` MUST be non-null and point to `casm_len` bytes of CASM bytecode.
/// - `witness_ptr` MUST be non-null and point to `witness_len` bytes of JSON `ProverInput`.
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
        let _casm = std::slice::from_raw_parts(casm_ptr, casm_len);
        let witness = std::slice::from_raw_parts(witness_ptr, witness_len);
        let _public = std::slice::from_raw_parts(public_ptr, public_len);

        // Parse witness as JSON ProverInput (stwo_cairo_adapter format).
        let prover_input: ProverInput = match serde_json::from_slice(witness) {
            Ok(p) => p,
            Err(e) => {
                eprintln!("stwo-sys: failed to parse ProverInput JSON: {e}");
                return std::ptr::null_mut();
            }
        };

        // Generate proof via real STWO.
        let proof = match prove_cairo::<Blake2sMerkleChannel>(
            prover_input,
            default_prover_params(),
        ) {
            Ok(p) => p,
            Err(e) => {
                eprintln!("stwo-sys: prove_cairo failed: {e:?}");
                return std::ptr::null_mut();
            }
        };

        // Convert to verifier-compatible format + JSON-encode for FFI.
        let proof_for_verifier: CairoProofForRustVerifier<Blake2sMerkleHasher> =
            proof.into();
        let json_bytes = match serde_json::to_vec(&proof_for_verifier) {
            Ok(b) => b,
            Err(e) => {
                eprintln!("stwo-sys: failed to serialize proof: {e}");
                return std::ptr::null_mut();
            }
        };

        let handle = Box::new(ProofHandle { bytes: json_bytes });
        Box::into_raw(handle)
    }
}

/// Verify a STARK proof.
///
/// Real impl: parses `proof_bytes` as JSON `CairoProofForRustVerifier`,
/// invokes `cairo_air::verifier::verify_cairo::<Blake2sMerkleChannel>`.
/// Matches `stoolap/stwo-plugin/src/verify.rs::verify_proof_internal`.
///
/// Returns 0 on Ok, 1 on Err (parse error or verification failure).
///
/// # Safety
///
/// - `proof_ptr` MUST be non-null and point to `proof_len` bytes of JSON proof.
/// - `public_ptr` MUST be non-null and point to `public_len` bytes.
///   (Public inputs are validated by cipherocto's wrapper BEFORE calling
///   the FFI; we accept them here for ABI parity.)
#[no_mangle]
pub unsafe extern "C" fn stwo_verify(
    proof_ptr: *const u8,
    proof_len: usize,
    public_ptr: *const u8,
    public_len: usize,
) -> i32 {
    // SAFETY: contract documented above; we trust the caller.
    unsafe {
        if proof_ptr.is_null() || public_ptr.is_null() {
            return 1; // Err: null pointer
        }
        let proof = std::slice::from_raw_parts(proof_ptr, proof_len);
        let _public = std::slice::from_raw_parts(public_ptr, public_len);

        // Parse proof as JSON CairoProofForRustVerifier.
        let proof_for_verifier: Result<CairoProofForRustVerifier<Blake2sMerkleHasher>, _> =
            serde_json::from_slice(proof);

        let proof_for_verifier = match proof_for_verifier {
            Ok(p) => p,
            Err(e) => {
                eprintln!("stwo-sys: failed to parse proof JSON: {e}");
                return 1; // Err: parse
            }
        };

        // Verify via real STWO.
        match cairo_air::verifier::verify_cairo::<Blake2sMerkleChannel>(proof_for_verifier) {
            Ok(()) => 0, // Ok
            Err(e) => {
                eprintln!("stwo-sys: verify_cairo failed: {e:?}");
                1 // Err: verification
            }
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
        let text = s.to_str().unwrap();
        assert!(text.starts_with("stwo-sys"));
        assert!(text.contains("real STWO"), "version string should advertise real STWO impl");
    }

    #[test]
    fn verify_rejects_invalid_json() {
        let bad_proof = b"not json";
        let public = b"unused";
        // SAFETY: pointers are valid for the call duration.
        let ret = unsafe {
            stwo_verify(
                bad_proof.as_ptr(),
                bad_proof.len(),
                public.as_ptr(),
                public.len(),
            )
        };
        assert_eq!(ret, 1, "invalid JSON should return Err (1)");
    }

    #[test]
    fn verify_rejects_empty_proof() {
        let empty: &[u8] = &[];
        let public = b"unused";
        // SAFETY: pointers are valid for the call duration.
        let ret = unsafe {
            stwo_verify(empty.as_ptr(), 0, public.as_ptr(), public.len())
        };
        assert_eq!(ret, 1, "empty proof should return Err (1)");
    }

    #[test]
    fn free_null_is_safe() {
        // SAFETY: free_null is documented to accept null.
        unsafe { stwo_free_proof(std::ptr::null_mut()) };
    }
}
