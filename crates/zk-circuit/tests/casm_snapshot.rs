//! Cairo 2.x capability circuit smoke test (mission 0958-a Session 1).
//!
//! Replaces the prior Cairo 1.x `cairo-compile` snapshot test (which
//! assumed a binary that does not exist on Cairo 2.x toolchains). The
//! new path uses scarb (Cairo 2.x build orchestrator) which IS installed
//! and produces real Sierra IR from `cairo/src/lib.cairo`.
//!
//! **Session 1 scope:** prove the Cairo source compiles via the real
//! scarb toolchain, produces valid Sierra IR, and the IR is semantically
//! deterministic across builds. **CASM emission is Session 2** — the
//! Sierra→CASM pass lives in `cairo-lang-sierra-to-casm` and will be
//! wired into `crates/zk-circuit/src/lib.rs` in the next session.
//!
//! **Hard-fail policy:** this test does NOT skip silently. If `scarb` is
//! missing, the test panics with an actionable message. Local dev and CI
//! must both have scarb installed (CI workflow
//! `.github/workflows/zk-capability-circuit.yml` will install scarb 2.16.0).
//!
//! **Reference:** RFC-0958 §CASM Hash Drift Detection (full CASM path
//! ships Session 2); Session 1 is the Sierra path prerequisite.

#![allow(clippy::map_unwrap_or)] // intentional: temp-dir naming needs the map-unwrap-or chain

use std::path::{Path, PathBuf};
use std::process::Command;

use zk_circuit::BUNDLED_CAIRO_SOURCE;

/// Workspace-relative path to the Cairo 2.x project root.
fn cairo_project_root() -> PathBuf {
    let crate_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    crate_dir
        .parent()
        .and_then(Path::parent)
        .map(|ws| ws.join("cairo"))
        .expect("zk-circuit must live at crates/zk-circuit/ inside the workspace")
}

/// Run `scarb build` in the Cairo project directory, isolating the
/// target dir per test invocation so parallel test runs do not race on
/// the shared `cairo/target/dev/` output.
///
/// # Panics
/// Panics (with actionable message) if scarb is missing or the build fails.
/// Session 1 mandates hard-fail — no silent skip.
fn run_scarb_build(project: &Path) -> PathBuf {
    let scarb_check = Command::new("scarb").arg("--version").output();
    match scarb_check {
        Ok(out) if out.status.success() => {
            // scarb is available — proceed.
        }
        Ok(_) => panic!(
            "scarb --version exited non-zero. Install scarb 2.16.0 (https://docs.swmansion.com/scarb/). \
             Mission 0958-a Session 1 requires scarb to produce Sierra IR."
        ),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => panic!(
            "scarb not in PATH. Install scarb 2.16.0 (https://docs.swmansion.com/scarb/). \
             Mission 0958-a Session 1 requires scarb to produce Sierra IR."
        ),
        Err(e) => panic!("failed to spawn scarb: {e}"),
    }

    // Per-test isolated target dir — lets parallel tests run without
    // stomping on each other's output.
    let target_dir = std::env::temp_dir().join(format!(
        "cipherocto-scarb-target-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0)
    ));
    std::fs::create_dir_all(&target_dir).expect("create temp target dir");

    let output = Command::new("scarb")
        .arg("--target-dir")
        .arg(&target_dir)
        .arg("build")
        .current_dir(project)
        .output()
        .expect("spawn scarb build");
    assert!(
        output.status.success(),
        "scarb build failed: stderr={}",
        String::from_utf8_lossy(&output.stderr)
    );

    let sierra_path = target_dir.join("dev").join("capability_zk.sierra.json");
    assert!(
        sierra_path.exists(),
        "scarb build did not produce {}",
        sierra_path.display()
    );
    sierra_path
}

/// Run `scarb build` with an explicit target dir (used by the
/// determinism test which needs two independent builds).
fn run_scarb_build_into(project: &Path, target_dir: &Path) -> PathBuf {
    let output = Command::new("scarb")
        .arg("--target-dir")
        .arg(target_dir)
        .arg("build")
        .current_dir(project)
        .output()
        .expect("spawn scarb build");
    assert!(
        output.status.success(),
        "scarb build failed: stderr={}",
        String::from_utf8_lossy(&output.stderr)
    );
    let sierra = target_dir.join("dev").join("capability_zk.sierra.json");
    assert!(
        sierra.exists(),
        "scarb build did not produce {}",
        sierra.display()
    );
    sierra
}

fn unique_target_dir(label: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "cipherocto-scarb-{}-{}-{}",
        label,
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0)
    ));
    std::fs::create_dir_all(&dir).expect("create temp target dir");
    dir
}

/// Read the Sierra JSON file produced by scarb and parse it.
fn read_sierra_json(path: &Path) -> serde_json::Value {
    let bytes =
        std::fs::read(path).unwrap_or_else(|e| panic!("read sierra.json {}: {e}", path.display()));
    serde_json::from_slice(&bytes)
        .unwrap_or_else(|e| panic!("parse sierra.json {}: {e}", path.display()))
}

/// Extract the set of `debug_name` values from the Sierra IR. This is the
/// semantic-content fingerprint: it ignores salsa UUIDs (which change per
/// compile session) and only tracks the types + functions the compiler
/// actually emitted.
fn debug_name_set(sierra: &serde_json::Value) -> std::collections::BTreeSet<String> {
    let mut out = std::collections::BTreeSet::new();
    let Some(arr) = sierra.get("type_declarations").and_then(|v| v.as_array()) else {
        panic!("sierra.json missing 'type_declarations' array");
    };
    for entry in arr {
        if let Some(name) = entry
            .get("id")
            .and_then(|id| id.get("debug_name"))
            .and_then(|n| n.as_str())
        {
            out.insert(name.to_owned());
        }
    }
    let Some(funcs) = sierra.get("funcs").and_then(|v| v.as_array()) else {
        return out;
    };
    for func in funcs {
        if let Some(name) = func
            .get("id")
            .and_then(|id| id.get("debug_name"))
            .and_then(|n| n.as_str())
        {
            out.insert(format!("fn:{name}"));
        }
    }
    out
}

#[test]
fn scarb_build_produces_valid_sierra_json() {
    let project = cairo_project_root();
    let sierra_path = run_scarb_build(&project);
    assert!(
        sierra_path.exists(),
        "scarb build did not produce {}",
        sierra_path.display()
    );
    let sierra = read_sierra_json(&sierra_path);
    assert_eq!(
        sierra.get("version").and_then(serde_json::Value::as_i64),
        Some(1),
        "Sierra version must be 1"
    );
    assert!(
        sierra
            .get("type_declarations")
            .and_then(|v| v.as_array())
            .is_some(),
        "Sierra IR must contain type_declarations array"
    );
}

#[test]
fn sierra_ir_is_semantically_deterministic_across_builds() {
    // salsa (scarb's incremental compiler DB) generates fresh UUIDs per
    // session, so the raw JSON bytes are NOT byte-identical across
    // builds. The semantic content IS identical — we compare the
    // debug_name set, which captures types + function names without
    // the salsa UUID noise. Downstream Sierra→CASM (Session 2) is
    // also affected by salsa IDs, so the determinism check happens
    // there at the CASM-byte level.
    let project = cairo_project_root();

    let dir_a = unique_target_dir("det-a");
    let dir_b = unique_target_dir("det-b");
    let sierra_a = read_sierra_json(&run_scarb_build_into(&project, &dir_a));
    let sierra_b = read_sierra_json(&run_scarb_build_into(&project, &dir_b));

    let names_a = debug_name_set(&sierra_a);
    let names_b = debug_name_set(&sierra_b);

    assert_eq!(
        names_a,
        names_b,
        "Sierra IR must be semantically deterministic (same type_declarations \
         + function set across builds). diff: {:#?}",
        names_a.symmetric_difference(&names_b).collect::<Vec<_>>()
    );
}

#[test]
fn scarb_build_includes_main_function() {
    // The Sierra IR carries a `funcs` array of function entries. The
    // `capability_zk::main` function must be present (it is the STARK
    // entry point that the prover invokes). Struct types like
    // PublicInputs / PrivateWitness may be inlined by scarb's optimizer
    // — they are tracked at the source level (see bundled_cairo_source_*
    // tests) rather than in the IR debug_name set.
    let project = cairo_project_root();
    let sierra = read_sierra_json(&run_scarb_build(&project));
    let names = debug_name_set(&sierra);
    assert!(
        names.iter().any(|n| n.contains("capability_zk::main")),
        "Sierra IR must include `capability_zk::main` function entry. \
         Found types: {names:#?}"
    );
}

#[test]
fn bundled_cairo_source_defines_public_inputs_struct() {
    // Source-level contract: PublicInputs struct must be declared in the
    // bundled Cairo source. scarb may inline or rewrite the struct in
    // Sierra IR, so the IR debug_name set is not a reliable check.
    assert!(
        BUNDLED_CAIRO_SOURCE.contains("pub struct PublicInputs"),
        "BUNDLED_CAIRO_SOURCE must declare PublicInputs struct"
    );
    assert!(
        BUNDLED_CAIRO_SOURCE.contains("pub struct PrivateWitness"),
        "BUNDLED_CAIRO_SOURCE must declare PrivateWitness struct"
    );
    assert!(
        BUNDLED_CAIRO_SOURCE.contains("pub fn main"),
        "BUNDLED_CAIRO_SOURCE must declare a `main` function"
    );
}

#[test]
fn bundled_cairo_source_matches_cairo_src_lib_cairo() {
    // The BUNDLED_CAIRO_SOURCE constant in zk-circuit is sourced from
    // `cairo/src/lib.cairo` via `include_str!`. This test pins that
    // contract: any drift between the on-disk file and the compiled-in
    // constant is a build-system bug.
    let project = cairo_project_root();
    let on_disk = std::fs::read_to_string(project.join("src").join("lib.cairo"))
        .expect("read cairo/src/lib.cairo");
    assert_eq!(
        on_disk, BUNDLED_CAIRO_SOURCE,
        "BUNDLED_CAIRO_SOURCE (include_str!) must match cairo/src/lib.cairo on disk"
    );
}
