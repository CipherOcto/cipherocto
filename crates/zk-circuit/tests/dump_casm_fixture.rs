//! One-shot: emit the compiled **Cairo `Executable`** artifact for the
//! `stwo-sys` harness test.
//!
//! The artifact is the upstream `cairo_lang_executable::executable::Executable`
//! serialized to JSON — bytecode, the typed hint table, and the entrypoint list.
//! It carries the wrapper that `#[executable]` generates, which is what delivers
//! the entrypoint's arguments as calldata; without it the harness would have to
//! hand-build a stack frame and would run a program that never calls the
//! circuit. See `docs/architecture/zk-stwo-cairo-executable-recipe.md`.
//!
//! Regenerate with `cargo test -p zk-circuit --test dump_casm_fixture -- --ignored`.
use zk_circuit::bundled_executable;

#[test]
#[ignore = "fixture generator, not a gate"]
fn dump_casm_for_stwo_harness_fixture() {
    let exec = bundled_executable().expect("compile");

    assert!(
        !exec.program.bytecode.is_empty(),
        "executable bytecode must be non-empty"
    );
    assert!(
        !exec.program.hints.is_empty(),
        "artifact must carry the hint table; an empty one means hints were \
         dropped, which is unsound"
    );

    let standalone: Vec<_> = exec
        .entrypoints
        .iter()
        .filter(|e| matches!(e.kind, zk_circuit::ExecutableEntryPointKind::Standalone))
        .collect();
    assert_eq!(
        standalone.len(),
        1,
        "exactly one Standalone entrypoint expected, got {}",
        standalone.len()
    );
    assert_eq!(
        standalone[0].offset, 0,
        "the Standalone header starts at offset 0; a non-zero offset means the \
         wrapper was not emitted and the harness would start mid-program"
    );

    let dir = std::path::Path::new("../zk-vendor/stwo-sys/tests/fixtures");
    std::fs::create_dir_all(dir).expect("mkdir");
    let path = dir.join("capability_zk.json");
    let json = zk_circuit::bundled_executable_json().expect("serialize");
    std::fs::write(&path, json).expect("write");
    eprintln!(
        "WROTE executable: {} bytecode words, {} hints, entrypoints {:?}",
        exec.program.bytecode.len(),
        exec.program.hints.len(),
        exec.entrypoints.iter().map(|e| &e.kind).collect::<Vec<_>>()
    );
}
