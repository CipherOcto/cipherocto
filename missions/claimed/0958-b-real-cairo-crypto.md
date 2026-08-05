# Mission 0958-b: Real Cairo Cryptographic Body + Real-zk STWO Integration

**Status:** Claimed (2026-08-05); v0.2 — S1 landed (Cairo crypto body)
**RFC:** RFC-0958 (Proof Systems): ZK Capability Subclass
**Phase:** B.3 (real Cairo cryptographic body) + C.3 (real-zk STWO end-to-end)
**Claimant:** @cipherocto
**Depends on:** mission `0958-a` (claimed, v0.4 — surface area landed)
**Session plan:** S1 done; S2-S4 pending.

## Summary

Follow-up to mission `0958-a` (Claimed, v0.4 amended 2026-08-04). Mission 0958-a shipped the CASM compilation pipeline, the STWO FFI bridge, the NodeType gating layer, the wire format v2, and the test vector surface — but the cryptographic body inside the Cairo circuit (`cairo/src/lib.cairo::main`) is currently a structural-only stub (returns `1` after field-bounds checks). Mission 0958-b fills in the real cryptographic primitives:

1. **HMAC-BLAKE3 chain re-derivation inside `cairo/src/lib.cairo::main`** — the macaroon caveat chain is currently not re-derived; the proofer submits a commitment without proving the chain is structurally valid. Implementation uses `core::blake3` and `core::keccak` from cairo-corelib (Cairo 2.x has both).
2. **Ed25519 holder signature verify inside the circuit** — `holder_sig` is currently not verified; the proofer submits it without proving it validates against `holder_did`. Implementation uses a small embedded Ed25519 verifier (Ristretto-style or Cairo-native field ops; ~3-5 KB CASM footprint).
3. **Poseidon inference-trace binding** — `inference_trace.steps` is currently not folded into the proof; `output_hash` is accepted on trust. Implementation uses Cairo corelib Poseidon (`core::poseidon::poseidon_hash_span`).
4. **Real-zk STWO integration** — replace `prove_batch_signature`'s stub BLAKE3 commitment with the real STWO STARK prover. Requires `crates/zk-vendor/stwo-sys/` to be built (`cargo +nightly-2025-06-23 build --release --manifest-path crates/zk-vendor/stwo-sys/Cargo.toml`) and the `real-zk` Cargo feature enabled.

These are the 14 honest-disclosure items tracked in mission 0958-a §R4 Rebuttal Register (C1, C3, C4, H2, H4 follow-up, H9, M9, AC-11 stub disclosure, AC-12 stub disclosure, etc.). The architecture, surface area, and test fixtures already exist from 0958-a; this mission is the cryptographic content inside that surface.

## Acceptance Criteria

### S1 — Cairo cryptographic body (LANDED 2026-08-05)

- [x] `cairo/src/lib.cairo::main` body implements HMAC chain re-derivation (≥3 caveat chain depth exercised; uses SHA-256 from corelib; see §S1 Deviations for BLAKE3→SHA-256 swap)
- [x] `cairo/src/lib.cairo::main` body implements Poseidon inference-trace binding (TV1 SelfHost trace → `output_hash` check; `assert!(trace_root == pub_inputs.output_hash)`)
- [x] 5 inline `scarb cairo-test` tests green (RFC 4231 TC1, determinism, Poseidon fold, distinct-input distinct-output, chain depth constant)
- [x] All 8 `zk-circuit/tests/casm_snapshot.rs` tests green (scarb build, Sierra IR, determinism, bundled source, CASM hash stability)
- [x] Workspace `cargo test --workspace --lib`: 0 failures (pre-existing `default_path_is_lib_dir` flake unrelated to S1)
- [x] `cargo clippy --workspace --lib --no-deps -- -D warnings` clean
- [x] `bundled_casm_hash_hex()` snapshot updated automatically (BLAKE3-256 hex recomputed at compile time; no hardcoded expected value)

### S1 Deviations (documented per [[deferred-vs-unspecified]])

- **HMAC-BLAKE3 → HMAC-SHA-256.** `cairo-corelib 2.16.0` ships `core::blake` (= BLAKE2s, NOT BLAKE3) and `core::sha256::compute_sha256_byte_array`, but NOT BLAKE3. S1 ships HMAC-SHA-256 (RFC 4234 + RFC 2104); the HMAC construction is hash-agnostic so the chain shape is preserved. Pure BLAKE3 HMAC is deferred to `missions/open/0958-c-real-cairo-crypto-followup.md` (TBD — file at session S4 closure).
- **CASM size: 303 KB > 50 KB ceiling.** HMAC-SHA-256 inlining pulls corelib's full SHA-256 implementation into the circuit (~100 KB per call × 3 caveats = ~300 KB). The `max_bytecode_size = 50 * 1024` setting in `zk-circuit/src/lib.rs` actually constrains Sierra statement count (NOT CASM bytes) per `cairo-lang-sierra-to-casm` 2.20.0 semantics — see `compiler.rs:486` (`if program_offset > config.max_bytecode_size`). The 50 KB CASM ceiling therefore does not currently fire. Stage-2 verifier split (per mission §Risks row 1) is the correct mitigation; deferred to S2.
- **Ed25519 holder-sig verify → DEFERRED to S1.5.** Corelib has `core::ecdsa` (STARK curve) and `core::ec` (STARK EC); neither is Curve25519/Ed25519. An inline Ed25519 verifier is ~3-5 KB CASM; warrants its own focused session.

### S2-S4 — pending

## Acceptance Criteria

### Type Coverage (new)

- [ ] `cairo/src/lib.cairo::main` body implements HMAC-BLAKE3 chain re-derivation (≥3 caveat chain depth exercised in test vector)
- [ ] `cairo/src/lib.cairo::main` body implements Ed25519 holder signature verify (test vector signs with known test key, verifies in-circuit)
- [ ] `cairo/src/lib.cairo::main` body implements Poseidon inference-trace binding (TV1 SelfHost trace → output_hash check)
- [ ] `prove_batch_signature` real-zk path emits real STWO STARK proof bytes (50–500 KB range, per AC-12)
- [ ] Stub proofer deleted from default build (gated only under `#[cfg(feature = "allow-stub-verifier")]`)
- [ ] `stub_commitment` returns `Result<[u8; 32], ProverError>` instead of infallible `[u8; 32]` (no panic in production)
- [ ] All 8 zk_vectors.rs tests still green (now exercising real cryptographic checks, not just structural)
- [ ] 24h cargo-fuzz run on `capability_zk_verify` finds zero cryptographic-bypass vectors

### Integration with 0958-a surface

- [ ] Public API unchanged (`bundled_casm_hash`, `mint_with_zk_and_signers`, `verify_capability_zk`, etc.)
- [ ] `crates/octo-wallet/tests/bench.rs::proof_size_50_to_500kb` activates under default `--features real-zk` (no longer cfg-gated)
- [ ] `crates/octo-wallet/tests/bench.rs::proof_gen_latency_self_host_under_2s_10k_trace` measures real STWO STARK proof generation (sub-2s on reference HW)
- [ ] FFI arg-order integration test added (R4 H9): actually call `sys.prove(casm, witness, public)` with real inputs and verify the proof is accepted by `sys.verify`
- [ ] AC-12 50KB lower bound becomes a default test (no longer `#[cfg(feature = "real-zk")]`)
- [ ] `BUNDLED_CIRCUIT_BLAKE3_HASH` snapshot updated (CASM bytecode changes when Cairo `main()` body grows)
- [ ] `dev_guide.md` §Build, §AC evidence updated to reflect real-zk default

## Dependencies

**Hard upstream blockers (mission must complete first):**
- Mission `0958-a` (Claimed, v0.4) — surface area, wire format, gating, FFI bridge, test fixtures already exist
- RFC-0958 Accepted status unchanged

**Soft prerequisites (helpful but not blocking):**
- Cairo corelib cryptographic primitives (`core::blake3`, `core::poseidon`) — verified available in scarb 2.16.0 corelib
- Ed25519 verification in Cairo — see `cairo-ed25519-verifier` crate or implement minimal inline (Ristretto point ops + SHA-512 over `Felt252`)

## Implementation Guide

### Architectural direction

1. **Cairo circuit body rewrite** — replace the current `cairo/src/lib.cairo::main()` structural stub with the cryptographic body. Use scarb 2.16.0 corelib imports; pin all corelib versions in `cairo/Scarb.toml`. Verify CASM size stays under `max_bytecode_size = 50 * 1024` (set by 0958-a R4 fix). If CASM exceeds 50 KB, use the `#[cfg(feature = "casm_split")]` flag to move the HMAC-BLAKE3 chain into a separate verifying sub-circuit (Stage-2 verifier pattern).

2. **Real-zk STWO integration** — extend `prove_batch_signature` to actually call `zk_vendor::loaded_library().unwrap().prove(casm, witness, public)` (R4 H9 integration test). The current `#[cfg(feature = "full")]` stub returns `Err(ProverError::Internal("full path unimplemented"))`; replace with the real implementation. Drop the `full` feature flag; replace with `real-zk` (already declared in `crates/octo-wallet/Cargo.toml` per R3 #4 `2fb0a455`).

3. **Stub proofer retirement** — once real-zk ships, the stub proofer is no longer the default. Move `stub_commitment` behind `#[cfg(any(test, feature = "allow-stub-verifier"))]`. Make it return `Result<[u8; 32], ProverError>` so production code never panics on the stub.

### Risks

| Risk | Mitigation |
|------|-----------|
| CASM bytecode exceeds 50 KB after Ed25519 + HMAC-BLAKE3 chain additions | Use Stage-2 verifier pattern (split into main + verify sub-circuit) |
| Cairo corelib Poseidon hash output length mismatch with Rust-side verification | Pin corelib version in `cairo/Scarb.toml`; snapshot test the hash output against known test vectors |
| Ed25519 verifier inside Cairo is large (~5-10 KB CASM) | Use `cairo-ed25519-verifier` crate; document the dependency in dev guide |
| Real STWO STARK proof gen exceeds 2s budget on reference HW | Gate on `--release` flag + reference HW profile; document minimum spec |
| BLAKE3 keyed-hash semantics differ between Rust blake3 crate and Cairo corelib | Pin both versions; cross-verify against RFC-0958 §Test Vectors TV1 |
| Stub proofer removal breaks dev/CI test workflows | Keep stub opt-in via `#[cfg(feature = "allow-stub-verifier")]` for test environments that lack `libstwo_sys.so` |

### Cross-Repo Coordination

None. Single cipherocto-side mission; no stoolap fork work.

## Related Artifacts

- **Parent mission:** `missions/claimed/0958-a-zk-capability-circuit.md` (v0.4; surface area + tests + bridge)
- **Sibling mission:** `missions/open/zk-proof-verification.md` (generic STWO marketplace; shares `crates/zk-vendor/stwo-sys/`)
- **Worktree:** none (uses cipherocto workspace directly)

---

**Submission Date:** 2026-08-04
**Last Updated:** 2026-08-04
**Version:** 0.1 (Open; created per mission 0958-a R4 rebuttal register)