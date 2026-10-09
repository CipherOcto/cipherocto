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
use stwo::core::fri::FriConfig;
use stwo::core::pcs::PcsConfig;
use stwo::core::vcs_lifted::blake2_merkle::{Blake2sMerkleChannel, Blake2sMerkleHasher};
use stwo_cairo_adapter::ProverInput;
use stwo_cairo_common::preprocessed_columns::preprocessed_trace::PreProcessedTraceVariant;
use stwo_cairo_prover::prover::{prove_cairo, ChannelHash, ProverParameters};

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
// `Executable` artifact + serialized witness -> run the VM -> `adapt()` ->
// `ProverInput`.
//
// This lives in `stwo-sys` rather than `zk-vendor` because `cairo-vm` and
// `stwo-cairo-adapter` are nightly-only dependencies of the excluded cdylib
// crate; `zk-vendor` must stay on stable rust.
// Import paths are pinned to cairo-vm **3.2.0** (see Cargo.toml). This crate
// was previously on 3.1.0, where `Program::from_bytes`, `LayoutName` and
// `CairoArg` lived in different modules and `run_from_entrypoint` existed;
// 3.2.0 moved/removed them. When bumping further, check those three against
// the new tree rather than assuming either layout.
// Cairo executable ABI (see docs/architecture/zk-stwo-cairo-executable-recipe.md).
// `Executable` is the program artifact; `build_hints_dict` / `CairoHintProcessor`
// are how the circuit's arguments reach the VM, as calldata through hints rather
// than on the stack.
use cairo_lang_executable::executable::EntryPointKind;
use cairo_lang_runner::{build_hints_dict, Arg, CairoHintProcessor};
use cairo_vm::cairo_run::{cairo_run_program_with_initial_scope, CairoRunConfig};
use cairo_vm::types::exec_scope::ExecutionScopes;
use cairo_vm::types::layout_name::LayoutName;
use cairo_vm::types::program::Program;
use cairo_vm::types::relocatable::MaybeRelocatable;
use cairo_vm::Felt252 as Felt;

use std::os::raw::c_char;

/// Opaque proof handle returned by `stwo_prove`. We hold the JSON-encoded
/// proof bytes (cipherocto consumes JSON-encoded
/// `CairoProofForRustVerifier`); FFI caller treats this as opaque pointer.
#[repr(C)]
pub struct ProofHandle {
    bytes: Vec<u8>,
}

/// The four arguments the capability circuit entry point accepts, mirrored
/// from `cairo/src/lib.cairo`. Serde field order MUST match the Cairo struct
/// declaration order, because the executable ABI deserializes calldata
/// positionally.
///
/// The Rust-side producer of this JSON is `octo_cap_zk` (which owns the
/// mint-path `PublicInputs` / `PrivateWitness` / `TraceStep` types). These
/// are deliberately *not* shared types: this crate is a nightly cdylib that
/// `zk-vendor` loads dynamically, so a shared crate could not be a
/// dependency of both without making stable consumers require nightly.
/// The mirror is pinned by the `calldata_matches_cairo_serde_layout` and
/// `calldata_rejects_unparsable_felt` tests below, plus
/// `zk_circuit::build_cairo_run_input`'s emitter test.
#[derive(serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct PublicInputsArg {
    pub ask_id_lo: String,
    pub ask_id_hi: String,
    pub axes_count: u32,
    pub cap_root_hash_lo: String,
    pub cap_root_hash_hi: String,
    /// Low half of the macaroon chain tip, hex-encoded felt. Must match the
    /// declaration order in `cairo/src/lib.cairo`.
    pub chain_tip_lo: String,
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
    /// The 32-byte input hash, split into two 16-byte felt-halves.
    ///
    /// **Split, not single.** A felt252 holds ~251 bits and a 32-byte digest is
    /// 256, so one felt cannot carry a real hash. The previous single-field
    /// shape sent each hash through `parse_felt`, which rejected every real
    /// 32-byte value with "representative out of range" — so `build_calldata`
    /// returned `None` and no genuine PoI trace could reach the circuit. This
    /// mirrors `PublicInputsArg::cap_root_hash_lo` / `_hi`, which already split
    /// a 32-byte value the same way.
    pub input_hash_lo: String,
    pub input_hash_hi: String,
    pub output_hash_lo: String,
    pub output_hash_hi: String,
}

/// Everything the harness needs to execute the circuit: the public inputs, the
/// private witness, the chain witness arrays, and the inference trace steps.
/// Passed as a single JSON object so the ABI stays one buffer.
///
/// **The three chain arrays are `#[serde(default)]` and that is safe only
/// because the circuit fails closed on them.** An absent `mint_felts` derives
/// `chain[0]` without the nonce, which cannot match the verifier-supplied tip;
/// absent `prev_ids` trips `CaveatIdDepthMismatch`. Defaulting keeps existing
/// JSON producers working while any *semantically* wrong value is still refused
/// inside the program. What is NOT acceptable is the deserializer silently
/// absorbing an array in the wrong position, which is why
/// `the_calldata_layout_matches_the_cairo_main_signature` reads the Cairo
/// source rather than trusting this comment.
#[derive(serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct CairoRunInput {
    pub pub_inputs: PublicInputsArg,
    pub priv_witness: PrivateWitnessArg,
    /// `bytes_to_felts(MACAR_ID_DOMAIN ‖ mint_nonce)` — folded into the chain seed.
    #[serde(default)]
    pub mint_felts: Vec<String>,
    /// The packed caveat messages, concatenated. Length-prefixed per caveat.
    #[serde(default)]
    pub caveat_felts: Vec<String>,
    /// Felt width of each caveat, in the same order as `caveat_felts`.
    #[serde(default)]
    pub caveat_widths: Vec<String>,
    /// Two felts per caveat: the `capability_id` in force at that step, split
    /// into two 128-bit halves.
    #[serde(default)]
    pub prev_ids: Vec<String>,
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

/// The program artifact: a real Cairo **`Executable`**, serialized.
///
/// This is the upstream `cairo_lang_executable::executable::Executable` type
/// itself — bytecode, the typed hint table, and the entrypoint list — not a
/// hand-rolled mirror. That matters: the executable wrapper is what carries
/// `main`'s arguments as calldata, and a wrapper that silently disagrees with
/// the harness produces a program that runs *something* without ever reaching
/// the circuit. Using the upstream type removes that whole class of wire
/// drift.
///
/// Produced by `zk_circuit::bundled_executable_json()`. See
/// `docs/architecture/zk-stwo-cairo-executable-recipe.md`.
pub type CairoProgramSpec = cairo_lang_executable::executable::Executable;

/// Width, in felts, of one serialized `TraceStep`.
///
/// `op_code` + two felts per 32-byte hash (input, output) = 5.
///
/// Mirrors `cairo/src/lib.cairo::TRACE_STRIDE`. **Keep in lockstep** — the
/// conformance test below asserts the whole slot, not a sample, precisely
/// because a mismatch here produces a proof over attacker-chosen inputs with
/// every internal hash check self-consistent.
const TRACE_STRIDE: usize = 5;

/// Serialize the harness's typed input into the flat `Array<felt252>` calldata
/// the executable wrapper deserializes.
///
/// Layout, in `main`'s declaration order:
///
/// ```text
/// [ PublicInputs(11) , PrivateWitness(5)
/// , [len, mint_felts…] , [len, caveat_felts…] , [len, caveat_widths…]
/// , [len, prev_ids…] , [len, steps_flat…] ]
/// ```
///
/// Cairo's `Serde` for a struct is a **plain member sequence** — no header, no
/// offsets — and `ArraySerde` is `[len, elems…]` with the element pointer at
/// `elems[0]` (`core/src/array.cairo`).
///
/// The array order is the part that rots silently. Adding a parameter to the
/// Cairo entry point shifts every array after it, and a deserializer handed the
/// old shape does not error — it reads a length cell as a value and carries on.
/// That is exactly how a proof ends up over attacker-chosen inputs with every
/// internal check self-consistent, which is what
/// `the_calldata_layout_matches_the_cairo_main_signature` exists to stop. It
/// reads `cairo/src/lib.cairo` rather than trusting this comment, because a
/// comment and a test that agree with each other catch nothing.
///
/// `steps_flat` is the flat `[op, input, output]` encoding the circuit folds
/// directly, which is why the step fields are flattened here rather than
/// reconstructed as structs.
fn build_calldata(input: &CairoRunInput) -> Option<Vec<Felt>> {
    let p = &input.pub_inputs;
    let w = &input.priv_witness;

    let mut out = Vec::with_capacity(
        16 + 1
            + input.mint_felts.len()
            + 1
            + input.caveat_felts.len()
            + 1
            + input.caveat_widths.len()
            + 1
            + input.prev_ids.len()
            + 1
            + input.steps.len() * TRACE_STRIDE,
    );

    // PublicInputs — declaration order from `cairo/src/lib.cairo`.
    out.push(parse_felt(&p.ask_id_lo)?);
    out.push(parse_felt(&p.ask_id_hi)?);
    out.push(Felt::from(p.axes_count));
    out.push(parse_felt(&p.cap_root_hash_lo)?);
    out.push(parse_felt(&p.cap_root_hash_hi)?);
    out.push(parse_felt(&p.chain_tip_lo)?);
    out.push(parse_felt(&p.invocation_hash)?);
    out.push(parse_felt(&p.holder_did)?);
    out.push(Felt::from(p.current_unix_time));
    out.push(Felt::from(p.has_output_hash));
    out.push(parse_felt(&p.output_hash)?);

    // PrivateWitness — declaration order.
    out.push(parse_felt(&w.cap_root_secret_lo)?);
    out.push(parse_felt(&w.cap_root_secret_hi)?);
    out.push(Felt::from(w.caveats_count));
    out.push(Felt::from(w.discharges_count));
    out.push(Felt::from(w.inference_trace_present));

    // Chain witness arrays, in `main`'s order. Each is [len, elems…].
    for array in [
        &input.mint_felts,
        &input.caveat_felts,
        &input.caveat_widths,
        &input.prev_ids,
    ] {
        out.push(Felt::from(array.len()));
        for cell in array {
            out.push(parse_felt(cell)?);
        }
    }

    // steps_flat: Array<felt252> = [len, elems…], stride TRACE_STRIDE.
    let flat_len = input.steps.len() * TRACE_STRIDE;
    out.push(Felt::from(flat_len));
    for st in &input.steps {
        out.push(parse_felt(&st.op_code)?);
        out.push(parse_felt(&st.input_hash_lo)?);
        out.push(parse_felt(&st.input_hash_hi)?);
        out.push(parse_felt(&st.output_hash_lo)?);
        out.push(parse_felt(&st.output_hash_hi)?);
    }

    Some(out)
}

/// Execute the compiled Executable against a serialized witness and produce
/// the `ProverInput` that STWO consumes.
///
/// Returns the JSON-encoded `ProverInput`, or `None` on any failure — malformed
/// JSON, an unparsable felt, a circuit assertion failure, or an adapter error.
///
/// **Why a failed assertion is a hard error here.** A Cairo 2.x `assert!`
/// compiles into a call to `core::panics::panic`, which writes its payload and
/// *returns through the caller*; cairo-vm's `VirtualMachineError` has no
/// `Panic` variant, so calling the bare `main` directly yields an ordinary
/// `ProverInput` for a rejected witness. The executable wrapper closes that
/// hole: it checks the panic flag and fails with
/// `ASSERT_EQ instruction failed: 0 != 1`. Gating on the *run result* is
/// therefore a real check, not a formality — and it is what makes the
/// positive control in `harness_accepts_a_satisfying_witness` meaningful.
fn run_cairo_to_prover_input(spec: &CairoProgramSpec, run_input_json: &[u8]) -> Option<Vec<u8>> {
    let input: CairoRunInput = serde_json::from_slice(run_input_json).ok()?;
    let calldata = build_calldata(&input)?;

    let entrypoint = spec
        .entrypoints
        .iter()
        .find(|e| matches!(e.kind, EntryPointKind::Standalone))?;

    // The wrapper header is `ap += builtins.len(); call rel 4; jmp rel 0`; the
    // `call`'s return address is `offset + 4`, and that is where the trace ends.
    let data: Vec<MaybeRelocatable> = spec
        .program
        .bytecode
        .iter()
        .map(Felt::from)
        .map(MaybeRelocatable::from)
        .collect();
    let (hints, string_to_hint) = build_hints_dict(&spec.program.hints);

    let program = Program::new_for_proof(
        entrypoint.builtins.clone(),
        data,
        entrypoint.offset,
        entrypoint.offset + 4,
        hints,
        Default::default(),
        Default::default(),
        vec![],
        None,
    )
    .ok()?;

    // Arguments cross the boundary as calldata through the hint processor, NOT
    // on the stack. `user_args` is one inner `Vec<Arg>` per *user parameter*;
    // the wrapper has exactly one (the calldata array).
    let mut hint_processor = CairoHintProcessor {
        runner: None,
        user_args: vec![vec![Arg::Array(
            calldata.into_iter().map(Arg::Value).collect(),
        )]],
        string_to_hint,
        starknet_state: Default::default(),
        run_resources: Default::default(),
        syscalls_used_resources: Default::default(),
        no_temporary_segments: false,
        markers: Default::default(),
        panic_traceback: Default::default(),
    };

    // ProofModeCanonical is what produces the canonical public memory the
    // verifier checks: `[initial_ap - 2] == initial_ap`, `[initial_ap - 1] == 0`.
    let runner = cairo_run_program_with_initial_scope(
        &program,
        &CairoRunConfig {
            trace_enabled: true,
            relocate_trace: false,
            layout: LayoutName::all_cairo_stwo,
            fill_holes: true,
            proof_mode: true,
            disable_trace_padding: true,
            ..Default::default()
        },
        &mut hint_processor,
        ExecutionScopes::new(),
    )
    .ok()?;

    let mut prover_input = stwo_cairo_adapter::adapter::adapt(&runner).ok()?;

    // `adapt()` hardcodes `PublicSegmentContext::bootloader_context()` (its own
    // `TODO(spapini): take this from the input`), which is `[true; 11]`. That is
    // wrong twice over here:
    //
    // 1. `LayoutName::all_cairo_stwo` has NINE builtins — no ecdsa, no keccak —
    //    so zipping 9 real ranges against 11 names mislabels every segment from
    //    `bitwise` onward and lands poseidon's cells on `ec_op`.
    // 2. The prover reads `n = count(present)` segment start pointers off the
    //    stack, and cairo-vm only pushes pointers for `program.builtins` — not
    //    every builtin the layout instantiates.
    //
    // So the context must mirror the *declared* entrypoint builtins. Getting
    // this wrong surfaces as `ECDSA segment is not empty` (case 1) or
    // `Builtin segment start doesn't match claim` (case 2).
    prover_input.public_segment_context =
        stwo_cairo_adapter::PublicSegmentContext::new(&entrypoint.builtins);

    serde_json::to_vec(&prover_input).ok()
}

/// Prove by executing the compiled circuit against a serialized witness.
///
/// This is the entry point that makes STWO meaningful: it runs the Cairo VM
/// over `program_json` with `run_input_json`, extracts the execution trace
/// via `stwo_cairo_adapter::adapter::adapt`, and proves that trace. The
/// plain `stwo_prove` remains for callers that already hold a
/// `ProverInput`.
///
/// `program_json` is the [`CairoProgramSpec`] shape, i.e. a JSON-encoded
/// upstream `cairo_lang_executable::executable::Executable`: a bytecode
/// word vector, a hint table, and an entrypoint table naming each
/// entrypoint's offset and *declared* builtins. The bundled circuit is
/// emitted by `cargo test -p zk-circuit --test dump_casm_fixture -- --ignored`.
///
/// It is the upstream `Executable` rather than a hand-rolled mirror because
/// the entrypoint wrapper (`Executable::new` emits `ap += builtins.len();
/// call rel 4; jmp rel 0;`) and the argument-delivery ABI are the compiler's
/// contract, not ours; mirroring them is how the wire shape drifts out from
/// under the prover. Note that the `PublicSegmentContext` must be built from
/// the entrypoint's *declared* builtins, not the runner's layout-wide set —
/// the prover reads segment pointers off the stack and cairo-vm only pushes
/// pointers for `program.builtins`.
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
    program_ptr: *const u8,
    program_len: usize,
    run_input_ptr: *const u8,
    run_input_len: usize,
    public_ptr: *const u8,
    public_len: usize,
) -> *mut ProofHandle {
    // SAFETY: contract documented above; we trust the caller.
    unsafe {
        if program_ptr.is_null() || run_input_ptr.is_null() || public_ptr.is_null() {
            return std::ptr::null_mut();
        }
        let program_json = std::slice::from_raw_parts(program_ptr, program_len);
        let run_input = std::slice::from_raw_parts(run_input_ptr, run_input_len);
        let _public = std::slice::from_raw_parts(public_ptr, public_len);

        let spec: CairoProgramSpec = match serde_json::from_slice(program_json) {
            Ok(s) => s,
            Err(e) => {
                eprintln!("stwo-sys: bad program JSON: {e}");
                return std::ptr::null_mut();
            }
        };

        // CASM -> Cairo execution trace -> ProverInput. This is the step that
        // was missing: without it the commitment was computed over caller
        // bytes rather than over an actual execution.
        let prover_input_json = match run_cairo_to_prover_input(&spec, run_input) {
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
                // Named `line_fold_step` in stwo 2.1.0; renamed by stwo 2.3.0.
                fold_step: 1,
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
/// real STWO proof is bound to the witness and `ProverInput` alone. The CASM
/// hash and the public inputs are checked by the cipherocto wrapper BEFORE
/// calling the FFI.
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
        let proof = match prove_cairo::<Blake2sMerkleChannel>(prover_input, default_prover_params())
        {
            Ok(p) => p,
            Err(e) => {
                eprintln!("stwo-sys: prove_cairo failed: {e:?}");
                return std::ptr::null_mut();
            }
        };

        // Convert to verifier-compatible format + JSON-encode for FFI.
        let proof_for_verifier: CairoProofForRustVerifier<Blake2sMerkleHasher> = proof.into();
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

/// Length in bytes of the serialized proof held by `handle`.
///
/// Returns 0 for a null handle. Pairs with [`stwo_proof_copy`]: the caller
/// allocates, asks for the length, copies, then calls `stwo_free_proof`.
///
/// This exists because the Rust-side `ProofHandle` is opaque to the FFI
/// caller. Without an accessor the prover could compute a proof and then
/// nothing but a digest of the *inputs* could cross the boundary — which is
/// exactly the C-2 defect: `stwo_verify` was being handed 32 bytes instead
/// of a proof, so every real proof was rejected as unparsable.
///
/// # Safety
///
/// `handle` MUST be a pointer returned by `stwo_prove` /
/// `stwo_prove_from_casm` that has not yet been freed.
#[no_mangle]
pub unsafe extern "C" fn stwo_proof_len(handle: *const ProofHandle) -> usize {
    if handle.is_null() {
        return 0;
    }
    // SAFETY: contract says this pointer came from `stwo_prove`, so it is a
    // live `Box<ProofHandle>` that no one has mutated.
    unsafe { (*handle).bytes.len() }
}

/// Copy the serialized proof out of `handle` into `dst`.
///
/// Returns the number of bytes written, or 0 on a null handle, null `dst`, or
/// a `dst` smaller than [`stwo_proof_len`] (in which case nothing is written).
///
/// # Safety
///
/// - `handle` MUST be a live pointer from `stwo_prove` / `stwo_prove_from_casm`.
/// - `dst` MUST point to at least `len` writable bytes.
#[no_mangle]
pub unsafe extern "C" fn stwo_proof_copy(
    handle: *const ProofHandle,
    dst: *mut u8,
    len: usize,
) -> usize {
    if handle.is_null() || dst.is_null() {
        return 0;
    }
    // SAFETY: contract says this pointer came from `stwo_prove`, so the
    // `Vec` it owns is alive and not aliased mutably.
    let bytes = unsafe { &(*handle).bytes };
    if len < bytes.len() {
        return 0;
    }
    // SAFETY: caller guarantees `dst` has `len` writable bytes and
    // `len >= bytes.len()`.
    unsafe { std::ptr::copy_nonoverlapping(bytes.as_ptr(), dst, bytes.len()) };
    bytes.len()
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
        assert!(
            text.contains("real STWO"),
            "version string should advertise real STWO impl"
        );
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
        let ret = unsafe { stwo_verify(empty.as_ptr(), 0, public.as_ptr(), public.len()) };
        assert_eq!(ret, 1, "empty proof should return Err (1)");
    }

    #[test]
    fn free_null_is_safe() {
        // SAFETY: free_null is documented to accept null.
        unsafe { stwo_free_proof(std::ptr::null_mut()) };
    }
}

#[cfg(test)]
#[cfg(test)]
mod harness_tests {
    use super::*;

    /// Raw bytes of the bundled `Executable` fixture — the exact artifact the
    /// mint path hands the FFI.
    fn fixture_bytes() -> Vec<u8> {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures/capability_zk.json");
        std::fs::read(&path).unwrap_or_else(|e| panic!("fixture {} missing: {e}", path.display()))
    }

    fn fixture_spec() -> CairoProgramSpec {
        let raw = fixture_bytes();
        serde_json::from_slice(&raw).unwrap_or_else(|e| {
            panic!(
                "fixture is not a Cairo Executable: {e}. Regenerate with \
                 `cargo test -p zk-circuit --test dump_casm_fixture -- --ignored`"
            )
        })
    }

    /// A witness that satisfies **every** `stage2_main` assertion.
    ///
    /// This is not an assumption: `cairo/src/lib.cairo::tests::harness_witness_is_satisfying`
    /// runs `stage2_main` on these exact values under `scarb cairo-test` and
    /// asserts it returns `1`.
    ///
    /// * `axes_count = 1` (< 1000)
    /// * `caveats_count = 3` == the caveat count the chain walk sees
    /// * `has_output_hash = 0`, so the trace-root binding comparison is not armed
    /// * one `TraceStep` → `steps_flat.len() == 5 == TRACE_STRIDE`, non-zero
    ///
    /// The chain arrays are the shape `build_cairo_run_input` emits: two mint
    /// felts, three caveat felts, one width each, and two id felts per caveat.
    fn satisfying_input() -> String {
        r#"{
          "pub_inputs": {
            "ask_id_lo": "1", "ask_id_hi": "2", "axes_count": 1,
            "cap_root_hash_lo": "4", "cap_root_hash_hi": "5", "chain_tip_lo": "240762416164593195965731284530788510451429005798628693311909781404607892050",
            "invocation_hash": "6", "holder_did": "7",
            "current_unix_time": 1700000000, "has_output_hash": 0, "output_hash": "0"
          },
          "priv_witness": {
            "cap_root_secret_lo": "11", "cap_root_secret_hi": "12",
            "caveats_count": 3, "discharges_count": 0, "inference_trace_present": 1
          },
          "mint_felts": ["41", "42"],
          "caveat_felts": ["15", "16", "22"],
          "caveat_widths": ["1", "1", "1"],
          "prev_ids": ["51", "52", "53", "54", "55", "56"],
          "steps": [{"op_code": "17", "input_hash_lo": "18", "input_hash_hi": "0", "output_hash_lo": "19", "output_hash_hi": "0"}]
        }"#
        .to_owned()
    }

    /// The satisfying witness with individual JSON fields replaced, so each
    /// mutation test changes exactly one thing.
    ///
    /// The replacement text is spliced in **verbatim, including quotes**. The
    /// schema is mixed — `axes_count` / `caveats_count` are JSON numbers, but
    /// every felt field is a JSON *string* — so a felt override must be
    /// written `"cap_root_secret_lo", "\"987654321\""`. Writing the bare
    /// number yields `"cap_root_secret_lo": 987654321`, which fails to
    /// deserialize and is rejected as a parse error rather than as the
    /// circuit rejection the test is trying to provoke.
    fn input_with(overrides: &[(&str, &str)]) -> String {
        let mut json = satisfying_input();
        for (k, v) in overrides {
            let needle = format!("\"{k}\":");
            let start = json
                .find(needle.as_str())
                .unwrap_or_else(|| panic!("no key {k}"))
                + needle.len();
            let end = json[start..]
                .find([',', '\n'])
                .map_or(json.len(), |i| start + i);
            json.replace_range(start..end, v);
        }
        json
    }

    // ------------------------------------------------------------------
    // POSITIVE CONTROL
    // ------------------------------------------------------------------

    /// **The positive control.** A witness the circuit accepts must be accepted
    /// by the harness.
    ///
    /// This is the test whose absence made every `harness_rejects_*` test below
    /// worthless: an `assert!(...is_none())` is satisfied by a harness that
    /// rejects *everything*, including one that never runs the circuit. Without
    /// an acceptance test those guards prove nothing.
    #[test]
    fn harness_accepts_a_satisfying_witness() {
        let spec = fixture_spec();
        let json = satisfying_input();
        let prover_input = run_cairo_to_prover_input(&spec, json.as_bytes())
            .expect("a circuit-satisfying witness must produce a ProverInput");

        let parsed: ProverInput =
            serde_json::from_slice(&prover_input).expect("ProverInput round-trips");
        assert!(
            !parsed.public_memory_addresses.is_empty(),
            "a real execution has public memory; an empty set means the trace \
             was never populated"
        );
        assert!(parsed.pc_count > 0, "a real execution has program counters");
    }

    /// Audit finding C-2 is **closed**, and this is the test that says so.
    ///
    /// This used to be a characterization test asserting the opposite: the
    /// circuit computed a witness-only chain commitment, asserted only that it
    /// was non-zero, and so accepted an unrelated root secret. It was written to
    /// fail once the circuit gained the real macaroon chain re-derivation —
    /// "that failure is the point".
    ///
    /// The re-derivation is here now: the derived chain tip must equal the
    /// verifier-supplied `chain_tip_lo`. Changing the root secret changes the
    /// seed, so the derived tip moves and no proof can be produced. The test is
    /// inverted rather than deleted, because the property it now asserts is the
    /// one the audit asked for and it should stay visible in the harness.
    #[test]
    fn harness_refuses_an_unrelated_root_secret() {
        let spec = fixture_spec();
        assert!(
            run_cairo_to_prover_input(
                &spec,
                input_with(&[("cap_root_secret_lo", "\"987654321\"")]).as_bytes()
            )
            .is_none(),
            "an unrelated root secret must not satisfy the chain: the derived \
             tip has to equal the verifier-supplied chain_tip_lo"
        );
    }

    // ------------------------------------------------------------------
    // Rejection tests — each is meaningful only together with the control above
    // ------------------------------------------------------------------

    /// `stage2_main`: `assert!(pub_inputs.axes_count < 1000, "AxesCountTooLarge")`.
    #[test]
    fn harness_rejects_axes_count_over_limit() {
        let spec = fixture_spec();
        assert!(
            run_cairo_to_prover_input(&spec, input_with(&[("axes_count", "5000")]).as_bytes())
                .is_none(),
            "axes_count 5000 must trip AxesCountTooLarge"
        );
    }

    /// `stage2_main`: `assert!(caveat_felts.len() >= CHAIN_DEPTH_MIN)`.
    ///
    /// The mutation is structural — a 2-felt caveat array — so `caveats_count`
    /// is lowered to match, keeping the depth check as the only thing broken.
    #[test]
    fn harness_rejects_shallow_caveat_chain() {
        let spec = fixture_spec();
        let json = r#"{
          "pub_inputs": {
            "ask_id_lo": "1", "ask_id_hi": "2", "axes_count": 1,
            "cap_root_hash_lo": "4", "cap_root_hash_hi": "5", "chain_tip_lo": "240762416164593195965731284530788510451429005798628693311909781404607892050",
            "invocation_hash": "6", "holder_did": "7",
            "current_unix_time": 1700000000, "has_output_hash": 0, "output_hash": "0"
          },
          "priv_witness": {
            "cap_root_secret_lo": "11", "cap_root_secret_hi": "12",
            "caveats_count": 2, "discharges_count": 0, "inference_trace_present": 1
          },
          "mint_felts": ["41", "42"],
          "caveat_felts": ["15", "16"],
          "caveat_widths": ["1", "1"],
          "prev_ids": ["51", "52", "53", "54"],
          "steps": [{"op_code": "17", "input_hash_lo": "18", "input_hash_hi": "0", "output_hash_lo": "19", "output_hash_hi": "0"}]
        }"#;
        assert!(
            run_cairo_to_prover_input(&spec, json.as_bytes()).is_none(),
            "a 2-deep caveat chain must trip CaveatChainDepthMismatch (CHAIN_DEPTH_MIN = 3)"
        );
    }

    /// `stage2_main`: `assert!(trace_root == pub_inputs.output_hash)` once
    /// `has_output_hash` arms the comparison.
    #[test]
    fn harness_rejects_unbound_inference_trace() {
        let spec = fixture_spec();
        assert!(
            run_cairo_to_prover_input(
                &spec,
                input_with(&[("has_output_hash", "1"), ("output_hash", "\"12345\"")]).as_bytes()
            )
            .is_none(),
            "a trace that cannot fold to output_hash must be rejected"
        );
    }

    /// A `caveats_count` that disagrees with the felts actually supplied.
    #[test]
    fn harness_rejects_caveat_count_mismatch() {
        let spec = fixture_spec();
        assert!(
            run_cairo_to_prover_input(&spec, input_with(&[("caveats_count", "4")]).as_bytes())
                .is_none(),
            "declaring 4 caveats while supplying 3 felts must be rejected"
        );
    }

    // ------------------------------------------------------------------
    // Calldata marshalling
    // ------------------------------------------------------------------

    /// The calldata layout is the circuit's ABI. Getting it wrong produces a
    /// program that runs without ever calling the circuit, so it is pinned.
    ///
    /// **Every slot is asserted, not a sample.** The previous version checked
    /// the total length plus ten sampled index/value pairs, leaving thirteen
    /// slots unasserted — so a transposition among those passed. That is
    /// exactly the failure the RFC-0973 conformance discipline exists to
    /// catch: the harness supplies the public inputs, so a mis-ordered slot
    /// produces a proof over attacker-chosen inputs with every internal hash
    /// check self-consistent.
    ///
    /// Layout for the satisfying witness:
    ///
    /// ```text
    /// [ PublicInputs(11) , PrivateWitness(5)
    /// , [1+2 mint] , [1+3 caveats] , [1+3 widths] , [1+6 prev_ids] , [1+5 steps] ]
    ///  = 11 + 5 + 3 + 4 + 4 + 7 + 6 = 40
    /// ```
    ///
    /// The total alone is not enough, which is why every slot is asserted:
    /// `chain_tip_lo` was once inserted at index 5 without touching this test,
    /// which kept "passing" only because it was already red on the length.
    #[test]
    fn calldata_matches_cairo_serde_layout() {
        let input: CairoRunInput = serde_json::from_str(&satisfying_input()).unwrap();
        let c = build_calldata(&input).expect("build");

        assert_eq!(c.len(), 40, "calldata length: 11 + 5 + 3 + 4 + 4 + 7 + 6");

        // PublicInputs — every field, in declaration order.
        assert_eq!(c[0], Felt::from(1), "ask_id_lo");
        assert_eq!(c[1], Felt::from(2), "ask_id_hi");
        assert_eq!(c[2], Felt::from(1), "axes_count");
        assert_eq!(c[3], Felt::from(4), "cap_root_hash_lo");
        assert_eq!(c[4], Felt::from(5), "cap_root_hash_hi");
        assert_eq!(
            c[5],
            "240762416164593195965731284530788510451429005798628693311909781404607892050"
                .parse::<Felt>()
                .expect("chain tip literal"),
            "chain_tip_lo"
        );
        assert_eq!(c[6], Felt::from(6), "invocation_hash");
        assert_eq!(c[7], Felt::from(7), "holder_did");
        assert_eq!(c[8], Felt::from(1_700_000_000u64), "current_unix_time");
        assert_eq!(c[9], Felt::from(0), "has_output_hash");
        assert_eq!(c[10], Felt::from(0), "output_hash");

        // PrivateWitness — every field.
        assert_eq!(c[11], Felt::from(11), "cap_root_secret_lo");
        assert_eq!(c[12], Felt::from(12), "cap_root_secret_hi");
        assert_eq!(c[13], Felt::from(3), "caveats_count");
        assert_eq!(c[14], Felt::from(0), "discharges_count");
        assert_eq!(c[15], Felt::from(1), "inference_trace_present");

        // Chain witness arrays, `main`'s order, each [len, elems…].
        assert_eq!(c[16], Felt::from(2), "mint_felts length cell");
        assert_eq!(c[17], Felt::from(41), "mint_felts[0]");
        assert_eq!(c[18], Felt::from(42), "mint_felts[1]");

        assert_eq!(c[19], Felt::from(3), "caveat_felts length cell");
        assert_eq!(c[20], Felt::from(15), "caveat_felts[0]");
        assert_eq!(c[21], Felt::from(16), "caveat_felts[1]");
        assert_eq!(c[22], Felt::from(22), "caveat_felts[2]");

        assert_eq!(c[23], Felt::from(3), "caveat_widths length cell");
        assert_eq!(c[24], Felt::from(1), "caveat_widths[0]");
        assert_eq!(c[25], Felt::from(1), "caveat_widths[1]");
        assert_eq!(c[26], Felt::from(1), "caveat_widths[2]");

        assert_eq!(c[27], Felt::from(6), "prev_ids length cell = 2 per caveat");
        assert_eq!(c[28], Felt::from(51), "prev_ids[0]");
        assert_eq!(c[29], Felt::from(52), "prev_ids[1]");

        // steps_flat: [len, elems…], one step of TRACE_STRIDE = 5.
        assert_eq!(
            c[34],
            Felt::from(5),
            "steps_flat length cell = 1 step * stride 5"
        );
        assert_eq!(c[35], Felt::from(17), "step[0].op_code");
        assert_eq!(c[36], Felt::from(18), "step[0].input_hash_lo");
        assert_eq!(c[37], Felt::from(0), "step[0].input_hash_hi");
        assert_eq!(c[38], Felt::from(19), "step[0].output_hash_lo");
        assert_eq!(c[39], Felt::from(0), "step[0].output_hash_hi");
    }

    /// The layout is pinned against the Cairo **source**, not against a
    /// hand-maintained index table.
    ///
    /// The previous conformance test asserted `c[15]`, `c[16]`, … and so on, all
    /// hand-written. It passed happily while `main` grew three parameters,
    /// because a comment and a test that agree with each other catch nothing.
    /// Adding `mint_felts`, `caveat_widths` and `prev_ids` to the Cairo entry
    /// point shifted every array after them and this crate was still emitting
    /// the old shape — a deserializer fed the wrong layout does not error, it
    /// reads a length cell as a value.
    ///
    /// So this parses `main`'s actual parameter list out of the Cairo file and
    /// asserts it against the order `build_calldata` emits. `include_str!` is
    /// resolved at build time, so it cannot go stale without a rebuild.
    #[test]
    fn the_calldata_layout_matches_the_cairo_main_signature() {
        let cairo = include_str!("../../../../cairo/src/lib.cairo");

        let tail = cairo
            .split_once("pub fn main(")
            .expect("cairo/src/lib.cairo must declare `pub fn main`")
            .1;
        let params = tail
            .split_once(')')
            .expect("`main` parameter list must close")
            .0;

        let names: Vec<&str> = params
            .split(',')
            .filter_map(|p| p.split(':').next())
            .map(str::trim)
            .filter(|p| !p.is_empty())
            .collect();

        assert_eq!(
            names,
            vec![
                "pub_inputs",
                "priv_witness",
                "mint_felts",
                "caveat_felts",
                "caveat_widths",
                "prev_ids",
                "steps_flat",
            ],
            "the Cairo entry point's parameter list changed; `build_calldata` \
             emits calldata positionally, so every array after the insertion \
             point is now misaligned"
        );

        // `executable_main` is the entrypoint cairo-lang actually wraps, so a
        // divergence between the two entrypoints is just as fatal.
        let exe_tail = cairo
            .split_once("pub fn executable_main(")
            .expect("cairo/src/lib.cairo must declare `pub fn executable_main`")
            .1;
        let exe_params = exe_tail
            .split_once(')')
            .expect("parameter list must close")
            .0;
        let exe_names: Vec<&str> = exe_params
            .split(',')
            .filter_map(|p| p.split(':').next())
            .map(str::trim)
            .filter(|p| !p.is_empty())
            .collect();
        assert_eq!(
            exe_names, names,
            "`executable_main` and `main` must take the same arguments; \
             cairo-lang wraps the former"
        );
    }

    /// The field SET, not only the order.
    ///
    /// Order alone would not catch an input the circuit never declared: a
    /// `TraceStep` carrying an extra field, or missing one, still produces a
    /// correctly-ordered array of the wrong width. So the stride is asserted
    /// against the constant the Cairo program declares, and a second step must
    /// widen the array by exactly one stride.
    #[test]
    fn the_step_field_set_matches_the_declared_stride() {
        assert_eq!(
            TRACE_STRIDE, 5,
            "op + input_lo + input_hi + output_lo + output_hi"
        );

        let one: CairoRunInput = serde_json::from_str(&satisfying_input()).unwrap();
        let c1 = build_calldata(&one).expect("build one step");

        let two_json = satisfying_input().replace(
            r#""output_hash_hi": "0"}]"#,
            r#""output_hash_hi": "0"}, {"op_code": "20", "input_hash_lo": "21", "input_hash_hi": "22", "output_hash_lo": "23", "output_hash_hi": "24"}]"#,
        );
        let two: CairoRunInput = serde_json::from_str(&two_json).expect("parse two steps");
        let c2 = build_calldata(&two).expect("build two steps");

        // 11 PublicInputs + 5 PrivateWitness + 4 chain arrays (2+3+3+6 cells plus
        // one length cell each) + 1 steps length cell + 1 step of stride 5.
        assert_eq!(c1.len(), 40, "one step");
        assert_eq!(
            c2.len(),
            40 + TRACE_STRIDE,
            "a second step widens by exactly one stride"
        );
        assert_eq!(
            c2[34],
            Felt::from(10),
            "steps_flat length cell = 2 steps * stride 5"
        );
        assert_eq!(c2[35], Felt::from(17), "step[0] is unchanged");
        assert_eq!(c2[40], Felt::from(20), "step[1].op_code");
        assert_eq!(c2[44], Felt::from(24), "step[1].output_hash_hi");
    }

    /// A real 32-byte digest must now be expressible.
    ///
    /// This is the regression the stride change exists for. Under the old
    /// one-felt-per-hash encoding a full digest hit `parse_felt` and returned
    /// `None`, so no genuine PoI trace could reach the circuit at all. Each
    /// half is 16 bytes, far below the ~251-bit modulus.
    #[test]
    fn a_real_thirty_two_byte_digest_now_parses() {
        // `0x` is required: `parse_felt` only treats a string as hex when it is
        // prefixed, so the previous unprefixed fixture was parsed as decimal,
        // failed, and the test passed only because it was already red.
        let json = satisfying_input().replace(
            r#""input_hash_lo": "18", "input_hash_hi": "0""#,
            r#""input_hash_lo": "0x33333333333333333333333333333333", "input_hash_hi": "0xffffffffffffffffffffffffffffffff""#,
        );
        let input: CairoRunInput = serde_json::from_str(&json).expect("parse");
        assert!(
            build_calldata(&input).is_some(),
            "a full-width digest must survive the encoding"
        );
    }

    /// A felt that is neither decimal nor hex must be rejected, not coerced.
    #[test]
    fn calldata_rejects_unparsable_felt() {
        let input: CairoRunInput =
            serde_json::from_str(&satisfying_input().replace("\"15\"", "\"not-a-felt\"")).unwrap();
        assert!(
            build_calldata(&input).is_none(),
            "an unparsable felt must not be silently coerced"
        );
    }

    // ------------------------------------------------------------------
    // The C-2 property
    // ------------------------------------------------------------------

    /// End-to-end soundness round trip across the **real FFI surface**:
    /// `stwo_prove_from_casm` -> `stwo_proof_len` -> `stwo_proof_copy` ->
    /// `stwo_verify`.
    ///
    /// This is the property C-2 is about. Driving `stwo_prove_from_casm`
    /// (rather than the inner `run_cairo_to_prover_input` helper) means the
    /// test covers the entire production chain across the FFI boundary:
    /// `Executable` JSON -> calldata -> Cairo VM run -> `adapt()` ->
    /// `prove_cairo` -> handle -> bytes -> real STWO verifier.
    ///
    /// The commitment must be over an execution of the real circuit, and a
    /// full STARK proof — not a digest of the inputs — must cross the handle
    /// boundary. The C-2 defect was exactly that last part: the handle
    /// exposed 32 bytes of input digest, so `stwo_verify` rejected every
    /// real proof as unparsable. The `proof_len` floor below pins that
    /// regression shut.
    ///
    /// The tamper check is the control on the verifier: a verifier that
    /// accepts a mutated proof is not verifying anything.
    ///
    /// Slow (~1 min): proving 440 program counters through STWO.
    ///
    /// The body runs on an explicitly-sized thread rather than the harness
    /// thread. STWO's EC arithmetic recurses deeply enough to overrun the
    /// default test-thread stack, and it does so with an abort
    /// (`has overflowed its stack`), not a panic — so it cannot be caught
    /// here and takes the whole test binary down. The requirement appeared
    /// when the Schnorr module put `core::ec` on the proving path; before
    /// that the circuit stayed in field arithmetic and the default stack was
    /// fine. `RUST_MIN_STACK` in the environment also works, but only for
    /// whoever remembered to set it, so the requirement is pinned here.
    #[test]
    fn harness_proof_verifies_and_tampered_proof_does_not() {
        std::thread::Builder::new()
            .stack_size(PROVER_STACK_BYTES)
            .spawn(proof_round_trip)
            .expect("spawn prover thread")
            .join()
            .expect("prover thread must not panic");
    }

    /// Stack the STWO prover needs, measured on the `core::ec` proving path
    /// with headroom. Not a tuning knob: below this the process aborts.
    const PROVER_STACK_BYTES: usize = 134_217_728;

    fn proof_round_trip() {
        // Raw `Executable` JSON — the exact bytes the mint path hands the FFI.
        let program_json = fixture_bytes();
        let run_input_json = satisfying_input();

        // Prove through the FFI entry point, exactly as zk-vendor calls it.
        // SAFETY: all pointers are to live buffers for the call duration.
        let handle = unsafe {
            stwo_prove_from_casm(
                program_json.as_ptr(),
                program_json.len(),
                run_input_json.as_ptr(),
                run_input_json.len(),
                [0u8].as_ptr(),
                1,
            )
        };
        assert!(
            !handle.is_null(),
            "stwo_prove_from_casm must succeed on a valid execution"
        );

        // SAFETY: `handle` is a live pointer just returned by `stwo_prove_from_casm`.
        let len = unsafe { stwo_proof_len(handle) };
        assert!(
            len > 1_000_000,
            "the handle must carry a STARK proof, not an input digest \
             (C-2 regression: got {len} bytes)"
        );

        let mut bytes = vec![0u8; len];
        // SAFETY: `dst` has exactly `len` writable bytes and `handle` is live.
        let copied = unsafe { stwo_proof_copy(handle, bytes.as_mut_ptr(), len) };
        assert_eq!(copied, len, "proof copy must be complete");
        // SAFETY: the handle is released immediately below on every path.
        unsafe { stwo_free_proof(handle) };

        let ret = unsafe { stwo_verify(bytes.as_ptr(), bytes.len(), [0u8].as_ptr(), 1) };
        assert_eq!(ret, 0, "a genuine proof must verify");

        let mut tampered = bytes.clone();
        let mid = tampered.len() / 2;
        tampered[mid] ^= 0x01;
        let ret = unsafe { stwo_verify(tampered.as_ptr(), tampered.len(), [0u8].as_ptr(), 1) };
        assert_eq!(ret, 1, "a tampered proof must be rejected");
    }
}
