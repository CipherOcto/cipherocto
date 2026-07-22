//! `verify_capability_zk` wrapper (RFC-0958 §Algorithms).
//!
//! For S05 MVP: implements the verify logic with stub STWO delegation.
//! Production wiring (cairo-compile + stwo-plugin) lives in the stoolap fork.
//!
//! Per RFC-0958 v1.1 R1 H8 fix: clock skew bounds check added
//! (`MAX_SKEW_SECS = 300`); per R3 N5 fix: emits `ZkVerifyError::ClockSkewExceeded`.

use super::{ProofBundle, PublicInputs, ZkVerifyError};

/// Maximum tolerable clock skew between prover and verifier (RFC-0958 §Time Bounds).
/// Defense against malicious prover setting arbitrary wall-clock.
pub const MAX_SKEW_SECS: u64 = 300;

/// Verify a ZK capability proof against expected public inputs.
///
/// Algorithm (RFC-0958 §3.5):
/// 1. `proof.public_inputs != expected_public_inputs` → PublicInputMismatch
/// 2. `proof.casm_hash != COMPILED_CASM_BLAKE3_HASH` → CasmHashMismatch
/// 3. Clock skew bounds check (R1 H8 fix): `|proof.current_unix_time - verifier_local_unix_time| > MAX_SKEW_SECS` → ClockSkewExceeded
/// 4. STWO verify (constant-time) → StwoVerifyError
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

    // 3. Clock skew bounds check (R1 H8 fix). Defense against malicious prover
    //    setting arbitrary wall-clock.
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

    // 4. STWO verify (stub for S05 MVP; real STWO delegation lands in stoolap fork).
    #[allow(unused_variables)]
    {
        let _ = &proof.stark_proof; // silence unused warning in MVP
        Ok(())
    }
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

    fn sample_proof() -> ProofBundle {
        ProofBundle {
            stark_proof: vec![0xab; 64],
            public_inputs: PublicInputs {
                ask_id: [1u8; 32],
                axes_consumed: vec![("input_tokens_per_1k".to_owned(), 100)],
                cap_root_hash: [2u8; 32],
                invocation_hash: [3u8; 32],
                holder_did: "did:octo:holder".to_owned(),
                current_unix_time: 1_700_000_000,
                output_hash: None,
            },
            casm_hash: [0x42; 32],
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
    fn verify_accepts_matching_inputs() {
        let proof = sample_proof();
        let expected = proof.public_inputs.clone();
        let casm = proof.casm_hash;
        let now = proof.public_inputs.current_unix_time;
        // MVP stub: passes without real STWO verify.
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
}
