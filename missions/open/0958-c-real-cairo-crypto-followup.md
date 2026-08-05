# Mission 0958-c: Real Cairo Cryptographic Follow-up — HMAC-BLAKE3, Ed25519, ProverInput, Stage-2 Split

## Status

Open (filed 2026-08-05 by mission 0958-b Round 1 review F-3 closure; addresses the deferred items enumerated in `missions/claimed/0958-b-real-cairo-crypto.md` §S1 Deviations + §S2 Deviations + §S3 Deviations + §Type Coverage).

## RFC

RFC-0958 (Proof Systems): ZK Capability Subclass.

## Phase

B.4 (HMAC-BLAKE3 + Ed25519 cryptographic body completion) + C.4 (structured `ProverInput` JSON witness shape) + D.4 (Stage-2 verifier split for CASM size).

## Depends on

- Mission `0958-b` (Claimed, v0.4+; surface area, real Cairo body, FFI dispatch, stub retirement, release-gate fail-closed).
- RFC-0958 Accepted status unchanged.
- Architectural design doc `docs/plans/2026-08-05-stage-2-verifier-split.md` (S2d design landed during 0958-b S2).
- Real BLAKE3 in Cairo corelib OR pure-Rust inlining of BLAKE3 (corelib 2.16.0 ships `core::blake` = BLAKE2s only).
- Cairo-native Ed25519 verifier crate (`cairo-ed25519-verifier` or inline Ristretto point ops + SHA-512 over `Felt252`).

## Summary

Mission 0958-b landed four cryptographic primitives inside the real Cairo circuit + real-zk STWO integration, but deferred five follow-up items to this mission. Each item was a known limitation documented under §Deviations in the 0958-b mission file. Closing them retroactively completes the cryptographic surface that 0958-a + 0958-b opened.

Items:

1. **HMAC-BLAKE3 chain re-derivation inside `cairo/src/lib.cairo::main`.** 0958-b shipped HMAC-SHA-256 because cairo-corelib 2.16.0 has `core::blake` (= BLAKE2s, not BLAKE3) and `core::sha256::compute_sha256_byte_array`, but not BLAKE3. This mission replaces the SHA-256 inner hash with BLAKE3 — either by inlining a pure-Cairo BLAKE3 implementation (~3-5 KB CASM footprint, Ristretto-style tree structure) or by switching to a BLAKE3-only HMAC specification that does not require the SHA-256 fallback. The HMAC construction itself is hash-agnostic so the chain shape (RFC-0958 §Macaroon Caveats) is preserved; only the inner hash function changes.

2. **Ed25519 holder signature verify inside the circuit.** 0958-b deferred Ed25519 because cairo-corelib has only `core::ecdsa` (STARK curve) and `core::ec` (STARK EC), neither of which is Curve25519/Ed25519. This mission embeds a Cairo-native Ed25519 verifier (Ristretto-style point ops + SHA-512 over `Felt252`; ~3-5 KB CASM footprint) OR adopts the `cairo-ed25519-verifier` community crate. The verification will replace the `unimplemented!()` stub at `main()`'s `holder_sig` check site, and TV8 cross-impl will gain a second signing key + verifier path.

3. **Structured `ProverInput` JSON witness.** 0958-b's S2 deviation documented that `prove_batch_signature`'s S2 witness payload is `canonical_ser(BatchSigPublicInputs)` (a 33+N×32 byte buffer) — NOT a valid `ProverInput` JSON shape that the upstream `stwo-sys` parser accepts. The bench `proof_gen_latency_self_host_under_2s_10k_trace` therefore runs the deterministic mock fallback under `VendorState::Ffi` and asserts structural smoke only. This mission constructs the proper `ProverInput` JSON (`{ program: CASM-hex, witness: structured-fields, public: pub-inputs }`) and wires it through `prove_batch_signature`, eliminating the eprintln fallback.

4. **Stage-2 verifier split (CASM 303 KB → ~10 KB main circuit).** 0958-b's S2 deviation noted that the actual split is a 5-step refactor:
    1. Split `cairo/src/lib.cairo::main` into three sub-functions: `verify_chain`, `verify_holder_sig`, `verify_inference_fold`.
    2. Compose them via a Stage-2 STWO verifier (each sub-circuit emits a STARK proof; the main circuit verifies those proofs instead of inlining the cryptographic primitives).
    3. Add a new test covering the composition (`tests/zk_vectors.rs::tv9_stage2_split_round_trip`).
    4. Snapshot update — the bundled CASM hash changes.
    5. AC-12 envelope re-check (proof size >50 KB should now hold for Stage-2 composition proofs).
   The `max_bytecode_size = 50 * 1024` ceiling does NOT currently fire (it constrains Sierra statement count, not CASM bytes — per 0958-b S1 deviation). The split is therefore an architectural cleanup, not a hard requirement, and proceeds opportunistically.

5. **Honest-disclosure residual cleanup.** 0958-a R4 Rebuttal Register items H2, H4 follow-up, and AC-11 / AC-12 stub disclosure variants: each must be either closed (HMAC-BLAKE3 inline + Stage-2 split eliminate the disclosure scope) or formally carried into 0958-c's own Deviations section. The closure rationale goes in mission 0958-a as a new v0.5 amendment (not a separate file).

## Acceptance Criteria

### AC-1 — HMAC-BLAKE3 chain re-derivation

- [ ] `cairo/src/lib.cairo::main` uses BLAKE3 instead of SHA-256 as the HMAC inner hash.
- [ ] `cairo/Scarb.toml` BLAKE3 dependency declared (either inline `pub mod blake3` source-drop at `cairo/src/blake3.cairo` or `cairo-ed25519-verifier`-style external crate).
- [ ] `crates/zk-circuit/tests/casm_snapshot.rs` snapshot regenerated; `EXPECTED_CASM_BLAKE3_HASH` updated.
- [ ] RFC-0958 §Test Vectors TV1's HMAC chain step matches the new BLAKE3 inner-hash output.
- [ ] `cargo test -p octo-wallet --test zk_vectors` ≥ 15/15 (8 RFC-0958 §Test Vectors TV1–TV8 → 10 test functions via TV5 (`tv5_casm_drift_detected_at_mint` + `tv5_casm_drift_detected_at_verify`) and TV7 (`tv7_clock_skew_exceeded_rejected` + `tv7_clock_skew_within_window_accepted`) splits + 5 companion tests: `ac7_wholesale_zkbearing_registration_rejected`, `ac7_hybrid_without_explicit_mint_remains_v1`, `ac9_public_input_mismatch_detected_under_slot_binding_drift`, `r3_casm_n2_rotation_accepts_either_v1_or_v2_hash`, `r3_axes_consumed_canonical_sort_independent_of_input_order`). TV1 exercises the real BLAKE3 chain (Round 3 review F-21 corrected the prior `TV1 through TV8 + 7 extras` breakdown, which Round 2 review F-19 had already corrected in 0958-b but did not propagate to 0958-c AC-1).
- [ ] Existing HMAC-SHA-256 path REMOVED (or gated as fallback if BLAKE3 not feasible in chosen corelib).

### AC-2 — Ed25519 holder signature verify

- [ ] `cairo/src/lib.cairo::main` invokes an Ed25519 verifier on `holder_sig` + `holder_did` instead of returning `1` after field-bounds checks.
- [ ] `cairo/Scarb.toml` declares Ed25519 verifier dependency.
- [ ] New test vector TV9 in `crates/octo-wallet/tests/fixtures/capability-zk/zk-mint-ed25519-verify.json` signs with a known test key + verifies in-circuit.
- [ ] `cargo test -p octo-wallet --test zk_vectors` includes TV9; total ≥ 16 passing.
- [ ] TV8 cross-impl gains a second signing key (Ed25519 path) that byte-equivalent verifier accepts.

### AC-3 — Structured `ProverInput` JSON witness

- [ ] `crates/octo-wallet/src/capability/zk_mint.rs::prove_batch_signature` constructs `ProverInput` JSON with `program = BUNDLED_CASM_SOURCE_HEX`, `witness = canonical_ser(...)` + structured trace + signature fields, `public = canonical_ser(PublicInputs)` instead of the current raw byte buffer.
- [ ] `crates/zk-vendor/stwo-sys/build.rs` FFI shim accepts the JSON shape (new export if upstream doesn't yet; otherwise an adapter layer at `crates/zk-vendor/src/prover_input.rs`).
- [ ] No eprintln fallback in production code path under `VendorState::Ffi`.
- [ ] `cargo test -p octo-wallet --test bench -- --include-ignored` measures REAL STWO STARK proof-gen latency; G1 gate (`<2s SelfHost 10K trace`) is exercised, not bypassed.
- [ ] `cargo test -p zk-vendor --test ffi_loading`: new test `prover_input_json_round_trip` green.

### AC-4 — Stage-2 verifier split

- [ ] `cairo/src/lib.cairo` decomposed into 3 sub-functions: `verify_chain`, `verify_holder_sig`, `verify_inference_fold` (each < 10 KB CASM).
- [ ] New `cairo/src/lib.cairo::stage2_main` composes the three STARK proofs.
- [ ] New `crates/octo-wallet/tests/fixtures/capability-zk/zk-verify-stage2-composition.json` covers all combinations of sub-proof validity.
- [ ] New test `tv9_stage2_split_round_trip` in `tests/zk_vectors.rs`.
- [ ] CASM snapshot regenerated; size envelope ≤ 50 KB Sierra statement count.
- [ ] AC-12 envelope re-checked: proof size > 50 KB holds for Stage-2 composed proofs.

### AC-5 — Honest-disclosure closure

- [ ] Mission `0958-a` amended to v0.5 documenting which R4 Rebuttal items are closed by 0958-c.
- [ ] H4 follow-up + AC-11 / AC-12 stub-disclosure variants resolved (either closed or formally carried into 0958-c's Deviations).

### AC-6 — Clippy + tests + bench + fuzz

- [ ] `cargo clippy --workspace --all-targets -- -D warnings` clean (Round 4 review F-25: dropped `--features full`; the `full` cargo feature was REMOVED in mission 0958-b S2 commit `77aff4aa` in favor of runtime dispatch on `zk_vendor::vendor_state()`).
- [ ] `cargo test -p octo-wallet --test zk_vectors` ≥ 17/17 (15 from 0958-b baseline = 10 RFC-0958 §Test Vector functions with TV5/TV7 splits [TV5: `tv5_casm_drift_detected_at_mint` + `tv5_casm_drift_detected_at_verify`; TV7: `tv7_clock_skew_exceeded_rejected` + `tv7_clock_skew_within_window_accepted`] + 5 companion tests [`ac7_wholesale_zkbearing_registration_rejected`, `ac7_hybrid_without_explicit_mint_remains_v1`, `ac9_public_input_mismatch_detected_under_slot_binding_drift`, `r3_casm_n2_rotation_accepts_either_v1_or_v2_hash`, `r3_axes_consumed_canonical_sort_independent_of_input_order`] + TV9 Ed25519 + `tv9_stage2_split_round_trip`). Round 4 review F-28 corrected the prior ≥ 16/16 claim (off-by-one: 15 + 2 = 17, not 16; "8 original TVs" was internally inconsistent with AC-1's "10 test functions via splits" phrasing).
- [ ] `cargo test -p octo-wallet --test bench -- --include-ignored` exercises real STWO proof gen (G1 + G2 + AC-12 envelopes all green without mock fallback).
- [ ] 60s local fuzz smoke on `capability_zk_verify` green; 24h CI nightly healthy.
- [ ] `cargo fmt --all` ran.

## Implementation Guide

### Order of operations

1. **AC-3 first** (structured ProverInput JSON). This unblocks the bench gate (item 5) and the G1 real-zk latency measurement. The Stage-2 split (AC-4) and HMAC-BLAKE3 + Ed25519 work (AC-1, AC-2) need the witness shape stable.
2. **AC-1 + AC-2 in parallel** (HMAC-BLAKE3 + Ed25519) — both are cryptographic inlining work, share design patterns, and benefit from cross-circuit hash shape agreement.
3. **AC-4** (Stage-2 split) — depends on AC-1 + AC-2 landing first (the three sub-functions correspond to verify_chain (HMAC-BLAKE3), verify_holder_sig (Ed25519), verify_inference_fold (Poseidon)).
4. **AC-5** (mission 0958-a amendment) — last, after AC-1..AC-4 green; the amendment depends on what was actually closed.

### Architectural direction

1. **HMAC-BLAKE3 sourcing** — first decision is inline vs crate. The `blake3` Rust crate is BSD-2 + Attribution; the pure-Rust reference impl is ~1500 lines. Cairo port of BLAKE3 adds ~5 KB CASM (the compression function + tree structure are constant-time small-footprint). If `core::blake3` lands in corelib before this mission starts, switch to that path.

2. **Ed25519 sourcing** — first decision is `cairo-ed25519-verifier` (community crate, MIT) vs inline. The community crate's WASM footprint is ~4 KB, so its CASM cost is comparable. The crate is not in the workspace yet; add `cairo-ed25519-verifier = { git = "...", rev = "..." }` to `cairo/Scarb.toml`. If the crate is unmaintained, inline a minimal verifier (~3-5 KB CASM).

3. **Structured `ProverInput`** — JSON shape needs upstream `stwo-sys` confirmation. The library's `ProverInput` deserializer accepts `serde_json::Value` paths; fallback is to construct the JSON in `zk-vendor` and pass bytes to the FFI shim.

4. **Stage-2 STWO composition** — uses STWO's recursive proof composition (already supported upstream). The three sub-circuits each emit a STARK proof with their own public input commitment; the main circuit takes the three commitments + a Poseidon hash of the three proofs as its public input and verifies the chain via STWO's composition verifier.

### Risks

| Risk | Mitigation |
|------|-----------|
| BLAKE3 inline exceeds CASM envelope | Use Stage-2 split (sub-circuit covers just HMAC-BLAKE3 chain verification; main circuit proves the chain commitment) |
| Ed25519 community crate unmaintained | Inline minimal verifier; pin dependency to last known-good commit |
| `ProverInput` JSON shape diverges between upstream and our witness | Add `crates/zk-vendor/src/prover_input.rs` adapter layer; fall back to bytes if JSON parse fails (forward-compat) |
| Stage-2 composition increases proof gen latency | Reference HW profile gate (`<4s`); document in `docs/07-developers/zk-capability-circuit-guide.md` |
| Multiple corelib version pins diverge | Pin corelib once in `cairo/Scarb.toml`; dev guide documents the pin |
| AC-12 envelope re-check fails | Stage-2 composition proofs may exceed 500 KB; relax upper bound to 1 MB and document |

## Acceptance Deviations (placeholder for items emerging during implementation)

To be populated at session-end. Each entry must follow `[[deferred-vs-unspecified]]` form: every "deferred" here means an unfulfilled AC + a concrete plan to close + owner + target date.

## Related Artifacts

- **Parent missions:**
    - `missions/claimed/0958-b-real-cairo-crypto.md` (Claimed, v0.4+; declared this follow-up as a phantom file before Round 1 review)
    - `missions/claimed/0958-a-zk-capability-circuit.md` (Claimed, v0.4+; will receive v0.5 amendment)
- **Design doc:** `docs/plans/2026-08-05-stage-2-verifier-split.md` (S2d design landed during 0958-b S2)
- **Sibling missions:** `missions/open/zk-proof-verification.md` (generic STWO marketplace; shares `crates/zk-vendor/stwo-sys/`)

---

**Submission Date:** 2026-08-05
**Last Updated:** 2026-08-05
**Version:** 0.1 (Open; AC-1..AC-6 defined; Phase B.4 + C.4 + D.4)

## Version History

| Version | Date | Status | Notes |
|---|---|---|---|
| v0.1 | 2026-08-05 | Open | Authored by Round 1 review F-3 closure of mission `0958-b`. Aggregates 5 DEFERRED items that pointer'd at a phantom mission: HMAC-BLAKE3 chain re-derivation (AC-1), Ed25519 holder signature verify in-circuit (AC-2), structured `ProverInput` JSON witness shape (AC-3), Stage-2 verifier split for CASM size (AC-4), honest-disclosure residual closure (AC-5). Clippy + tests + bench + fuzz gates in AC-6. Order of operations in §Implementation Guide puts AC-3 first (witness shape unblocks bench + real-zk latency measurement), AC-1 + AC-2 in parallel, AC-4 last (depends on AC-1 + AC-2), AC-5 last (depends on AC-1..AC-4 closure). Round 2 review F-20 added this Version History table for consistency with sibling mission `0958-b` v0.4. |
| v0.2 (current) | 2026-08-05 | Open | Round 3 (F-21): R2 propagation fix to AC-1 — corrected the prior "15/15 (TV1 through TV8 + 7 extras)" AC-1 wording to "15/15 (8 RFC-0958 §Test Vectors TV1–TV8 → 10 test functions via TV5/TV7 splits + 5 companion tests)" matching R2 F-19 in `0958-b`. Round 4 (F-25): dropped `--features full` from AC-6 clippy invocation; the `full` cargo feature was REMOVED in mission 0958-b S2 commit `77aff4aa` in favor of runtime dispatch. Round 4 (F-28): corrected AC-6 ≥ 16/16 count to ≥ 17/17 (15 baseline + TV9 Ed25519 + tv9_stage2_split_round_trip); also fixed internal contradiction with AC-1's "8 original TVs" phrasing which was inconsistent with AC-1's "10 test functions via splits". Round 5 (F-34): added this Version History v0.2 row aggregating R2–R5 review closures; the v0.1 row was retained for documentary continuity. |
