//! Vendored STWO STARK prover (cipherocto workspace).
//!
//! **Status (2026-07-31, mission 0958-a S05 Session 2):** This crate
//! currently provides the stable-rust API surface (`Prover`, `verify`,
//! `Proof`, `PublicInputs`, `ProverError`, `VerifyError`) but the
//! implementations are **deterministic mocks** — same byte layout as the
//! legacy BLAKE3 stub commitment in `zk-verifier`. The real
//! `keep-stwo/stwo` source drop is pending the `cipherocto-stable` tag
//! on the cipherocto fork (see [`PATCHES.md`](./PATCHES.md)). When the
//! real source lands, this crate replaces the mock bodies with
//! upstream STWO function calls; the API surface does not change.
//!
//! **Crypto home:** cipherocto workspace per [[stoolap-general-purpose-db]]
//! (2026-07-22). NOT the stoolap fork.
//!
//! **Stable-rust only:** no `#![feature(...)]`, no nightly intrinsics.
//! MSRV pinned at the workspace root (`crates/zk-vendor/rust-toolchain.toml`).
//! See [`RUSTTOOLCHAIN.md`](./RUSTTOOLCHAIN.md).

#![deny(unsafe_code)]
#![warn(missing_debug_implementations)]
#![allow(clippy::doc_markdown)]

use std::sync::OnceLock;

use blake3::Hasher;
use serde::{Deserialize, Serialize};
use thiserror::Error;
use tracing::warn;

/// Domain-separated BLAKE3 commitment used by the vendored STWO mock.
///
/// **Matches the legacy `zk-verifier::stub_commitment` shape exactly**
/// (`blake3(casm_hash || canonical_public)`) so existing mint→verify
/// round-trip tests stay green. When the real `keep-stwo/stwo` source
/// lands, this becomes the Fiat-Shamir transcript over the canonical
/// public inputs + witness; the public API surface does not change.
const VENDORED_COMMIT_DOMAIN: &[u8] = b"zkp:";

/// Public inputs (RFC-0958 §Public Inputs + vendored STWO prover shape).
///
/// Mirrors `zk_verifier::PublicInputs` for the vendored in-process path.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PublicInputs {
    /// Unix timestamp at which the proof was generated.
    pub proof_issued_at_unix: u64,
    /// Unix timestamp at which the proving wallet verified the proof.
    pub verifier_local_unix_time: u64,
    /// BLAKE3 hash of the compiled CASM (hex-encoded; matches
    /// `CompiledCircuit.hash`).
    pub compiled_casm_hash: String,
    /// Capability root hash (HMAC-BLAKE3 root per RFC-0957 §Macaroon root).
    pub capability_root_hash: String,
    /// Provider slot ID (RFC-0009 vault slot).
    pub provider_slot_id: String,
}

/// STARK proof (RFC-0958 §Proof Bundle + vendored STWO proof shape).
///
/// Mock: opaque `Vec<u8>` carrying the deterministic BLAKE3 commitment.
/// Real implementation wraps `stwo::core::proof::Proof` from upstream
/// keep-stwo/stwo.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Proof {
    /// Proof bytes (STWO canonical encoding; mock = BLAKE3 commitment).
    pub proof_bytes: Vec<u8>,
}

/// Errors emitted by the vendored STWO prover.
#[derive(Debug, Error, PartialEq, Eq)]
pub enum ProverError {
    #[error("CASM hash mismatch: expected {expected}, got {got}")]
    CasmHashMismatch { expected: String, got: String },
    #[error("internal prover error: {0}")]
    Internal(String),
}

/// Errors emitted by the vendored STWO verifier.
#[derive(Debug, Error, PartialEq, Eq)]
pub enum VerifyError {
    #[error("CASM hash mismatch: expected {expected}, got {got}")]
    CasmHashMismatch { expected: String, got: String },
    #[error("vendored STWO verifier: BLAKE3 commitment mismatch")]
    ProofRejected,
    #[error("internal verifier error: {0}")]
    Internal(String),
}

/// Prover handle (RFC-0958 + vendored STWO).
///
/// Wraps the in-process STWO prover state. The vendored mock emits
/// deterministic BLAKE3 commitments; the real implementation will wrap
/// `stwo::core::prover::Prover` from upstream.
#[derive(Debug, Clone, Default)]
pub struct Prover;

impl Prover {
    /// Construct a new prover.
    #[must_use]
    pub fn new() -> Self {
        Self
    }

    /// Generate a proof (mock: BLAKE3 commitment matching the
    /// `zk_verifier::stub_commitment` byte shape).
    ///
    /// The commitment is `blake3(casm_hash_str || canonical_public(public))`
    /// — byte-identical to the legacy stub so `verify_capability_zk`
    /// accepts both vendored + stub proofs interchangeably. When the real
    /// `keep-stwo/stwo` source lands, this becomes the Fiat-Shamir
    /// transcript over the canonical public inputs + witness.
    ///
    /// # Errors
    /// Returns `ProverError::CasmHashMismatch` if `casm_hash_hex`
    /// disagrees with the commitment contract.
    pub fn prove(&self, casm_hash_hex: &str, public: &PublicInputs) -> Result<Proof, ProverError> {
        if public.compiled_casm_hash != casm_hash_hex {
            return Err(ProverError::CasmHashMismatch {
                expected: casm_hash_hex.to_owned(),
                got: public.compiled_casm_hash.clone(),
            });
        }
        let canon_pub = canonicalize_public(public);
        let commitment = compute_commitment(casm_hash_hex.as_bytes(), &canon_pub);
        Ok(Proof {
            proof_bytes: commitment.to_vec(),
        })
    }
}

/// One-shot warning so dev / CI logs surface that the vendored STWO is
/// still a deterministic mock (not the real STWO source drop).
fn warn_vendored_mock_once() {
    static WARNED: OnceLock<()> = OnceLock::new();
    WARNED.get_or_init(|| {
        warn!(
            "vendored STWO is a deterministic mock (real source drop pending \
             keep-stwo/stwo@cipherocto-stable tag access). Mint→verify \
             round-trip is structurally sound but NOT cryptographically binding."
        );
    });
}

/// Canonicalize public inputs (matches `zk_verifier::canonicalize_public`
/// byte-for-byte: `b"zkp:"` + LEB128-length-prefixed strings +
/// little-endian u64 fields). Real STWO source drop will replace this
/// with the upstream canonical serializer.
fn canonicalize_public(public: &PublicInputs) -> Vec<u8> {
    let mut out = Vec::with_capacity(160);
    out.extend_from_slice(VENDORED_COMMIT_DOMAIN);
    out.extend_from_slice(&leb128_len(public.compiled_casm_hash.as_bytes()));
    out.extend_from_slice(public.compiled_casm_hash.as_bytes());
    out.extend_from_slice(&leb128_len(public.capability_root_hash.as_bytes()));
    out.extend_from_slice(public.capability_root_hash.as_bytes());
    out.extend_from_slice(&leb128_len(public.provider_slot_id.as_bytes()));
    out.extend_from_slice(public.provider_slot_id.as_bytes());
    out.extend_from_slice(&public.proof_issued_at_unix.to_le_bytes());
    out.extend_from_slice(&public.verifier_local_unix_time.to_le_bytes());
    out
}

/// LEB128-style length prefix (matches `zk_verifier::leb128_len`).
fn leb128_len(bytes: &[u8]) -> [u8; 4] {
    let len = bytes.len();
    u32::try_from(len)
        .expect("string length fits in u32")
        .to_le_bytes()
}

/// Compute the mock BLAKE3 commitment (matches `zk_verifier::stub_commitment`).
///
/// Matches `zk_verifier::stub_commitment` byte-for-byte:
/// `blake3(casm_hash_str || canonical_public(public))`. Takes `casm_hash`
/// as `&[u8]` (raw bytes of the hex-encoded string) so callers can hash
/// directly without re-decoding.
fn compute_commitment(casm_hash: &[u8], canon_pub: &[u8]) -> [u8; 32] {
    let mut h = Hasher::new();
    h.update(casm_hash);
    h.update(canon_pub);
    let digest = h.finalize();
    let mut out = [0u8; 32];
    out.copy_from_slice(digest.as_bytes());
    out
}

/// Verify a proof (mock: recompute BLAKE3 commitment + compare).
///
/// Takes the same shape as `zk_verifier::stub_commitment` —
/// `blake3(casm_hash_str || canonical_public(public))` — so vendored +
/// stub proofs are interchangeable.
///
/// # Errors
/// Returns `VerifyError::ProofRejected` if the proof bytes do not match
/// the recomputed commitment.
pub fn verify(
    proof: &Proof,
    public: &PublicInputs,
    casm_hash_hex: &str,
) -> Result<(), VerifyError> {
    warn_vendored_mock_once();
    let canon_pub = canonicalize_public(public);
    let expected = compute_commitment(casm_hash_hex.as_bytes(), &canon_pub);
    if proof.proof_bytes.len() != 32 {
        return Err(VerifyError::Internal(format!(
            "proof bytes length {} != 32",
            proof.proof_bytes.len()
        )));
    }
    let mut provided = [0u8; 32];
    provided.copy_from_slice(&proof.proof_bytes[..32]);
    if provided == expected {
        Ok(())
    } else {
        Err(VerifyError::ProofRejected)
    }
}

/// Hex-encode 32 bytes to 64-char lowercase hex.
#[allow(dead_code)] // used only in tests + may be needed by future real source drop
fn hex_encode(bytes: &[u8; 32]) -> String {
    use std::fmt::Write as _;
    let mut s = String::with_capacity(64);
    for b in bytes {
        let _ = write!(s, "{b:02x}");
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_public() -> PublicInputs {
        PublicInputs {
            proof_issued_at_unix: 1_700_000_000,
            verifier_local_unix_time: 1_700_000_000,
            compiled_casm_hash: hex_encode(&[0xCD; 32]),
            capability_root_hash: hex_encode(&[0xAB; 32]),
            provider_slot_id: "slot-test-001".to_owned(),
        }
    }

    #[test]
    fn prove_emits_32_byte_commitment() {
        let prover = Prover::new();
        let casm_hex = hex_encode(&[0xCD; 32]);
        let proof = prover
            .prove(&casm_hex, &sample_public())
            .expect("prove succeeds");
        assert_eq!(proof.proof_bytes.len(), 32);
    }

    #[test]
    fn prove_then_verify_round_trip() {
        let prover = Prover::new();
        let public = sample_public();
        let casm_hex = hex_encode(&[0xCD; 32]);
        let proof = prover.prove(&casm_hex, &public).expect("prove succeeds");
        verify(&proof, &public, &casm_hex).expect("verify round-trip");
    }

    #[test]
    fn verify_rejects_tampered_proof() {
        let prover = Prover::new();
        let public = sample_public();
        let casm_hex = hex_encode(&[0xCD; 32]);
        let mut proof = prover.prove(&casm_hex, &public).expect("prove succeeds");
        proof.proof_bytes[0] ^= 0xFF;
        let err = verify(&proof, &public, &casm_hex).expect_err("tampered proof must be rejected");
        assert!(matches!(err, VerifyError::ProofRejected));
    }

    #[test]
    fn prove_rejects_casm_hash_mismatch() {
        let prover = Prover::new();
        let mut public = sample_public();
        public.compiled_casm_hash = hex_encode(&[0xEE; 32]);
        let err = prover
            .prove(&hex_encode(&[0xCD; 32]), &public)
            .expect_err("casm hash mismatch rejected");
        assert!(matches!(err, ProverError::CasmHashMismatch { .. }));
    }

    #[test]
    fn commitment_matches_zk_verifier_stub_shape() {
        // Cross-impl: vendored commitment MUST match the byte shape of
        // `zk_verifier::stub_commitment` (`blake3(casm_hash_str ||
        // canonical_public)`) so existing mint→verify round-trip tests
        // pass when vendored_stwo() delegation lands.
        let public = sample_public();
        let casm_hex = hex_encode(&[0xCD; 32]);
        let vendored_commit = {
            let prover = Prover::new();
            prover.prove(&casm_hex, &public).unwrap().proof_bytes
        };
        let mut h = blake3::Hasher::new();
        h.update(casm_hex.as_bytes());
        h.update(&canonicalize_public(&public));
        let expected = h.finalize();
        let mut expected_bytes = [0u8; 32];
        expected_bytes.copy_from_slice(expected.as_bytes());
        assert_eq!(vendored_commit, expected_bytes.to_vec());
    }

    #[test]
    fn hex_encode_produces_64_lowercase_chars() {
        let original = [0xAB; 32];
        let encoded = hex_encode(&original);
        assert_eq!(encoded.len(), 64);
        assert!(encoded.chars().all(|c| c.is_ascii_hexdigit()));
        assert!(encoded.chars().all(|c| !c.is_ascii_uppercase()));
    }
}
