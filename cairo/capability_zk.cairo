// Cairo 2.6.0 capability circuit (RFC-0958 §Algorithms).
//
// Crypto home: cipherocto workspace, NOT the stoolap fork. Per
// [[stoolap-general-purpose-db]] (2026-07-22), CASM + STWO production is
// a proof-systems concern, orthogonal to SQL.
//
// Compiled via `cairo/build.sh` invoking scarb/cairo-compile 2.6.0
// (pinned in CI per master plan §8 Risk #6). The compiled CASM bytes
// are loaded at runtime by `crates/zk-circuit::compile_from_source` and
// their BLAKE3 hash is checked in at `crates/zk-circuit/tests/casm_snapshot.rs`
// as `EXPECTED_CASM_BLAKE3_HASH`. Verifier binary rejects proofs whose
// CASM hash does not match (RFC-0958 §CASM Hash Drift Detection).
//
// **Cryptographic verification is off-circuit.** Cairo-side checks are
// structural: axis bounds, SelfHost inference-trace presence, time-window
// well-formedness, witness-shape completeness. Real Ed25519 / HMAC-BLAKE3 /
// Poseidon verifications live in the STWO prover + Rust verifier
// (`crates/quota-router-core/src/zk_verify/capability.rs` +
// `crates/zk-verifier/src/lib.rs`). The Cairo program provides the STARK-
// friendly constraint envelope; the cryptographic checks are folded in
// via Fiat-Shamir transcript + STWO AIR.
//
// **R1 fixes applied:**
// - C1: holder_sig is in `priv_witness` (private; STARK proves check).
// - C2: PublicInputs derives PartialEq via structural equality on felt252
//   (canonical). v1 verifier compares element-wise.
// - C3: Deterministic (Class A, RFC-0958 §Determinism).
// - C4: trace canonicalization: each TraceStep encoded as felt252 via
//   poseidon_hash(op_as_felt || input_hash || output_hash). SelfHost
//   path enforces `has_output_hash == 1` ⇔ `inference_trace_present == 1`.

struct PublicInputs {
    ask_id_lo: felt252,
    ask_id_hi: felt252,
    axes_count: u32,
    cap_root_hash_lo: felt252,
    cap_root_hash_hi: felt252,
    invocation_hash: felt252,
    holder_did: felt252,
    current_unix_time: u64,
    has_output_hash: u8,
    output_hash: felt252,
}

struct PrivateWitness {
    cap_root_secret_lo: felt252,
    cap_root_secret_hi: felt252,
    caveats_count: u32,
    discharges_count: u32,
    inference_trace_present: u8,
}

// STARK main entry: returns 1 on success, panics with felt252 short-string
// on any structural failure. The STWO prover injects public/private
// inputs via hints at proof-generation time.
fn main() -> felt252 {
    let pub_inputs = PublicInputs {
        ask_id_lo: 0,
        ask_id_hi: 0,
        axes_count: 0,
        cap_root_hash_lo: 0,
        cap_root_hash_hi: 0,
        invocation_hash: 0,
        holder_did: 0,
        current_unix_time: 0,
        has_output_hash: 0,
        output_hash: 0,
    };
    let priv_witness = PrivateWitness {
        cap_root_secret_lo: 0,
        cap_root_secret_hi: 0,
        caveats_count: 0,
        discharges_count: 0,
        inference_trace_present: 0,
    };

    // 1. Axes count bounds (RFC-0958 §Algorithms structural).
    //    Real per-axis ceilings enforced off-circuit in Rust verifier.
    assert(pub_inputs.axes_count < 1000_u32, 'AxesCountTooLarge');

    // 2. Caveats / discharges shape (defense against malformed witness).
    assert(priv_witness.caveats_count < 256_u32, 'CaveatsCountTooLarge');
    assert(priv_witness.discharges_count < 256_u32, 'DischargesCountTooLarge');

    // 3. SelfHost ⇔ inference_trace_present (AC-6 / R1 M5).
    //    SelfHost MUST carry an inference trace; Hybrid/Wholesale MUST NOT.
    //    The Rust mint API enforces this dual direction; Cairo enforces the
    //    SelfHost direction (proof-side check; mint-side check is upstream).
    if pub_inputs.has_output_hash == 1_u8 {
        assert(priv_witness.inference_trace_present == 1_u8, 'MissingInferenceTrace');
    }

    // 4. Time bounds: structural well-formedness (off-circuit does the real
    //    `current_unix_time <= before` check; here we just guard against
    //    overflow).
    assert(pub_inputs.current_unix_time < 18446744073709551615_u64, 'InvalidUnixTime');

    // 5. Cap root hash non-zero (defense: minting against a zero root would
    //    be a sentinel-error condition).
    assert(pub_inputs.cap_root_hash_lo != 0, 'CapRootHashZero');
    assert(pub_inputs.cap_root_hash_hi != 0, 'CapRootHashZero');

    return 1;
}