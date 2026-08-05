# Mission 0958-c: Real Cairo Cryptographic Follow-up — HMAC-BLAKE3, Ed25519, ProverInput, Stage-2 Split

## Status

Open (filed 2026-08-05 by mission 0958-b Round 1 review F-3 closure; updated 2026-08-05 Round 1 review of 0958-c itself per multi-round adversarial review methodology). Addresses the deferred items enumerated in `missions/claimed/0958-b-real-cairo-crypto.md` §S1 Deviations + §S2 Deviations + §S3 Deviations + §Type Coverage.

**Submitter (Round 1 fix F-16):** @cipherocto (BLUEPRINT mission-lifecycle implies claimant identity for `claimed/` transition; this mission is Open with explicit submitter for traceability).

**Review ownership:** multi-round review rounds 1..N (this file) by @cipherocto adversarial review pool.

**RFC-0958 base spec unchanged in 0958-c scope (Round 1 fix F-19).** 0958-c does NOT amend RFC-0958 §Performance Targets in this mission. AC-4 + §Risks row 6 originally proposed an AC-12 envelope lax (50 KB – 1 MB) — that proposal has been REMOVED in R1 closure (see F-06 fix below); any envelope amendment must be filed as a separate RFC-0958 amendment proposal before AC-4 closure.

## RFC

RFC-0958 (Proof Systems): ZK Capability Subclass.

## Phase

B.3-amendment (HMAC-BLAKE3 + Ed25519 cryptographic body completion in `cairo/src/lib.cairo::main`) + C.3-amendment (structured `ProverInput` JSON witness shape) + Stage-2-amendment (verifier split for CASM size; touches both Phase B and C surfaces). Round 2 fix F-33 + F-37: renumbered from `B.2-amendment`/`C.2-amendment`/`D-amendment` to inherit 0958-b's `.3` pattern; the ``D-amendment`` suffix was wrong because 0958-a's Phase D is "integration", not cryptographic body work.

## Depends on

- Mission `0958-b` (Claimed, v0.5; surface area, real Cairo body, FFI dispatch, stub retirement, release-gate fail-closed).
- RFC-0958 base spec (no amendments in 0958-c scope). Round 3 fix F-45: dropped the "Accepted 2026-07-21" status pin per CLAUDE.md RFC referencing rule.
- Architectural design doc `docs/plans/2026-08-05-stage-2-verifier-split.md` (S2d design landed during 0958-b S2).
- Real BLAKE3 in Cairo corelib OR pure-Rust inlining of BLAKE3 (corelib 2.16.0 ships `core::blake` = BLAKE2s only). Per Round 1 fix F-22 + Round 3 fix F-42: **commit to the inlining path**; corelib `core::blake3` migration tracked separately as 0958-c post-v1 follow-up work (RFC-0958 F2 = Plonky3 migration + F3 = Cairo version upgrades — neither covers BLAKE3, so a new F-item must be filed before any corelib migration lands).
- Cairo-native Ed25519 verifier crate — Round 1 fix F-10 + Round 2 fix F-39: default to the inline-verifier path (Ristretto point ops + SHA-512 over `Felt252` ~3-5 KB CASM). Community crate (`cairo-ed25519-verifier` at `https://github.com/tdelabro/cairo-ed25519`, MIT) is FALLBACK only; selected only if upstream is reachable + has a pinned concrete commit SHA at claim time. If no upstream is reachable at claim time, inline-only is the chosen path (F-10 default).

## Summary

Mission 0958-b landed four cryptographic primitives inside the real Cairo circuit + real-zk STWO integration, but deferred five follow-up items to this mission. Each item was a known limitation documented under §Deviations in the 0958-b mission file. Closing them retroactively completes the cryptographic surface that 0958-a + 0958-b opened.

Items:

1. **HMAC-BLAKE3 chain re-derivation inside `cairo/src/lib.cairo::main` (function `main`).** 0958-b shipped HMAC-SHA-256 because cairo-corelib 2.16.0 has `core::blake` (= BLAKE2s, not BLAKE3) and `core::sha256::compute_sha256_byte_array`, but not BLAKE3. This mission replaces the SHA-256 inner hash with BLAKE3 — by inlining a pure-Cairo BLAKE3 implementation (~3-5 KB CASM footprint; BLAKE3 binary Merkle tree over 1024-byte chunks, NOT Ristretto-style per Round 1 fix F-13). The HMAC construction itself is hash-agnostic so the chain shape (RFC-0958 §Macaroon Caveats) is preserved; only the inner hash function changes. **Round 1 fix F-03:** Cairo path syntax is `cairo/src/lib.cairo` (file path) or `cairo::lib::main` (Cairo module path); the `::main` suffix is Rust path syntax and is NOT valid for Cairo.

2. **Ed25519 holder signature verify inside the circuit.** 0958-b deferred Ed25519 because cairo-corelib has only `core::ecdsa` (STARK curve) and `core::ec` (STARK EC), neither of which is Curve25519/Ed25519. This mission embeds a Cairo-native Ed25519 verifier, defaulting to the inline verifier path (Ristretto point ops + SHA-512 over `Felt252`; ~3-5 KB CASM footprint per Round 1 fix F-10 — community crate is fallback only). The verification will replace the `unimplemented!()` stub at `cairo/src/lib.cairo::main` `holder_sig` check site, and TV8 cross-impl will gain a second signing key + verifier path.

3. **Structured `ProverInput` JSON witness.** 0958-b's S2 deviation documented that `prove_batch_signature`'s S2 witness payload is `canonical_ser(BatchSigPublicInputs)` (a 33+N×32 byte buffer) — NOT a valid `ProverInput` JSON shape that the upstream `stwo-sys` parser accepts. The bench `proof_gen_latency_self_host_under_2s_10k_trace` therefore runs the deterministic mock fallback under `VendorState::Ffi` and asserts structural smoke only. This mission constructs the proper `ProverInput` JSON (`{ program: CASM-hex, witness: structured-fields, public: pub-inputs }`) and wires it through `prove_batch_signature`, eliminating the eprintln fallback. Round 1 fix F-21: the fallback path must emit a `ProofBundle.witness_format: 'prover-input-json' | 'bytes-fallback'` enum field, be covered by a `prover_input_fallback_observable` integration test, and be kill-switchable via real-zk configuration (`ProverInput::bytes_fallback = false` trips fail-closed).

4. **Stage-2 verifier split (CASM 303 KB → ~10 KB main circuit).** 0958-b's S2 deviation noted that the actual split is a 5-step refactor:
    1. Split `cairo/src/lib.cairo::main` into three sub-functions: `verify_chain`, `verify_holder_sig`, `verify_inference_fold`.
    2. Compose them via a Stage-2 STWO verifier (each sub-circuit emits a STARK proof; the main circuit verifies those proofs instead of inlining the cryptographic primitives).
    3. Add a new test covering the composition (`tests/zk_vectors.rs::tv9_stage2_split_round_trip`).
    4. Snapshot update — the bundled CASM hash changes.
    5. AC-12 envelope re-check (proof size 50–500 KB holds for Stage-2 composition proofs).
   The `max_bytecode_size = 50 * 1024` ceiling does NOT currently fire (it constrains Sierra statement count, not CASM bytes — per 0958-b S1 deviation). The split is therefore an architectural cleanup, not a hard requirement, and proceeds opportunistically. **Round 1 fix F-09:** STWO recursive composition is RFC-0958 §Future Work F7; AC-4 baseline plan is to add a forked STWO commit (or upstream PR) that lands the composition API; if the upstream merge is rejected, AC-4 escalates to a corelib-pinned fork and §AC-4 Deviations captures the fork path.

5. **Honest-disclosure residual cleanup.** Round 1 fix F-04: 0958-c item 5 should NOT cite "H2, H4 follow-up" — H2 (crypto) was an honest disclosure in 0958-a AC-3 tracked in 0958-b (CLOSED in 0958-b S2 commit `77aff4aa`, before 0958-c was authored) and H4 (crypto) was FIXED in 0958-a itself (FFI stub-shaped proof rejection). The actual 0958-c item 5 scope is: 0958-a §R4 Rebuttal Register items that 0958-b S1–S4 did not close (e.g. L1–L3/L5 LOW cleanup that 0958-c's dependency-on-0958-a-v0.5 amendment should address). Closure rationale goes in mission 0958-a as a new v0.5 amendment (not a separate file); 0958-c AC-5 owns the amendment.

## Acceptance Criteria

### AC-1 — HMAC-BLAKE3 chain re-derivation

- [ ] `cairo/src/lib.cairo` (function `main`) uses BLAKE3 instead of SHA-256 as the HMAC inner hash. Round 1 fix F-03: Cairo path syntax is `cairo/src/lib.cairo` (file) or `cairo::lib::main` (module); the `::main` suffix is wrong.
- [ ] `cairo/Scarb.toml` BLAKE3 dependency declared (inline `pub mod blake3` source-drop at `cairo/src/blake3.cairo` per Round 1 fix F-22 — corelib migration is a separate workstream).
- [ ] `crates/zk-circuit/tests/casm_snapshot.rs` snapshot regenerated; `EXPECTED_CASM_BLAKE3_HASH` updated.
- [ ] RFC-0958 §Test Vectors TV1's HMAC chain step matches the new BLAKE3 inner-hash output. Round 1 fix F-20: TV1 SHA-256 reference is retained as `tv1_sha256_caveat_chain_round_trip`; the new BLAKE3 path is `tv1b_blake3_caveat_chain_round_trip` to preserve RFC-0958 §Test Vectors traceability.
- [ ] `cargo test -p octo-wallet --test zk_vectors` ≥ 15/15 at AC-1 closure time (HMAC-BLAKE3 chain re-derivation does NOT add new tests; it renames `tv1_sha256_caveat_chain_round_trip` to `tv1b_blake3_caveat_chain_round_trip` per F-20). AC-6 final-state gate verifies ≥ 17/17 (15 baseline + TV9 Ed25519 + `tv9_stage2_split_round_trip`) — but those 2 tests are added by AC-2 / AC-4 respectively, not AC-1. Round 2 fix F-31: AC-1 cannot reach 17/17 at its closure time per Order of operations (AC-1 lands before AC-2 and AC-4); the prior R1 fix F-01/F-02 made the count "≥ 17/17" match AC-6 but introduced an impossible-at-closure-time requirement.
- [ ] Existing HMAC-SHA-256 path REMOVED. Round 1 fix F-05: the hedge "or gated as fallback if BLAKE3 not feasible in chosen corelib" is REMOVED; the fallback is non-conformant with HMAC-BLAKE3 RFC-0958 specification. If BLAKE3 inline exceeds the CASM envelope, the escalation path is AC-4 (Stage-2 split) per F-14 below, not a SHA-256 fallback.

### AC-2 — Ed25519 holder signature verify

- [ ] `cairo/src/lib.cairo` (function `main`) invokes an Ed25519 verifier on `holder_sig` + `holder_did` instead of returning `1` after field-bounds checks. Round 1 fix F-03: path syntax.
- [ ] `cairo/Scarb.toml` declares Ed25519 verifier dependency.
- [ ] New test vector TV9 in `crates/octo-wallet/tests/fixtures/capability-zk/zk-mint-ed25519-verify.json` signs with a known test key + verifies in-circuit.
- [ ] `cargo test -p octo-wallet --test zk_vectors` includes TV9; total ≥ 16 passing.
- [ ] TV8 cross-impl gains a second signing key (Ed25519 path) that byte-equivalent verifier accepts.

### AC-3 — Structured `ProverInput` JSON witness

- [ ] `crates/octo-wallet/src/capability/zk_mint.rs::prove_batch_signature` constructs `ProverInput` JSON with `program = BUNDLED_CASM_SOURCE_HEX`, `witness = canonical_ser(...)` + structured trace + signature fields, `public = canonical_ser(PublicInputs)` instead of the current raw byte buffer.
- [ ] `crates/zk-vendor/src/prover_input.rs` adapter layer (PRIMARY path) constructs the JSON shape and passes bytes to the FFI shim. `crates/zk-vendor/stwo-sys/build.rs` upstream export is FALLBACK only if the adapter layer proves infeasible. Round 2 fix F-35: commit to ONE approach at mission-claim time, not "new export if upstream doesn't yet; otherwise ...".
- [ ] No eprintln fallback in production code path under `VendorState::Ffi`.
- [ ] `cargo test -p octo-wallet --test bench -- --include-ignored` measures REAL STWO STARK proof-gen latency; G1 gate (`<2s SelfHost 10K trace`) is exercised, not bypassed.
- [ ] `cargo test -p zk-vendor --test ffi_loading`: new test `prover_input_json_round_trip` green.

### AC-4 — Stage-2 verifier split

- [ ] `cairo/src/lib.cairo` (function `main`) decomposed into 3 sub-functions: `verify_chain`, `verify_holder_sig`, `verify_inference_fold` (each targeting < 10 KB CASM as a soft budget; hard ceiling is the AC-4 hard constraints below per Round 2 fix F-36). Round 1 fix F-03: path syntax. Round 3 fix F-44: the design doc `docs/plans/2026-08-05-stage-2-verifier-split.md` uses `verify_hmac_chain` + `verify_poseidon_fold` (HMAC-SHA-256 naming); 0958-c renames to `verify_chain` (BLAKE3 inner hash, not HMAC-SHA-256 specifically) + `verify_inference_fold` (Poseidon) + adds `verify_holder_sig` (new Ed25519 sub-function, not in original design doc). The design doc's S2d plan is updated to reflect 0958-c's renamed + new sub-functions at 0958-c AC-4 implementation start.
- [ ] New `cairo/src/lib.cairo` function (caller composed of the three STARK proofs). Round 1 fix F-03: the composition entry-point is a new function in `cairo/src/lib.cairo` (file path), not `cairo/src/lib.cairo::stage2_main`. Round 1 fix F-09: stage-2 composition API requires a forked STWO commit if upstream `prove_cairo` composition is unavailable.
- [ ] New `crates/octo-wallet/tests/fixtures/capability-zk/zk-verify-stage2-composition.json` covers all combinations of sub-proof validity.
- [ ] New test `tv9_stage2_split_round_trip` in `tests/zk_vectors.rs`.
- [ ] Sierra statement count ≤ 50 KB (enforced by `max_bytecode_size` in `crates/zk-circuit/src/lib.rs`). Round 1 fix F-27: split constraint (a) — Sierra statement count vs (b) CASM bytes.
- [ ] CASM bytes, after Stage-2 split, ≤ 50 KB (new assertion to be added to `crates/zk-circuit/tests/casm_snapshot.rs` per Round 2 fix F-32). The current `casm_snapshot.rs` does NOT enforce this constraint — `max_bytecode_size = 50 * 1024` enforces Sierra statement count only (per 0958-b S1 deviation). A new `casm_bytes_under_50kb_after_stage2_split` test is required. Round 1 fix F-27: constraint (b).
- [ ] AC-12 envelope re-checked: proof size 50–500 KB holds for Stage-2 composed proofs. Round 1 fix F-11: the prior `> 50 KB` formulation dropped the 500 KB upper bound; F-06 dropped the silent RFC-0958 amendment path; the AC-12 envelope is the contract, not a hedge.

### AC-5 — Honest-disclosure closure

- [ ] Mission `0958-a` amended to v0.5 documenting which R4 Rebuttal items are closed by 0958-c. Round 1 fix F-04: the "H2, H4 follow-up" wording is REMOVED; the actual scope is the 0958-a R4 LOW items (L1–L3/L5) that 0958-b S1–S4 did not close. The amendment goes in 0958-a as a new v0.5 row.
- [ ] AC-11 / AC-12 stub-disclosure variants resolved (either closed by 0958-c cryptographic body completion OR formally carried into 0958-c's own §Acceptance Deviations subsections per F-17 below). Round 1 fix F-19: 0958-c does NOT amend RFC-0958 AC-12 envelope; the 50–500 KB envelope is the contract.

### AC-6 — Clippy + tests + bench + fuzz

- [ ] `cargo clippy --workspace --all-targets -- -D warnings` clean (Round 2 fix F-38: this cross-ref to "Round 4 review F-25" was the 0958-b R4 review (per 0958-b v0.5 row), which dropped `--features full`; the `full` cargo feature was REMOVED in mission 0958-b S2 commit `77aff4aa` in favor of runtime dispatch on `zk_vendor::vendor_state()`. The same F-25 ID in 0958-c's own R1 review (per v0.3 row) refers to a different finding (Phase B.4/C.4/D.4 duplication in footer); the cross-ref here is to 0958-b's R4 review F-25, not 0958-c's R1 F-25).
- [ ] `cargo test -p octo-wallet --test zk_vectors` ≥ 17/17 (15 from 0958-b baseline = 10 RFC-0958 §Test Vector functions with TV5/TV7 splits [TV5: `tv5_casm_drift_detected_at_mint` + `tv5_casm_drift_detected_at_verify`; TV7: `tv7_clock_skew_exceeded_rejected` + `tv7_clock_skew_within_window_accepted`] + 5 companion tests [`ac7_wholesale_zkbearing_registration_rejected`, `ac7_hybrid_without_explicit_mint_remains_v1`, `ac9_public_input_mismatch_detected_under_slot_binding_drift`, `r3_casm_n2_rotation_accepts_either_v1_or_v2_hash`, `r3_axes_consumed_canonical_sort_independent_of_input_order`] + TV9 Ed25519 + `tv9_stage2_split_round_trip`). Round 2 fix F-38: this cross-ref to "Round 4 review F-28" was the 0958-b R4 review (per 0958-b v0.5 row), which corrected the prior ≥ 16/16 claim (off-by-one: 15 + 2 = 17, not 16; "8 original TVs" was internally inconsistent with AC-1's "10 test functions via splits" phrasing). The same F-28 ID in 0958-c's own R1 review (per v0.3 row) refers to a different finding (WASM ↔ CASM size conflation); the cross-ref here is to 0958-b's R4 review F-28, not 0958-c's R1 F-28.
- [ ] `cargo test -p octo-wallet --test bench -- --include-ignored` exercises real STWO proof gen (G1 + G2 + AC-12 envelopes all green without mock fallback).
- [ ] 60s local fuzz smoke on `capability_zk_verify` green; 24h CI nightly healthy.
- [ ] `cargo fmt --all` ran.

## Implementation Guide

### Order of operations

1. **AC-3 first** (structured ProverInput JSON). This unblocks the bench gate (AC-6 G1 real-zk latency measurement) and the G1 gate. The Stage-2 split (AC-4) and HMAC-BLAKE3 + Ed25519 work (AC-1, AC-2) need the witness shape stable. Round 1 fix F-12: the prior "item 5" dangling cross-reference is replaced with the actual AC-6 reference.
2. **AC-1 + AC-2 sequentially** (HMAC-BLAKE3 then Ed25519). Round 1 fix F-23: not actually parallel — both modify `cairo/src/lib.cairo::main` (the same function in the same file) and merge-conflict at every commit. AC-1 lands first (sets the hash chain pattern); AC-2 follows (verifies holder sig against the chain commitment).
3. **AC-4** (Stage-2 split) — depends on AC-1 + AC-2 landing first (the three sub-functions correspond to verify_chain (HMAC-BLAKE3), verify_holder_sig (Ed25519), verify_inference_fold (Poseidon)).
4. **AC-5** (mission 0958-a amendment) — last, after AC-1..AC-4 green; the amendment depends on what was actually closed.
5. **AC-6** (bench + fuzz green without mock fallback) — exec'd after AC-3 lands; same status as the 0958-b S3 bench gate but on the real-zk path.

### Architectural direction

1. **HMAC-BLAKE3 sourcing** — commit to inline path (Round 1 fix F-22). The `blake3` Rust crate is BSD-2 + Attribution; the pure-Rust reference impl is ~1500 lines. Cairo port of BLAKE3 adds ~5 KB CASM (compression function + binary Merkle tree over 1024-byte chunks). Round 1 fix F-13: the tree structure is binary Merkle tree, NOT Ristretto-style. Future corelib `core::blake3` migration tracked separately as 0958-c post-v1 follow-up work (Round 3 fix F-42: not on RFC-0958 F2/F3 future-work list).

2. **Ed25519 sourcing** — default to inline verifier (Round 1 fix F-10). Ristretto point ops + SHA-512 over `Felt252`; ~3-5 KB CASM. Community crate `cairo-ed25519-verifier` (https://github.com/tdelabro/cairo-ed25519, MIT) is fallback only — pin concrete commit SHA before claim. Round 1 fix F-28: WASM footprint ~4 KB does NOT directly map to CASM cost (CASM is more verbose; budget 5-8 KB CASM after WASM-to-CASM transpile).

3. **Structured `ProverInput`** — JSON shape needs upstream `stwo-sys` confirmation. The library's `ProverInput` deserializer accepts `serde_json::Value` paths; fallback is to construct the JSON in `zk-vendor` and pass bytes to the FFI shim. Round 1 fix F-21: fallback path must be observable (emit `ProofBundle.witness_format` enum field), testable (`prover_input_fallback_observable` integration test), and kill-switchable (`ProverInput::bytes_fallback = false` trips fail-closed).

4. **Stage-2 STWO composition** — uses STWO's recursive proof composition. Round 1 fix F-09: cite a concrete upstream branch/commit and the specific API used (e.g. `StarkProof::compose` at `starkware-industries/stwo@<sha>` or `mmacedoeu/stwo@<sha>`). If upstream lacks composition, escalate to a forked STWO commit; the forked-STWO path is captured in §AC-4 Deviations. The three sub-circuits each emit a STARK proof with their own public input commitment; the main Cairo circuit takes the three commitments + a Poseidon hash of the three proofs as its public input and verifies the chain via STWO's composition verifier.

### Risks

| # | Risk | Mitigation |
|---|------|-----------|
| 1 | BLAKE3 inline exceeds CASM envelope | Escalate to AC-4 (Stage-2 split) as fallback. Round 1 fix F-14: AC-1 and AC-4 are SEPARATE deliverables; mitigation is escalation, not coupled execution. Order of operations says AC-1 lands before AC-4; if AC-1 hits CASM envelope, AC-4 is the explicit fallback (own AC, own owner, own deliverable). |
| 2 | Ed25519 community crate unmaintained | Inline minimal verifier (Ristretto + SHA-512 over `Felt252`); pin concrete commit SHA if community crate is selected. Round 1 fix F-10. |
| 3 | `ProverInput` JSON shape diverges between upstream and our witness | Add `crates/zk-vendor/src/prover_input.rs` adapter layer; fall back to bytes if JSON parse fails (forward-compat). Round 1 fix F-21: fallback is observable + testable + kill-switchable. |
| 4 | Stage-2 composition increases proof gen latency | Inherit RFC-0958 AC-11 gate (`<2s for SelfHost 10K trace steps on reference HW`). Round 1 fix F-07: the prior `<4s` gate was invented and contradicted AC-11. The 2s gate is the contract; AC-4 must measure against it. |
| 5 | Corelib pin duplication (closed by 0958-a R4 H5) | Round 1 fix F-15: closed by 0958-a R4 H5; 0958-c inherits the pin. No 0958-c work. Round 3 fix F-47: removed the "(REMOVED in R1)" placeholder row, renumbered to 5 with explicit closure status. |
| 6 | AC-12 envelope re-check fails | Round 1 fix F-06: the prior "relax upper bound to 1 MB" escape hatch is REMOVED. The 50–500 KB envelope is RFC-0958's contract; if Stage-2 composition cannot meet it, AC-4 fails-closed. Any envelope amendment requires an RFC-0958 amendment proposal before AC-4 closure. Round 3 fix F-48: dropped the "v1.4" version pin per CLAUDE.md RFC referencing rule. |

## Acceptance Deviations (per-AC subsections, per Round 1 fix F-17 + F-26)

Each entry follows `[[deferred-vs-unspecified]]` form: unfulfilled AC + concrete plan to close + owner + target date.

### AC-1 Deviations (HMAC-BLAKE3 chain re-derivation)

- **BLAKE3 inline exceeds CASM envelope.** Mitigation: escalate to AC-4 (Stage-2 split) — see Risks row 1. Owner: 0958-c claimant. Target: AC-1 closure date.
- **Future corelib `core::blake3` migration.** Tracked separately as 0958-c post-v1 follow-up work (Round 3 fix F-42: not on RFC-0958 F2/F3 future-work list; needs a new F-item filed before any corelib migration lands). NOT in 0958-c scope.

### AC-2 Deviations (Ed25519 holder signature verify)

- **[mitigated — Round 1 fix F-10] Community crate `cairo-ed25519-verifier` upstream reachable + pinned SHA.** Round 1 fix F-10 resolved the only deviation scenario by making inline the default. Round 2 fix F-41: this bullet was already mitigated at R1; retained here for traceability per §Acceptance Deviations structural consistency, marked `[mitigated]` to distinguish from open deviations.

### AC-3 Deviations (structured ProverInput JSON witness)

- **`ProverInput` JSON shape diverges between upstream and our witness.** Round 1 fix F-21: fallback path emits `ProofBundle.witness_format` enum field, is testable via `prover_input_fallback_observable` integration test, and is kill-switchable via `ProverInput::bytes_fallback = false`. Owner: 0958-c claimant. Target: AC-3 closure date. Round 2 fix F-30: added Owner + Target fields matching the [[deferred-vs-unspecified]] form at line 134.

### AC-4 Deviations (Stage-2 verifier split)

- **STWO recursive composition + upstream citation.** Round 1 fix F-09 + Round 2 fix F-40 + Round 3 fix F-50: merged into single deviation. AC-4 baseline plan is to add a forked STWO commit (or upstream PR) that lands the composition API; if the upstream merge is rejected, AC-4 escalates to a corelib-pinned fork. The concrete SHA citation (`starkware-industries/stwo@<sha>` / `mmacedoeu/stwo@<sha>`) is filled at mission-claim time when upstream is reachable. Owner: 0958-c claimant. Target: AC-4 implementation start (fork plan) + AC-4 claim time (citation).
- **AC-12 envelope 50–500 KB not met by Stage-2 composition.** Round 1 fix F-06: AC-4 fails-closed. Any envelope amendment requires an RFC-0958 amendment proposal before AC-4 closure. Owner: 0958-c claimant. Target: AC-4 closure date (or fail-closed verdict).
- **Sub-function CASM bytes &lt; 10 KB not achievable.** Round 1 fix F-27: split into two constraints (Sierra statement count ≤ 50 KB + CASM bytes ≤ 50 KB); the 10 KB per-sub-function target is a soft budget, not a hard AC. Round 3 fix F-46: this deviation is a soft-budget note, not an unfulfilled AC — kept for traceability but explicit non-blocking. Owner: 0958-c claimant. Target: AC-4 closure date (advisory).
- **Reference HW profile not defined.** Round 2 fix F-34: moved from §AC-6 Deviations (where it was mis-located) to §AC-4 Deviations. AC-4 inherits RFC-0958 AC-11 gate (`<2s SelfHost 10K trace steps on reference HW`). Reference HW profile specification (CPU model + core count + RAM + STWO build flags + OS) goes in `docs/07-developers/zk-capability-circuit-guide.md` §Reference HW before AC-4 closure. Owner: 0958-c claimant. Target: AC-4 closure date.

### AC-5 Deviations (honest-disclosure closure)

- **0958-a R4 Register items that 0958-b S1–S4 did not close.** Round 1 fix F-04 + Round 3 fix F-43: scope is L1–L3/L5 LOW items. Closure rationale goes in 0958-a as a new v0.5 row. Owner: 0958-c claimant (in coordination with 0958-a claimant @cipherocto per `missions/claimed/0958-a-zk-capability-circuit.md` §Claim gate). Target: AC-5 closure date.

### AC-6 Deviations (clippy + tests + bench + fuzz)

(none after Round 2 fix F-34 moved Reference HW profile to §AC-4 Deviations)

## Related Artifacts

- **Parent missions:**
    - `missions/claimed/0958-b-real-cairo-crypto.md` (Claimed, v0.4+; declared this follow-up as a phantom file before Round 1 review)
    - `missions/claimed/0958-a-zk-capability-circuit.md` (Claimed, v0.4+; will receive v0.5 amendment)
- **Design doc:** `docs/plans/2026-08-05-stage-2-verifier-split.md` (S2d design landed during 0958-b S2)
- **Sibling missions:** `missions/open/zk-proof-verification.md` (generic STWO marketplace; shares `crates/zk-vendor/stwo-sys/`)

---

**Submission Date:** 2026-08-05T00:00:00Z
**Last Updated:** 2026-08-05T23:59:00Z
**Version:** 0.3 (Open; AC-1..AC-6 defined; multi-round review R1 closed; rounds 2..N pending)

## Version History

| Version | Date | Status | Notes |
|---|---|---|---|
| v0.1 | 2026-08-05 | Open | Authored by Round 1 review F-3 closure of mission `0958-b`. Aggregates 5 DEFERRED items that pointer'd at a phantom mission: HMAC-BLAKE3 chain re-derivation (AC-1), Ed25519 holder signature verify in-circuit (AC-2), structured `ProverInput` JSON witness shape (AC-3), Stage-2 verifier split for CASM size (AC-4), honest-disclosure residual closure (AC-5). Clippy + tests + bench + fuzz gates in AC-6. Order of operations in §Implementation Guide puts AC-3 first (witness shape unblocks bench + real-zk latency measurement), AC-1 + AC-2 in parallel, AC-4 last (depends on AC-1 + AC-2), AC-5 last (depends on AC-1..AC-4 closure). Round 2 review F-20 added this Version History table for consistency with sibling mission `0958-b` v0.4. |
| v0.2 (current) | 2026-08-05 | Open | Round 3 (F-21): R2 propagation fix to AC-1 — corrected the prior "15/15 (TV1 through TV8 + 7 extras)" AC-1 wording to "15/15 (8 RFC-0958 §Test Vectors TV1–TV8 → 10 test functions via TV5/TV7 splits + 5 companion tests)" matching R2 F-19 in `0958-b`. Round 4 (F-25): dropped `--features full` from AC-6 clippy invocation; the `full` cargo feature was REMOVED in mission 0958-b S2 commit `77aff4aa` in favor of runtime dispatch. Round 4 (F-28): corrected AC-6 ≥ 16/16 count to ≥ 17/17 (15 baseline + TV9 Ed25519 + tv9_stage2_split_round_trip); also fixed internal contradiction with AC-1's "8 original TVs" phrasing which was inconsistent with AC-1's "10 test functions via splits". Round 5 (F-34): added this Version History v0.2 row aggregating R2–R5 review closures; the v0.1 row was retained for documentary continuity. |
| v0.3 (current) | 2026-08-05T23:59:00Z | Open | **Round 1 of 0958-c's own review (NOT 0958-b's review).** 28 findings closed: 9 CRITICAL (F-01 AC-1 test-count contradiction, F-02 AC-1 broken arithmetic, F-03 `cairo/src/lib.cairo::main` Rust path syntax applied to Cairo, F-04 H2/H4 follow-up miscited (H2 closed in 0958-b S2; H4 fixed in 0958-a), F-05 AC-1 SHA-256 fallback hedge violates `[[deferred-vs-unspecified]]`, F-06 silent RFC-0958 AC-12 amendment via Risks row 6, F-07 invented "<4s" gate contradicted AC-11, F-08 Phase B.4/C.4/D.4 phantom numbering, F-09 unverifiable "STWO recursive composition already supported upstream"); 14 MAJOR (F-10 cairo-ed25519-verifier placeholder dependency, F-11 AC-4 ">50KB" check dropped 500 KB upper bound, F-12 Order of operations "item 5" dangling cross-ref, F-13 "Ristretto-style tree structure" factual error, F-14 Risks row 1 conflates AC-1 + AC-4, F-15 corelib pin already closed by 0958-a R4, F-16 missing Claimant + version metadata lag, F-17 empty §Acceptance Deviations placeholder, F-18 0958-c item 5 mis-walks 0958-b Type Coverage, F-19 "RFC-0958 unchanged" contradicts 0958-c's own AC-12 amendment, F-20 tv1 re-purposing breaks RFC-0958 §Test Vectors traceability, F-21 ProverInput fallback lacks observability, F-22 corelib-watching open-ended, F-23 AC-1+AC-2 not actually parallel); 4 MINOR (F-24 timestamp granularity, F-25 Phase B.4/C.4/D.4 duplicated in footer, F-26 missing per-AC §Deviations subsections, F-27 AC-4 conflated Sierra statement count + CASM bytes); 1 NIT (F-28 WASM ↔ CASM size conflation). Phase numbering normalized to B.3-amendment + C.3-amendment + Stage-2-amendment (inherits 0958-b .3 phase base); §Acceptance Deviations expanded to per-AC subsections; §Depends on updated with concrete dependency policy; §Status added Submitter field per `BLUEPRINT.md` lifecycle. RFC-0958 AC-12 envelope amendment path explicitly REMOVED per F-06; AC-4 fails-closed on envelope violation. Round 2 fix F-29: corrected the prior v0.3 row severity counts (prior count was 9/11/6/2 = 28 totals but actual enumerated IDs are 9/14/4/1 = 28 totals). | `afc5be3f` |
| v0.4 | 2026-08-05T23:59:30Z | Open | **Round 2 of 0958-c's own review (NOT 0958-b's review).** 13 NEW findings closed: 1 CRITICAL (F-29 §Version History v0.3 row severity counts math wrong — listed 9/11/6/2 but actual enumerated IDs are 9/14/4/1); 7 MAJOR (F-30 §AC-2/§AC-3 Deviations missing Owner/Target fields, F-31 AC-1 "≥ 17/17" impossible at AC-1 closure time per Order of operations, F-32 AC-4 checkbox references non-existent CASM-bytes-≤-50KB test in `casm_snapshot.rs`, F-33 §Phase "D-amendment" wrongly maps to 0958-a's D integration when work amends 0958-b's B.3 cryptographic body, F-34 Reference HW profile deviation in §AC-6 Deviations belongs in §AC-4 Deviations, F-35 AC-3 ambiguous path needs commitment to ONE approach, F-36 AC-4 "<10 KB CASM" hard vs §AC-4 Deviations soft budget); 5 MINOR (F-37 Phase suffix consistency breaks 0958-b .3 pattern, F-38 F-25/F-28 ID collision between 0958-b R4 and 0958-c R1 reviews, F-39 §Depends on community crate selectable vs §AC-2 Deviations treats as unmaintained, F-40 STWO composition `<sha>` placeholder is an Open mission artifact, F-41 §AC-2 Deviations bullet resolved by F-10 should be marked or removed). Phase renumbered to B.3-amendment + C.3-amendment + Stage-2-amendment; AC-1 test count relaxed to ≥ 15/15 with AC-6 as final-state gate; AC-3 commits to `prover_input.rs` adapter as PRIMARY; new `casm_bytes_under_50kb_after_stage2_split` test planned; AC-6 cross-refs disambiguated with "0958-b R4 review" prefix. | `374f40ca` |
| v0.5 | 2026-08-05T23:59:45Z | Open | **Round 3 of 0958-c's own review (NOT 0958-b's review).** 9 NEW findings closed: 1 CRITICAL (F-42 phantom F2/F3 cross-reference for BLAKE3 corelib migration — RFC-0958 F2 = Plonky3 migration, F3 = Cairo version upgrades; neither covers BLAKE3; appears in 3 locations: §Depends on, §Architectural direction item 1, §AC-1 Deviations); 4 MAJOR (F-43 §AC-5 Deviations "0958-a line 6" violates [[no-line-refs-anywhere]] AND points to the wrong content (line 6 is Master plan metadata, not claimant statement), F-44 AC-4 sub-function naming drift between 0958-c (`verify_chain`, `verify_holder_sig`, `verify_inference_fold`) and the design doc (`verify_hmac_chain`, `verify_poseidon_fold`); design doc updated to reflect 0958-c's renamed + new sub-functions, F-45 §Depends on "RFC-0958 base spec (Accepted 2026-07-21; no amendments in 0958-c scope)" violates CLAUDE.md RFC referencing rule (status pin), F-46 §AC-4 Deviations two bullets missing Owner/Target fields); 2 MINOR (F-47 §Risks row 5 "(REMOVED in R1)" placeholder left numbering gap; renumbered to 5 with explicit closure status, F-48 "RFC-0958 v1.4 amendment proposal" version pin in 2 locations violates CLAUDE.md RFC referencing rule); 2 NIT (F-49 empty §AC-6 Deviations subsection, F-50 redundant §AC-4 STWO bullets merged). Phantom F2/F3 references replaced with "tracked separately as 0958-c post-v1 follow-up work" in 3 locations; CLAUDE.md RFC referencing rule applied to §Depends on + §Risks row 6. | `TBD (R3 closure SHA)` |
