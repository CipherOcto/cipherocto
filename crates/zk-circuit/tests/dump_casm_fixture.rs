//! One-shot: emit the compiled CASM artifact for the stwo-sys harness test.
//!
//! The artifact is JSON, not a raw `.casm` blob, because the harness needs
//! three things and the bytecode carries only one of them: the bytecode,
//! the hint table, and the entrypoint offset. See
//! `zk_circuit::HarnessProgram`.
//!
//! Regenerate with `cargo test -p zk-circuit --test dump_casm_fixture -- --ignored`.
use zk_circuit::{bundled_casm_bytes, bundled_harness_program};

#[test]
#[ignore = "fixture generator, not a gate"]
fn dump_casm_for_stwo_harness_fixture() {
    let program = bundled_harness_program().expect("compile");

    // The bytecode and its hash must move together: if they ever diverge,
    // the harness would be proving a different program than the one whose
    // hash the mint path commits to.
    assert_eq!(
        program.casm_hex,
        hex::encode(bundled_casm_bytes().expect("compile")),
        "artifact bytecode must equal bundled_casm_bytes"
    );
    assert!(
        !program.hints.is_empty(),
        "artifact must carry the CASM hint table; an empty one means \
         hints were dropped, which is unsound"
    );
    assert_ne!(
        program.main_pc, 0,
        "entrypoint must not be offset 0 (that is verify_chain)"
    );

    let dir = std::path::Path::new("../zk-vendor/stwo-sys/tests/fixtures");
    std::fs::create_dir_all(dir).expect("mkdir");
    let path = dir.join("capability_zk.json");
    let json = serde_json::to_string(&program).expect("serialize");
    std::fs::write(&path, json).expect("write");
    eprintln!(
        "WROTE {} bytes of bytecode, {} hints, main_pc {}",
        bundled_casm_bytes().expect("compile").len(),
        program.hints.len(),
        program.main_pc
    );
}
