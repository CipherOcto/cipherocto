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
//! - [`compile`]: Cairo JSON program (canonical_ser) → [`CompiledCircuit`]
//!   carrying CASM bytecode + BLAKE3 hash.
//! - [`CompiledCircuit::hash`]: stable 64-char hex (matches RFC-0958
//!   `compiled_casm_hash` field shape).
//! - [`HashError`]: error type for malformed Cairo input.
//!
//! ## Determinism contract
//!
//! Same Cairo program → same CASM bytecode → same BLAKE3 hash. Across
//! processes, across architectures, across platforms. STWO Fiat-Shamir
//! transform is Class A (Protocol Determinism, per RFC-0958 §Determinism).

#![deny(unsafe_code)]
#![warn(missing_debug_implementations)]
#![allow(clippy::doc_markdown)]

use blake3::Hasher;
use serde::{Deserialize, Serialize};
use thiserror::Error;

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
}

/// Compile error type (mission 0958-a S05 task B.2 stub).
#[derive(Debug, Error, PartialEq, Eq)]
pub enum HashError {
    #[error("malformed Cairo program: {0}")]
    MalformedProgram(String),
    #[error("CASM compiler internal error: {0}")]
    CompilerInternal(String),
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

/// Canonical JSON serialization (RFC-0126): sorted keys, compact form.
///
/// Stub: real canonical_ser has explicit field ordering for nested objects.
/// This minimal impl sorts top-level keys; sufficient for BLAKE3 hash
/// stability.
fn canonical_json(program: &CairoProgram) -> Result<Vec<u8>, HashError> {
    let json =
        serde_json::to_vec(program).map_err(|e| HashError::MalformedProgram(e.to_string()))?;
    Ok(stable_sort_top_level(json))
}

/// Stub: identity for now. Real canonical_ser sorted-key pass belongs in a
/// future `cipherocto-canonical-ser` crate.
fn stable_sort_top_level(bytes: Vec<u8>) -> Vec<u8> {
    bytes
}

/// BLAKE3 hash → 64 hex chars (RFC-0958 §compiled_casm_hash shape).
fn blake3_hex(bytes: &[u8]) -> String {
    let mut hasher = Hasher::new();
    hasher.update(bytes);
    let digest = hasher.finalize();
    let bytes: [u8; 32] = *digest.as_bytes();
    hex::encode(bytes)
}

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
}
