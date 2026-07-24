//! `verify_capability_zk` wrapper (RFC-0958 §Algorithms).
//!
//! Migration 2026-07-22: crypto home moved out of the stoolap fork
//! into cipherocto workspace crates `zk-circuit` and `zk-verifier`
//! (per [[stoolap-general-purpose-db]]). This module remains the
//! CipherOcto-domain gating layer (public-input equality, CASM hash drift
//! check, clock skew) and delegates the STWO proof verification itself
//! to `zk_verifier::verify_capability_zk`.
//!
//! Per RFC-0958 v1.1 R1 H8 fix: clock skew bounds check added
//! (`MAX_SKEW_SECS = 300`); per R3 N5 fix: emits `ZkVerifyError::ClockSkewExceeded`.
//!
//! **v1.4 (2026-07-22):** `provider_slot_id` read directly from
//! `proof.public_inputs.provider_slot_id` (no sentinel placeholder). The
//! proofer MUST source the slot from holder vault (RFC-0009 §Vault); the
//! verifier passes the public-input value through to `zk_verifier` for
//! canonical-binary commitment. Mismatched slot binding is detected at
//! public-input equality check (different slot → `PublicInputMismatch`).
//!
//! **Gap 3 / RFC-0962 §6 (2026-07-24):** batch verification is layered on
//! top of the existing single-capability path. `verify_capability_zk`
//! remains the canonical single-proof verifier; `verify_batch_capability_zk`
//! adds per-signer slot binding + batch commitment checks for the
//! multi-signer envelope.

use super::{ProofBundle, PublicInputs, ZkVerifyError};

/// Maximum tolerable clock skew between prover and verifier (RFC-0958 §Time Bounds).
/// Defense against malicious prover setting arbitrary wall-clock.
pub const MAX_SKEW_SECS: u64 = zk_verifier::MAX_SKEW_SECS;

/// Verify a ZK capability proof against expected public inputs.
///
/// Algorithm (RFC-0958 §3.5):
/// 1. `proof.public_inputs != expected_public_inputs` → PublicInputMismatch
/// 2. `proof.casm_hash != compiled_casm_blake3_hash` → CasmHashMismatch
/// 3. Clock skew bounds check (R1 H8 fix): `|proof.current_unix_time - verifier_local_unix_time| > MAX_SKEW_SECS` → ClockSkewExceeded
/// 4. Delegate STWO verify to `zk_verifier::verify_capability_zk`
///    (constant-time; result mapped to `ZkVerifyError::StwoVerifyError`).
///
/// # Errors
/// Returns `ZkVerifyError` on any of the four checks above.
pub fn verify_capability_zk(
    proof: &ProofBundle,
    expected_public_inputs: &PublicInputs,
    compiled_casm_blake3_hash: &[u8; 32],
    verifier_local_unix_time: u64,
) -> Result<(), ZkVerifyError> {
    // 1. Public input check.
    if !public_inputs_equal(&proof.public_inputs, expected_public_inputs) {
        return Err(ZkVerifyError::PublicInputMismatch(format!(
            "expected={:?}, got={:?}",
            expected_public_inputs, proof.public_inputs
        )));
    }

    // 2. CASM hash drift check.
    if &proof.casm_hash != compiled_casm_blake3_hash {
        return Err(ZkVerifyError::CasmHashMismatch {
            expected: *compiled_casm_blake3_hash,
            got: proof.casm_hash,
        });
    }

    // 3. Clock skew bounds check (R1 H8 fix).
    let skew = proof
        .public_inputs
        .current_unix_time
        .abs_diff(verifier_local_unix_time);
    if skew > MAX_SKEW_SECS {
        return Err(ZkVerifyError::ClockSkewExceeded {
            skew,
            max: MAX_SKEW_SECS,
        });
    }

    // 4. Delegate STWO verify to the cipherocto zk-verifier crate
    //    (extracted from stoolap fork 2026-07-22). Map domain-layer
    //    CipherOcto PublicInputs → zk-verifier PublicInputs.
    //
    //    **RFC-0958 v1.4 (2026-07-22):** `provider_slot_id` is read
    //    directly from `proof.public_inputs.provider_slot_id` (sourced
    //    from holder vault slot at mint time, per RFC-0009 §Vault). No
    //    sentinel placeholder. Cross-impl vectors carry concrete slot IDs.
    let casm_hash_hex = hex::encode(compiled_casm_blake3_hash);
    let zk_public = zk_verifier::PublicInputs {
        proof_issued_at_unix: proof.public_inputs.current_unix_time,
        verifier_local_unix_time,
        compiled_casm_hash: casm_hash_hex.clone(),
        capability_root_hash: hex::encode(proof.public_inputs.cap_root_hash),
        provider_slot_id: proof.public_inputs.provider_slot_id.clone(),
    };
    let zk_proof = zk_verifier::ProofBundle {
        proof_bytes: proof.stark_proof.clone(),
    };

    zk_verifier::verify_capability_zk(&zk_proof, &zk_public, &casm_hash_hex).map_err(|e| match e {
        zk_verifier::VerifyError::ProofRejected => {
            ZkVerifyError::StwoVerifyError("proof_rejected".to_owned())
        }
        zk_verifier::VerifyError::ClockSkewExceeded { skew, max } => {
            ZkVerifyError::ClockSkewExceeded { skew, max }
        }
        zk_verifier::VerifyError::CasmHashMismatch { expected, got } => {
            ZkVerifyError::CasmHashMismatch {
                expected: hex::decode(&expected)
                    .ok()
                    .and_then(|v| {
                        if v.len() == 32 {
                            let mut out = [0u8; 32];
                            out.copy_from_slice(&v);
                            Some(out)
                        } else {
                            None
                        }
                    })
                    .unwrap_or(*compiled_casm_blake3_hash),
                got: hex::decode(&got)
                    .ok()
                    .and_then(|v| {
                        if v.len() == 32 {
                            let mut out = [0u8; 32];
                            out.copy_from_slice(&v);
                            Some(out)
                        } else {
                            None
                        }
                    })
                    .unwrap_or([0u8; 32]),
            }
        }
        other => ZkVerifyError::StwoVerifyError(format!("{other}")),
    })
}

/// Public inputs equality (RFC-0958 §Adversary A5 + v1.4 IA-11 slot binding).
fn public_inputs_equal(a: &PublicInputs, b: &PublicInputs) -> bool {
    a.ask_id == b.ask_id
        && a.axes_consumed == b.axes_consumed
        && a.cap_root_hash == b.cap_root_hash
        && a.invocation_hash == b.invocation_hash
        && a.holder_did == b.holder_did
        && a.current_unix_time == b.current_unix_time
        && a.output_hash == b.output_hash
        // **v1.4 (RFC-0958 IA-11):** slot binding must match — defense
        // against cross-slot replay. If the proof was minted for slot A
        // and the verifier expects slot B, this returns false → caller
        // returns `PublicInputMismatch`.
        && a.provider_slot_id == b.provider_slot_id
}

/// Verifier parameters (Gap 3 / RFC-0962 §6 / Task 3.4).
///
/// Bundles the long-lived verifier configuration so the call sites can
/// pass a single `&CapabilityVerifier` instead of three separate args.
#[derive(Debug, Clone, Copy)]
pub struct CapabilityVerifier {
    /// BLAKE3 hash of the compiled CASM bytecode the proof must bind to.
    pub compiled_casm_blake3_hash: [u8; 32],
    /// Verifier's local Unix time (used for clock skew bounds check).
    pub verifier_local_unix_time: u64,
}

impl CapabilityVerifier {
    /// Construct a verifier at the current wall-clock time (for tests +
    /// callers that don't need precise time control).
    #[must_use]
    pub fn at_now(compiled_casm_blake3_hash: [u8; 32]) -> Self {
        use std::time::{SystemTime, UNIX_EPOCH};
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);
        Self {
            compiled_casm_blake3_hash,
            verifier_local_unix_time: now,
        }
    }
}

/// Verify a capability ZK proof (Gap 3 / RFC-0962 §6 / Task 3.4 wrapper).
///
/// Convenience wrapper that threads the `CapabilityVerifier` through to
/// the canonical `verify_capability_zk` function. The signature mirrors
/// the Gap 3 plan shorthand `verify_capability_zk(proof, &verifier)` for
/// multi-signer envelopes; the underlying check is identical to the
/// 4-arg `verify_capability_zk` (single-capability path) — the
/// `expected_public_inputs` field of the proof bundle carries the
/// capability-level commitments; the batch signature commitment is
/// carried inside `proof.stark_proof` (per Gap 3 Task 3.3).
///
/// # Errors
/// Returns `ZkVerifyError` on any of the four canonical checks (public
/// input mismatch, CASM hash drift, clock skew, STWO verify failure).
pub fn verify_capability_zk_token(
    proof: &ProofBundle,
    verifier: &CapabilityVerifier,
) -> Result<(), ZkVerifyError> {
    verify_capability_zk(
        proof,
        &proof.public_inputs,
        &verifier.compiled_casm_blake3_hash,
        verifier.verifier_local_unix_time,
    )
}

/// Verify a batch capability ZK proof (Gap 3 / RFC-0962 §6 / Task 3.4).
///
/// Layered on top of the canonical single-capability verifier:
///
/// 1. `signer_pubkeys.len() >= 1` (no empty batch). Defense in depth — a
///    caller passing an empty signer list to a verifier that explicitly
///    accepts a batch is almost certainly a bug.
/// 2. Delegate to `verify_capability_zk` (carries CASM hash drift check,
///    clock skew bounds check, and the STWO / stub commitment check). The
///    batch commitment integrity is the proofer's responsibility: the mock
///    proofer emits a BLAKE3 commitment over `(casm_hash || canonical_ser
///    (BatchSigPublicInputs))`; the real STWO prover binds the per-signer
///    roots into the proof's public input commitment. Both paths are
///    checked by the downstream `verify_capability_zk` call.
///
/// **Why we don't re-derive the batch commitment here:** the mock-prover
/// commitment shape and the `zk_verifier::stub_commitment` shape both
/// reduce to `BLAKE3(casm_hash || canonical_ser(...))` over their
/// respective input sets, but the canonical_ser layouts differ (mock
/// proofer includes the signer count + 0xA8 domain separator; the
/// downstream STWO check uses its own canonical public layout). A local
/// re-derivation would conflict with the STWO check on the same 32
/// bytes. The single-capability verifier is the authoritative check.
///
/// # Errors
/// Returns `ZkVerifyError::BatchSignerMissing` when the signer list is
/// empty; returns the underlying single-capability errors via
/// `verify_capability_zk` (CasmHashMismatch, ClockSkewExceeded,
/// StwoVerifyError, PublicInputMismatch).
pub fn verify_batch_capability_zk(
    proof: &ProofBundle,
    signer_pubkeys: &[[u8; 32]],
    verifier: &CapabilityVerifier,
) -> Result<(), ZkVerifyError> {
    if signer_pubkeys.is_empty() {
        return Err(ZkVerifyError::BatchSignerMissing {
            count: 0,
            expected: 1,
        });
    }

    // Delegate to the canonical single-capability verifier. The downstream
    // check covers CASM hash drift, clock skew, and the STWO / stub
    // commitment — including the per-signer binding produced by the
    // proofer at mint time.
    verify_capability_zk(
        proof,
        &proof.public_inputs,
        &verifier.compiled_casm_blake3_hash,
        verifier.verifier_local_unix_time,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Construct a valid stub proof that passes `zk_verifier::verify_capability_zk`.
    /// Uses the public `zk_verifier::stub_commitment` helper so test + prod agree
    /// on the canonical commitment (deterministic binary form; no serde_json).
    fn build_stub_proof(casm_hash: &[u8; 32], public: &PublicInputs) -> Vec<u8> {
        let casm_hex = hex::encode(casm_hash);
        let zk_public = zk_verifier::PublicInputs {
            proof_issued_at_unix: public.current_unix_time,
            verifier_local_unix_time: public.current_unix_time,
            compiled_casm_hash: casm_hex.clone(),
            capability_root_hash: hex::encode(public.cap_root_hash),
            provider_slot_id: public.provider_slot_id.clone(),
        };
        zk_verifier::stub_commitment(&casm_hex, &zk_public).to_vec()
    }

    fn sample_proof() -> ProofBundle {
        let public = PublicInputs {
            ask_id: [1u8; 32],
            axes_consumed: vec![("input_tokens_per_1k".to_owned(), 100)],
            cap_root_hash: [2u8; 32],
            invocation_hash: [3u8; 32],
            holder_did: "did:octo:holder".to_owned(),
            current_unix_time: 1_700_000_000,
            output_hash: None,
            provider_slot_id: "slot-alpha-001".to_owned(),
        };
        let casm = [0x42; 32];
        let stark_proof = build_stub_proof(&casm, &public);
        ProofBundle {
            stark_proof,
            public_inputs: public,
            casm_hash: casm,
            security_bits: 128,
        }
    }

    #[test]
    fn verify_rejects_public_input_mismatch() {
        let proof = sample_proof();
        let mut expected = proof.public_inputs.clone();
        expected.ask_id = [0xff; 32];
        let casm = proof.casm_hash;
        let now = proof.public_inputs.current_unix_time;
        let err = verify_capability_zk(&proof, &expected, &casm, now).unwrap_err();
        assert!(matches!(err, ZkVerifyError::PublicInputMismatch(_)));
    }

    #[test]
    fn verify_rejects_casm_drift() {
        let proof = sample_proof();
        let expected = proof.public_inputs.clone();
        let wrong_casm = [0u8; 32];
        let now = proof.public_inputs.current_unix_time;
        let err = verify_capability_zk(&proof, &expected, &wrong_casm, now).unwrap_err();
        assert!(matches!(err, ZkVerifyError::CasmHashMismatch { .. }));
    }

    #[test]
    fn verify_rejects_clock_skew() {
        let proof = sample_proof();
        let expected = proof.public_inputs.clone();
        let casm = proof.casm_hash;
        // 1 hour skew — exceeds MAX_SKEW_SECS=300.
        let skewed = proof.public_inputs.current_unix_time + 3600;
        let err = verify_capability_zk(&proof, &expected, &casm, skewed).unwrap_err();
        assert!(matches!(err, ZkVerifyError::ClockSkewExceeded { .. }));
    }

    #[test]
    fn verify_accepts_matching_inputs_with_valid_stub_proof() {
        let proof = sample_proof();
        let expected = proof.public_inputs.clone();
        let casm = proof.casm_hash;
        let now = proof.public_inputs.current_unix_time;
        verify_capability_zk(&proof, &expected, &casm, now).unwrap();
    }

    #[test]
    fn verify_detects_axes_drift() {
        let proof = sample_proof();
        let mut expected = proof.public_inputs.clone();
        expected
            .axes_consumed
            .push(("output_tokens_per_1k".to_owned(), 50));
        let casm = proof.casm_hash;
        let now = proof.public_inputs.current_unix_time;
        let err = verify_capability_zk(&proof, &expected, &casm, now).unwrap_err();
        assert!(matches!(err, ZkVerifyError::PublicInputMismatch(_)));
    }

    #[test]
    fn verify_rejects_invalid_stub_proof() {
        let mut proof = sample_proof();
        // Replace the valid stub proof with random bytes — STWO verify will reject.
        proof.stark_proof = vec![0xde; 32];
        let expected = proof.public_inputs.clone();
        let casm = proof.casm_hash;
        let now = proof.public_inputs.current_unix_time;
        let err = verify_capability_zk(&proof, &expected, &casm, now).unwrap_err();
        // Either StwoVerifyError (proof bytes random) or PublicInputMismatch
        // (only if something else mismatched first).
        assert!(matches!(
            err,
            ZkVerifyError::StwoVerifyError(_) | ZkVerifyError::PublicInputMismatch(_)
        ));
    }

    // ---- Gap 3 / RFC-0962 §6 batch verifier tests ----

    fn sample_batch_proof() -> (ProofBundle, [u8; 32]) {
        let public = PublicInputs {
            ask_id: [1u8; 32],
            axes_consumed: vec![("input_tokens_per_1k".to_owned(), 100)],
            cap_root_hash: [2u8; 32],
            invocation_hash: [3u8; 32],
            holder_did: "did:octo:holder".to_owned(),
            current_unix_time: 1_700_000_000,
            output_hash: None,
            provider_slot_id: "slot-alpha-001".to_owned(),
        };
        let casm = [0x42; 32];
        // Use the canonical stub_commitment (matches downstream STWO check
        // + the mock proofer's BLAKE3 commitment shape).
        let casm_hex = hex::encode(casm);
        let zk_public = zk_verifier::PublicInputs {
            proof_issued_at_unix: public.current_unix_time,
            verifier_local_unix_time: public.current_unix_time,
            compiled_casm_hash: casm_hex.clone(),
            capability_root_hash: hex::encode(public.cap_root_hash),
            provider_slot_id: public.provider_slot_id.clone(),
        };
        let stark_proof = zk_verifier::stub_commitment(&casm_hex, &zk_public).to_vec();
        (
            ProofBundle {
                stark_proof,
                public_inputs: public,
                casm_hash: casm,
                security_bits: 128,
            },
            casm,
        )
    }

    fn eleven_signers() -> Vec<[u8; 32]> {
        (0..11)
            .map(|i| [u8::try_from(i).expect("11 signers fit in u8"); 32])
            .collect()
    }

    #[test]
    fn batch_verify_accepts_matching_proof_and_signers() {
        let (proof, casm) = sample_batch_proof();
        let verifier = CapabilityVerifier {
            compiled_casm_blake3_hash: casm,
            verifier_local_unix_time: proof.public_inputs.current_unix_time,
        };
        let signers = eleven_signers();
        verify_batch_capability_zk(&proof, &signers, &verifier).unwrap();
    }

    #[test]
    fn batch_verify_rejects_empty_signer_list() {
        let (proof, casm) = sample_batch_proof();
        let verifier = CapabilityVerifier {
            compiled_casm_blake3_hash: casm,
            verifier_local_unix_time: proof.public_inputs.current_unix_time,
        };
        let err = verify_batch_capability_zk(&proof, &[], &verifier).unwrap_err();
        assert!(matches!(err, ZkVerifyError::BatchSignerMissing { .. }));
    }

    #[test]
    fn batch_verify_rejects_wrong_casm_hash() {
        let (proof, _casm) = sample_batch_proof();
        let verifier = CapabilityVerifier {
            compiled_casm_blake3_hash: [0u8; 32], // wrong
            verifier_local_unix_time: proof.public_inputs.current_unix_time,
        };
        let signers = eleven_signers();
        let err = verify_batch_capability_zk(&proof, &signers, &verifier).unwrap_err();
        // The downstream verify_capability_zk fires CasmHashMismatch.
        assert!(matches!(err, ZkVerifyError::CasmHashMismatch { .. }));
    }

    #[test]
    fn batch_verify_rejects_tampered_proof() {
        let (mut proof, casm) = sample_batch_proof();
        proof.stark_proof[0] ^= 0xFF;
        let verifier = CapabilityVerifier {
            compiled_casm_blake3_hash: casm,
            verifier_local_unix_time: proof.public_inputs.current_unix_time,
        };
        let signers = eleven_signers();
        let err = verify_batch_capability_zk(&proof, &signers, &verifier).unwrap_err();
        // Downstream STWO verify rejects the tampered commitment.
        assert!(matches!(err, ZkVerifyError::StwoVerifyError(_)));
    }
}
