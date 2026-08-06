//! Cairo 2.x capability circuit smoke test (mission 0958-a Sessions 1 + 2).
//!
//! Replaces the prior Cairo 1.x `cairo-compile` snapshot test (which
//! assumed a binary that does not exist on Cairo 2.x toolchains). The
//! new path uses scarb (Cairo 2.x build orchestrator) which IS installed
//! and produces real Sierra IR from `cairo/src/lib.cairo`; the in-process
//! `cairo-lang-sierra-to-casm` pass (Session 2) lowers that IR to CASM
//! bytecode whose BLAKE3 hash is the canonical `compiled_casm_hash`.
//!
//! **Session 1 scope:** prove the Cairo source compiles via the real
//! scarb toolchain, produces valid Sierra IR, and the IR is semantically
//! deterministic across builds.
//!
//! **Session 2 scope:** prove the Sierra→CASM in-process pass emits real
//! bytecode (not a stub), the bytecode is byte-deterministic across
//! builds (salsa UUIDs in the IR are absorbed by the canonical
//! `Program` AST), and `compile_from_source` round-trips through the
//! full pipeline end-to-end.
//!
//! **Hard-fail policy:** this test does NOT skip silently. If `scarb` is
//! missing, the test panics with an actionable message. Local dev and CI
//! must both have scarb installed (CI workflow
//! `.github/workflows/zk-capability-circuit.yml` installs scarb 2.16.0).
//!
//! **Reference:** RFC-0958 §CASM Hash Drift Detection.

#![allow(clippy::map_unwrap_or)] // intentional: temp-dir naming needs the map-unwrap-or chain
#![allow(clippy::similar_names)] // intentional: sierra_a_bytes / sierra_b_bytes naming

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
    // the salsa UUID noise. The downstream Sierra→CASM pass (Session 2)
    // absorbs the salsa UUIDs entirely (it canonicalizes through the
    // `Program` AST), so the CASM bytes ARE byte-identical — see
    // `casm_is_byte_identical_across_builds` below.
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

// =========================================================================
// Session 2 tests — Sierra→CASM in-process pass via cairo-lang-sierra-to-casm.
// =========================================================================

#[test]
fn compile_from_source_returns_non_empty_casm() {
    // Session 2 contract: the production `compile_from_source` path
    // emits REAL CASM bytecode via the in-process Sierra→CASM pass,
    // not a JSON stub. Verifies:
    // - bytecode is non-empty (real instructions emitted),
    // - bytecode length is a multiple of 32 (felt252 wire format),
    // - hash is 64 hex chars (BLAKE3-256 shape),
    // - hash is stable across two calls (OnceLock memoization).
    let a = zk_circuit::compile_from_source(BUNDLED_CAIRO_SOURCE)
        .expect("compile_from_source must succeed; scarb + cairo-lang crates must be installed");
    assert!(
        !a.casm_bytecode.is_empty(),
        "CASM bytecode must be non-empty (real CASM, not a stub)"
    );
    assert_eq!(
        a.casm_bytecode.len() % 32,
        0,
        "CASM bytecode must be multiple of 32 bytes (felt252 wire format); got {} bytes",
        a.casm_bytecode.len()
    );
    assert_eq!(
        a.compiled_casm_hash.len(),
        64,
        "BLAKE3-256 hex must be 64 chars"
    );
    assert!(
        a.compiled_casm_hash.chars().all(|c| c.is_ascii_hexdigit()),
        "compiled_casm_hash must be hex; got {}",
        a.compiled_casm_hash
    );

    // Memoization: second call returns the same circuit (OnceLock caches).
    let b = zk_circuit::compile_from_source(BUNDLED_CAIRO_SOURCE)
        .expect("second call must also succeed");
    assert_eq!(
        a.compiled_casm_hash, b.compiled_casm_hash,
        "compile_from_source must be deterministic (OnceLock memoization)"
    );
}

#[test]
fn casm_is_byte_identical_across_builds() {
    // Session 2 strong determinism contract: the in-process Sierra→CASM
    // pass absorbs salsa UUIDs entirely (the `Program` AST is canonical,
    // so two Sierra JSON files that differ only in salsa UUIDs produce
    // byte-identical CASM). This is the property that makes
    // `bundled_casm_hash()` meaningful across processes / platforms.
    //
    // Two independent scarb builds → two Sierra JSONs (different
    // salsa UUIDs) → two CASM byte streams that MUST be byte-identical.
    let project = cairo_project_root();
    let dir_a = unique_target_dir("casm-det-a");
    let dir_b = unique_target_dir("casm-det-b");
    let sierra_a_bytes =
        std::fs::read(run_scarb_build_into(&project, &dir_a)).expect("read sierra A");
    let sierra_b_bytes =
        std::fs::read(run_scarb_build_into(&project, &dir_b)).expect("read sierra B");

    // Sanity: the two Sierra JSONs are NOT byte-identical (salsa UUIDs).
    assert_ne!(
        sierra_a_bytes, sierra_b_bytes,
        "sanity: two independent scarb builds MUST produce different Sierra \
         JSON bytes (salsa UUIDs differ); if they are byte-identical, the \
         determinism check below is trivially true and provides no signal"
    );

    let casm_a = compile_sierra_to_casm(&sierra_a_bytes);
    let casm_b = compile_sierra_to_casm(&sierra_b_bytes);
    assert_eq!(
        casm_a, casm_b,
        "CASM bytes MUST be byte-identical across builds (salsa UUIDs absorbed \
         by the Sierra→CASM pass); diverging implies a non-deterministic \
         compiler pass and breaks RFC-0958 §compiled_casm_hash contract"
    );
}

#[test]
fn bundled_casm_hash_is_stable_across_compile_from_source_calls() {
    // OnceLock + the byte-identical-across-builds property together
    // imply `bundled_casm_hash()` is process-stable: the first call
    // computes the CASM via the scarb+Sierra→CASM pipeline; every
    // subsequent call returns the same bytes without invoking scarb
    // again.
    let hex_a = zk_circuit::bundled_casm_hash_hex()
        .expect("bundled_casm_hash_hex must succeed once scarb + crates installed");
    let hex_b = zk_circuit::bundled_casm_hash_hex()
        .expect("bundled_casm_hash_hex must succeed on subsequent calls");
    assert_eq!(hex_a, hex_b);
    assert_eq!(hex_a.len(), 64);
    assert!(hex_a.chars().all(|c| c.is_ascii_hexdigit()));
}

/// In-process Sierra→CASM pass via the same library code `compile_from_source`
/// uses internally. Re-implemented here against the `cairo_lang_*` crates
/// so the test exercises the lower-level pipeline independently of
/// `compile_from_source`'s scarb-subprocess wrapper.
fn compile_sierra_to_casm(sierra_bytes: &[u8]) -> Vec<u8> {
    use cairo_lang_sierra::program::Program as SierraProgram;
    use cairo_lang_sierra_to_casm::compiler::{compile as sierra_compile, SierraToCasmConfig};
    use cairo_lang_sierra_to_casm::metadata::calc_metadata_ap_change_only;
    use cairo_lang_sierra_type_size::ProgramRegistryInfo;

    let program: SierraProgram = serde_json::from_slice(sierra_bytes).expect("parse Sierra IR");
    let registry_info = ProgramRegistryInfo::new(&program).expect("ProgramRegistryInfo::new");
    let metadata = calc_metadata_ap_change_only(&program, &registry_info)
        .expect("calc_metadata_ap_change_only");
    let casm = sierra_compile(
        &program,
        &registry_info,
        &metadata,
        SierraToCasmConfig {
            gas_usage_check: false,
            max_bytecode_size: usize::MAX,
        },
    )
    .expect("Sierra→CASM compile");
    let assembled = casm.assemble();
    // Reuse the same wire-format encoding as the production code.
    zk_circuit_test_helpers::felt_vec_to_casm_bytes(&assembled.bytecode)
}

#[test]
fn measure_current_casm_size() {
    // Pre-AC-4 baseline measurement.
    let bytes = zk_circuit::bundled_casm_bytes().expect("compile");
    println!(
        "AC-4 baseline: CASM bytes={} ({:.2} KB), words={}",
        bytes.len(),
        bytes.len() as f64 / 1024.0,
        bytes.len() / 32,
    );
    println!("AC-4 hard ceilings: 50 KB serialized / 1600 words (Round 18 fix F-122/F-139)");
}

/// AC-4 hard gate: serialized CASM bytes ≤ 50 KB after Stage-2 split.
///
/// **Status (mission 0958-c AC-4, 2026-08-06):** currently FAILING
/// (CASM = ~267 KB / 8,534 words). AC-4 closes fail-closed per
/// mission text until STWO recursive composition lands (RFC-0958
/// §Future Work F7 — `prove_cairo` composition API not yet upstream).
/// The actual size reduction requires each sub-circuit as its own
/// scarb project + STARK proof, with `main()` verifying the proofs
/// rather than inlining the cryptographic primitives.
#[test]
#[ignore = "AC-4 fail-closed: CASM bytes > 50 KB until STWO composition lands (RFC-0958 §F7)"]
fn casm_bytes_under_50kb_after_stage2_split() {
    let bytes = zk_circuit::bundled_casm_bytes().expect("compile");
    assert!(
        bytes.len() <= 50 * 1024,
        "AC-4 fail-closed: serialized CASM = {} bytes ({:.2} KB) > 50 KB ceiling. \
         Required: Stage-2 STWO composition (RFC-0958 §Future Work F7).",
        bytes.len(),
        bytes.len() as f64 / 1024.0,
    );
}

/// AC-4 hard gate: CASM word count ≤ 1,600 after Stage-2 split
/// (Round 18 fix F-122 — `max_bytecode_size` measures CASM words,
/// NOT Sierra statements or serialized bytes).
///
/// **Status:** currently FAILING (CASM = ~8,534 words). See
/// `casm_bytes_under_50kb_after_stage2_split` for the failure rationale.
#[test]
#[ignore = "AC-4 fail-closed: CASM words > 1600 until STWO composition lands (RFC-0958 §F7)"]
fn casm_words_under_1600_after_stage2() {
    let bytes = zk_circuit::bundled_casm_bytes().expect("compile");
    let words = bytes.len() / 32;
    assert!(
        words <= 1600,
        "AC-4 fail-closed: CASM word count = {} > 1600 ceiling. \
         Required: Stage-2 STWO composition (RFC-0958 §Future Work F7).",
        words,
    );
}

/// Test-local helper module mirroring `zk_circuit::felt_vec_to_casm_bytes`
/// so the test does not depend on the private helper. Kept here (not in
/// the lib crate) because exposing it as `pub` would expand the
/// crate's surface area for no consumer benefit.
mod zk_circuit_test_helpers {
    pub fn felt_vec_to_casm_bytes(words: &[num_bigint::BigInt]) -> Vec<u8> {
        let mut out = Vec::with_capacity(words.len() * 32);
        for w in words {
            let raw = w.to_signed_bytes_be();
            let mut word = [0u8; 32];
            let len = raw.len();
            if len <= 32 {
                word[32 - len..].copy_from_slice(&raw);
            } else {
                word.copy_from_slice(&raw[len - 32..]);
            }
            out.extend_from_slice(&word);
        }
        out
    }
}
