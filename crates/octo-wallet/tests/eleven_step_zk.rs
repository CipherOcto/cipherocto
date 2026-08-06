//! 11-step ZK mint flow (Gap 3 / RFC-0958 + RFC-0962 §9 / Task 3.4).
//!
//! Builds a 11-signer batch signature envelope via `mint_with_zk_and_signers`,
//! then verifies the resulting `ProofBundle` with `verify_capability_zk`
//! (single-capability verify is authoritative; the batch path delegates
//! to it after asserting the signer list is non-empty). Both the
//! `crate::capability::zk_mint` mint API and the
//! `quota_router_core::zk_verify::capability` verify API are exercised
//! end-to-end.
//!
//! The mock prover path (default; `full` feature off) emits a
//! deterministic BLAKE3 commitment; the canonical
//! `zk_verifier::stub_commitment` shape is what the verifier's STWO
//! check accepts. Both proofer and verifier agree on the same
//! commitment layout for the same inputs, so the full mint -> verify
//! round-trip is exercised.

use octo_ident::test_helpers::sample_did;
use octo_wallet::capability::zk_mint::{
    bundled_casm_hash, mint_with_zk_and_signers, PrivateWitness, PublicInputs,
};
use octo_wallet::node::NodeType;
use quota_router_core::zk_verify::capability::{
    reconstruct_batch_sig_inputs, verify_batch_capability_zk, CapabilityVerifier,
};
use quota_router_core::zk_verify::ProofBundle as CoreProofBundle;
use zk_circuit::verify_mock_batch_proof;

use ed25519_dalek::Signature;

/// Build a 11-signer batch envelope and verify it.
#[test]
fn eleven_step_batch_zk_round_trip() {
    // 11 signers (matches the 11-step exercise).
    let signers: Vec<[u8; 32]> = (0..11)
        .map(|i| [u8::try_from(i).expect("11 signers fit in u8"); 32])
        .collect();

    // Sample witness (RFC-0958 §Data Structures).
    // **v1.2 M5:** SelfHost requires `inference_trace: Some(_)` (carries the PoI).
    let witness = PrivateWitness {
        cap_root_secret: [0x42; 32],
        holder_sig: Signature::from_bytes(&[0xab; 64]),
        caveats_full: vec![],
        discharges_full: vec![],
        inference_trace: Some(octo_wallet::capability::zk_mint::ExecutionTrace {
            step_count: 1,
            step_records: vec![octo_wallet::capability::zk_mint::TraceStep {
                op_code: 0,
                input_hash: [0x33; 32],
                output_hash: [0x44; 32],
            }],
        }),
    };

    // Capability public inputs (RFC-0958 §Data Structures).
    let public_inputs = PublicInputs {
        ask_id: [0x11; 32],
        axes_consumed: vec![("input_tokens_per_1k".to_owned(), 1000)],
        cap_root_hash: [0x22; 32],
        invocation_hash: [0x33; 32],
        holder_did: sample_did(86).clone(),
        current_unix_time: 1_700_000_000,
        output_hash: Some([0x44; 32]), // SelfHost mode
        provider_slot_id: "slot-eleven-step-001".to_owned(),
    };

    let casm = bundled_casm_hash();
    let bundle =
        mint_with_zk_and_signers(NodeType::SelfHost, &witness, &public_inputs, casm, &signers)
            .expect("11-signer batch mint succeeds");
    assert_eq!(
        bundle.stark_proof.len(),
        32,
        "mock prover emits 32-byte commitment"
    );
    assert_eq!(bundle.casm_hash, casm);
    assert_eq!(bundle.security_bits, 128);

    // Mock-prover round-trip: re-derive the `zk_verifier::PublicInputs`
    // the prover saw, then confirm the proof bytes match.
    let zk_public = zk_verifier::PublicInputs {
        proof_issued_at_unix: public_inputs.current_unix_time,
        verifier_local_unix_time: public_inputs.current_unix_time,
        compiled_casm_hash: hex::encode(casm),
        capability_root_hash: hex::encode(public_inputs.cap_root_hash),
        provider_slot_id: public_inputs.provider_slot_id.clone(),
    };
    let proof_obj = zk_circuit::Proof {
        bytes: bundle.stark_proof.clone(),
        casm_hash: casm,
    };
    // Convert octo-wallet PublicInputs → quota-router-core PublicInputs
    // (the verifier layer reconstructs batch_sig_inputs from the q-r-core
    // shape; field-by-field mirror).
    let q_pi = quota_router_core::zk_verify::PublicInputs {
        ask_id: public_inputs.ask_id,
        axes_consumed: public_inputs.axes_consumed.clone(),
        cap_root_hash: public_inputs.cap_root_hash,
        invocation_hash: public_inputs.invocation_hash,
        holder_did: public_inputs.holder_did.clone(),
        current_unix_time: public_inputs.current_unix_time,
        output_hash: public_inputs.output_hash,
        provider_slot_id: public_inputs.provider_slot_id.clone(),
    };
    let batch_inputs = reconstruct_batch_sig_inputs(&q_pi, &signers);
    assert!(
        verify_mock_batch_proof(&proof_obj, &batch_inputs, &zk_public),
        "mock proofer commitment must match re-derived zk_verifier::PublicInputs"
    );

    // Full verifier round-trip (canonical single-capability check via
    // the batch wrapper). The verifier constructs the same canonical
    // commitment shape as the proofer for the mock path.
    let verifier = CapabilityVerifier {
        compiled_casm_blake3_hash: casm,
        verifier_local_unix_time: public_inputs.current_unix_time,
    };
    let core_bundle = CoreProofBundle {
        stark_proof: bundle.stark_proof.clone(),
        public_inputs: quota_router_core::zk_verify::PublicInputs {
            ask_id: bundle.public_inputs.ask_id,
            axes_consumed: bundle.public_inputs.axes_consumed.clone(),
            cap_root_hash: bundle.public_inputs.cap_root_hash,
            invocation_hash: bundle.public_inputs.invocation_hash,
            holder_did: bundle.public_inputs.holder_did.clone(),
            current_unix_time: bundle.public_inputs.current_unix_time,
            output_hash: bundle.public_inputs.output_hash,
            provider_slot_id: bundle.public_inputs.provider_slot_id.clone(),
        },
        casm_hash: bundle.casm_hash,
        casm_version: bundle.casm_version,
        security_bits: bundle.security_bits,
    };
    verify_batch_capability_zk(
        &core_bundle,
        &signers,
        Some(&core_bundle.public_inputs),
        &verifier,
    )
    .expect("11-signer batch proof verifies end-to-end");
}

/// Round-trip with Wholesale node type fails closed (fail-closed per
/// RFC-0958 §`NodeType` Gating Rule).
#[test]
fn eleven_step_wholesale_mint_fails_closed() {
    let signers: Vec<[u8; 32]> = (0..11)
        .map(|i| [u8::try_from(i).expect("11 signers fit in u8"); 32])
        .collect();
    let witness = PrivateWitness {
        cap_root_secret: [0x42; 32],
        holder_sig: Signature::from_bytes(&[0xab; 64]),
        caveats_full: vec![],
        discharges_full: vec![],
        // Wholesale → inference_trace is None (no PoI for Wholesale)
        inference_trace: None,
    };
    let public_inputs = PublicInputs {
        ask_id: [0x11; 32],
        axes_consumed: vec![],
        cap_root_hash: [0x22; 32],
        invocation_hash: [0x33; 32],
        holder_did: sample_did(86).clone(),
        current_unix_time: 1_700_000_000,
        output_hash: None,
        provider_slot_id: "slot-wholesale-001".to_owned(),
    };
    let err = mint_with_zk_and_signers(
        NodeType::Wholesale,
        &witness,
        &public_inputs,
        bundled_casm_hash(),
        &signers,
    )
    .unwrap_err();
    assert!(matches!(
        err,
        octo_wallet::capability::zk_mint::ZkMintError::NodeTypeCannotMintZKCap
    ));
}

/// Determinism: same inputs → same proof bytes (mock path).
#[test]
fn eleven_step_batch_proof_is_deterministic() {
    let signers: Vec<[u8; 32]> = (0..11)
        .map(|i| [u8::try_from(i).expect("11 signers fit in u8"); 32])
        .collect();
    let witness = PrivateWitness {
        cap_root_secret: [0x42; 32],
        holder_sig: Signature::from_bytes(&[0xab; 64]),
        caveats_full: vec![],
        discharges_full: vec![],
        // SelfHost → inference_trace required (v1.2 M5 rename, AC-6)
        inference_trace: Some(octo_wallet::capability::zk_mint::ExecutionTrace {
            step_count: 1,
            step_records: vec![octo_wallet::capability::zk_mint::TraceStep {
                op_code: 0,
                input_hash: [0x33; 32],
                output_hash: [0x44; 32],
            }],
        }),
    };
    let public_inputs = PublicInputs {
        ask_id: [0x11; 32],
        axes_consumed: vec![],
        cap_root_hash: [0x22; 32],
        invocation_hash: [0x33; 32],
        holder_did: sample_did(86).clone(),
        current_unix_time: 1_700_000_000,
        output_hash: Some([0x44; 32]),
        provider_slot_id: "slot-det-001".to_owned(),
    };
    let casm = bundled_casm_hash();
    let a = mint_with_zk_and_signers(NodeType::SelfHost, &witness, &public_inputs, casm, &signers)
        .unwrap();
    let b = mint_with_zk_and_signers(NodeType::SelfHost, &witness, &public_inputs, casm, &signers)
        .unwrap();
    assert_eq!(
        a.stark_proof, b.stark_proof,
        "mock proofer is deterministic"
    );
}
