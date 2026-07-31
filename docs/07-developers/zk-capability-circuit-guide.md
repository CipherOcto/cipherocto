# CipherOcto ZK Capability Circuit — Developer Guide

> **RFC:** RFC-0958 v1.3 (ZK Capability Subclass) — Accepted
> **Mission:** `0958-a-zk-capability-circuit.md` (Claimed 2026-07-22, v0.3 crypto extraction)
> **Crypto home:** cipherocto workspace crates `zk-circuit/`, `zk-verifier/`,
> `zk-vendor/` (per [[stoolap-general-purpose-db]] red line; stoolap fork untouched)

This guide covers the operational surface for the ZK capability circuit:
how to build, run tests, add test vectors, bump the CASM/MSRV/STWO substrate,
and debug the FFI bridge.

## Contents

1. [Architecture](#architecture)
2. [Build invocation](#build-invocation)
3. [Run tests](#run-tests)
4. [AC traceability](#ac-traceability)
5. [Test vector table](#test-vector-table)
6. [Vendoring strategy](#vendoring-strategy)
7. [Wire v1 / v2 dual support](#wire-v1--v2-dual-support)
8. [Performance targets](#performance-targets)
9. [Fuzz target + CI nightly](#fuzz-target--ci-nightly)
10. [Operator runbook](#operator-runbook)
11. [Known gotchas](#known-gotchas)

## Architecture

```
┌─────────────────────────────────────────────────────────────────────────┐
│ Holder Wallet (octo-wallet/src/capability/zk_mint.rs)                   │
│   mint_with_zk(node_type, witness, public, casm) -> ProofBundle         │
│      │                                                                  │
│      ├─ NodeType gating   (fail-closed Wholesale → ZkMintError)         │
│      ├─ capability class  (ZKBearing; V1 only via CapabilityToken::mint)│
│      ├─ CASM hash drift   (bundled_casm_hash() vs supplied)             │
│      └─ Proofer delegation (batch-signature or legacy empty path)       │
└─────────────────────────────────────────────────────────────────────────┘
                                  │
                                  ▼
┌─────────────────────────────────────────────────────────────────────────┐
│ Verifier (quota-router-core/src/zk_verify/capability.rs)                │
│   verify_capability_zk(proof, expected, casm, now)                      │
│      │                                                                  │
│      ├─ Public inputs equality (v1.4 IA-11 includes provider_slot_id)    │
│      ├─ CASM hash drift    (proof.casm_hash == compiled_casm_blake3)   │
│      ├─ Clock skew         (|issued - now| <= 300s = MAX_SKEW_SECS)    │
│      └─ Delegate STWO verify → zk_verifier::verify_capability_zk        │
└─────────────────────────────────────────────────────────────────────────┘
                                  │
                                  ▼
┌─────────────────────────────────────────────────────────────────────────┐
│ STWO Layering (crates/zk-verifier/src/lib.rs + crates/zk-vendor/)       │
│   1. FFI (loaded_library()) if libstwo_sys.so is present                │
│   2. BLAKE3 stub fallback (dev / CI without nightly-built cdylib)       │
│   Both paths preserved Class A determinism per RFC-0958.                │
└─────────────────────────────────────────────────────────────────────────┘
                                  │
                                  ▼
┌─────────────────────────────────────────────────────────────────────────┐
│ STWO FFI (crates/zk-vendor/stwo-sys/) — workspace-excluded cdylib       │
│   Built with `cargo +nightly-2025-06-23 build --release`                │
│   Loaded at runtime via `libloading::Library::new` + symbol resolution   │
│   Decoupled pattern: cipherocto workspace stays MSRV-stable (1.75.0)     │
└─────────────────────────────────────────────────────────────────────────┘
```

## Build invocation

### Workspace (stable rust)

```bash
# Build with all current features
cargo build --workspace --features full

# Lint (zero warnings required)
cargo clippy --workspace --all-targets --features full -- -D warnings

# Format
cargo fmt --all
```

**Important:** `--all-features` is **NOT** used because `litellm-mode` and
`any-llm-mode` are mutually exclusive (`compile_error!` at
`quota-router-core/src/router.rs`). The mode gate controls HOW (reqwest vs
PyO3), not WHETHER (RFC-0917 invariant). Use `--features full` for
feature-bounded coverage.

### STWO FFI cdylib (nightly)

```bash
# Build libstwo_sys.so (39MB ELF + 4 FFI symbols)
cd crates/zk-vendor/stwo-sys
cargo +nightly-2025-06-23 build --release

# Output:
#   crates/zk-vendor/stwo-sys/target/release/libstwo_sys.so
```

The cipherocto workspace loads this artifact at runtime via
`zk_vendor::loaded_library()`. Production deployments ship the `.so`
alongside the binary.

### CASM compilation (Cairo 2.6.0)

```bash
# Install cairo-compile via scarb (or asdf fallback)
curl --proto '=https' --tlsv1.2 -sSf https://docs.swmansion.com/scarb/install.sh | sh -s -- -v 2.6.0

# Compile the bundled Cairo source → CASM bytecode
cairo-compile cairo/capability_zk.cairo --output cairo/capability_zk.casm

# Hash the compiled CASM (BLAKE3, 32-byte hash)
blake3 cairo/capability_zk.casm
# expect: <EXPECTED_CASM_BLAKE3_HASH>
```

The CASM BLAKE3 hash is checked into
`crates/zk-circuit/tests/casm_snapshot.rs::EXPECTED_CASM_BLAKE3_HASH` as
a snapshot assertion. Drift detection = PR fails.

## Run tests

```bash
# Default (185 passing tests; ignores #[ignore] perf gates)
cargo test --workspace --lib

# Acceptance integration suites (S05 deliverable)
cargo test -p octo-wallet --test zk_vectors        # 13 tests (8 vectors + 5 AC companions)
cargo test -p octo-wallet --test wire_v2_roundtrip # 6 tests (v2 wire format)
cargo test -p zk-circuit --test casm_snapshot     # 1 test (CASM BLAKE3 hash)

# Perf gates (#[ignore] → opt-in via --include-ignored)
cargo test -p octo-wallet --test bench -- --include-ignored --nocapture
# expected: 3 passed; proof_gen <2s SelfHost 10K trace; verify <100ms;
#           proof size 50-500KB (stub path = 32B structural smoke; real-zk gate under feature)

# FFI libloading (#[ignore] → opt-in via --include-ignored)
cargo test -p zk-vendor --test ffi_loading -- --include-ignored --nocapture
# expected: 3 passed (libloading + symbol resolution + verify error path); version string contains "real STWO"

# Wholesale lint (fail-closed; MUST exit 0)
bash .github/linters/no-wholesale-zk.sh

# Verify the canonical clean-state build
cargo clippy --workspace --all-targets --features full -- -D warnings
```

## AC traceability

| AC | Deliverable | Test entry | Commit |
|----|-------------|-----------|--------|
| **AC-1** | Real Cairo 2.6.0 program body (no `unimplemented!()`) | `cairo/capability_zk.cairo` exists + compiles via `cairo-compile` | `26fa53f6` (S1) |
| **AC-2** | `zk_circuit::compile_from_source` shells out to cairo-compile; CASM BLAKE3 snapshot | `tests/casm_snapshot.rs` | `26fa53f6` (S1) |
| **AC-3** | Decoupled FFI workspace + BLAKE3 stub fallback | `tests/ffi_loading.rs` (hardened) | `4f7f47db`, `be113cb1` (S2) |
| **AC-4** | 8 RFC-0958 §Test Vectors + cross-impl TV1/TV2 | `tests/zk_vectors.rs` (13 tests; 8/8 vectors green) | `46e29fa2` (S3) |
| **AC-5** | Wholesale mint fail-closed + CI lint | `.github/linters/no-wholesale-zk.sh` + `zk-vectors.rs::tv3` + `ac7_wholesale_zkbearing_registration_rejected` | `46e29fa2` (S3) |
| **AC-6** | `MissingInferenceTrace` rename + witness-side guard | `zk_mint::tests::selfhost_mint_rejected_without_inference_trace` | `26fa53f6` (S1) |
| **AC-7** | Hybrid opt-in semantics (V1 default + ZKBearing opt-in) | `tests/zk_vectors.rs::ac7_*` | `46e29fa2` (S3) |
| **AC-8** | Wire format v2 (4th segment = `proof_bundle_borsh`) | `tests/wire_v2_roundtrip.rs` (6 tests) | `46e29fa2` (S3) |
| **AC-9** | `PublicInputMismatch` + slot-binding drift detection | `tests/zk_vectors.rs::tv4 + ac9` | `46e29fa2` (S3) |
| **AC-10** | CASM drift at mint AND verify paths | `tests/zk_vectors.rs::tv5_*` | `46e29fa2` (S3) |
| **AC-11** | Proof gen <2s SelfHost 10K trace + verify <100ms | `tests/bench.rs` (3 #[ignore] gates) | `46e29fa2` (S3) |
| **AC-12** | Proof size 50-500KB (real-zk feature gate) | `tests/bench.rs::proof_size_50_to_500kb` | `46e29fa2` (S3) |
| **AC-13** | cargo-fuzz `capability_zk_verify` 24h nightly | `crates/octo-wallet/fuzz/fuzz_targets/capability_zk_verify.rs` + CI `fuzz-nightly` job | `46e29fa2` (S3) |
| **AC-14** | `cargo clippy --workspace --all-targets --features full -- -D warnings` clean | full workspace lint | `46e29fa2` (S3) |
| **AC-15** | Master plan §8 R12 exit criteria + single cipherocto-side PR | this doc + `git diff --stat` scope check | this commit (S4) |
| **AC-16** | **CANCELED v0.3** (no fork PR; crypto in cipherocto workspace) | — | — |

## Test vector table

| Vector | File | NodeType | Public inputs | Expected outcome |
|--------|------|----------|---------------|------------------|
| TV1 | `tests/fixtures/capability-zk/zk-mint-self-host.json` | SelfHost | `ask_id=0x11..`, `cap_root_hash=0x22..`, `output_hash=Some(0x44..)`, `provider_slot_id="slot-tv1-001"` | MintAccept + VerifyOk |
| TV2 | `tests/fixtures/capability-zk/zk-mint-hybrid-no-trace.json` | Hybrid | `ask_id=0x55..`, `output_hash=None`, `provider_slot_id="slot-tv2-001"` | MintAccept + VerifyOk |
| TV3 | `tests/fixtures/capability-zk/zk-mint-wholesale-reject.json` | Wholesale | `ask_id=0x88..`, `provider_slot_id="slot-wholesale-001"` | MintReject `NodeTypeCannotMintZKCap` |
| TV4 | `tests/fixtures/capability-zk/zk-verify-public-input-mismatch.json` | SelfHost | `ask_id` mutated to `0xff..` | VerifyReject `PublicInputMismatch` |
| TV5 | `tests/fixtures/capability-zk/zk-verify-casm-drift.json` | SelfHost | `casm_hash = 0x00..` | MintReject + VerifyReject `CasmHashMismatch` |
| TV6 | `tests/fixtures/capability-zk/zk-verify-stwo-fail.json` | SelfHost | `stark_proof[0] ^= 0xFF` | VerifyReject `StwoVerifyError` |
| TV7 | `tests/fixtures/capability-zk/zk-verify-expired.json` | SelfHost | `verifier_local_unix_time + 301s` | VerifyReject `ClockSkewExceeded` |
| TV8 | `tests/fixtures/capability-zk/zk-cross-impl-tv1.json` | SelfHost | two prover paths: `mint_with_zk_and_signers` + `zk_verifier::stub_commitment` | Both paths byte-equivalent + VerifyOk |

## Vendoring strategy

**Decoupled workspace pattern (mission 0958-a S05 Session 2 fix-up,
2026-07-31):** STWO is NOT compiled into the cipherocto workspace directly
because its upstream needs nightly toolchain (`curve25519-dalek` SIMD
intrinsics + `iter_array_chunks` polyfill). The cipherocto workspace stays
MSRV-stable (`stable 1.75.0`).

```
crates/zk-vendor/stwo-sys/        # workspace-excluded cdylib (own Cargo.toml)
crates/zk-vendor/stwo-sys/rust-toolchain.toml   # pins nightly-2025-06-23
crates/zk-vendor/rust-toolchain.toml          # pins stable 1.75.0 (cipherocto MSRV)
crates/zk-vendor/src/lib.rs       # libloading FFI bridge + OnceLock cache
crates/zk-verifier/src/lib.rs     # zk-verifier delegates to zk_vendor (FFI > stub)
```

**No vendored STWO source** into the cipherocto workspace. No patches
needed — the decoupled pattern keeps the cipherocto build on stable rust
while letting STWO upstream use nightly.

To bump STWO upstream: increment the `stwo = "=2.1.0"` pin in
`crates/zk-vendor/stwo-sys/Cargo.toml`, rebuild the cdylib, verify
`crates/zk-vendor/tests/ffi_loading.rs` passes with `--include-ignored`.

## Wire v1 / v2 dual support

| Layer | Segments | Forward-compat | Backward-compat |
|-------|----------|---------------|----------------|
| **v1 wire** | `s1.s2.s3` (macaroon, sig, discharges) | ignores 4th segment | — |
| **v2 wire** | `s1.s2.s3.s4` (+ optional `proof_bundle_borsh`) | — | accepts 3-segment wire; `proof_bundle = None` |

**Serialize:**
- `serialize_wire(token)` — emits v1 (3 segments).
- `serialize_wire_v2(token, Some(pb_bytes))` — emits v2 (4 segments).
- `serialize_wire_v2(token, None)` — emits v1 (3 segments) for backward compat.

**Deserialize:**
- `deserialize_wire(s, did, pub)` — recovers `CapabilityToken`; discards 4th segment if present.
- `deserialize_wire_v2(s, did, pub) -> WireV2` — recovers `WireV2 { token, proof_bundle: Option<Vec<u8>> }`.

Base64url no-padding encoding per RFC-0957 §3.7.

## Performance targets

| Gate | Target | Test | Status |
|------|--------|------|--------|
| **G1** (AC-11) | Proof gen <2s on 10K trace (SelfHost reference HW) | `tests/bench.rs::proof_gen_latency_self_host_under_2s_10k_trace` | sub-ms on stub; real-STWO gate under `--features real-zk` |
| **G2** (AC-11) | Verify <100ms | `tests/bench.rs::verify_latency_under_100ms` | sub-ms on stub |
| **AC-12** | Proof size 50-500KB | `tests/bench.rs::proof_size_50_to_500kb` | stub = 32B structural smoke; 50-500KB gate under `--features real-zk` |

**Reference HW:** 4-core x86_64, 16GB RAM, NVMe SSD (per RFC-0958 §Implementation Reference).

## Fuzz target + CI nightly

**Target:** `crates/octo-wallet/fuzz/fuzz_targets/capability_zk_verify.rs`.

**Coverage invariant:** `verify_capability_zk` + `PublicInputs` constructor
must not panic. The harness accepts Ok OR any `ZkVerifyError` variant;
the assertion is "no panic across all variants".

**Run locally:**

```bash
cargo install cargo-fuzz      # one-time
cd crates/octo-wallet
cargo fuzz run capability_zk_verify
# stop at any time; corpus persists to fuzz/corpus/capability_zk_verify/
```

**CI nightly:** `.github/workflows/zk-capability-circuit.yml::fuzz-nightly`
runs `cargo fuzz run` for 90-minute budget per run; corpus accumulates
across runs to reach 24h effective coverage. Schedule: 02:00 UTC daily.

## Operator runbook

### Adding a new test vector

1. Author the JSON golden under
   `crates/octo-wallet/tests/fixtures/capability-zk/zk-<name>.json`. Use
   existing vectors as templates; deterministic hex literals (no random).
2. Add the corresponding test in
   `crates/octo-wallet/tests/zk_vectors.rs`. Use the existing
   `mint_with_stub_proof` helper for canonical mint → verify round-trips.
3. Run `cargo test -p octo-wallet --test zk_vectors`. The new vector must
   pass on first attempt — if it doesn't, fix the implementation, not the
   fixture.

### Bumping MSRV

1. Edit `crates/zk-vendor/rust-toolchain.toml` (cipherocto MSRV) or
   `crates/zk-vendor/stwo-sys/rust-toolchain.toml` (FFI cdylib).
2. Run `cargo +<toolchain> build --workspace --features full`.
3. Run `cargo +<toolchain> clippy --workspace --all-targets --features full -- -D warnings`.
4. Update `docs/BLUEPRINT.md` §RFC Process MSRV section.

### Bumping STWO upstream

1. Edit `crates/zk-vendor/stwo-sys/Cargo.toml::stwo` pin.
2. Rebuild the cdylib: `cd crates/zk-vendor/stwo-sys && cargo +nightly-2025-06-23 build --release`.
3. Run `cargo test -p zk-vendor --test ffi_loading -- --include-ignored --nocapture`.
   The version string must still contain `"real STWO"`; the verify error
   path must return a real `serde_json` parse failure (not a stub).
4. Run all integration tests to confirm no semantic divergence.

## Known gotchas

1. **Stub proofer 32-byte commitment shape:** the mock batch proofer emits
   a 32-byte BLAKE3 commitment, NOT a real STARK. Use `mint_with_stub_proof`
   helper (single-signer batch path) when the verifier needs ≥32-byte
   `stark_proof`; the legacy `mint_with_zk` (empty signers) emits
   empty `stark_proof` which the stub verifier rejects with
   `StwoVerifyError("malformed proof bundle: proof bytes must be >=32")`.
2. **Verifier time alignment:** when constructing `verify_capability_zk`
   tests, set `verifier_local_unix_time == proof.public_inputs.current_unix_time`
   so the stub BLAKE3 commitment matches. Drift within `MAX_SKEW_SECS` is
   fine in production but trips the stub path because the canonical
   commitment folds verifier time.
3. **`CapsmHashMismatch` error variant name on the mint side is
   `ZkMintError::CasmHashMismatch` (not `VerifyError`); on the verify
   side it's `ZkVerifyError::CasmHashMismatch`. Two distinct enums — match
   the right one per call site.
4. **`missing_at_align!` cargo-fuzz invocation:** the `cargo fuzz` binary
   is not pre-installed in CI; the `fuzz-nightly` job runs
   `cargo install cargo-fuzz` before `cargo fuzz run`.
5. **Workspace duplicate warning:** the cipherocto workspace prints
   `warning: skipping duplicate package octo-determin` on every build.
   This is a benign duplicate `Cargo.toml` location (workspace root +
   `determin/` subdirectory). Ignored — does NOT affect correctness.

## Cross-mission dependencies

- **Depends on:** RFC-0957 (Capability Token Format) — substrate;
  RFC-0630 (Proof-of-Inference Consensus) — `ExecutionTrace` type;
  RFC-0009 (Identity Management) — DID + NodeType;
  RFC-0102 (Wallet Cryptography) — wallet substrate;
  RFC-0853 (Overlay Cryptography) — BLAKE3 primitive.
- **Optional consumers:** mission `0957-b` (provider-boundary exercise path)
  may consume ZK-bearing caps in optional ZK flag (Phase F extension; not
  required for S04 acceptance); sibling mission `zk-proof-verification.md`
  shares `crates/zk-vendor/` STWO substrate.
- **Non-overlapping:** Phase C (mission `0959-a-ask-pricing-stoolap`) and
  Phase F (mission `0957-b-provider-boundary-exercise-path`) per master
  plan §8 Risk #10 R12 mitigation. The cipherocto-side PR for 0958-a is
  the only cipherocto-side PR for this mission; the stoolap fork PR
  (AC-16) was CANCELED in v0.3.
