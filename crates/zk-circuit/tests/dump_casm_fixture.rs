//! One-shot: emit the compiled CASM as a fixture for the stwo-sys harness test.
//! Regenerate with `cargo test -p zk-circuit --test dump_casm_fixture -- --ignored`.
#[test]
#[ignore = "fixture generator, not a gate"]
fn dump_casm_for_stwo_harness_fixture() {
    let bytes = zk_circuit::bundled_casm_bytes().expect("compile");
    let dir = std::path::Path::new("../zk-vendor/stwo-sys/tests/fixtures");
    std::fs::create_dir_all(dir).expect("mkdir");
    std::fs::write(dir.join("capability_zk.casm"), bytes).expect("write");
    eprintln!("WROTE {} bytes", bytes.len());
}
