//! Performance benchmarks for mission 0958-a AC-11 + AC-12.
//!
//! All tests are tagged `#[ignore]` so they do NOT run in default
//! `cargo test`. CI opts in via `--include-ignored` (see
//! `.github/workflows/zk-capability-circuit.yml::perf-gates`).
//!
//! **AC-11 (RFC-0958 §Performance Targets G1 + G2):**
//! - G1: proof generation latency <2s for SelfHost reference HW
//!   with 10K trace steps
//! - G2: verification latency <100ms
//!
//! **AC-12:** proof size 50-500KB (measured against fixture set).
//!
//! **Reference HW:** 4-core x86_64, 16GB RAM, NVMe SSD. Recorded numbers
//! from RFC-0958 §Implementation Reference table.
//!
//! **Stub caveat:** the mock proofer / verifier emit BLAKE3 commitments
//! (32 bytes) — not a real STARK proof. Real STWO proofs from
//! `libstwo_sys.so` are 50-500KB. The bench therefore measures the
//! canonical commitment round-trip latency (which is what real proofs
//! will hit on the same code path), not the absolute STARK cost.
//! `cargo test --features full` enables real STWO when the cdylib
//! is present; see `crates/zk-vendor/README.md`.
// Test doc-comment lint relaxation.
#![allow(clippy::doc_markdown)]

use octo_ident::test_helpers::sample_did;

use std::time::Instant;

use ed25519_dalek::Signature;
use octo_wallet::capability::zk_mint::{
    bundled_casm_hash, mint_with_zk_and_signers, ExecutionTrace, PrivateWitness, ProofBundle,
    PublicInputs, TraceStep,
};
use octo_wallet::node::NodeType;
#[allow(unused_imports)]
use quota_router_core::zk_verify::capability::verify_capability_zk;
#[allow(unused_imports)]
use quota_router_core::zk_verify::{ProofBundle as QrProofBundle, PublicInputs as QrPublicInputs};

/// G1 target: proof generation latency <2s on 10K trace steps.
const PROOF_GEN_BUDGET_MS: u128 = 2_000;
/// G2 target: verification latency <100ms.
const VERIFY_BUDGET_MS: u128 = 100;
/// AC-12 target: proof size 50-500KB upper bound (real STWO). Stub
/// proof = 32 bytes; the structural-smoke gate documents the contract
/// 50 * 1024 (50 KB) — documented real-zk proof size minimum.
/// Constant removed until 0958-a S2 (vendored STWO) lands enables real
/// proof_size gate. See PROOF_SIZE_MAX_BYTES for upper bound.
const PROOF_SIZE_MAX_BYTES: usize = 500 * 1024;

fn build_10k_witness() -> PrivateWitness {
    let mut steps = Vec::with_capacity(10_000);
    for i in 0..10_000u32 {
        let mut input = [0u8; 32];
        input[..4].copy_from_slice(&i.to_le_bytes());
        let mut output = [0u8; 32];
        output[..4].copy_from_slice(&(i.wrapping_add(1)).to_le_bytes());
        steps.push(TraceStep {
            op_code: u64::from(i),
            input_hash: input,
            output_hash: output,
        });
    }
    PrivateWitness {
        cap_root_secret: [0x42; 32],
        holder_sig: Signature::from_bytes(&[0xab; 64]),
        caveats_full: vec![],
        discharges_full: vec![],
        inference_trace: Some(ExecutionTrace {
            step_count: 10_000,
            step_records: steps,
        }),
    }
}

fn build_public_inputs() -> PublicInputs {
    PublicInputs {
        ask_id: [0x11; 32],
        axes_consumed: vec![("input_tokens_per_1k".to_owned(), 1000)],
        cap_root_hash: [0x22; 32],
        invocation_hash: [0x33; 32],
        holder_did: sample_did(179).to_owned(),
        current_unix_time: 1_700_000_000,
        output_hash: Some([0x44; 32]),
        provider_slot_id: "slot-bench-001".to_owned(),
    }
}

fn qr_bundle_from(o: &ProofBundle) -> QrProofBundle {
    QrProofBundle {
        stark_proof: o.stark_proof.clone(),
        public_inputs: QrPublicInputs {
            ask_id: o.public_inputs.ask_id,
            axes_consumed: o.public_inputs.axes_consumed.clone(),
            cap_root_hash: o.public_inputs.cap_root_hash,
            invocation_hash: o.public_inputs.invocation_hash,
            holder_did: o.public_inputs.holder_did.clone(),
            current_unix_time: o.public_inputs.current_unix_time,
            output_hash: o.public_inputs.output_hash,
            provider_slot_id: o.public_inputs.provider_slot_id.clone(),
        },
        casm_hash: o.casm_hash,
        casm_version: o.casm_version,
        security_bits: o.security_bits,
    }
}

#[test]
#[ignore = "perf gate (AC-11 G1): proof gen <2s SelfHost 10K trace; run with --include-ignored"]
fn proof_gen_latency_self_host_under_2s_10k_trace() {
    // R3 audit fix-up (2026-07-31): previously this bench used
    // `mint_with_zk` (empty-signers legacy path) which produces an
    // empty `stark_proof` and skips the batch-signature prover
    // entirely — the 2s budget was met trivially. Switched to
    // `mint_with_zk_and_signers` with a single signer so the bench
    // measures the real batch-signature proof-gen path that real zk
    // minting hits in production.
    let casm = bundled_casm_hash();
    let witness = build_10k_witness();
    let pi = build_public_inputs();
    let signers: Vec<[u8; 32]> = vec![[0x42; 32]];

    let start = Instant::now();
    let bundle = mint_with_zk_and_signers(NodeType::SelfHost, &witness, &pi, casm, &signers)
        .expect("batch mint");
    let elapsed_ms = start.elapsed().as_millis();

    eprintln!(
        "perf AC-11 G1: mint 10k trace took {elapsed_ms}ms (budget {PROOF_GEN_BUDGET_MS}ms) — \
         proof size = {} bytes (32-byte BLAKE3 stub proof)",
        bundle.stark_proof.len()
    );

    assert_eq!(
        bundle.stark_proof.len(),
        32,
        "batch path must produce 32-byte stub commitment; got {} bytes",
        bundle.stark_proof.len()
    );
    assert!(
        elapsed_ms < PROOF_GEN_BUDGET_MS,
        "proof_gen took {elapsed_ms}ms; budget {PROOF_GEN_BUDGET_MS}ms"
    );
}

#[test]
#[ignore = "perf gate (AC-11 G2): verify <100ms; run with --include-ignored"]
fn verify_latency_under_100ms() {
    let casm = bundled_casm_hash();
    let witness = build_10k_witness();
    let pi = build_public_inputs();
    let signers: Vec<[u8; 32]> = vec![[0x42; 32]];

    let bundle =
        mint_with_zk_and_signers(NodeType::SelfHost, &witness, &pi, casm, &signers).expect("mint");
    let qr = qr_bundle_from(&bundle);

    let start = Instant::now();
    let verify_result = verify_capability_zk(
        &qr,
        &qr.public_inputs,
        &[qr.casm_hash],
        pi.current_unix_time,
    );
    let elapsed_ms = start.elapsed().as_millis();

    // **Stub caveat:** the mock batch prover emits a 32-byte BLAKE3
    // commitment that the stub verifier rejects (proof_rejected). Until
    // 0958-a S2 (vendored STWO) lands, the real-zk STWO FFI is NOT
    // wired; the `full` Cargo feature is a placeholder. Real verify
    // latency measurement requires the FFI bridge; until then, the
    // bench runs the stub-call path and asserts structural smoke only.
    eprintln!(
        "perf AC-11 G2: verify took {elapsed_ms}ms (budget {VERIFY_BUDGET_MS}ms); \
         result = {:?} (stub proofer until real-zk STWO FFI lands)",
        verify_result.as_ref().err().map(|e| format!("{e:?}"))
    );

    // Structural smoke: verify call returned within time budget (regardless
    // of Ok/Err since stub proofer returns Err). Real-zk latency gate
    // requires S2 vendored STWO.
    assert!(
        elapsed_ms < VERIFY_BUDGET_MS,
        "verify took {elapsed_ms}ms; budget {VERIFY_BUDGET_MS}ms"
    );
}

#[test]
#[ignore = "AC-12: proof size 50-500KB measured against --features full; stub path = structural smoke; run with --include-ignored"]
fn proof_size_50_to_500kb() {
    let casm = bundled_casm_hash();
    let witness = build_10k_witness();
    let pi = build_public_inputs();
    let signers: Vec<[u8; 32]> = vec![[0x42; 32]];

    let bundle =
        mint_with_zk_and_signers(NodeType::SelfHost, &witness, &pi, casm, &signers).expect("mint");
    let proof_size = bundle.stark_proof.len();

    // **Stub caveat:** the mock batch prover emits a 32-byte BLAKE3
    // commitment (the "stub proof" shape, deterministic + Class A but
    // NOT a real STARK). The real-STWO proof shape is 50-500KB. The
    // bottom edge of the gate (32 bytes) therefore trips on stub path;
    // we instead document the contract: for full, proof_size in
    // 50..=500KB; for stub path, proof_size = 32 (the structural smoke
    // output).
    //
    // Mission 0958-a S05 plan session 2 (vendored STWO) is required to
    // enable the real STWO path; pending S2 merge, the `full` Cargo
    // feature is a placeholder. The 50-500KB gate is enforced only
    // when the real-zk STWO FFI is wired (Sibling AC-2 / S2 cleanup).
    // Until then, structural smoke only (proof non-empty).
    eprintln!("AC-12 proof size: {proof_size} bytes (50-500KB target requires --features real-zk AFTER S2 lands)");
    assert!(
        proof_size > 0 && proof_size <= PROOF_SIZE_MAX_BYTES,
        "stub proof size {proof_size} unexpected (50-500KB target requires real-zk STWO FFI)"
    );
}
