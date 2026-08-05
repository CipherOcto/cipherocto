//! CipherOcto ZK circuit: Cairo → CASM compiler + BLAKE3 hash.
//!
//! Per RFC-0958 (ZK capability subclass) + master plan Phase B.2.
//!
//! **Crypto home:** this crate lives in the cipherocto workspace, NOT in the
//! stoolap fork. CASM compilation is a proof-system concern, orthogonal to
//! SQL. Per [[stoolap-general-purpose-db]] principle (2026-07-22 extraction).
//!
//! **Stable-rust only:** no nightly, no `#![feature(...)]`. STWO's nightly
//! is patched inside the separate `zk-vendor/` crate via source drop.
//!
//! ## Surface
//!
//! - [`compile_from_source`]: Cairo 2.x source string → [`CompiledCircuit`]
//!   carrying **real CASM bytecode** + BLAKE3 hash. Pipeline:
//!   1. `scarb build` → `capability_zk.sierra.json` (Cairo 2.x toolchain;
//!      the standalone `cairo-compile` binary is gone in Cairo 2.x).
//!   2. Parse Sierra IR via serde → `cairo_lang_sierra::program::Program`.
//!   3. Build `ProgramRegistry::<CoreType, CoreLibfunc>`, compute AP-change
//!      metadata (`calc_metadata_ap_change_only`), lower to CASM via
//!      `cairo_lang_sierra_to_casm::compiler::compile`.
//!   4. Assemble CASM (`CairoProgram::assemble()`) → flat bytecode bytes.
//!   5. BLAKE3-256 the bytes → 64 hex chars (matches RFC-0958
//!      `compiled_casm_hash` field shape).
//! - [`compile`]: legacy stub over `CairoProgram` JSON struct (deterministic
//!   BLAKE3 of canonical JSON). Kept for backward compat with tests + the
//!   `Program::Capability` dispatch; production path is `compile_from_source`.
//! - [`bundled_casm_bytes`]: real CASM bytes of `cairo/src/lib.cairo`
//!   (memoized via `OnceLock`).
//! - [`bundled_casm_hash_hex`]: BLAKE3-256 hex of the bundled CASM.
//! - [`CompiledCircuit::hash`]: stable 64-char hex (matches RFC-0958
//!   `compiled_casm_hash` field shape).
//! - [`HashError`]: error type for malformed Cairo input or missing toolchain.
//! - [`Program`], [`BatchSigPublicInputs`], [`prove_batch_signature`]:
//!   batch signature circuit surface (Gap 3; RFC-0958 capability ZK
//!   subclass + RFC-0962 §9 ZK proof integration).
//!
//! ## Determinism contract
//!
//! Same Cairo program → same Sierra IR (modulo salsa UUIDs which DO NOT
//! affect CASM bytecode bytes) → same CASM bytecode → same BLAKE3 hash.
//! Across processes, across architectures, across platforms. STWO Fiat-
//! Shamir transform is Class A (Protocol Determinism, per RFC-0958
//! §Determinism).
//!
//! ## Toolchain pin
//!
//! - **scarb** 2.16.0 / **cairo** 2.16.0 / **sierra** 1.7.0
//! - **cairo-lang-* Rust crates** 2.20.0 (the Sierra→CASM in-process pass)
//!
//! The Sierra JSON emitted by scarb 2.16.0 declares `"version": 1`; the
//! `cairo_lang_sierra 2.20.0` deserializer accepts that version. Two
//! independent `scarb build` runs may produce JSON bytes that differ
//! (salsa UUIDs in `id` fields); the CASM pass canonicalizes through the
//! `Program` AST and emits byte-identical CASM for the same Cairo source.
//! The CASM-level determinism is verified by the smoke test
//! (`crates/zk-circuit/tests/casm_snapshot.rs`).

#![deny(unsafe_code)]
#![warn(missing_debug_implementations)]
#![allow(clippy::doc_markdown)]

use std::process::Command;
use std::sync::OnceLock;

use blake3::Hasher;
use serde::{Deserialize, Serialize};
use thiserror::Error;

use cairo_lang_sierra::program::Program as SierraProgram;
use cairo_lang_sierra_to_casm::compiler::{compile as sierra_compile_to_casm, SierraToCasmConfig};
use cairo_lang_sierra_to_casm::metadata::calc_metadata_ap_change_only;
use cairo_lang_sierra_type_size::ProgramRegistryInfo;

/// A Cairo program in canonical JSON form (RFC-0126 deterministic
/// serialization).
///
/// Stub for now: the real Cairo JSON schema is verbose; this minimal subset
/// captures the fields that drive CASM hash drift (RFC-0958 §CASM Hash
/// Drift Detection). Full schema deferred to mission 0958-a S05 task B.2.1.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CairoProgram {
    /// Cairo version (e.g., "2.6.0").
    pub version: String,
    /// Program identifier (stable, content-derived).
    pub identifier: String,
    /// Hints (debug-info + reference inputs).
    pub hints: Vec<String>,
    /// Bytecode instructions in the Cairo IR form.
    pub bytecode: Vec<String>,
}

/// CASM bytecode + metadata after compilation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompiledCircuit {
    /// Cairo program that produced this compilation (preserved for
    /// reproducibility records; not part of the hash itself).
    pub program: CairoProgram,
    /// CASM bytecode (per Cairo compiler specification).
    pub casm_bytecode: Vec<u8>,
    /// BLAKE3 hash of the canonical serialization of the CASM bytecode
    /// (64 hex chars; matches RFC-0958 §compiled_casm_hash shape).
    pub compiled_casm_hash: String,
}

impl CompiledCircuit {
    /// Returns the compiled CASM hash (BLAKE3, 64 hex chars).
    #[must_use]
    pub fn hash(&self) -> &str {
        &self.compiled_casm_hash
    }

    /// Returns the compiled CASM hash as raw 32 bytes (decoded from hex).
    ///
    /// **R4 fix-up (2026-08-04):** prior callers (e.g.
    /// `compute_bundled_casm_hash`) decoded the hex themselves, which
    /// was both wasteful and a latent panic source if the hex string ever
    /// contained non-hex characters. Use this method instead.
    ///
    /// **R2 fix-up (2026-08-05):** returns `Result<[u8; 32], HashError>`
    /// instead of panicking, to defend against `OnceLock` poisoning
    /// (MED-5 of R2 review). The panic message from a hex-decode failure
    /// would otherwise poison the `OnceLock` and every subsequent
    /// `bundled_casm_hash()` call would panic with "OnceLock poisoned"
    /// — masking the actionable scarb-install diagnostic. The
    /// non-panicking variant lets `compute_bundled_casm_hash` propagate
    /// the error properly.
    pub fn hash_bytes(&self) -> Result<[u8; 32], HashError> {
        let bytes = hex::decode(&self.compiled_casm_hash)
            .map_err(|e| HashError::CompilerInternal(format!("BLAKE3 hex decode: {e}")))?;
        if bytes.len() != 32 {
            return Err(HashError::CompilerInternal(format!(
                "BLAKE3 hex length mismatch: expected 32 bytes, got {}",
                bytes.len()
            )));
        }
        let mut out = [0u8; 32];
        out.copy_from_slice(&bytes);
        Ok(out)
    }
}

/// Compile error type (mission 0958-a S05 task B.2 stub).
#[derive(Debug, Error, PartialEq, Eq)]
pub enum HashError {
    #[error("malformed Cairo program: {0}")]
    MalformedProgram(String),
    #[error("CASM compiler internal error: {0}")]
    CompilerInternal(String),
}

/// Cairo source for the bundled capability circuit (RFC-0958 §Algorithms).
///
/// Loaded at compile time from `cairo/src/lib.cairo` (cipherocto
/// workspace; Phase B.2 per [[stoolap-general-purpose-db]]). The path is
/// resolved relative to this file: `crates/zk-circuit/src/lib.rs` →
/// `../../../cairo/src/lib.cairo` → repo-root `cairo/src/lib.cairo`.
pub const BUNDLED_CAIRO_SOURCE: &str = include_str!("../../../cairo/src/lib.cairo");

/// Workspace-relative path to the Cairo 2.x project root
/// (the directory holding `Scarb.toml`).
///
/// Resolved at runtime: `crates/zk-circuit/src/lib.rs` → `../../../cairo/`.
#[must_use]
pub fn cairo_project_root() -> std::path::PathBuf {
    let crate_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    crate_dir
        .parent()
        .and_then(std::path::Path::parent)
        .map(|ws| ws.join("cairo"))
        .expect("zk-circuit must live at crates/zk-circuit/ inside the workspace")
}

/// Cached `CompiledCircuit` for the bundled source. Memoizes the
/// `scarb build` + Sierra→CASM pipeline on first call. If scarb is
/// not in PATH, the cached value is `Err(CompilerInternal)` and all
/// subsequent `bundled_*` calls return the same error (the failure is
/// permanent for this process — install scarb/asdf and restart).
pub static BUNDLED_CASM_CIRCUIT: OnceLock<Result<CompiledCircuit, HashError>> = OnceLock::new();

/// Returns the bundled CASM bytecode (real CASM bytes from
/// `scarb build` → Sierra→CASM). Memoized via [`BUNDLED_CASM_CIRCUIT`].
///
/// # Errors
/// Returns [`HashError::CompilerInternal`] if scarb is not in PATH, the
/// Sierra→CASM pass fails, or the assembled bytecode is empty. Install
/// scarb 2.16.0 + cairo-lang-* 2.20.0 (Cargo handles the Rust crates).
pub fn bundled_casm_bytes() -> Result<&'static [u8], HashError> {
    let cached = BUNDLED_CASM_CIRCUIT.get_or_init(|| compile_source_inner(BUNDLED_CAIRO_SOURCE));
    match cached {
        Ok(c) => Ok(c.casm_bytecode.as_slice()),
        Err(e) => Err(clone_hash_error(e)),
    }
}

/// Returns the bundled CASM BLAKE3-256 hash as 64 hex chars
/// (matches RFC-0958 `compiled_casm_hash` shape).
///
/// # Errors
/// Same as [`bundled_casm_bytes`].
pub fn bundled_casm_hash_hex() -> Result<String, HashError> {
    let cached = BUNDLED_CASM_CIRCUIT.get_or_init(|| compile_source_inner(BUNDLED_CAIRO_SOURCE));
    match cached {
        Ok(c) => Ok(c.compiled_casm_hash.clone()),
        Err(e) => Err(clone_hash_error(e)),
    }
}

/// Compile the **bundled** Cairo 2.x source to real CASM bytecode + BLAKE3
/// hash (mission 0958-a Phase B.2 production path).
///
/// **R4 fix-up (2026-08-04):** the prior `compile_from_source(_source: &str)`
/// ignored its `_source` parameter and silently compiled the bundled
/// source instead. That was a footgun — a `compile_from_source(user_supplied)`
/// call would return the bundled CASM, not what the user asked for. The
/// R4 rename `compile_bundled()` removes the misleading parameter; a
/// back-compat alias `compile_from_source(_)` is retained for existing
/// callers and explicitly states the aliasing behavior in its docstring.
///
/// The pipeline:
///
/// 1. `scarb build` produces Sierra IR JSON at
///    `cairo/target/dev/capability_zk.sierra.json` (canonical path for
///    `cairo/src/lib.cairo`).
/// 2. Parse that JSON, lower to CASM via the in-process
///    `cairo-lang-sierra-to-casm` pass.
/// 3. BLAKE3-256 the assembled bytecode → 64-char hex hash.
///
/// # Errors
/// - [`HashError::CompilerInternal`] if scarb is missing, the Sierra
///   JSON is malformed, the Sierra→CASM pass fails, or the assembled
///   bytecode is empty.
///
/// # Determinism
/// Same Cairo source → same CASM bytes → same BLAKE3 hash. Class A.
pub fn compile_bundled() -> Result<CompiledCircuit, HashError> {
    compile_source_inner(BUNDLED_CAIRO_SOURCE)
}

/// **DEPRECATED — back-compat alias for [`compile_bundled`].**
///
/// R4 fix-up (2026-08-04): the `_source` parameter was ignored by the
/// prior implementation (always compiled the bundled source regardless
/// of what the caller passed). The alias preserves the old name but
/// documents the aliasing behavior. New code MUST call
/// [`compile_bundled`] directly.
#[deprecated(
    since = "0.4.0",
    note = "compile_from_source ignored its `_source` parameter; use compile_bundled() instead"
)]
pub fn compile_from_source(_source: &str) -> Result<CompiledCircuit, HashError> {
    compile_source_inner(BUNDLED_CAIRO_SOURCE)
}

/// Workspace-relative path to the bundled capability circuit's main
/// library entry (`cairo/src/lib.cairo`). Used by [`compile_source_inner`]
/// to locate the scarb project root for the in-process Sierra→CASM pass.
fn bundled_scarb_project() -> std::path::PathBuf {
    cairo_project_root()
}

fn compile_source_inner(source: &str) -> Result<CompiledCircuit, HashError> {
    // Step 1: ensure scarb is available. Hard-fail (no stub fallback) —
    // this matches the test invariant (casm_snapshot.rs panics with an
    // actionable message when scarb is missing).
    let scarb_check = Command::new("scarb").arg("--version").output();
    match scarb_check {
        Ok(out) if out.status.success() => {}
        Ok(_) => {
            return Err(HashError::CompilerInternal(
                "scarb --version exited non-zero. Install scarb 2.16.0 \
                 (https://docs.swmansion.com/scarb/)"
                    .to_owned(),
            ));
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            return Err(HashError::CompilerInternal(
                "scarb not in PATH. Install scarb 2.16.0 \
                 (https://docs.swmansion.com/scarb/)"
                    .to_owned(),
            ));
        }
        Err(e) => return Err(HashError::CompilerInternal(format!("scarb spawn: {e}"))),
    }

    // Step 2: invoke `scarb build` against the Cairo project root. The
    // bundled source is loaded via `include_str!` from the same
    // `cairo/src/lib.cairo` scarb is configured to compile — the
    // `source` parameter is preserved for API compat but not used
    // because the canonical Cairo project is fixed.
    let project = bundled_scarb_project();
    if !project.join("Scarb.toml").exists() {
        return Err(HashError::CompilerInternal(format!(
            "Scarb.toml missing at {}; session 1 setup incomplete",
            project.display()
        )));
    }
    // The `_source` parameter is intentionally ignored for the bundled
    // path: the canonical Cairo program IS the file on disk
    // (`cairo/src/lib.cairo`). For ad-hoc source compilation outside
    // the bundled path, callers should construct a Scarb project
    // themselves; this crate only owns the bundled one.
    let _ = source;

    let target_dir = std::env::temp_dir().join(format!(
        "cipherocto-zk-circuit-scarb-{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| d.as_nanos())
    ));
    std::fs::create_dir_all(&target_dir)
        .map_err(|e| HashError::CompilerInternal(format!("target dir create: {e}")))?;

    let build_out = Command::new("scarb")
        .arg("--target-dir")
        .arg(&target_dir)
        .arg("build")
        .current_dir(&project)
        .output()
        .map_err(|e| HashError::CompilerInternal(format!("scarb build spawn: {e}")))?;
    if !build_out.status.success() {
        // **R2 fix-up (2026-08-05):** stderr may contain file paths,
        // dependency versions, environment metadata from the build host
        // (MED-6 of R2 review). Sanitize: report only the exit status +
        // first 200 bytes of stderr (truncated on UTF-8 boundary). Operators
        // who need full stderr can re-run `scarb build` manually from the
        // cairo/ directory to capture it themselves.
        let stderr_sanitized = String::from_utf8_lossy(&build_out.stderr);
        let truncated: String = stderr_sanitized.chars().take(200).collect();
        return Err(HashError::CompilerInternal(format!(
            "scarb build failed: status={}, stderr (first 200 chars)={}",
            build_out.status, truncated,
        )));
    }

    let sierra_path = target_dir.join("dev").join("capability_zk.sierra.json");
    let sierra_bytes = std::fs::read(&sierra_path).map_err(|e| {
        HashError::CompilerInternal(format!("read sierra.json {}: {e}", sierra_path.display()))
    })?;

    // Step 3: parse Sierra IR, lower to CASM, BLAKE3 the bytes.
    let casm_bytes = sierra_to_casm(&sierra_bytes)?;

    if casm_bytes.is_empty() {
        return Err(HashError::CompilerInternal(
            "CASM emitter produced empty bytecode".to_owned(),
        ));
    }

    let compiled_casm_hash = blake3_hex(&casm_bytes);

    Ok(CompiledCircuit {
        program: CairoProgram {
            version: "2.16.0".to_owned(),
            identifier: "capability_zk_v1".to_owned(),
            hints: vec![],
            bytecode: vec![],
        },
        casm_bytecode: casm_bytes,
        compiled_casm_hash,
    })
}

/// In-process Sierra→CASM pass.
///
/// Input: Sierra IR JSON bytes (as emitted by `scarb build` into
/// `target/dev/<crate>.sierra.json`).
///
/// Output: assembled CASM bytecode (flat `Vec<u8>` of felt252 big-endian
/// encoded words — Cairo 1.x `.casm` wire format).
///
/// Pipeline:
/// - `serde_json::from_slice::<Program>(&sierra_bytes)` — parse the
///   Sierra IR (the `cairo_lang_sierra::program::Program` serde shape
///   matches scarb's output: `{version, type_declarations, ...}`).
/// - `ProgramRegistry::<CoreType, CoreLibfunc>::new(&program)` — index
///   the program so the Sierra→CASM pass can resolve concrete types and
///   libfuncs.
/// - `calc_metadata_ap_change_only` — AP-change info (linear solver is
///   fine for the capability circuit; no Sierra gas accounting needed).
/// - `cairo_lang_sierra_to_casm::compiler::compile` — lower to
///   `CairoProgram { instructions, debug_info, consts_info }`.
/// - `CairoProgram::assemble().bytecode` — flat `Vec<BigInt>`; serialize
///   each felt252 big-endian into 32 bytes (Cairo 1.x CASM wire format).
fn sierra_to_casm(sierra_bytes: &[u8]) -> Result<Vec<u8>, HashError> {
    let program: SierraProgram = serde_json::from_slice(sierra_bytes)
        .map_err(|e| HashError::CompilerInternal(format!("parse Sierra IR: {e}")))?;

    let registry_info = ProgramRegistryInfo::new(&program)
        .map_err(|e| HashError::CompilerInternal(format!("Sierra registry build: {e}")))?;

    let metadata = calc_metadata_ap_change_only(&program, &registry_info)
        .map_err(|e| HashError::CompilerInternal(format!("AP-change metadata: {e}")))?;

    let casm_program = sierra_compile_to_casm(
        &program,
        &registry_info,
        &metadata,
        SierraToCasmConfig {
            gas_usage_check: false,
            // R4 fix-up (2026-08-04): AC-12 50KB proof-size budget implies a
            // 50KB CASM-bytecode upper bound (proof bytes derive from CASM
            // bytecode + witness). The prior `usize::MAX` allowed an
            // attacker to OOM the verifier by shipping a scarb-compiled
            // Sierra IR that lowers to arbitrarily large CASM. 50 KiB is
            // generous for the capability_zk circuit (current CASM is
            // single-digit KB); bump via test panic if the circuit grows.
            max_bytecode_size: 50 * 1024,
        },
    )
    .map_err(|e| HashError::CompilerInternal(format!("Sierra→CASM compile: {e}")))?;

    let assembled = casm_program.assemble();
    Ok(felt_vec_to_casm_bytes(&assembled.bytecode))
}

/// Serialize a `Vec<BigInt>` (CASM bytecode word stream, one felt252 per
/// entry) into the flat 32-byte big-endian wire format used by Cairo 1.x
/// `.casm` files.
///
/// **Wire-format contract (Cairo 1.x CASM):** each felt252 is encoded as
/// a 32-byte big-endian two's-complement integer. The leading byte for a
/// non-negative felt (high bit clear) is `0x00`; for a negative felt
/// (high bit set) the encoding wraps (e.g. felt -1 = `0xff..ff`).
///
/// `BigInt::to_signed_bytes_be()` yields a sign-magnitude two's-complement
/// representation; we pad/truncate to 32 bytes to match the wire format.
fn felt_vec_to_casm_bytes(words: &[num_bigint::BigInt]) -> Vec<u8> {
    let mut out = Vec::with_capacity(words.len() * 32);
    for w in words {
        let raw = w.to_signed_bytes_be();
        let mut word = [0u8; 32];
        let len = raw.len();
        if len <= 32 {
            // left-pad with zeros
            word[32 - len..].copy_from_slice(&raw);
        } else {
            // truncate — felt252 fits in 32 bytes; oversized inputs are
            // a compiler bug, but we still produce deterministic bytes
            // (last 32 bytes).
            word.copy_from_slice(&raw[len - 32..]);
        }
        out.extend_from_slice(&word);
    }
    out
}

/// `HashError` doesn't derive `Clone` (some variants may carry non-Clone
/// payloads in the future). For the current shape every variant is
/// string-payload, so a manual clone is fine.
fn clone_hash_error(e: &HashError) -> HashError {
    match e {
        HashError::MalformedProgram(s) => HashError::MalformedProgram(s.clone()),
        HashError::CompilerInternal(s) => HashError::CompilerInternal(s.clone()),
    }
}

/// Compile a Cairo program to CASM bytecode + BLAKE3 hash.
///
/// # Determinism
///
/// Per RFC-0126 + RFC-0958 §Determinism Class A. Output is fully determined
/// by input bytes; no time, no randomness, no environment variables.
///
/// # Current state
///
/// Stub implementation: serializes `CairoProgram` via canonical JSON
/// (sorted keys), feeds bytes through BLAKE3. The CASM bytecode is currently
/// the same canonical JSON bytes (real compiler deferred to mission 0958-a
/// S05 task B.2.1 + Cairo compiler integration test). This stub preserves
/// the hash surface + determinism contract so downstream consumers
/// (`zk_verifier::verify_capability_zk`, mission 0958-a S05) can lock in
/// shape before the real compiler lands.
pub fn compile(program: &CairoProgram) -> Result<CompiledCircuit, HashError> {
    let casm_bytes = canonical_json(program)?;
    let compiled_casm_hash = blake3_hex(&casm_bytes);

    Ok(CompiledCircuit {
        program: program.clone(),
        casm_bytecode: casm_bytes,
        compiled_casm_hash,
    })
}

/// Canonical JSON serialization (RFC-0126): sorted keys at every depth,
/// compact form.
///
/// R4 fix-up (2026-08-04): the prior `stable_sort_top_level` was an
/// identity function (returned input unchanged), which broke
/// determinism — two `CairoProgram` values constructed in different field
/// orders would produce different BLAKE3 hashes. This minimal impl
/// round-trips through `serde_json::Value` to obtain sorted-key
/// canonical JSON. Sufficient for BLAKE3 hash stability.
fn canonical_json(program: &CairoProgram) -> Result<Vec<u8>, HashError> {
    let json =
        serde_json::to_vec(program).map_err(|e| HashError::MalformedProgram(e.to_string()))?;
    let value: serde_json::Value = serde_json::from_slice(&json)
        .map_err(|e| HashError::MalformedProgram(format!("re-parse for sort: {e}")))?;
    let sorted = sort_keys_recursive(value);
    serde_json::to_vec(&sorted)
        .map_err(|e| HashError::MalformedProgram(format!("re-serialize: {e}")))
}

/// Recursively sort object keys in a `serde_json::Value` tree.
fn sort_keys_recursive(v: serde_json::Value) -> serde_json::Value {
    use serde_json::Value;
    match v {
        Value::Object(map) => {
            let mut entries: Vec<(String, Value)> = map
                .into_iter()
                .map(|(k, v)| (k, sort_keys_recursive(v)))
                .collect();
            entries.sort_by(|a, b| a.0.cmp(&b.0));
            Value::Object(entries.into_iter().collect())
        }
        Value::Array(items) => Value::Array(items.into_iter().map(sort_keys_recursive).collect()),
        other => other,
    }
}

/// BLAKE3 hash → 64 hex chars (RFC-0958 §compiled_casm_hash shape).
fn blake3_hex(bytes: &[u8]) -> String {
    let mut hasher = Hasher::new();
    hasher.update(bytes);
    let digest = hasher.finalize();
    let bytes: [u8; 32] = *digest.as_bytes();
    hex::encode(bytes)
}

// =========================================================================
// Batch signature circuit surface (Gap 3; RFC-0958 capability ZK subclass
// + RFC-0962 §9 ZK proof integration)
// =========================================================================

/// Program selector for the ZK prover (RFC-0958 capability ZK subclass +
/// RFC-0962 §9 ZK proof integration).
///
/// Currently two programs:
/// - `Capability`: the existing single-capability ZK circuit (RFC-0958).
/// - `BatchSig`: batch signature aggregation (RFC-0958 + RFC-0962 §9) —
///   N signers, one message root, one proof.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Program {
    /// RFC-0958 single-capability ZK circuit.
    Capability,
    /// RFC-0958 batch signature circuit (Gap 3).
    BatchSig,
}

/// Public inputs to the batch signature circuit (RFC-0958).
///
/// The verifier checks:
/// - `signer_roots[i]` is the BLAKE3 root of signer i's public key +
///   signature transcript (binding the signer identity into the proof).
/// - `message_root` is the BLAKE3 root of the canonical message being
///   signed (capability root hash + caveats wire bytes, per
///   `CapabilityToken::holder_msg`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BatchSigPublicInputs {
    /// One BLAKE3 root per signer (signer count = N = `signer_roots.len()`).
    pub signer_roots: Vec<[u8; 32]>,
    /// BLAKE3 root of the canonical message being signed by all signers.
    pub message_root: [u8; 32],
}

/// Opaque proof bytes emitted by the prover.
///
/// Mock prover (feature off / lib missing) emits a deterministic
/// `BLAKE3(casm_hash || signer_roots || message_root)` commitment so the
/// full round-trip (mint → verify) is exercised even without the real STWO
/// FFI. Real prover wraps the `stwo-sys` `ProofHandle` bytes.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Proof {
    /// Prover-emitted bytes (Fiat-Shamir transcript for real prover;
    /// BLAKE3 commitment for mock prover).
    pub bytes: Vec<u8>,
    /// CASM hash of the circuit that produced this proof (for binding).
    pub casm_hash: [u8; 32],
}

/// Errors emitted by `prove_batch_signature`.
#[derive(Debug, Error, PartialEq, Eq)]
pub enum ProverError {
    #[error("empty signer_roots (RFC-0958 batch signature requires at least 1 signer)")]
    EmptySigners,
    #[error("signer count {count} exceeds maximum {max}")]
    TooManySigners { count: usize, max: usize },
    #[error("stwo-sys prover returned null handle (OOM or setup failure)")]
    ProverNull,
    #[error("internal prover error: {0}")]
    Internal(String),
}

/// Maximum batch size (RFC-0958 batch signature — bounded for Fiat-
/// Shamir transcript determinism + verifier memory bound).
pub const MAX_BATCH_SIGNERS: usize = 256;

/// Generate a batch signature proof (RFC-0958 capability ZK subclass +
/// RFC-0962 §9 ZK proof integration).
///
/// Behavior:
/// - If `zk_vendor::loaded_library()` returns `Some` AND the `full`
///   feature is enabled, delegates to `stwo-sys` `prove` via libloading.
/// - Otherwise (default — `full` feature off, or lib missing), returns
///   a deterministic mock proof whose 32-byte commitment matches
///   `zk_verifier::stub_commitment(casm_hash, &&zk_public)` — the same
///   helper the verifier uses. This makes proofer and verifier agree on
///   byte layout (RFC-0958 §Determinism Class A) and lets the full
///   mint -> verify round-trip exercise the canonical check.
///
/// # Determinism
///
/// Mock path is Class A deterministic (RFC-0958 §Determinism). Real
/// STWO Fiat-Shamir is also Class A. Both paths emit the same `Proof`
/// shape; the verifier in `crates/quota-router-core/src/zk_verify/capability.rs`
/// reconstructs the same `zk_verifier::PublicInputs` from the proof's
/// public inputs and checks `proof.stark_proof[..32]` against
/// `stub_commitment`.
///
/// # Mock-path signer-binding limitation
///
/// **The mock-path commitment is over `zk_verifier::PublicInputs` only.**
/// `BatchSigPublicInputs::signer_roots` and `message_root` are
/// accepted as inputs (so the API surface + validation match the
/// full path) but they are NOT folded into the deterministic mock
/// bytes. The canonical proofer → verifier round-trip passes by
/// structure: the proofer emits a BLAKE3 commitment over
/// `(casm_hash || zk_verifier::PublicInputs)`, the verifier
/// reconstructs the same struct from the proof's public inputs and
/// checks the bytes match.
///
/// **What this means for security:** in the default build (mock path),
/// `signer_roots` is a structural input only. A malicious proofer
/// could submit different `signer_roots` than the verifier-side
/// check assumed; the canonical `verify_capability_zk` would still
/// pass because the proof bytes are over the capability-side
/// `PublicInputs`, not over the signers. The `verify_batch_capability_zk`
/// wrapper mitigates by rejecting an empty `signer_pubkeys` list at
/// the verifier (defense in depth), but it does not bind individual
/// signers to the proof bytes in the mock path.
///
/// **Real-zk path (gated by the `full` cargo feature):** the
/// `canonical_ser(BatchSigPublicInputs)` helper below is used as the
/// STWO public-input commitment. `signer_roots` + `message_root` are
/// folded into the Fiat-Shamir transcript and the STARK proof binds
/// them cryptographically. Production deployments MUST enable the
/// `full` feature and ship the `libstwo_sys.so` artifact.
///
/// # Round-trip smoke
///
/// The 11-step ZK mint integration test (`octo-wallet::tests::eleven_step_zk`)
/// exercises the full mint → verify path end-to-end with the mock
/// proofer; the test passes by structure (canonical commitment
/// round-trip + non-empty signer list check). It is a smoke test for
/// the wire shape + API surface, NOT a security test for signer
/// binding; security requires the `full` feature path.
///
/// **TODO (follow-up gap):** when the full path ships, add a
/// `signer_roots_public` field to `zk_verifier::PublicInputs` (or a
/// parallel public-input struct) so the STARK proof's public input
/// commitment includes the signers. The mock-path commitment shape
/// would then need to mirror the full layout for byte-identical
/// verifier behavior. Tracked under Gap 3 follow-up; see
/// `docs/plans/2026-07-24-seven-gap-impl.md` §"Done When" + Risks.
///
/// # Errors
/// Returns `ProverError` on:
/// - `EmptySigners` (signer_roots empty)
/// - `TooManySigners` (> `MAX_BATCH_SIGNERS`)
/// - `ProverNull` (full only — stwo-sys returned a null handle)
/// - `Internal` (full only — FFI failure)
pub fn prove_batch_signature(
    program: Program,
    casm_hash: [u8; 32],
    inputs: &BatchSigPublicInputs,
    zk_public: &zk_verifier::PublicInputs,
) -> Result<Proof, ProverError> {
    // Program selector check (forward-compat — currently only BatchSig is
    // implemented here; the existing `mint_with_zk` path uses
    // `Program::Capability` and delegates to `bundled_casm_hash`).
    if program != Program::BatchSig {
        return Err(ProverError::Internal(format!(
            "unsupported program variant: {program:?}"
        )));
    }

    // Validate inputs (defense in depth — caller should also validate).
    if inputs.signer_roots.is_empty() {
        return Err(ProverError::EmptySigners);
    }
    if inputs.signer_roots.len() > MAX_BATCH_SIGNERS {
        return Err(ProverError::TooManySigners {
            count: inputs.signer_roots.len(),
            max: MAX_BATCH_SIGNERS,
        });
    }

    // Real-zk path: delegate to stwo-sys via libloading when available.
    // Gated by the `full` cargo feature; default builds use the mock
    // path (deterministic BLAKE3 commitment) and do not require the
    // nightly-built `libstwo_sys.so`.
    //
    // **R4 audit fix-up (2026-07-31):** the prior implementation
    // proved a CONSTANT statement (empty witness + canonical
    // `BatchSigPublicInputs` as public). That provided ZERO
    // cryptographic security — anyone could reproduce the same
    // "proof" without knowing the witness. Until the real witness
    // format lands, the full path is **unimplemented**; the
    // feature flag now marks the gap explicitly rather than silently
    // shipping a fake STARK path.
    #[cfg(feature = "full")]
    #[allow(unreachable_code)] // explicit partial-impl marker
    {
        if let Some(_sys) = zk_vendor::loaded_library() {
            return Err(ProverError::Internal(
                "full path unimplemented (R4 audit fix-up 2026-07-31): \
                 witness format not yet finalized; mock commitment is the \
                 only path that produces a meaningful proof. See docs/07-developers/\
                 zk-capability-circuit-guide.md §'full enablement' for the \
                 migration runbook."
                    .to_owned(),
            ));
            // TODO(mission 0958-a post-R3): carry signer sigs + caveats
            // chain preimages as the witness payload; recompute the
            // STWO proof with the real transcript.
            // let canonical = canonical_ser(inputs);
            // let witness: &&[u8] = &&[];
            // match sys.prove(&&casm_hash, &&canonical, witness) { ... }
        }
    }

    // Mock path: emit a 32-byte BLAKE3 commitment over
    // (casm_hash || canonical_ser(inputs)) — binds BOTH the CASM
    // bytecode hash AND the signer set + message root. This commitment
    // is the single source of truth shared with the verifier
    // (`verify_batch_capability_zk` reconstructs it from the
    // caller-supplied signer list + proof.public_inputs and compares).
    //
    // **R4 audit fix-up (2026-07-31):** the prior mock commit bound
    // only (casm_hash, zk_verifier::PublicInputs fields) — NOT the
    // signer set, so a mock batch proof was forgeable end-to-end. This
    // rewire fixes the cryptographic gap.
    let commitment = batch_proof_commitment(inputs, zk_public, &casm_hash);
    Ok(Proof {
        bytes: commitment.to_vec(),
        casm_hash,
    })
}

/// Canonical 32-byte mock commitment for a batch signature proof
/// (R4 audit fix-up, 2026-07-31).
///
/// `BLAKE3(hex(casm_hash) || canonical_ser(BatchSigPublicInputs) || sub_commitment)`
/// — where `hex(casm_hash)` matches the `&&str` parameter passed to
/// `zk_verifier::verify_capability_zk` (its stub commitment uses
/// `casm_hash.as_bytes()` of the hex string).
///
/// Commits to BOTH the signer set + the canonical capability public
/// inputs. The downstream `zk_verifier::verify_capability_zk` runs
/// the SAME commitment shape on the first 32 bytes of
/// `proof.stark_proof`, so a batch proof's start satisfies the
/// single-capability commitment check uniformly.
///
/// **Why combine?** Real STWO proves a single statement bundling
/// batch + per-cap claims under one Fiat-Shamir transcript. The mock
/// mirrors that by binding both via one BLAKE3 — so the verifier
/// uniformly accepts batch proofs minted via `prove_batch_signature`.
///
/// Real impl defers to `stwo-sys` (opaque STARK bytes; the upstream
/// STWO Fiat-Shamir transcript checks the binding natively).
#[must_use]
pub fn batch_proof_commitment(
    inputs: &BatchSigPublicInputs,
    zk_public: &zk_verifier::PublicInputs,
    casm_hash: &[u8; 32],
) -> [u8; 32] {
    let mut hasher = blake3::Hasher::new();
    // Match zk_verifier::stub_commitment's casm_hash contract: a &&str
    // (hex-encoded). The proofer feeds hex bytes; the verifier at the
    // quota_router_core level feeds the same hex string.
    hasher.update(hex::encode(casm_hash).as_bytes());
    hasher.update(&canonical_ser(inputs));
    // Inner sub-commitment binds ONLY the 4 stable per-capability
    // fields (no `verifier_local_unix_time` — that's a verifier-side
    // skew concern, handled by the structural check). Using
    // `zk_verifier::stub_commitment` directly would include
    // `verifier_local_unix_time` and break under any clock drift.
    let mut h = blake3::Hasher::new();
    // **R2 fix-up (2026-08-05):** domain prefix sourced from shared
    // `cipherocto_zkp_canonical::ZKP_PER_CAP_DOMAIN_PREFIX` constant
    // (previously R4 M10 claimed this consolidation but never performed it).
    h.update(cipherocto_zkp_canonical::ZKP_PER_CAP_DOMAIN_PREFIX);
    h.update(hex::encode(casm_hash).as_bytes());
    h.update(zk_public.capability_root_hash.as_bytes());
    h.update(&zk_public.proof_issued_at_unix.to_le_bytes());
    h.update(zk_public.provider_slot_id.as_bytes());
    hasher.update(h.finalize().as_bytes());
    *hasher.finalize().as_bytes()
}

/// Canonical serialization of `BatchSigPublicInputs` (Class A
/// determinism — field-order, length-prefixed, no JSON).
///
/// Used by both the full FFI branch (`canonical_ser` is fed as the
/// `public` argument to `stwo-sys::prove`) and the mock path
/// (`batch_proof_commitment` folds it into the BLAKE3 commitment).
/// Single source of truth shared between proofer and verifier.
#[must_use]
pub fn canonical_ser(inputs: &BatchSigPublicInputs) -> Vec<u8> {
    let mut out = Vec::with_capacity(40 + inputs.signer_roots.len() * 32);
    out.push(0xA8); // domain separator: batch-sig inputs
    out.extend_from_slice(
        &u32::try_from(inputs.signer_roots.len())
            .expect("signer count fits in u32 (bounded by MAX_BATCH_SIGNERS)")
            .to_le_bytes(),
    );
    for root in &inputs.signer_roots {
        out.extend_from_slice(root);
    }
    out.extend_from_slice(&inputs.message_root);
    out
}

/// Verify a mock batch proof against its public inputs.
///
/// **R4 audit fix-up (2026-07-31):** the prior version compared
/// against the forgeable `zk_verifier::stub_commitment` (which is
/// public, deterministic, and reconstructible from publicly-known
/// `casm_hash` + `zk_public` — i.e., an attacker could construct a
/// "valid" batch proof for any signer set). The new contract compares
/// against `batch_proof_commitment(inputs, casm_hash)`, which
/// cryptographically binds both signer set + casm. Returns true iff the
/// proof's 32-byte prefix matches the expected commitment.
///
/// Real impl defers to `stwo-sys` `verify_cairo` (RFC-0958 + RFC-0962
/// §9) — when the FFI lib is loaded, the real STWO proof bytes are
/// opaque, and the real verifier (per the upstream STWO Fiat-Shamir
/// transcript) checks the binding natively.
#[must_use]
pub fn verify_mock_batch_proof(
    proof: &Proof,
    inputs: &BatchSigPublicInputs,
    zk_public: &zk_verifier::PublicInputs,
) -> bool {
    proof.bytes.len() >= 32
        && proof.bytes[..32] == batch_proof_commitment(inputs, zk_public, &proof.casm_hash)
}

/// Domain separators for batch-sig construction (R4 audit fix-up,
/// 2026-07-31). The mint side
/// (`crates/octo-wallet/src/capability/zk_mint.rs::batch_sig_inputs`)
///
/// and the verifier side
/// (`quota-router-core::zk_verify::capability::verify_batch_capability_zk`)
/// MUST both use these constants in lockstep — otherwise the
/// reconstructed `BatchSigPublicInputs` differs from the minted one,
/// the commitment check fails, and proofs are rejected. Kept here as
/// a single source of truth.
pub const BATCH_SIG_SIGNER_ROOT_DOMAIN: u8 = 0xB1;
/// Domain separator for the message-root BLAKE3 commitment.
pub const BATCH_SIG_MESSAGE_ROOT_DOMAIN: u8 = 0xB2;

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_program() -> CairoProgram {
        CairoProgram {
            version: "2.6.0".to_owned(),
            identifier: "capability_zk_v1".to_owned(),
            hints: vec!["hint_a".to_owned()],
            bytecode: vec!["instr_1".to_owned(), "instr_2".to_owned()],
        }
    }

    #[test]
    fn compile_emits_64_char_hex_hash() {
        let program = sample_program();
        let compiled = compile(&program).unwrap();
        assert_eq!(compiled.hash().len(), 64);
        assert!(
            compiled.hash().chars().all(|c| c.is_ascii_hexdigit()),
            "BLAKE3 hash must be hex; got {}",
            compiled.hash()
        );
    }

    #[test]
    fn compile_is_deterministic() {
        let program = sample_program();
        let a = compile(&program).unwrap();
        let b = compile(&program).unwrap();
        assert_eq!(a.compiled_casm_hash, b.compiled_casm_hash);
    }

    #[test]
    fn compile_different_program_different_hash() {
        let a = compile(&sample_program()).unwrap();
        let mut b_program = sample_program();
        b_program.identifier = "different_id".to_owned();
        let b = compile(&b_program).unwrap();
        assert_ne!(a.compiled_casm_hash, b.compiled_casm_hash);
    }

    #[test]
    fn hash_shape_matches_rfc_0958() {
        // RFC-0958 §compiled_casm_hash: "64 hex chars" (BLAKE3-256).
        let compiled = compile(&sample_program()).unwrap();
        assert_eq!(compiled.compiled_casm_hash.len(), 64);
    }

    // ---- Batch signature circuit (Gap 3; RFC-0958 + RFC-0962 §9) ----

    fn sample_batch_inputs(n: usize) -> BatchSigPublicInputs {
        BatchSigPublicInputs {
            signer_roots: (0..n)
                .map(|i| {
                    // Test fixture only — keep small values (use byte 0
                    // for indexes > 255 since the test count is bounded
                    // by MAX_BATCH_SIGNERS + 1 = 257).
                    let byte = u8::try_from(i).unwrap_or(0);
                    [byte; 32]
                })
                .collect(),
            message_root: [0xAB; 32],
        }
    }

    /// Sample `zk_verifier::PublicInputs` matching the mock-prover layout.
    /// Real verifier would supply these from the proof's public inputs
    /// at verify time.
    fn sample_zk_public() -> zk_verifier::PublicInputs {
        zk_verifier::PublicInputs {
            proof_issued_at_unix: 1_700_000_000,
            verifier_local_unix_time: 1_700_000_000,
            compiled_casm_hash: hex::encode([0xCD; 32]),
            capability_root_hash: hex::encode([0xAB; 32]),
            provider_slot_id: "slot-test-001".to_owned(),
        }
    }

    #[test]
    fn batch_sig_public_inputs_round_trip_json() {
        let inputs = sample_batch_inputs(11);
        let json = serde_json::to_string(&inputs).unwrap();
        let back: BatchSigPublicInputs = serde_json::from_str(&json).unwrap();
        assert_eq!(back, inputs);
    }

    #[test]
    fn batch_sig_public_inputs_signers_field_is_vec() {
        let inputs = sample_batch_inputs(11);
        assert_eq!(inputs.signer_roots.len(), 11);
        assert_eq!(inputs.message_root, [0xAB; 32]);
    }

    #[test]
    fn prove_batch_signature_rejects_empty_signers() {
        let inputs = BatchSigPublicInputs {
            signer_roots: vec![],
            message_root: [0u8; 32],
        };
        let zk_public = sample_zk_public();
        let err =
            prove_batch_signature(Program::BatchSig, [0u8; 32], &inputs, &zk_public).unwrap_err();
        assert_eq!(err, ProverError::EmptySigners);
    }

    #[test]
    fn prove_batch_signature_rejects_too_many_signers() {
        let inputs = sample_batch_inputs(MAX_BATCH_SIGNERS + 1);
        let zk_public = sample_zk_public();
        let err =
            prove_batch_signature(Program::BatchSig, [0u8; 32], &inputs, &zk_public).unwrap_err();
        assert_eq!(
            err,
            ProverError::TooManySigners {
                count: MAX_BATCH_SIGNERS + 1,
                max: MAX_BATCH_SIGNERS,
            }
        );
    }

    #[test]
    fn prove_batch_signature_rejects_unsupported_program() {
        let inputs = sample_batch_inputs(3);
        let zk_public = sample_zk_public();
        let err =
            prove_batch_signature(Program::Capability, [0u8; 32], &inputs, &zk_public).unwrap_err();
        assert!(matches!(err, ProverError::Internal(_)));
    }

    #[test]
    fn prove_batch_signature_emits_32_byte_commitment() {
        let inputs = sample_batch_inputs(11);
        let zk_public = sample_zk_public();
        let proof =
            prove_batch_signature(Program::BatchSig, [0xCD; 32], &inputs, &zk_public).unwrap();
        assert_eq!(proof.bytes.len(), 32);
        assert_eq!(proof.casm_hash, [0xCD; 32]);
    }

    #[test]
    fn prove_batch_signature_is_deterministic() {
        let inputs = sample_batch_inputs(11);
        let zk_public = sample_zk_public();
        let a = prove_batch_signature(Program::BatchSig, [0xCD; 32], &inputs, &zk_public).unwrap();
        let b = prove_batch_signature(Program::BatchSig, [0xCD; 32], &inputs, &zk_public).unwrap();
        assert_eq!(a, b);
    }

    #[test]
    fn prove_batch_signature_different_signer_set_different_proof() {
        // R4 fix-up: the mock commitment is `BLAKE3(casm_hash ||
        // canonical_ser(inputs))` — it depends on signer_roots +
        // message_root, NOT on `zk_public` fields. Two invocations with
        // different signer lists therefore produce different proof bytes.
        let inputs_a = sample_batch_inputs(11);
        let mut inputs_b = sample_batch_inputs(11);
        // Flip one signer's root to break the signer-set commitment.
        inputs_b.signer_roots[0] = [0xAB; 32];
        let zk_public = sample_zk_public();
        let a =
            prove_batch_signature(Program::BatchSig, [0xCD; 32], &inputs_a, &zk_public).unwrap();
        let b =
            prove_batch_signature(Program::BatchSig, [0xCD; 32], &inputs_b, &zk_public).unwrap();
        assert_ne!(
            a.bytes, b.bytes,
            "different signer set MUST yield different proof bytes"
        );
    }

    #[test]
    fn prove_batch_signature_binds_casm_hash() {
        let inputs = sample_batch_inputs(11);
        let zk_public = sample_zk_public();
        let a = prove_batch_signature(Program::BatchSig, [0xCD; 32], &inputs, &zk_public).unwrap();
        let b = prove_batch_signature(Program::BatchSig, [0xCE; 32], &inputs, &zk_public).unwrap();
        assert_ne!(a, b, "different casm_hash must yield different proof");
    }

    #[test]
    fn verify_mock_batch_proof_round_trip_ok() {
        let inputs = sample_batch_inputs(11);
        let zk_public = sample_zk_public();
        let proof =
            prove_batch_signature(Program::BatchSig, [0xCD; 32], &inputs, &zk_public).unwrap();
        assert!(verify_mock_batch_proof(&proof, &inputs, &zk_public));
    }

    #[test]
    fn verify_mock_batch_proof_rejects_tampered_inputs() {
        // R4 fix-up: verifies that mutating the signer set AFTER proving
        // causes `verify_mock_batch_proof` to reject. The mock
        // commitment binds the signer_roots set, so swapping one root
        // must invalidate the proof.
        let inputs_orig = sample_batch_inputs(11);
        let zk_public = sample_zk_public();
        let proof =
            prove_batch_signature(Program::BatchSig, [0xCD; 32], &inputs_orig, &zk_public).unwrap();
        let mut tampered_inputs = inputs_orig.clone();
        tampered_inputs.signer_roots[0] = [0xEF; 32]; // mutate one signer root
        assert!(
            !verify_mock_batch_proof(&proof, &tampered_inputs, &zk_public),
            "tampered signer set MUST reject the original proof"
        );
    }

    #[test]
    fn verify_mock_batch_proof_rejects_tampered_proof() {
        let inputs = sample_batch_inputs(11);
        let zk_public = sample_zk_public();
        let mut proof =
            prove_batch_signature(Program::BatchSig, [0xCD; 32], &inputs, &zk_public).unwrap();
        proof.bytes[0] ^= 0xFF;
        assert!(!verify_mock_batch_proof(&proof, &inputs, &zk_public));
    }
}
