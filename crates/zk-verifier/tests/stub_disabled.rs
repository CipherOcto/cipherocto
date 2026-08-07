//! Integration test: `verify_capability_zk` fail-closed invariant
//! (mission 0958-a R3 fix-up).
//! Test-internal doc-comment lint relaxation.
// Integration tests are not user-facing API.
#![allow(clippy::doc_markdown)]
#![allow(clippy::doc_lazy_continuation)]

//!
//! Asserts that a production build (`--no-default-features`,
//! missing `libstwo_sys.so`) returns `Err(VerifyError::StubDisabled)`
//! instead of silently accepting the forgeable BLAKE3 stub path.
//!
//! Default-features run (with `allow-stub-verifier`) takes the BLAKE3
//! stub path and rejects the arbitrary proof bytes with
//! `ProofRejected` — also NOT a silent acceptance. Both branches are
//! safe; the release invariant asserts the FIRST.

#[cfg(not(feature = "allow-stub-verifier"))]
use zk_verifier::VerifyError;
#[cfg(not(feature = "allow-stub-verifier"))]
use zk_verifier::ProverError;
use zk_verifier::{ProofBundle, PublicInputs};

const TV_FIXED_TIME: u64 = 1_700_000_000;

fn sample(casm: &str) -> (ProofBundle, PublicInputs) {
    let public = PublicInputs {
        proof_issued_at_unix: TV_FIXED_TIME,
        verifier_local_unix_time: TV_FIXED_TIME,
        compiled_casm_hash: casm.to_owned(),
        capability_root_hash: "caproot-pg".to_owned(),
        provider_slot_id: "slot-pg".to_owned(),
    };
    let proof = ProofBundle {
        proof_bytes: vec![0xcd; 32],
    };
    (proof, public)
}

/// R3 fix-up invariant (release-build): the production-gate must fail
/// closed when neither `libstwo_sys.so` is reachable NOR the
/// `allow-stub-verifier` feature is enabled. Run with
/// `--no-default-features` to exercise.
#[cfg(not(feature = "allow-stub-verifier"))]
#[test]
fn release_gate_fails_closed() {
    let (proof, public) = sample("casm-release-gate");
    let result = zk_verifier::verify_capability_zk(&proof, &public, "casm-release-gate");
    assert!(
        matches!(result, Err(VerifyError::StubDisabled)),
        "release gate must return StubDisabled; got {result:?}"
    );
}

/// **Mission 0958-b S3 (2026-08-05):** `stub_commitment` returns
/// `Result<[u8; 32], ProverError>`. In a release build with no
/// `libstwo_sys.so` AND no `allow-stub-verifier` feature, the
/// forgeable BLAKE3 stub path MUST refuse to compute a commitment.
/// Companion invariant to `release_gate_fails_closed`: the verifier
/// gates `verify_capability_zk`; the proofer gates `stub_commitment`.
/// Both must fail closed under release configuration.
#[cfg(not(feature = "allow-stub-verifier"))]
#[test]
fn stub_commitment_returns_err_in_release_build() {
    let (_proof, public) = sample("casm-release-stub-commit");
    let result = zk_verifier::stub_commitment("casm-release-stub-commit", &public);
    match result {
        Err(ProverError::StubVerifierDisabled { casm_hash, context }) => {
            assert_eq!(casm_hash, "casm-release-stub-commit");
            assert!(
                context.contains("allow-stub-verifier"),
                "context must point operators to the opt-in feature; got {context:?}"
            );
        }
        other => panic!(
            "release build must return Err(ProverError::StubVerifierDisabled); got {other:?}"
        ),
    }
}

/// R3 coverage check (default-features + dev tools present): the
/// verifier MUST NOT take the forgeable BLAKE3 stub path with
/// arbitrary bytes. Either the FFI path fires (returning Err on
/// bogus bytes) or the stub path fires (returning `ProofRejected`).
/// A `ProofRejected` IS the canonical stub-path result for bogus
/// bytes; the invariant is that we never return `Ok(())` for an
/// attacker-supplied commitment.
#[test]
#[allow(unused_imports)] // VerifyError only used in --no-default-features variant
fn default_build_does_not_accept_arbtrary_stub_commit() {
    let (proof, public) = sample("casm-default-gate");
    let result = zk_verifier::verify_capability_zk(&proof, &public, "casm-default-gate");
    assert!(
        result.is_err(),
        "arbitrary proof bytes must NOT be accepted (no forge); got Ok"
    );
}
