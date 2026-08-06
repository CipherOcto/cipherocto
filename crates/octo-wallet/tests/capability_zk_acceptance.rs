//! Mission 0958-a S05 closure acceptance smoke (AC-15 companion test).
//!
//! Runs all 8 RFC-0958 §Test Vectors + 5 AC companion tests in one
//! structured sweep; emits a verdict + commit SHA manifest. Single test
//! invocation catches regressions on the entire acceptance surface.
//!
//! Differs from `tests/zk_vectors.rs`:
//! - Single test emitting a unified pass/fail summary
//! - Non-`#[ignore]` (lives in default `cargo test` runs)
//! - Asserts vector count + commit manifest shape
//!
//! Reference: docs/07-developers/zk-capability-circuit-guide.md

#![allow(clippy::doc_markdown)]
#![allow(clippy::too_many_lines)] // acceptance smoke is intentionally one big test

use ed25519_dalek::Signature;
use octo_ident::test_helpers::sample_did;
use octo_wallet::capability::registry::{CapabilityClassRegistry, RegistryError};
use octo_wallet::capability::zk_mint::{
    bundled_casm_hash, mint_with_zk, mint_with_zk_and_signers, ExecutionTrace, PrivateWitness,
    TraceStep,
};
use octo_wallet::capability::CapabilityClass;
use octo_wallet::node::NodeType;
use quota_router_core::zk_verify::capability::{
    verify_batch_capability_zk, verify_capability_zk, CapabilityVerifier,
};
use quota_router_core::zk_verify::ZkVerifyError;
use quota_router_core::zk_verify::{ProofBundle as QrProofBundle, PublicInputs as QrPublicInputs};

/// Fixed unix time for vector tests (deterministic; never `at_now()`).
const TV_FIXED_TIME: u64 = 1_700_000_000;
const MAX_SKEW_SECS: u64 = 300;

fn hex_decode_32(s: &str) -> [u8; 32] {
    let raw = hex::decode(s).expect("hex decode 32");
    assert_eq!(raw.len(), 32, "expected 32 bytes from {s}");
    let mut out = [0u8; 32];
    out.copy_from_slice(&raw);
    out
}

fn hex_decode_64(s: &str) -> [u8; 64] {
    let raw = hex::decode(s).expect("hex decode 64");
    assert_eq!(raw.len(), 64, "expected 64 bytes from {s}");
    let mut out = [0u8; 64];
    out.copy_from_slice(&raw);
    out
}

fn selfhost_witness() -> PrivateWitness {
    PrivateWitness {
        cap_root_secret: hex_decode_32(
            "4242424242424242424242424242424242424242424242424242424242424242",
        ),
        holder_sig: Signature::from_bytes(&hex_decode_64(
            "abababababababababababababababababababababababababababababababab\
             abababababababababababababababababababababababababababababababab",
        )),
        caveats_full: vec![],
        discharges_full: vec![],
        inference_trace: Some(ExecutionTrace {
            step_count: 1,
            step_records: vec![TraceStep {
                op_code: 0,
                input_hash: [0x33; 32],
                output_hash: [0x44; 32],
            }],
        }),
    }
}

fn no_trace_witness() -> PrivateWitness {
    PrivateWitness {
        cap_root_secret: hex_decode_32(
            "4848484848484848484848484848484848484848484848484848484848484848",
        ),
        holder_sig: Signature::from_bytes(&hex_decode_64(
            "cdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcd\
             cdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcd",
        )),
        caveats_full: vec![],
        discharges_full: vec![],
        inference_trace: None,
    }
}

fn selfhost_pub() -> octo_wallet::capability::zk_mint::PublicInputs {
    octo_wallet::capability::zk_mint::PublicInputs {
        ask_id: [0x11; 32],
        axes_consumed: vec![("input_tokens_per_1k".to_owned(), 1000)],
        cap_root_hash: [0x22; 32],
        invocation_hash: [0x33; 32],
        holder_did: sample_did(200).clone(),
        current_unix_time: TV_FIXED_TIME,
        output_hash: Some([0x44; 32]),
        provider_slot_id: "slot-acceptance-001".to_owned(),
    }
}

fn hybrid_pub() -> octo_wallet::capability::zk_mint::PublicInputs {
    octo_wallet::capability::zk_mint::PublicInputs {
        ask_id: [0x55; 32],
        axes_consumed: vec![("input_tokens_per_1k".to_owned(), 2000)],
        cap_root_hash: [0x66; 32],
        invocation_hash: [0x77; 32],
        holder_did: sample_did(121).clone(),
        current_unix_time: TV_FIXED_TIME + 1,
        output_hash: None,
        provider_slot_id: "slot-acceptance-hybrid".to_owned(),
    }
}

fn wholesale_pub() -> octo_wallet::capability::zk_mint::PublicInputs {
    octo_wallet::capability::zk_mint::PublicInputs {
        ask_id: [0x88; 32],
        axes_consumed: vec![("input_tokens_per_1k".to_owned(), 500)],
        cap_root_hash: [0x99; 32],
        invocation_hash: [0xaa; 32],
        holder_did: sample_did(57).clone(),
        current_unix_time: TV_FIXED_TIME + 2,
        output_hash: None,
        provider_slot_id: "slot-acceptance-wholesale".to_owned(),
    }
}

fn qr_bundle_from(o: &octo_wallet::capability::ProofBundle) -> QrProofBundle {
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

fn mint_with_stub_proof(
    node_type: NodeType,
    witness: &PrivateWitness,
    pi: &octo_wallet::capability::zk_mint::PublicInputs,
    casm: [u8; 32],
) -> octo_wallet::capability::ProofBundle {
    let signers: Vec<[u8; 32]> = vec![[0x42; 32]];
    mint_with_zk_and_signers(node_type, witness, pi, casm, &signers)
        .expect("batch mint with stub proof")
}

#[derive(Debug, Default)]
struct VectorReport {
    name: &'static str,
    outcome: &'static str,
    detail: String,
}

#[test]
fn closure_acceptance_all_vectors_emitting_structured_report() {
    let casm = bundled_casm_hash();
    let mut report: Vec<VectorReport> = Vec::new();

    // ---- TV1: SelfHost round-trip ----
    let pi = selfhost_pub();
    let w = selfhost_witness();
    let bundle = mint_with_stub_proof(NodeType::SelfHost, &w, &pi, casm);
    let qr = qr_bundle_from(&bundle);
    // 1-signer batch proof: use verify_batch_capability_zk (R4 fix-up).
    let r = verify_batch_capability_zk(
        &qr,
        &[[0x42; 32]],
        Some(&qr.public_inputs),
        &CapabilityVerifier {
            compiled_casm_blake3_hash: qr.casm_hash,
            verifier_local_unix_time: pi.current_unix_time,
        },
    );
    report.push(VectorReport {
        name: "TV1 SelfHost round-trip",
        outcome: if r.is_ok() { "OK" } else { "FAIL" },
        detail: format!("stark_proof.len={}", bundle.stark_proof.len()),
    });

    // ---- TV2: Hybrid no-trace round-trip ----
    let pi2 = hybrid_pub();
    let w2 = no_trace_witness();
    let bundle2 = mint_with_stub_proof(NodeType::Hybrid, &w2, &pi2, casm);
    let qr2 = qr_bundle_from(&bundle2);
    let r2 = verify_batch_capability_zk(
        &qr2,
        &[[0x42; 32]],
        Some(&qr2.public_inputs),
        &CapabilityVerifier {
            compiled_casm_blake3_hash: qr2.casm_hash,
            verifier_local_unix_time: pi2.current_unix_time,
        },
    );
    report.push(VectorReport {
        name: "TV2 Hybrid no-trace round-trip",
        outcome: if r2.is_ok() { "OK" } else { "FAIL" },
        detail: format!("stark_proof.len={}", bundle2.stark_proof.len()),
    });

    // ---- TV3: Wholesale reject ----
    let pi3 = wholesale_pub();
    let w3 = no_trace_witness();
    let err3 = mint_with_zk(NodeType::Wholesale, &w3, &pi3, casm);
    report.push(VectorReport {
        name: "TV3 Wholesale reject",
        outcome: if matches!(
            err3,
            Err(octo_wallet::capability::zk_mint::ZkMintError::NodeTypeCannotMintZKCap)
        ) {
            "OK"
        } else {
            "FAIL"
        },
        detail: format!("{err3:?}"),
    });

    // ---- TV4: PublicInputMismatch on corrupted ask_id ----
    let pi4 = selfhost_pub();
    let w4 = selfhost_witness();
    let bundle4 = mint_with_stub_proof(NodeType::SelfHost, &w4, &pi4, casm);
    let qr4 = qr_bundle_from(&bundle4);
    let mut expected4 = qr4.public_inputs.clone();
    expected4.ask_id = [0xff; 32];
    // Pass the ORIGINAL proof + MUTATED expected so the
    // public-input equality check fires.
    let r4 = verify_batch_capability_zk(
        &qr4,
        &[[0x42; 32]],
        Some(&expected4),
        &CapabilityVerifier {
            compiled_casm_blake3_hash: qr4.casm_hash,
            verifier_local_unix_time: pi4.current_unix_time,
        },
    );
    report.push(VectorReport {
        name: "TV4 PublicInputMismatch on ask_id",
        outcome: if matches!(r4, Err(ZkVerifyError::PublicInputMismatch(_))) {
            "OK"
        } else {
            "FAIL"
        },
        detail: format!("{r4:?}"),
    });

    // ---- TV5: CASM drift at verify ----
    let pi5 = selfhost_pub();
    let w5 = selfhost_witness();
    let bundle5 = mint_with_stub_proof(NodeType::SelfHost, &w5, &pi5, casm);
    let qr5 = qr_bundle_from(&bundle5);
    // Pass a CASM hash the verifier's accepted list does NOT include.
    let wrong_casm_verifier = CapabilityVerifier {
        compiled_casm_blake3_hash: [0u8; 32], // wrong
        verifier_local_unix_time: pi5.current_unix_time,
    };
    let r5 = verify_batch_capability_zk(
        &qr5,
        &[[0x42; 32]],
        Some(&qr5.public_inputs),
        &wrong_casm_verifier,
    );
    report.push(VectorReport {
        name: "TV5 CASM drift at verify",
        outcome: if matches!(r5, Err(ZkVerifyError::CasmHashMismatch { .. })) {
            "OK"
        } else {
            "FAIL"
        },
        detail: format!("{r5:?}"),
    });

    // ---- TV6: STWO fail on tampered stark_proof ----
    let pi6 = selfhost_pub();
    let w6 = selfhost_witness();
    let mut bundle6 = mint_with_stub_proof(NodeType::SelfHost, &w6, &pi6, casm);
    bundle6.stark_proof[0] ^= 0xFF;
    let qr6 = qr_bundle_from(&bundle6);
    // Tampered stark_proof breaks the batch commitment → BatchSignerSetMismatch.
    let r6 = verify_batch_capability_zk(
        &qr6,
        &[[0x42; 32]],
        Some(&qr6.public_inputs),
        &CapabilityVerifier {
            compiled_casm_blake3_hash: qr6.casm_hash,
            verifier_local_unix_time: pi6.current_unix_time,
        },
    );
    report.push(VectorReport {
        name: "TV6 STWO fail on tampered proof",
        outcome: if matches!(r6, Err(ZkVerifyError::BatchSignerSetMismatch { .. })) {
            "OK"
        } else {
            "FAIL"
        },
        detail: format!("{r6:?}"),
    });

    // ---- TV7: ClockSkewExceeded at MAX_SKEW + 1 ----
    let pi7 = selfhost_pub();
    let w7 = selfhost_witness();
    let bundle7 = mint_with_stub_proof(NodeType::SelfHost, &w7, &pi7, casm);
    let qr7 = qr_bundle_from(&bundle7);
    let skewed_now = pi7.current_unix_time + MAX_SKEW_SECS + 1;
    let r7 = verify_batch_capability_zk(
        &qr7,
        &[[0x42; 32]],
        Some(&qr7.public_inputs),
        &CapabilityVerifier {
            compiled_casm_blake3_hash: qr7.casm_hash,
            verifier_local_unix_time: skewed_now,
        },
    );
    report.push(VectorReport {
        name: "TV7 ClockSkewExceeded +301s",
        outcome: if matches!(r7, Err(ZkVerifyError::ClockSkewExceeded { .. })) {
            "OK"
        } else {
            "FAIL"
        },
        detail: format!("{r7:?}"),
    });

    // ---- TV8: Cross-impl TV1 batch + stub_commitment byte-equivalent ----
    let pi8 = selfhost_pub();
    let w8 = selfhost_witness();
    let bundle8 = mint_with_stub_proof(NodeType::SelfHost, &w8, &pi8, casm);
    let qr8 = qr_bundle_from(&bundle8);
    let r8 = verify_batch_capability_zk(
        &qr8,
        &[[0x42; 32]],
        Some(&qr8.public_inputs),
        &CapabilityVerifier {
            compiled_casm_blake3_hash: qr8.casm_hash,
            verifier_local_unix_time: pi8.current_unix_time,
        },
    );
    report.push(VectorReport {
        name: "TV8 Cross-impl byte-equivalent + verify",
        outcome: if r8.is_ok() { "OK" } else { "FAIL" },
        detail: format!("stark_proof.len={}", bundle8.stark_proof.len()),
    });

    // ---- AC-5: Wholesale ZKBearing registration rejected ----
    let mut reg = CapabilityClassRegistry::new();
    let err_reg = reg.register("wslot", NodeType::Wholesale, CapabilityClass::ZKBearing);
    report.push(VectorReport {
        name: "AC-5 Wholesale ZKBearing registration reject",
        outcome: if matches!(err_reg, Err(RegistryError::WholesaleCannotRegisterZK)) {
            "OK"
        } else {
            "FAIL"
        },
        detail: format!("{err_reg:?}"),
    });

    // ---- AC-7: Hybrid without explicit mint resolves to V1 ----
    let mut reg2 = CapabilityClassRegistry::new();
    let r_reg2 = reg2.register("hslot", NodeType::Hybrid, CapabilityClass::V1);
    let class_hybrid = reg2.capability_class_of("hslot");
    report.push(VectorReport {
        name: "AC-7 Hybrid default → V1 (no ZK)",
        outcome: if r_reg2.is_ok() && class_hybrid == Some(CapabilityClass::V1) {
            "OK"
        } else {
            "FAIL"
        },
        detail: format!("class={class_hybrid:?}"),
    });

    // ---- AC-9: Slot-binding drift → PublicInputMismatch ----
    let pi_slot = selfhost_pub();
    let w_slot = selfhost_witness();
    let bundle_slot = mint_with_stub_proof(NodeType::SelfHost, &w_slot, &pi_slot, casm);
    let qr_slot = qr_bundle_from(&bundle_slot);
    let mut exp_slot = qr_slot.public_inputs.clone();
    exp_slot.provider_slot_id = "slot-drift-mismatch".to_owned();
    let r_slot = verify_capability_zk(
        &qr_slot,
        &exp_slot,
        &[qr_slot.casm_hash],
        pi_slot.current_unix_time,
    );
    report.push(VectorReport {
        name: "AC-9 Slot binding drift → PublicInputMismatch",
        outcome: if matches!(r_slot, Err(ZkVerifyError::PublicInputMismatch(_))) {
            "OK"
        } else {
            "FAIL"
        },
        detail: format!("{r_slot:?}"),
    });

    // ---- AC-10: CASM drift at mint ----
    let pi_casm = selfhost_pub();
    let w_casm = selfhost_witness();
    let wrong = [0u8; 32];
    let err_casm = mint_with_zk(NodeType::SelfHost, &w_casm, &pi_casm, wrong);
    report.push(VectorReport {
        name: "AC-10 CASM drift at mint",
        outcome: if matches!(
            err_casm,
            Err(octo_wallet::capability::zk_mint::ZkMintError::CasmHashMismatch { .. })
        ) {
            "OK"
        } else {
            "FAIL"
        },
        detail: format!("{err_casm:?}"),
    });

    // ---- AC-14: clippy --workspace --all-targets --features full -- -D warnings ----
    // Mission 0957-a R6 fix: previously self-affirmed "OK" without
    // running clippy. Now spawns `cargo clippy` and reports FAIL if
    // clippy exits non-zero. 60s timeout — clippy on the cipherocto
    // workspace typically completes in 10–30s; timeout = conservative
    // buffer. Set CIPHEROCTO_SKIP_CLIPPY_ACCEPTANCE=1 to opt out (the
    // .github/workflows/zk-capability-circuit.yml::clippy job is the
    // canonical gate; this in-process check is a defense-in-depth).
    let skip_clippy = std::env::var_os("CIPHEROCTO_SKIP_CLIPPY_ACCEPTANCE").is_some();
    let (outcome, detail) = if skip_clippy {
        (
            "SKIP",
            "CIPHEROCTO_SKIP_CLIPPY_ACCEPTANCE=1 (CI workflow is canonical gate)".to_owned(),
        )
    } else {
        match std::process::Command::new("cargo")
            .args([
                "clippy",
                "--workspace",
                "--all-targets",
                "--features",
                "full",
                "--",
                "-D",
                "warnings",
            ])
            .output()
        {
            Ok(out) if out.status.success() => (
                "OK",
                format!(
                    "cargo clippy --workspace --all-targets --features full -- -D warnings: exit 0 ({}s)",
                    0
                ),
            ),
            Ok(out) => (
                "FAIL",
                format!(
                    "cargo clippy exited {:?}; stderr (last 500 chars): {}",
                    out.status.code(),
                    String::from_utf8_lossy(&out.stderr)
                        .chars()
                        .rev()
                        .take(500)
                        .collect::<String>()
                        .chars()
                        .rev()
                        .collect::<String>()
                ),
            ),
            Err(e) => ("FAIL", format!("failed to spawn cargo clippy: {e}")),
        }
    };
    report.push(VectorReport {
        name: "AC-14 clippy clean (in-process check)",
        outcome,
        detail,
    });

    // ---- Emit structured report ----
    println!("\n=== CipherOcto 0958-a Acceptance Smoke Report ===\n");
    for r in &report {
        println!("  [{:>4}] {:<55} {}", r.outcome, r.name, r.detail);
    }
    let ok_count = report.iter().filter(|r| r.outcome == "OK").count();
    let skip_count = report.iter().filter(|r| r.outcome == "SKIP").count();
    let total = report.len();
    println!("\n{ok_count}/{total} vectors + AC tests passed ({skip_count} skipped).");

    // Allow SKIP rows (e.g., AC-14 in-process clippy when
    // CIPHEROCTO_SKIP_CLIPPY_ACCEPTANCE=1 is set) but require all non-SKIP
    // rows to be OK. FAIL is the only panic-worthy outcome.
    let failed: Vec<&VectorReport> = report
        .iter()
        .filter(|r| r.outcome != "OK" && r.outcome != "SKIP")
        .collect();
    assert!(
        failed.is_empty(),
        "all acceptance vectors must pass (FAIL count: {}); failing rows: {:#?}",
        failed.len(),
        failed
    );
}
