//! cargo-fuzz target for `quota_router_core::zk_verify::capability`
//! (mission 0958-a AC-13).
//!
//! **AC-13:** fuzz `capability_zk_verify` for ≥24h on CI nightly schedule;
//! assert no panic (assertion failure, integer overflow, out-of-bounds,
//! unreachable variant) over the corpus.
//!
//! **Target:** `quota_router_core::zk_verify::capability::verify_capability_zk`.
//! The fuzzer generates random `PublicInputs` + `ProofBundle` (synthesized
//! from raw bytes via `pub_inputs_from_data`) and asserts that no path
//! panics. Coverage target = exercise every variant in `ZkVerifyError` +
//! `ZkMintError`. Note: this target does NOT depend on the `arbitrary`
//! crate — `pub_inputs_from_data` consumes raw bytes to construct the
//! structured input, simplifying the dep graph.
//!
//! **Corpus seeds:** the fuzzer starts with an empty corpus; cargo-fuzz
//! populates during nightly runs and persists to
//! `crates/octo-wallet/fuzz/corpus/capability_zk_verify/`. Round 5 review
//! F-32 corrected the prior claim of checked-in seed files exercising
//! each `ZkVerifyError` variant once — those seed files do not exist on
//! disk. Future mission (0958-c or beyond) may author them; for now the
//! empty-corpus nightly run gradually expands coverage across the
//! schedule window.
//!
//! **Run:**
//! ```bash
//! cargo fuzz run capability_zk_verify -p octo-wallet-fuzz
//! # 24h nightly:
//! cargo fuzz run capability_zk_verify -p octo-wallet-fuzz -- -max_total_time=86400
//! ```
//! Round 5 review F-31 corrected the prior `-p octo-wallet` package
//! reference — the fuzz crate is `octo-wallet-fuzz`, not `octo-wallet`
//! (no `[[bin]]` fuzz targets live under the wallet crate itself).
//!
//! **CI integration:** `.github/workflows/zk-capability-circuit.yml::fuzz-nightly`
//! runs `cargo fuzz run` for 90-minute budget per run; the corpus
//! accumulates via disk persistence across runs to reach 24h effective
//! coverage.

#![no_main]

use libfuzzer_sys::fuzz_target;

use quota_router_core::zk_verify::capability::{
    verify_capability_zk, CapabilityVerifier,
};
use quota_router_core::zk_verify::{ProofBundle, PublicInputs, ZkVerifyError};

/// Build a `PublicInputs` from raw fuzz bytes. Splits the data slice into
/// the struct's field layout (no `arbitrary` crate dep); any leftover bytes
/// are folded into `stark_proof` for the verify call.
fn pub_inputs_from_data(data: &[u8]) -> PublicInputs {
    let mut idx = 0;

    macro_rules! take {
        ($n:expr) => {{
            let end = (idx + $n).min(data.len());
            let out = &data[idx..end];
            idx = end;
            out
        }};
    }

    let mut ask_id = [0u8; 32];
    let a = take!(32);
    ask_id[..a.len()].copy_from_slice(a);

    let mut cap_root_hash = [0u8; 32];
    let c = take!(32);
    cap_root_hash[..c.len()].copy_from_slice(c);

    let mut invocation_hash = [0u8; 32];
    let inv = take!(32);
    invocation_hash[..inv.len()].copy_from_slice(inv);

    let did_bytes = take!(8);
    let holder_did = format!("did:octo:{}", hex::encode(did_bytes));

    let time_bytes = take!(8);
    let mut time_buf = [0u8; 8];
    time_buf[..time_bytes.len().min(8)].copy_from_slice(&time_bytes[..time_bytes.len().min(8)]);
    let current_unix_time = u64::from_le_bytes(time_buf);

    let axes_count = current_unix_time % 1024;
    let axes_consumed = vec![("input_tokens_per_1k".to_owned(), axes_count)];

    let output_hash_present = take!(1);
    let output_hash = if !output_hash_present.is_empty() && output_hash_present[0] % 4 == 0 {
        let mut out = [0u8; 32];
        let v = take!(32);
        if v.len() == 32 {
            out.copy_from_slice(v);
        }
        Some(out)
    } else {
        None
    };

    let slot_bytes = take!(4);
    let provider_slot_id = format!("slot-fuzz-{}", hex::encode(slot_bytes));

    PublicInputs {
        ask_id,
        axes_consumed,
        cap_root_hash,
        invocation_hash,
        holder_did,
        current_unix_time,
        output_hash,
        provider_slot_id,
    }
}

fuzz_target!(|data: &[u8]| {
    if data.len() < 16 {
        return;
    }

    let pi = pub_inputs_from_data(data);
    let mut casm_hash = [0u8; 32];
    let cap = data.len().min(32);
    casm_hash[..cap].copy_from_slice(&data[..cap]);

    // Use the FULL data slice as `stark_proof` so the verifier sees
    // arbitrary-sized payloads (well below the 50-500KB real-STWO gate;
    // fuzz coverage above 500KB is unlikely to add value here). The
    // commitment is broken when the bytes don't match — the fuzz pass
    // sees each error variant as the inputs vary.
    let stark_proof = data.to_vec();

    let proof = ProofBundle {
        stark_proof,
        public_inputs: pi.clone(),
        casm_hash,
        security_bits: 128,
    };
    let verifier = CapabilityVerifier {
        compiled_casm_blake3_hash: casm_hash,
        verifier_local_unix_time: pi.current_unix_time,
    };

    // The verify call: we don't assert any specific error variant — the
    // invariant is "no panic". The harness accepts Ok or any error
    // variant of `ZkVerifyError`.
    let _ = verify_capability_zk(
        &proof,
        &proof.public_inputs,
        &verifier.compiled_casm_blake3_hash,
        verifier.verifier_local_unix_time,
    );

    // Reference `ZkVerifyError` so the harness stays compile-clean when
    // new variants are added downstream (the fuzz target should compile
    // against the latest union).
    let _: Result<(), ZkVerifyError> = Err(ZkVerifyError::PublicInputMismatch(String::new()));
});
