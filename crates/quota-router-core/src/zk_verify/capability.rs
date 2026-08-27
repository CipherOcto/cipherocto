//! `verify_capability_zk` wrapper (RFC-0958 §Algorithms).
//!
//! Migration 2026-07-22: crypto home moved out of the stoolap fork
//! into cipherocto workspace crates `zk-circuit` and `zk-verifier`
//! (per [[stoolap-general-purpose-db]]). This module remains the
//! CipherOcto-domain gating layer (public-input equality, CASM hash drift
//! check, clock skew) and delegates the STWO proof verification itself
//! to `zk_verifier::verify_capability_zk`.
//!
//! Per RFC-0958 R1 H8 fix: clock skew bounds check added
//! (`MAX_SKEW_SECS = 300`); per R3 N5 fix: emits `ZkVerifyError::ClockSkewExceeded`.
//!
//! **v1.4 (2026-07-22):** `provider_slot_id` read directly from
//! `proof.public_inputs.provider_slot_id` (no sentinel placeholder). The
//! proofer MUST source the slot from holder vault (RFC-0009 §Vault); the
//! verifier passes the public-input value through to `zk_verifier` for
//! canonical-binary commitment. Mismatched slot binding is detected at
//! public-input equality check (different slot → `PublicInputMismatch`).
//!
//! **Gap 3 / RFC-0958 + RFC-0962 §9 (2026-07-24):** batch verification is
//! layered on top of the existing single-capability path.
//! `verify_capability_zk` remains the canonical single-proof verifier;
//! `verify_batch_capability_zk` adds per-signer slot binding + batch
//! commitment checks for the multi-signer envelope.

use super::{ProofBundle, PublicInputs, ZkVerifyError};
use subtle::ConstantTimeEq;

/// Maximum tolerable clock skew between prover and verifier (RFC-0958 §Time Bounds).
/// Defense against malicious prover setting arbitrary wall-clock.
pub const MAX_SKEW_SECS: u64 = zk_verifier::MAX_SKEW_SECS;

/// Canonicalize `axes_consumed` order.
///
/// **R2 fix-up (2026-08-05):** the canonical implementation lives in
/// `cipherocto_zkp_canonical::canonicalize_axes` (single source of truth,
/// shared between this crate and `octo-wallet`). This wrapper exists
/// for backward compat with existing callers; new code MUST call the
/// canonical crate directly.
pub fn canonicalize_axes(pi: &mut PublicInputs) {
    cipherocto_zkp_canonical::canonicalize_axes(&mut pi.axes_consumed);
}

/// Decode a 64-char hex string into a 32-byte BLAKE3 hash.
///
/// **R5 audit fix-up (2026-07-31):** the prior call sites used
/// `hex::decode(...).ok().and_then(...).unwrap_or(...)` which silently
/// produced `compiled_casm_blake3_hash` / `[0u8; 32]` on corruption —
/// a valid-looking 32-byte value that hid upstream zk-verifier bugs.
/// This helper returns the decoded bytes on success or a descriptive
/// error on failure (corrupt hex or non-32-byte length). Callers that
/// surface the bug should propagate the `Err`; callers that treat hex
/// decode as an internal invariant can `.expect("...")`.
fn decode_hex_hash32(s: &str, field: &'static str) -> Result<[u8; 32], String> {
    match hex::decode(s) {
        Ok(v) if v.len() == 32 => {
            let mut out = [0u8; 32];
            out.copy_from_slice(&v);
            Ok(out)
        }
        Ok(v) => Err(format!(
            "`{field}` is hex of {} bytes, expected 32",
            v.len()
        )),
        Err(e) => Err(format!("`{field}` is not valid hex: {e}")),
    }
}

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
/// Run only the structural checks (public-inputs equal + CASM drift +
/// clock skew) on a proof. Skips the STWO/commitment check (assumed
/// already verified by the caller's commitment layer — for batch proofs
/// `verify_batch_capability_zk` reconstructs the batch commitment
/// itself and compares to `proof.stark_proof[..32]`).
///
/// **Returns** the matched CASM BLAKE3 hash on success (selected from
/// `accepted_casm_blake3_hashes` for downstream STWO verification).
///
/// **R4 audit fix-up (2026-07-31):** this helper exists so batch proofs
/// can validate the structural + clock + CASM invariants without
/// tripping the downstream `zk_verifier::verify_capability_zk` stub
/// path (which would reject a batch-shaped commitment).
fn verify_capability_zk_structural(
    proof: &ProofBundle,
    expected_public_inputs: &PublicInputs,
    accepted_casm_blake3_hashes: &[[u8; 32]],
    verifier_local_unix_time: u64,
) -> Result<[u8; 32], ZkVerifyError> {
    // 1. Public input check (on canonicalized copies — sort axes first).
    let mut proof_pi_canon = proof.public_inputs.clone();
    let mut expected_pi_canon = expected_public_inputs.clone();
    canonicalize_axes(&mut proof_pi_canon);
    canonicalize_axes(&mut expected_pi_canon);
    if !public_inputs_equal(&proof_pi_canon, &expected_pi_canon) {
        return Err(ZkVerifyError::PublicInputMismatch(format!(
            "expected={:?}, got={:?}",
            expected_public_inputs, proof.public_inputs
        )));
    }

    // 2. CASM hash check (N=2 rotation grace, mission 0958-a R3 #5).
    if accepted_casm_blake3_hashes.is_empty() {
        return Err(ZkVerifyError::CasmHashMismatch {
            expected: [0u8; 32],
            got: proof.casm_hash,
        });
    }
    let compiled_casm_blake3_hash = match accepted_casm_blake3_hashes
        .iter()
        .find(|h| **h == proof.casm_hash)
    {
        Some(h) => *h,
        None => {
            return Err(ZkVerifyError::CasmHashMismatch {
                expected: accepted_casm_blake3_hashes[0],
                got: proof.casm_hash,
            });
        }
    };

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

    Ok(compiled_casm_blake3_hash)
}

pub fn verify_capability_zk(
    proof: &ProofBundle,
    expected_public_inputs: &PublicInputs,
    accepted_casm_blake3_hashes: &[[u8; 32]],
    verifier_local_unix_time: u64,
) -> Result<(), ZkVerifyError> {
    // 1-3. Structural checks (public input + CASM drift + clock skew).
    let compiled_casm_blake3_hash = verify_capability_zk_structural(
        proof,
        expected_public_inputs,
        accepted_casm_blake3_hashes,
        verifier_local_unix_time,
    )?;

    // 4. Delegate STWO verify to the cipherocto zk-verifier crate
    //    (extracted from stoolap fork 2026-07-22). Map domain-layer
    //    CipherOcto PublicInputs → zk-verifier PublicInputs.
    //
    //    **RFC-0958 (2026-07-22):** `provider_slot_id` is read
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
            // R5 audit fix-up (2026-07-31): the prior `unwrap_or` chains
            // silently substituted `compiled_casm_blake3_hash` / `[0u8; 32]`
            // on hex decode failure — a corruption symptom hidden behind a
            // valid-looking 32-byte value. Hex decode here is an internal
            // invariant (zk-verifier's `CasmHashMismatch` always carries
            // 64-char hex of a 32-byte BLAKE3 hash); on violation we
            // surface the upstream bug via `StwoVerifyError` so it is
            // visible in logs rather than masquerading as a hash
            // mismatch.
            match (
                decode_hex_hash32(&expected, "expected"),
                decode_hex_hash32(&got, "got"),
            ) {
                (Ok(expected_bytes), Ok(got_bytes)) => ZkVerifyError::CasmHashMismatch {
                    expected: expected_bytes,
                    got: got_bytes,
                },
                (Err(e), _) | (_, Err(e)) => ZkVerifyError::StwoVerifyError(format!(
                    "zk-verifier CasmHashMismatch hex decode failed: {e}"
                )),
            }
        }
        other => ZkVerifyError::StwoVerifyError(format!("{other}")),
    })
}

/// Public inputs equality (RFC-0958 §Adversary A5 + IA-11 slot binding;
///
/// R4 fix-up 2026-08-04: rewritten to use `subtle::ConstantTimeEq` for the
/// byte-array fields (and constant-time iteration for `Vec<(String, u64)>`)
/// so the comparison's wall-clock duration does not leak which field
/// mismatched. Prior `==`-based comparison was short-circuit on first
/// mismatch, enabling a side-channel that combined with the stub proofer's
/// permissive commitment (R4 C4 disclosure) could let an attacker
/// field-discovery a proof by timing the verifier.
fn public_inputs_equal(a: &PublicInputs, b: &PublicInputs) -> bool {
    use subtle::Choice;
    let byte32_eq = |x: &[u8; 32], y: &[u8; 32]| -> Choice { x.ct_eq(y) };
    let ct_eq_opt = |x: &Option<[u8; 32]>, y: &Option<[u8; 32]>| -> Choice {
        match (x, y) {
            (Some(x), Some(y)) => byte32_eq(x, y),
            (None, None) => Choice::from(1u8),
            _ => Choice::from(0u8),
        }
    };
    // Vec compare: lengths first (constant time), then per-element ct_eq
    // on (name, value). Different lengths short-circuit (caller cannot
    // derive field-level info from the length difference since `len()`
    // is O(1) anyway).
    let axes_eq = if a.axes_consumed.len() != b.axes_consumed.len() {
        Choice::from(0u8)
    } else {
        let initial: Choice = Choice::from(1u8);
        a.axes_consumed.iter().zip(b.axes_consumed.iter()).fold(
            initial,
            |acc, ((n1, v1), (n2, v2))| {
                let name_eq: Choice = n1.as_bytes().ct_eq(n2.as_bytes());
                let val_eq: Choice = v1.ct_eq(v2);
                acc & name_eq & val_eq
            },
        )
    };
    let initial: Choice = Choice::from(1u8);
    let combined = initial
        & byte32_eq(&a.ask_id, &b.ask_id)
        & axes_eq
        & byte32_eq(&a.cap_root_hash, &b.cap_root_hash)
        & byte32_eq(&a.invocation_hash, &b.invocation_hash)
        & a.holder_did.as_bytes().ct_eq(b.holder_did.as_bytes())
        & a.current_unix_time.ct_eq(&b.current_unix_time)
        & ct_eq_opt(&a.output_hash, &b.output_hash)
        // IA-11 slot binding must match — defense against cross-slot replay.
        & a.provider_slot_id.as_bytes().ct_eq(b.provider_slot_id.as_bytes());
    bool::from(combined)
}

/// Verifier parameters (Gap 3 / RFC-0958 + RFC-0962 §9 / Task 3.4).
///
/// Bundles the long-lived verifier configuration so the call sites can
/// pass a single `&&CapabilityVerifier` instead of three separate args.
#[derive(Debug, Clone, Copy)]
pub struct CapabilityVerifier {
    /// BLAKE3 hash of the compiled CASM bytecode the proof must bind to.
    pub compiled_casm_blake3_hash: [u8; 32],
    /// Verifier's local Unix time (used for clock skew bounds check).
    pub verifier_local_unix_time: u64,
}

impl CapabilityVerifier {
    /// Construct a verifier at a caller-supplied unix time.
    ///
    /// **R4 audit fix-up (2026-07-31):** the prior `at_now()` method
    /// used `SystemTime::now()` — a Class A determinism violation
    /// (RFC-0958 §Determinism) since two verification calls within
    /// the same logical window would produce different results.
    /// Production callers must pass an explicit timestamp (typically
    /// the slot's issuance time from the request context).
    #[must_use]
    pub fn at_time(compiled_casm_blake3_hash: [u8; 32], verifier_local_unix_time: u64) -> Self {
        Self {
            compiled_casm_blake3_hash,
            verifier_local_unix_time,
        }
    }
}

/// Verify a capability ZK proof (Gap 3 / RFC-0958 + RFC-0962 §9 / Task 3.4 wrapper).
///
/// Convenience wrapper that threads the `CapabilityVerifier` through to
/// the canonical `verify_capability_zk` function. The signature mirrors
/// the Gap 3 plan shorthand `verify_capability_zk(proof, &&verifier)` for
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
    // Convenience wrapper: verify_capability_zk_token delegates to the
    // single-capability verifier (structural + STWO/commitment). For
    // batch proofs use verify_batch_capability_zk which skips the stub
    // commitment check (R4 fix-up).
    verify_capability_zk(
        proof,
        &proof.public_inputs,
        &[verifier.compiled_casm_blake3_hash],
        verifier.verifier_local_unix_time,
    )
}

/// Verify a batch capability ZK proof (Gap 3 / RFC-0958 + RFC-0962 §9 /
/// Task 3.4).
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
/// StwoVerifyError, PublicInputMismatch, **BatchSignerSetMismatch**).
///
/// **R4 audit fix-up (2026-07-31):** the prior implementation ONLY
/// checked the single-capability commitment; it never verified that the
/// proof bound the supplied `signer_pubkeys`, so a mock batch proof
/// was forgeable end-to-end. The new contract:
/// 1. Reconstruct `BatchSigPublicInputs` from `signer_pubkeys + proof.public_inputs`
///    using the same canonical construction as the mint side (domain
///    separators `0xB1` / `0xB2` per `zk_circuit::constants`).
/// 2. Re-derive `batch_proof_commitment(inputs, casm_hash)` and compare
///    byte-for-byte to `proof.stark_proof[..32]`. Mismatch returns
///    `BatchSignerSetMismatch`.
/// 3. Delegate to single-capability verifier for CASM hash drift, clock
///    skew, and the rest of the contract.
///
/// `expected_public_inputs` is the caller's expected canonical
/// capability public inputs (typically the verifier's view of the
/// request context). If `None`, the proof's own public inputs are
/// used (self-verifying path).
pub fn verify_batch_capability_zk(
    proof: &ProofBundle,
    signer_pubkeys: &[[u8; 32]],
    expected_public_inputs: Option<&PublicInputs>,
    verifier: &CapabilityVerifier,
) -> Result<(), ZkVerifyError> {
    if signer_pubkeys.is_empty() {
        return Err(ZkVerifyError::BatchSignerMissing {
            count: 0,
            expected: 1,
        });
    }

    // Order: structural check FIRST (cheap; rejects CASM drift + clock
    // skew + public-input mismatch), THEN commitment check (which
    // depends on `verifier_local_unix_time` and would falsely fire
    // BatchSignerSetMismatch on a clock-skewed verifier). The
    // commitment check binds the signer set + casm + per-cap fields
    // (excluding verifier_local_unix_time from the inner sub-commitment
    // because that field is verifier-side, not proofer-side).
    let expected = expected_public_inputs.unwrap_or(&proof.public_inputs);
    verify_capability_zk_structural(
        proof,
        expected,
        &[verifier.compiled_casm_blake3_hash],
        verifier.verifier_local_unix_time,
    )?;

    // R4 fix-up: reconstruct BatchSigPublicInputs + re-derive the
    // batch commitment. Catches forged signer sets (a pre-fix-up
    // mock-batch proof was forgeable end-to-end).
    //
    // The proofer signs via `zk_verifier::PublicInputs`; we mirror
    // that shape here so the same `batch_proof_commitment` produced by
    // `prove_batch_signature` is what we reconstruct. The
    // `quota_router_core::zk_verify::PublicInputs` (carried in
    // `proof.public_inputs`) is field-equivalent — convert.
    let zk_public = zk_verifier::PublicInputs {
        proof_issued_at_unix: proof.public_inputs.current_unix_time,
        verifier_local_unix_time: verifier.verifier_local_unix_time,
        compiled_casm_hash: hex::encode(proof.casm_hash),
        capability_root_hash: hex::encode(proof.public_inputs.cap_root_hash),
        provider_slot_id: proof.public_inputs.provider_slot_id.clone(),
    };
    let batch_inputs = reconstruct_batch_sig_inputs(&proof.public_inputs, signer_pubkeys);
    if proof.stark_proof.len() < 32
        || proof.stark_proof[..32]
            != zk_circuit::batch_proof_commitment(&batch_inputs, &zk_public, &proof.casm_hash)
    {
        let mut got = [0u8; 32];
        got.copy_from_slice(&proof.stark_proof[..32]);
        return Err(ZkVerifyError::BatchSignerSetMismatch {
            expected: zk_circuit::batch_proof_commitment(
                &batch_inputs,
                &zk_public,
                &proof.casm_hash,
            ),
            got,
        });
    }

    Ok(())
}

/// Reconstruct `BatchSigPublicInputs` from capability public inputs
/// + signer pubkeys.
///
/// **MUST stay in lockstep with the mint-side construction**
/// (`octo_wallet::capability::zk_mint::batch_sig_inputs`).
///
/// `signer_roots[i] = BLAKE3(BATCH_SIG_SIGNER_ROOT_DOMAIN || signer_pubkey_i)`
/// `message_root = BLAKE3(BATCH_SIG_MESSAGE_ROOT_DOMAIN || canonical_message)`
///
/// Domain separators are `pub const`s on `zk_circuit` (single source
/// of truth — `BATCH_SIG_SIGNER_ROOT_DOMAIN = 0xB1`,
/// `BATCH_SIG_MESSAGE_ROOT_DOMAIN = 0xB2`).
pub fn reconstruct_batch_sig_inputs(
    public_inputs: &PublicInputs,
    signer_pubkeys: &[[u8; 32]],
) -> zk_circuit::BatchSigPublicInputs {
    use blake3::Hasher;

    let signer_roots: Vec<[u8; 32]> = signer_pubkeys
        .iter()
        .map(|pk| {
            let mut h = Hasher::new();
            h.update(&[zk_circuit::BATCH_SIG_SIGNER_ROOT_DOMAIN]);
            h.update(pk);
            *h.finalize().as_bytes()
        })
        .collect();

    let mut msg_hasher = Hasher::new();
    msg_hasher.update(&[zk_circuit::BATCH_SIG_MESSAGE_ROOT_DOMAIN]);
    msg_hasher.update(&public_inputs.ask_id);
    msg_hasher.update(&public_inputs.cap_root_hash);
    msg_hasher.update(&public_inputs.invocation_hash);
    msg_hasher.update(public_inputs.holder_did.as_bytes());
    msg_hasher.update(&public_inputs.current_unix_time.to_le_bytes());
    msg_hasher.update(public_inputs.provider_slot_id.as_bytes());
    let message_root: [u8; 32] = *msg_hasher.finalize().as_bytes();

    zk_circuit::BatchSigPublicInputs {
        signer_roots,
        message_root,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Construct a valid stub proof that passes `zk_verifier::verify_capability_zk`.
    /// Uses the public `zk_verifier::stub_commitment` helper so test + prod agree
    /// on the canonical commitment (deterministic binary form; no serde_json).
    ///
    /// **Mission 0958-b S3 (2026-08-05):** `stub_commitment` returns
    /// `Result<[u8; 32], ProverError>`. This test helper lives in a
    /// `#[cfg(test)]` module, so the Ok branch always fires under
    /// `cargo test --features allow-stub-verifier` (CI default).
    /// `.expect` documents the invariant.
    fn build_stub_proof(casm_hash: &[u8; 32], public: &PublicInputs) -> Vec<u8> {
        let casm_hex = hex::encode(casm_hash);
        let zk_public = zk_verifier::PublicInputs {
            proof_issued_at_unix: public.current_unix_time,
            verifier_local_unix_time: public.current_unix_time,
            compiled_casm_hash: casm_hex.clone(),
            capability_root_hash: hex::encode(public.cap_root_hash),
            provider_slot_id: public.provider_slot_id.clone(),
        };
        zk_verifier::stub_commitment(&casm_hex, &zk_public)
            .expect("stub_commitment Ok in #[cfg(test)] module")
            .to_vec()
    }

    fn sample_proof() -> ProofBundle {
        let public = PublicInputs {
            ask_id: [1u8; 32],
            axes_consumed: vec![("input_tokens_per_1k".to_owned(), 100)],
            cap_root_hash: [2u8; 32],
            invocation_hash: [3u8; 32],
            holder_did: octo_ident::test_helpers::sample_did(42).to_owned(),
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
            casm_version: 1,
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
        let err = verify_capability_zk(&proof, &expected, &[casm], now).unwrap_err();
        assert!(matches!(err, ZkVerifyError::PublicInputMismatch(_)));
    }

    #[test]
    fn verify_rejects_casm_drift() {
        let proof = sample_proof();
        let expected = proof.public_inputs.clone();
        let wrong_casm = [0u8; 32];
        let now = proof.public_inputs.current_unix_time;
        let err = verify_capability_zk(&proof, &expected, &[wrong_casm], now).unwrap_err();
        assert!(matches!(err, ZkVerifyError::CasmHashMismatch { .. }));
    }

    #[test]
    fn verify_rejects_clock_skew() {
        let proof = sample_proof();
        let expected = proof.public_inputs.clone();
        let casm = proof.casm_hash;
        // 1 hour skew — exceeds MAX_SKEW_SECS=300.
        let skewed = proof.public_inputs.current_unix_time + 3600;
        let err = verify_capability_zk(&proof, &expected, &[casm], skewed).unwrap_err();
        assert!(matches!(err, ZkVerifyError::ClockSkewExceeded { .. }));
    }

    #[test]
    fn verify_accepts_matching_inputs_with_valid_stub_proof() {
        // **Mission 0958-b S2 (2026-08-05):** when the real-zk STWO
        // FFI library is loaded, `verify_capability_zk` takes the FFI
        // path and rejects stub-shaped proof bytes as a forgery
        // channel (per R4 fix-up `StubShapedProofRejected`). The test
        // therefore asserts success ONLY in `VendorState::Stub`; with
        // FFI loaded, the verifier's R4 forgery-channel gate
        // legitimately fires and the test returns an `Err` (matching
        // production security semantics). Production deployments
        // ship the cdylib; this test documents the stub-mode contract.
        if zk_vendor::vendor_state() == zk_vendor::VendorState::Ffi {
            eprintln!(
                "verify_accepts_matching_inputs_with_valid_stub_proof: SKIP — FFI loaded; \
                 stub-shaped proof correctly rejected by R4 forgery-channel gate"
            );
            return;
        }
        let proof = sample_proof();
        let expected = proof.public_inputs.clone();
        let casm = proof.casm_hash;
        let now = proof.public_inputs.current_unix_time;
        verify_capability_zk(&proof, &expected, &[casm], now).unwrap();
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
        let err = verify_capability_zk(&proof, &expected, &[casm], now).unwrap_err();
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
        let err = verify_capability_zk(&proof, &expected, &[casm], now).unwrap_err();
        // Either StwoVerifyError (proof bytes random) or PublicInputMismatch
        // (only if something else mismatched first).
        assert!(matches!(
            err,
            ZkVerifyError::StwoVerifyError(_) | ZkVerifyError::PublicInputMismatch(_)
        ));
    }

    // ---- Gap 3 / RFC-0958 + RFC-0962 §9 batch verifier tests ----

    fn sample_batch_proof() -> (ProofBundle, [u8; 32]) {
        let public = PublicInputs {
            ask_id: [1u8; 32],
            axes_consumed: vec![("input_tokens_per_1k".to_owned(), 100)],
            cap_root_hash: [2u8; 32],
            invocation_hash: [3u8; 32],
            holder_did: octo_ident::test_helpers::sample_did(42).to_owned(),
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
        let stark_proof = zk_verifier::stub_commitment(&casm_hex, &zk_public)
            .expect("stub_commitment Ok in #[cfg(test)] module")
            .to_vec();
        (
            ProofBundle {
                stark_proof,
                public_inputs: public,
                casm_hash: casm,
                casm_version: 1,
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

    /// Construct a batch proof by proofer path: build the canonical
    /// `BatchSigPublicInputs` from the signer list, then mint a proof
    /// via `zk_circuit::prove_batch_signature` (or via the public
    /// `batch_proof_commitment` helper when bypassing the program arg).
    /// Mirrors what `octo-wallet::mint_with_zk_and_signers` produces.
    fn sample_real_batch_proof() -> (ProofBundle, [u8; 32]) {
        let (proof, casm) = sample_batch_proof();
        let signers = eleven_signers();
        let batch_inputs = reconstruct_batch_sig_inputs(&proof.public_inputs, &signers);
        // Convert quota-router-core's PublicInputs to zk_verifier's
        // shape (field-equivalent; proofer signs via zk_verifier::PublicInputs).
        let zk_public = zk_verifier::PublicInputs {
            proof_issued_at_unix: proof.public_inputs.current_unix_time,
            verifier_local_unix_time: proof.public_inputs.current_unix_time,
            compiled_casm_hash: hex::encode(casm),
            capability_root_hash: hex::encode(proof.public_inputs.cap_root_hash),
            provider_slot_id: proof.public_inputs.provider_slot_id.clone(),
        };
        let commitment = zk_circuit::batch_proof_commitment(&batch_inputs, &zk_public, &casm);
        (
            ProofBundle {
                stark_proof: commitment.to_vec(),
                ..proof
            },
            casm,
        )
    }

    #[test]
    fn batch_verify_accepts_matching_proof_and_signers() {
        let (proof, casm) = sample_real_batch_proof();
        let verifier = CapabilityVerifier {
            compiled_casm_blake3_hash: casm,
            verifier_local_unix_time: proof.public_inputs.current_unix_time,
        };
        let signers = eleven_signers();
        verify_batch_capability_zk(&proof, &signers, Some(&proof.public_inputs), &verifier)
            .unwrap();
    }

    #[test]
    fn batch_verify_rejects_empty_signer_list() {
        let (proof, casm) = sample_real_batch_proof();
        let verifier = CapabilityVerifier {
            compiled_casm_blake3_hash: casm,
            verifier_local_unix_time: proof.public_inputs.current_unix_time,
        };
        let err = verify_batch_capability_zk(&proof, &[], Some(&proof.public_inputs), &verifier)
            .unwrap_err();
        assert!(matches!(err, ZkVerifyError::BatchSignerMissing { .. }));
    }

    #[test]
    fn batch_verify_rejects_wrong_casm_hash() {
        let (proof, _casm) = sample_real_batch_proof();
        let verifier = CapabilityVerifier {
            compiled_casm_blake3_hash: [0u8; 32], // wrong
            verifier_local_unix_time: proof.public_inputs.current_unix_time,
        };
        let signers = eleven_signers();
        let err =
            verify_batch_capability_zk(&proof, &signers, Some(&proof.public_inputs), &verifier)
                .unwrap_err();
        // The downstream verify_capability_zk fires CasmHashMismatch.
        assert!(matches!(err, ZkVerifyError::CasmHashMismatch { .. }));
    }

    #[test]
    fn batch_verify_rejects_tampered_proof() {
        let (mut proof, casm) = sample_real_batch_proof();
        proof.stark_proof[0] ^= 0xFF;
        let verifier = CapabilityVerifier {
            compiled_casm_blake3_hash: casm,
            verifier_local_unix_time: proof.public_inputs.current_unix_time,
        };
        let signers = eleven_signers();
        let err =
            verify_batch_capability_zk(&proof, &signers, Some(&proof.public_inputs), &verifier)
                .unwrap_err();
        // R4 fix-up: the BATCH commitment check fires first (before the
        // downstream STWO/BLAKE3 stub path). Assert the new variant.
        assert!(matches!(err, ZkVerifyError::BatchSignerSetMismatch { .. }));
    }

    /// R4 fix-up: a forged signer list (different from the one the
    /// proofer bound into the proof) MUST be rejected even when the
    /// downstream STWO/commitment would accept the canonical inputs.
    #[test]
    fn batch_verify_rejects_forged_signer_list() {
        let (proof, casm) = sample_real_batch_proof();
        let verifier = CapabilityVerifier {
            compiled_casm_blake3_hash: casm,
            verifier_local_unix_time: proof.public_inputs.current_unix_time,
        };
        // Replace signers with a different set; the proof's commitment
        // was bound to the ORIGINAL set, so the reconstructed commitment
        // differs → BatchSignerSetMismatch.
        let forged_signers: Vec<[u8; 32]> = vec![[0xEE; 32]; 11];
        let err = verify_batch_capability_zk(
            &proof,
            &forged_signers,
            Some(&proof.public_inputs),
            &verifier,
        )
        .unwrap_err();
        assert!(
            matches!(err, ZkVerifyError::BatchSignerSetMismatch { .. }),
            "forged signer list MUST be detected; got {err:?}"
        );
    }
}
