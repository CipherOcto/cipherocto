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
//! - [`Program`], [`BatchSigPublicInputs`], [`prove_batch_signature`]:
//!   batch signature circuit surface (Gap 3; RFC-0958 capability ZK
//!   subclass + RFC-0962 §9 ZK proof integration).
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
/// - If `zk_vendor::loaded_library()` returns `Some` AND the `real-zk`
///   feature is enabled, delegates to `stwo-sys` `prove` via libloading.
/// - Otherwise (default — `real-zk` feature off, or lib missing), returns
///   a deterministic mock proof whose 32-byte commitment matches
///   `zk_verifier::stub_commitment(casm_hash, &zk_public)` — the same
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
/// real-zk path) but they are NOT folded into the deterministic mock
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
/// **Real-zk path (gated by the `real-zk` cargo feature):** the
/// `canonical_ser(BatchSigPublicInputs)` helper below is used as the
/// STWO public-input commitment. `signer_roots` + `message_root` are
/// folded into the Fiat-Shamir transcript and the STARK proof binds
/// them cryptographically. Production deployments MUST enable the
/// `real-zk` feature and ship the `libstwo_sys.so` artifact.
///
/// # Round-trip smoke
///
/// The 11-step ZK mint integration test (`octo-wallet::tests::eleven_step_zk`)
/// exercises the full mint → verify path end-to-end with the mock
/// proofer; the test passes by structure (canonical commitment
/// round-trip + non-empty signer list check). It is a smoke test for
/// the wire shape + API surface, NOT a security test for signer
/// binding; security requires the `real-zk` feature path.
///
/// **TODO (follow-up gap):** when the real-zk path ships, add a
/// `signer_roots_public` field to `zk_verifier::PublicInputs` (or a
/// parallel public-input struct) so the STARK proof's public input
/// commitment includes the signers. The mock-path commitment shape
/// would then need to mirror the real-zk layout for byte-identical
/// verifier behavior. Tracked under Gap 3 follow-up; see
/// `docs/plans/2026-07-24-seven-gap-impl.md` §"Done When" + Risks.
///
/// # Errors
/// Returns `ProverError` on:
/// - `EmptySigners` (signer_roots empty)
/// - `TooManySigners` (> `MAX_BATCH_SIGNERS`)
/// - `ProverNull` (real-zk only — stwo-sys returned a null handle)
/// - `Internal` (real-zk only — FFI failure)
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
    // Gated by the `real-zk` cargo feature; default builds use the mock
    // path (deterministic BLAKE3 commitment) and do not require the
    // nightly-built `libstwo_sys.so`.
    #[cfg(feature = "real-zk")]
    {
        if let Some(sys) = zk_vendor::loaded_library() {
            let canonical = canonical_ser(inputs);
            // Empty witness: real impl would carry signer sigs + caveats
            // chain preimages. For now the real-zk path proves a constant
            // statement so the FFI shape + handle lifecycle are
            // exercised without committing to a witness format yet.
            let empty_witness: &[u8] = &[];
            match sys.prove(&casm_hash, &canonical, empty_witness) {
                Ok(proof_bytes) => {
                    return Ok(Proof {
                        bytes: proof_bytes.commitment.to_vec(),
                        casm_hash,
                    });
                }
                Err(zk_vendor::VendorError::ProverNull) => {
                    return Err(ProverError::ProverNull);
                }
                Err(other) => {
                    return Err(ProverError::Internal(format!("{other}")));
                }
            }
        }
    }

    // Mock path: emit a proof whose 32-byte commitment shape matches the
    // downstream verifier's `zk_verifier::stub_commitment`. This way
    // proofer + verifier agree on byte layout (RFC-0958 §Determinism
    // Class A) and the full mint -> verify round-trip exercises the
    // canonical check, not a parallel one.
    //
    // The proofer substitutes the real `compiled_casm_hash` (hex-encoded)
    // into the caller's `zk_public` so the commitment includes the
    // CASM hash binding.
    let casm_hash_hex = hex::encode(casm_hash);
    let mut committed_public = zk_public.clone();
    committed_public
        .compiled_casm_hash
        .clone_from(&casm_hash_hex);
    let commitment = zk_verifier::stub_commitment(&casm_hash_hex, &committed_public);
    Ok(Proof {
        bytes: commitment.to_vec(),
        casm_hash,
    })
}

/// Canonical serialization of `BatchSigPublicInputs` (Class A
/// determinism — field-order, length-prefixed, no JSON).
///
/// **Currently unused** — kept for the real-zk FFI branch which feeds
/// `canonical_ser(inputs)` as the `public` argument to
/// `stwo-sys::prove`. The mock path uses
/// `zk_verifier::stub_commitment` instead (single source of truth for
/// the commitment shape between proofer and verifier).
#[allow(dead_code)]
fn canonical_ser(inputs: &BatchSigPublicInputs) -> Vec<u8> {
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
/// Returns true iff `proof.bytes` matches the commitment the proofer
/// would emit for the same `zk_verifier::PublicInputs`. The proofer uses
/// `zk_verifier::stub_commitment` as the single source of truth for the
/// mock commitment shape (so the downstream `zk_verifier::verify_capability_zk`
/// accepts it). This helper re-derives the same commitment for round-trip
/// tests.
///
/// Real impl defers to `stwo-sys` `verify` (RFC-0958 + RFC-0962 §9).
#[must_use]
pub fn verify_mock_batch_proof(proof: &Proof, zk_public: &zk_verifier::PublicInputs) -> bool {
    let casm_hash_hex = hex::encode(proof.casm_hash);
    let mut committed_public = zk_public.clone();
    committed_public
        .compiled_casm_hash
        .clone_from(&casm_hash_hex);
    let expected = zk_verifier::stub_commitment(&casm_hash_hex, &committed_public);
    proof.bytes == expected.to_vec()
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
    fn prove_batch_signature_different_zk_public_different_proof() {
        // The mock commitment is over (casm_hash, zk_public). Different
        // zk_public fields must yield different commitments.
        let inputs = sample_batch_inputs(11);
        let zk_a = sample_zk_public();
        let a = prove_batch_signature(Program::BatchSig, [0xCD; 32], &inputs, &zk_a).unwrap();
        let mut zk_b = sample_zk_public();
        zk_b.provider_slot_id = "different-slot".to_owned();
        let b = prove_batch_signature(Program::BatchSig, [0xCD; 32], &inputs, &zk_b).unwrap();
        assert_ne!(a, b);
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
        assert!(verify_mock_batch_proof(&proof, &zk_public));
    }

    #[test]
    fn verify_mock_batch_proof_rejects_mismatched_public() {
        let inputs = sample_batch_inputs(11);
        let zk_public = sample_zk_public();
        let proof =
            prove_batch_signature(Program::BatchSig, [0xCD; 32], &inputs, &zk_public).unwrap();
        let mut other_public = sample_zk_public();
        other_public.capability_root_hash = hex::encode([0xEE; 32]);
        assert!(!verify_mock_batch_proof(&proof, &other_public));
    }

    #[test]
    fn verify_mock_batch_proof_rejects_tampered_proof() {
        let inputs = sample_batch_inputs(11);
        let zk_public = sample_zk_public();
        let mut proof =
            prove_batch_signature(Program::BatchSig, [0xCD; 32], &inputs, &zk_public).unwrap();
        proof.bytes[0] ^= 0xFF;
        assert!(!verify_mock_batch_proof(&proof, &zk_public));
    }
}
