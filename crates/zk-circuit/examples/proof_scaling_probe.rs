//! Proof-size scaling probe: does a real STARK proof grow with program size?
//!
//! WHY THIS EXISTS. Every alternative design for making the capability circuit
//! cryptographically bind adds program size (a hash primitive in-circuit, a
//! bigger witness, a longer trace). Whether that is *affordable* depends
//! entirely on how proof size responds to program size -- which is not
//! something to guess at. The wire format caps (`MAX_SEGMENT_BYTES` 1 MiB,
//! `MAX_WIRE_TOTAL` 2 MiB) and the RFC-0958 50-500 KB proof target are both
//! judged against real proof bytes.
//!
//! WHAT IT DOES. Compiles the CURRENT cairo source (whatever the caller has
//! injected), builds a satisfying witness, runs the real prover through the
//! real FFI, and prints program size next to proof size. Pair it with a
//! harness that injects N chained Poseidon hashes to sweep the size axis.
//!
//! Unlike `proof_wire_size.rs` -- which only does arithmetic on a hardcoded
//! constant and never proves anything -- this actually invokes the prover.
//!
//! Run:
//!   cargo run -p zk-circuit --example proof_scaling_probe

#![allow(
    clippy::cast_possible_truncation,
    clippy::cast_precision_loss,
    clippy::doc_markdown,
    clippy::print_stdout,
    clippy::uninlined_format_args,
    clippy::unreadable_literal,
    clippy::useless_vec
)]

use std::path::PathBuf;

use zk_vendor::try_load;

/// A witness the circuit accepts. Must stay byte-compatible with
/// `build_run_args` in `crates/zk-vendor/stwo-sys/src/lib.rs`.
fn satisfying_run_input() -> Vec<u8> {
    br#"{
      "pub_inputs": {
        "ask_id_lo": "1", "ask_id_hi": "2", "axes_count": 1,
        "cap_root_hash_lo": "4", "cap_root_hash_hi": "5",
        "chain_tip_lo": "77", "invocation_hash": "6", "holder_did": "7",
        "current_unix_time": 1700000000, "has_output_hash": 0, "output_hash": "0"
      },
      "priv_witness": {
        "cap_root_secret_lo": "11", "cap_root_secret_hi": "12",
        "caveats_count": 3, "discharges_count": 0, "inference_trace_present": 1
      },
      "caveat_felts": ["15", "16", "22"],
      "steps": [{"op_code": "17", "input_hash_lo": "18", "input_hash_hi": "0", "output_hash_lo": "19", "output_hash_hi": "0"}]
    }"#
    .to_vec()
}

fn nightly_lib_path() -> Option<PathBuf> {
    // crates/zk-circuit/examples/ -> crates/zk-vendor/stwo-sys/target/release
    let manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let mut p = manifest;
    p.pop();
    p.push("zk-vendor");
    p.push("stwo-sys");
    p.push("target");
    p.push("release");
    let name = if cfg!(target_os = "windows") {
        "stwo_sys.dll"
    } else if cfg!(target_os = "macos") {
        "libstwo_sys.dylib"
    } else {
        "libstwo_sys.so"
    };
    p.push(name);
    p.exists().then_some(p)
}

fn main() {
    let program = zk_circuit::bundled_executable_json().expect("compile executable");
    let casm = zk_circuit::bundled_casm_bytes().expect("compile casm");
    let words = casm.len() / 32;

    println!("program words            : {words}");
    println!("program json bytes       : {}", program.len());

    let lib = nightly_lib_path().unwrap_or_else(|| {
        panic!(
            "libstwo_sys.so not built. Run: cd crates/zk-vendor/stwo-sys && \
             cargo +nightly-2026-01-15 build --release"
        )
    });
    let sys = try_load(&lib).expect("try_load").expect("library present");

    let t0 = std::time::Instant::now();
    let out = sys
        .prove_from_casm(
            program.as_bytes(),
            &satisfying_run_input(),
            b"unused-public-inputs",
        )
        .expect("prove_from_casm on a satisfying witness");
    let secs = t0.elapsed().as_secs_f64();

    let proof = out.proof.len();
    println!("prove seconds            : {secs:.1}");
    println!("proof bytes              : {proof}");
    println!("proof words              : {}", proof.div_ceil(32));
    println!(
        "bytes per program word   : {:.2}",
        proof as f64 / words as f64
    );
    println!(
        "over MAX_SEGMENT(1MiB)   : {:.1}x",
        proof as f64 / (1024.0 * 1024.0)
    );
    println!(
        "RFC-0958 500KB target    : {:.1}x over",
        proof as f64 / 500_000.0
    );
}
