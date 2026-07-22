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
    //    provider_slot_id: migration 2026-07-22 — CipherOcto domain
    //    PublicInputs doesn't carry a provider_slot_id (slot is a wallet
    //    concern, not a public-input concern). TBD 0958 v1.4. For now, use
    //    a stable sentinel so the verifier's canonical JSON matches the
    //    proofer's. The proofer (zk-mint) is the one that needs to embed
    //    the slot ID; for cross-impl vectors, both sides must agree on the
    //    sentinel. Tests hard-code this same sentinel in
    //    `make_stub_proof_bytes`.
    let casm_hash_hex = hex::encode(compiled_casm_blake3_hash);
    let zk_public = zk_verifier::PublicInputs {
        proof_issued_at_unix: proof.public_inputs.current_unix_time,
        verifier_local_unix_time,
        compiled_casm_hash: casm_hash_hex.clone(),
        capability_root_hash: hex::encode(proof.public_inputs.cap_root_hash),
        provider_slot_id: "test-slot".to_owned(),
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

/// Public inputs equality (RFC-0958 §Adversary A5).
fn public_inputs_equal(a: &PublicInputs, b: &PublicInputs) -> bool {
    a.ask_id == b.ask_id
        && a.axes_consumed == b.axes_consumed
        && a.cap_root_hash == b.cap_root_hash
        && a.invocation_hash == b.invocation_hash
        && a.holder_did == b.holder_did
        && a.current_unix_time == b.current_unix_time
        && a.output_hash == b.output_hash
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
            provider_slot_id: "test-slot".to_owned(),
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
}
