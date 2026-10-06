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

// ---- Cairo-VM harness (audit C-2 remediation, 2026-10-06) ----
//
// `stwo_prove` below takes a JSON `ProverInput` and hands it to STWO. That is
// correct, but nothing in the workspace could ever *produce* one: a
// `ProverInput` is a Cairo execution trace, so it only exists after the Cairo
// VM has actually run the circuit. Cipherocto's circuit had no way to receive a
// witness at all (it hardcoded its inputs), so the whole path terminated in a
// mock prover.
//
// The circuit now takes its witness as `main` parameters (see
// `cairo/src/lib.cairo` §Input surface). The harness below closes the loop:
// CASM bytes + serialized witness -> run the VM -> `adapt()` -> `ProverInput`.
//
// This lives in `stwo-sys` rather than `zk-vendor` because `cairo-vm` and
// `stwo-cairo-adapter` are nightly-only dependencies of the excluded cdylib
// crate; `zk-vendor` must stay on stable rust.
// Import paths are pinned to cairo-vm **3.1.0** (see Cargo.toml). 3.2.x moved
// `Program::from_bytes`, `LayoutName` and `CairoArg` to different modules and
// removed `run_from_entrypoint`; do not follow 3.2 docs here.
use cairo_vm::types::layout_name::LayoutName;
use cairo_vm::types::relocatable::MaybeRelocatable;
use cairo_vm::types::program::Program;
use cairo_vm::vm::runners::cairo_runner::{CairoArg, CairoRunner};
use cairo_vm::hint_processor::builtin_hint_processor::builtin_hint_processor_definition::BuiltinHintProcessor;
use cairo_vm::Felt252 as Felt;

use std::os::raw::c_char;

/// Opaque proof handle returned by `stwo_prove`. We hold the JSON-encoded
/// proof bytes (cipherocto consumes JSON-encoded
/// `CairoProofForRustVerifier`); FFI caller treats this as opaque pointer.
#[repr(C)]
pub struct ProofHandle {
    bytes: Vec<u8>,
}

/// The four arguments `capability_zk::main` accepts, mirrored from
/// `cairo/src/lib.cairo`. Serde field order MUST match the Cairo struct
/// declaration order, because `CairoArg::Composed` serializes positionally.
///
/// Mirrors `zk-circuit`'s `PublicInputs` / `PrivateWitness` / `TraceStep`.
#[derive(serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct PublicInputsArg {
    pub ask_id_lo: String,
    pub ask_id_hi: String,
    pub axes_count: u32,
    pub cap_root_hash_lo: String,
    pub cap_root_hash_hi: String,
    pub invocation_hash: String,
    pub holder_did: String,
    pub current_unix_time: u64,
    pub has_output_hash: u8,
    pub output_hash: String,
}

#[derive(serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct PrivateWitnessArg {
    pub cap_root_secret_lo: String,
    pub cap_root_secret_hi: String,
    pub caveats_count: u32,
    pub discharges_count: u32,
    pub inference_trace_present: u8,
}

#[derive(serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct TraceStepArg {
    pub op_code: String,
    pub input_hash: String,
    pub output_hash: String,
}

/// Everything the harness needs to execute the circuit: the public inputs, the
/// private witness, the caveat felts, and the inference trace steps. Passed as
/// a single JSON object so the ABI stays one buffer.
#[derive(serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct CairoRunInput {
    pub pub_inputs: PublicInputsArg,
    pub priv_witness: PrivateWitnessArg,
    pub caveat_felts: Vec<String>,
    pub steps: Vec<TraceStepArg>,
}

/// Parse a decimal or 0x-hex felt string into a `Felt`.
///
/// Accepting both matters: felts exceed 64 bits routinely, and the existing
/// CipherOcto witness types serialize 32-byte values as hex while small
/// integers stay decimal.
fn parse_felt(s: &str) -> Option<Felt> {
    let t = s.trim();
    if let Some(hex) = t.strip_prefix("0x") {
        Felt::from_hex(hex).ok()
    } else {
        t.parse::<Felt>().ok()
    }
}

/// Decode compiled CASM bytecode into the felt-code vector cairo-vm expects.
///
/// CASM bytecode packs each instruction as a 63-bit value, big-endian, with one
/// padding bit, so 8 bytes per felt. The high bit of each 8-byte group is the
/// padding and is masked off.
fn casm_bytes_to_felt_code(bytes: &[u8]) -> Vec<MaybeRelocatable> {
    const FELT_BYTES: usize = 8;
    const FELT_MASK: u64 = (1u64 << 63) - 1;
    bytes
        .chunks(FELT_BYTES)
        .map(|chunk| {
            let mut buf = [0u8; FELT_BYTES];
            buf[..chunk.len()].copy_from_slice(chunk);
            MaybeRelocatable::Int(Felt::from(u64::from_be_bytes(buf) & FELT_MASK))
        })
        .collect()
}

fn felt_arg(s: &str) -> Option<CairoArg> {
    parse_felt(s).map(|f| CairoArg::Single(f.into()))
}

/// Build the positional `CairoArg` list matching `capability_zk::main`.
///
/// Signature: `main(pub_inputs, priv_witness, caveat_felts, steps)`.
/// Returns `None` if any felt fails to parse or the caveat count disagrees
/// with the supplied felts (which the circuit also asserts).
fn build_run_args(input: &CairoRunInput) -> Option<Vec<CairoArg>> {
    let p = &input.pub_inputs;
    let w = &input.priv_witness;

    let pub_inputs = CairoArg::Composed(vec![
        felt_arg(&p.ask_id_lo)?,
        felt_arg(&p.ask_id_hi)?,
        Some(CairoArg::Single(Felt::from(p.axes_count).into()))?,
        felt_arg(&p.cap_root_hash_lo)?,
        felt_arg(&p.cap_root_hash_hi)?,
        felt_arg(&p.invocation_hash)?,
        felt_arg(&p.holder_did)?,
        felt_arg(&p.current_unix_time.to_string())?,
        Some(CairoArg::Single(Felt::from(p.has_output_hash).into()))?,
        felt_arg(&p.output_hash)?,
    ]);

    let priv_witness = CairoArg::Composed(vec![
        felt_arg(&w.cap_root_secret_lo)?,
        felt_arg(&w.cap_root_secret_hi)?,
        Some(CairoArg::Single(Felt::from(w.caveats_count).into()))?,
        Some(CairoArg::Single(Felt::from(w.discharges_count).into()))?,
        Some(CairoArg::Single(Felt::from(w.inference_trace_present).into()))?,
    ]);

    if input.caveat_felts.len() != w.caveats_count as usize {
        return None;
    }
    let mut caveat_felts = Vec::with_capacity(input.caveat_felts.len());
    for c in &input.caveat_felts {
        caveat_felts.push(parse_felt(c)?.into());
    }
    let caveat_felts = CairoArg::Array(caveat_felts);

    // `CairoArg::Array` carries scalars only, so a nested `Array<TraceStep>`
    // cannot cross the boundary as such. The harness passes the steps FLAT
    // (op, input_hash, output_hash per step) and the circuit regroups them by
    // index. `TRACE_STRIDE` on the circuit side must stay in lockstep.
    let mut steps = Vec::with_capacity(input.steps.len() * 3);
    for s in &input.steps {
        steps.push(parse_felt(&s.op_code)?.into());
        steps.push(parse_felt(&s.input_hash)?.into());
        steps.push(parse_felt(&s.output_hash)?.into());
    }
    let steps = CairoArg::Array(steps);

    Some(vec![pub_inputs, priv_witness, caveat_felts, steps])
}

/// Execute compiled CASM against a serialized witness and produce the
/// `ProverInput` that STWO consumes.
///
/// Returns the JSON-encoded `ProverInput`, or `None` on any failure (bad
/// CASM, VM panic in the circuit, argument-shape mismatch, or an
/// adapter error). Callers distinguish these only by the stderr diagnostic.
fn run_cairo_to_prover_input(casm: &[u8], run_input_json: &[u8]) -> Option<Vec<u8>> {
    let input: CairoRunInput = match serde_json::from_slice(run_input_json) {
        Ok(i) => i,
        Err(e) => {
            eprintln!("stwo-sys: harness: bad run-input JSON: {e}");
            return None;
        }
    };

    let args = match build_run_args(&input) {
        Some(a) => a,
        None => {
            eprintln!(
                "stwo-sys: harness: could not build run args (bad felt, or caveats_count {} \
                 disagrees with {} supplied caveat felts)",
                input.priv_witness.caveats_count,
                input.caveat_felts.len()
            );
            return None;
        }
    };

    // cairo-vm 3.1.0 has NO raw-CASM loader: `Program::from_bytes` parses a
    // compiled-program **JSON**, and `CairoProgram` does not exist in this
    // version. Upstream stwo-cairo sidesteps this by decompiling into
    // `cairo_lang_casm::Instruction` and calling its own `program_from_casm`,
    // which re-encodes via `instruction.assemble().encode()`.
    //
    // That re-encode is a no-op round trip: the bytes zk-circuit emits already
    // ARE the encoded instruction stream. So decode them directly as 63-bit
    // big-endian bit-packed felts and build the proof program, skipping the
    // cairo-lang-casm dependency entirely.
    let felt_code = casm_bytes_to_felt_code(casm);
    if felt_code.is_empty() {
        eprintln!("stwo-sys: harness: CASM bytecode is empty");
        return None;
    }
    let program = Program::new_for_proof(
        vec![],
        felt_code,
        0,
        1,
        std::collections::HashMap::default(),
        Default::default(),
        Default::default(),
        Default::default(),
        Default::default(),
    )
    .ok()?;

    let mut runner = match CairoRunner::new(
        &program,
        LayoutName::all_cairo_stwo,
        None,
        true,
        true,
        true,
    ) {
        Ok(r) => r,
        Err(e) => {
            eprintln!("stwo-sys: harness: runner creation failed: {e:?}");
            return None;
        }
    };

    // `initialize` sets the program base / exec base. Without it the runner has
    // no exec segment and `run_from_entrypoint` fails with `NoExecBase`. This
    // matches upstream stwo-cairo's adapter setup.
    if let Err(e) = runner.initialize(true) {
        eprintln!("stwo-sys: harness: runner initialize failed: {e:?}");
        return None;
    }

    // Entry point 0 is `main`, the only entry point scarb emits for an
    // executable target. `BuiltinHintProcessor::new_empty()` is deliberate and
    // matches upstream stwo-cairo's adapter tests: nothing is injected via
    // hints, the witness arrives as `CairoArg`s on the stack.
    let mut hint_processor = BuiltinHintProcessor::new_empty();
    let arg_refs: Vec<&CairoArg> = args.iter().collect();
    if let Err(e) =
        runner.run_from_entrypoint(0, &arg_refs, true, None, &mut hint_processor)
    {
        eprintln!("stwo-sys: harness: circuit execution failed: {e:?}");
        return None;
    }

    let prover_input: ProverInput = match stwo_cairo_adapter::adapter::adapt(&runner) {
        Ok(p) => p,
        Err(e) => {
            eprintln!("stwo-sys: harness: adapt failed: {e:?}");
            return None;
        }
    };

    serde_json::to_vec(&prover_input).ok()
}

/// Prove by executing the compiled circuit against a serialized witness.
///
/// This is the entry point that makes STWO meaningful: it runs the Cairo VM
/// over `casm_bytes` with `run_input_json`, extracts the execution trace via
/// `stwo_cairo_adapter::adapter::adapt`, and proves that trace. The plain
/// `stwo_prove` remains for callers that already hold a `ProverInput`.
///
/// `run_input_json` is the `CairoRunInput` shape:
/// `{ "pub_inputs": {...}, "priv_witness": {...}, "caveat_felts": [...], "steps": [...] }`
/// with felts as decimal or `0x`-prefixed hex strings.
///
/// Returns null on any failure — malformed JSON, unparsable CASM, a circuit
/// assertion panic (which is how an invalid witness is rejected), or an
/// upstream prover error.
///
/// # Safety
///
/// - All pointers MUST be non-null and point to `len`-byte buffers that stay
///   valid for the duration of the call.
/// - The returned handle MUST be released via `stwo_free_proof`.
#[no_mangle]
pub unsafe extern "C" fn stwo_prove_from_casm(
    casm_ptr: *const u8,
    casm_len: usize,
    run_input_ptr: *const u8,
    run_input_len: usize,
    public_ptr: *const u8,
    public_len: usize,
) -> *mut ProofHandle {
    // SAFETY: contract documented above; we trust the caller.
    unsafe {
        if casm_ptr.is_null() || run_input_ptr.is_null() || public_ptr.is_null() {
            return std::ptr::null_mut();
        }
        let casm = std::slice::from_raw_parts(casm_ptr, casm_len);
        let run_input = std::slice::from_raw_parts(run_input_ptr, run_input_len);
        let _public = std::slice::from_raw_parts(public_ptr, public_len);

        // CASM -> Cairo execution trace -> ProverInput. This is the step that
        // was missing: without it the commitment was computed over caller
        // bytes rather than over an actual execution.
        let prover_input_json = match run_cairo_to_prover_input(casm, run_input) {
            Some(j) => j,
            None => {
                eprintln!("stwo-sys: prove_from_casm: harness failed");
                return std::ptr::null_mut();
            }
        };

        let prover_input: ProverInput = match serde_json::from_slice(&prover_input_json) {
            Ok(p) => p,
            Err(e) => {
                eprintln!("stwo-sys: prove_from_casm: bad ProverInput JSON: {e}");
                return std::ptr::null_mut();
            }
        };

        let proof = match prove_cairo::<Blake2sMerkleChannel>(prover_input, default_prover_params())
        {
            Ok(p) => p,
            Err(e) => {
                eprintln!("stwo-sys: prove_from_casm: prove_cairo failed: {e:?}");
                return std::ptr::null_mut();
            }
        };

        let proof_for_verifier: CairoProofForRustVerifier<Blake2sMerkleHasher> = proof.into();
        let json_bytes = match serde_json::to_vec(&proof_for_verifier) {
            Ok(b) => b,
            Err(e) => {
                eprintln!("stwo-sys: prove_from_casm: serialize failed: {e}");
                return std::ptr::null_mut();
            }
        };

        Box::into_raw(Box::new(ProofHandle { bytes: json_bytes }))
    }
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

#[cfg(test)]
mod harness_tests {
    use super::*;

    /// The harness must turn CASM + a serialized witness into a real
    /// `ProverInput` — i.e. an actual Cairo execution trace, not a digest of
    /// caller bytes. This is the property the whole C-2 remediation rests on.
    #[test]
    fn harness_produces_prover_input_from_a_real_execution() {
        // Compiled circuit, produced by the workspace pipeline.
        let casm = match std::fs::read(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("tests/fixtures/capability_zk.casm"),
        ) {
            Ok(c) => c,
            Err(e) => panic!("fixture CASM missing: {e}"),
        };

        let run_input = r#"{
          "pub_inputs": {
            "ask_id_lo": "0x11111111111111111111111111111111",
            "ask_id_hi": "0x11111111111111111111111111111111",
            "axes_count": 1,
            "cap_root_hash_lo": "0",
            "cap_root_hash_hi": "0",
            "invocation_hash": "0x33333333333333333333333333333333",
            "holder_did": "0x686f6c6465722d766563746f722d747631",
            "current_unix_time": 1700000000,
            "has_output_hash": 0,
            "output_hash": "0"
          },
          "priv_witness": {
            "cap_root_secret_lo": "0x42424242424242424242424242424242",
            "cap_root_secret_hi": "0x42424242424242424242424242424242",
            "caveats_count": 3,
            "discharges_count": 0,
            "inference_trace_present": 1
          },
          "caveat_felts": ["0xc0a1", "0xc0a2", "0xc0a3"],
          "steps": [
            {"op_code": "0", "input_hash": "0x33333333333333333333333333333333",
             "output_hash": "0x44444444444444444444444444444444444444444444444444444444444444"}
          ]
        }"#;

        let json = run_cairo_to_prover_input(&casm, run_input.as_bytes())
            .expect("harness must produce a ProverInput");

        let parsed: ProverInput = serde_json::from_slice(&json).expect("round-trips");
        // A real execution yields a non-trivial trace. Assert on the serialized
        // form rather than a specific collection API, which differs between
        // stwo-cairo versions.
        let encoded = serde_json::to_string(&parsed).expect("ProverInput serializes");
        assert!(
            encoded.len() > 1000,
            "a real execution must yield a substantial trace; got {} bytes",
            encoded.len()
        );
    }

    /// The harness must REJECT a witness whose declared caveat count disagrees
    /// with the felts supplied. Before the input surface existed this assertion
    /// compared a hardcoded constant with itself and could not fail.
    #[test]
    fn harness_rejects_caveat_count_mismatch() {
        let run_input = r#"{
          "pub_inputs": {"ask_id_lo":"0","ask_id_hi":"0","axes_count":1,
            "cap_root_hash_lo":"0","cap_root_hash_hi":"0","invocation_hash":"0",
            "holder_did":"0","current_unix_time":0,"has_output_hash":0,"output_hash":"0"},
          "priv_witness": {"cap_root_secret_lo":"0","cap_root_secret_hi":"0",
            "caveats_count":3,"discharges_count":0,"inference_trace_present":1},
          "caveat_felts": ["0xc0a1", "0xc0a2"],
          "steps": [{"op_code":"0","input_hash":"0","output_hash":"0"}]
        }"#;
        let input: CairoRunInput = serde_json::from_str(run_input).expect("parses");
        assert!(
            build_run_args(&input).is_none(),
            "declared 3 caveats but supplied 2 felts must be rejected"
        );
    }
}
