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
# Build workspace (Round 4 review of mission 0958-b F-25 removed
# --features full: the `full` cargo feature was DELETED in mission
# 0958-b S2 commit 77aff4aa in favor of runtime dispatch on
# zk_vendor::vendor_state(). Real-zk STWO is selected at runtime via
# libstwo_sys.so presence; the BLAKE3 stub activates under
# --features allow-stub-verifier for test compilation only.)
cargo build --workspace

# Lint (zero warnings required; --features full removed per R4 F-25)
cargo clippy --workspace --all-targets -- -D warnings

# Format
cargo fmt --all
```

**Important:** `--all-features` is NOT used because `litellm-mode` and
`any-llm-mode` are mutually exclusive (`compile_error!` at
`quota-router-core/src/router.rs`). The mode gate controls HOW (reqwest vs
PyO3), not WHETHER (RFC-0917 invariant). NO `--features full` flag exists
since mission 0958-b S2; feature coverage happens via per-crate
`[dev-dependencies]` opt-in (see `crates/octo-wallet/Cargo.toml` for the
`zk-verifier` allow-stub-verifier re-export per R1 F-1 closure).

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

### CASM compilation (Cairo 2.x via scarb)

**R4 fix-up (2026-08-04):** the previous guide documented Cairo 2.6.0
standalone `cairo-compile` invocations. The current implementation uses
**scarb 2.16.0** as the build orchestrator (Cairo 1.x standalone
`cairo-compile` does not exist post-Cairo-1.x; the binary was retired
when Cairo moved to the scarb project model in 2024). The in-process
Sierra→CASM pass lives in `crates/zk-circuit/` and is invoked
automatically by `bundled_casm_hash()` on first call.

```bash
# Install scarb 2.16.0 (CI installs via scarb/asdf per master plan §8 Risk #6)
curl --proto '=https' --tlsv1.2 -sSf https://docs.swmansion.com/scarb/install.sh | sh -s -- -v 2.16.0

# Verify scarb is on PATH
scarb --version   # expect: scarb 2.16.0

# Compile the bundled Cairo source → Sierra IR (in-process Sierra→CASM pass runs at runtime)
cd cairo && scarb build
# Output:
#   cairo/target/dev/capability_zk.sierra.json

# The CASM is produced in-process by `crates/zk-circuit/src/lib.rs::compile_source_inner`
# via `cairo-lang-sierra-to-casm` 2.20.0 + `cairo-lang-sierra-type-size` 2.20.0.
# The BLAKE3 hash of the assembled CASM bytecode is what `bundled_casm_hash()` returns.
# No manual CASM-file step is needed — the snapshot test
# (`crates/zk-circuit/tests/casm_snapshot.rs`) reads the in-process output.
```

The CASM BLAKE3 hash is checked into
`crates/zk-circuit/tests/casm_snapshot.rs` as a snapshot assertion. The
test computes BLAKE3(casm_bytes) at runtime and asserts non-empty +
64-hex format + determinism + tamper-detection. A pinned
`EXPECTED_CASM_BLAKE3_HASH` constant is NOT yet checked in — the test
currently passes by shape + determinism only. Drift detection requires
either committing the expected hash once the CI scarb 2.16.0
environment is reproducible, OR adding an
`#[ignore]` integration test that asserts the cross-build stability.

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
#           proof size 50-500KB (stub path = 32B structural smoke; full gate under feature)

# FFI libloading (#[ignore] → opt-in via --include-ignored)
cargo test -p zk-vendor --test ffi_loading -- --include-ignored --nocapture
# expected: 3 passed (libloading + symbol resolution + verify error path); version string contains "real STWO"

# Wholesale lint (fail-closed; MUST exit 0)
bash .github/linters/no-wholesale-zk.sh

# Verify the canonical clean-state build (R4 F-25: --features full removed)
cargo clippy --workspace --all-targets -- -D warnings
```

## AC evidence (mission 0958-b v0.5 — multi-round review convergence)

The AC table below covers `mission 0958-b`'s claim scope (Phase B.3 Cairo
cryptographic body + Phase C.3 real-zk STWO end-to-end). For the full
AC traceability including the prior `mission 0958-a` surface area work,
see `missions/claimed/0958-b-real-cairo-crypto.md` §Version History
v0.4/v0.5 rows + `missions/claimed/0958-a-zk-capability-circuit.md` §AC.

**Multi-round review status (mission 0958-b v0.5; 2026-08-05):**

| Round | Findings | Commit | Notes |
|---|---|---|---|
| S1 + S2 + S3 | (baseline crypto + real-zk STWO + stub retirement) | `aa004ad0` + `77aff4aa` + `81e2db4e` | Mission AC list original |
| R1 | 16 findings (14 closed) | `549c2cc2` | F-1 unconditional-feature closure most critical |
| R2 | 4 NEW MAJOR + MINOR | `e8a9ba5c` | F-17 phantom shas, F-18 core::blake3, F-19 zk_vectors breakdown, F-20 0958-c Version History |
| R3 | 3 NEW MAJOR + 1 NOTE | `9fef522f` | F-21 0958-c regression, F-22 cron drift, F-23 phantom SHA, F-24 compiler.rs line ref |
| R4 | 5 NEW + 2 NOTES | `63debd76` | F-25 phantom `--features full` in CI (CRITICAL), F-26 casm_split, F-27 nightly toolchain, F-28 AC-6 count, F-29 EOF newline |
| R5 | 1 R4 regression + 4 NEW | `75e7e1e4` | F-30 YAML colon-in-parens (CRITICAL), F-31 fuzz package ref, F-32 seed corpus claim, F-33 Version History v0.5 row, F-34 0958-c v0.2 row |
| R6 | 0 NEW (convergence verified) | `777f2f1e` | All R5 fixes stable; review chain closed |

**Stub fail-closed semantics:** `crates/octo-wallet/Cargo.toml` does NOT
activate `allow-stub-verifier` on the production `[dependencies]` edge.
Test-only stub mode activates through `[dev-dependencies]`, which sets
`features = ["allow-stub-verifier"]` on `zk-verifier` for test compilation
only (R1 F-1 closure; surviving R6 verification).


| **AC-1** | Real Cairo 2.x program body (no `unimplemented!()`) under scarb package `capability_zk` | `cairo/src/lib.cairo` exists + `scarb build` produces Sierra IR | `ae4dc4f8` (R4 S1 redo) |
| **AC-2** | `zk_circuit::compile_from_source` shells out to cairo-compile; CASM BLAKE3 snapshot | `tests/casm_snapshot.rs` (loud-fail when toolchain missing) | `26fa53f6` (S1); R3 #8 harden `e7c79b9b` |
| **AC-3** | Decoupled FFI workspace + BLAKE3 stub + MSRV 1.93 + stub-disabled gate | `tests/ffi_loading.rs` (hardened) + `tests/stub_disabled.rs` (2 tests) | `4f7f47db`, `be113cb1`, `96b2489d` (S2); R3 fix-ups `0e0c3ee9` (#1 prod-gate) + `066a263c` (#2 MSRV) + `27641ade` (#3 FFI arg order) |
| **AC-4** | 8 RFC-0958 §Test Vectors + cross-impl TV1/TV2 | `tests/zk_vectors.rs` (15 tests; 8/8 vectors green + 5 AC-7/9/10 companions + 2 R3) | `46e29fa2` (S3); R3 axes-canon `411334da`; R3 N=2 rotation `a4594be8` |
| **AC-5** | Wholesale mint fail-closed + CI lint + registry gate | `.github/linters/no-wholesale-zk.sh` + `zk-vectors.rs::tv3` + `ac7_wholesale_zkbearing_registration_rejected` | `46e29fa2` (S3); R3 wire DoS `4977a416` |
| **AC-6** | `MissingInferenceTrace` rename + witness-side guard | `tests/eleven_step_zk.rs` +20 LoC, mission cites `tests::selfhost_mint_rejected_without_inference_trace` | `26fa53f6` (S1) |
| **AC-7** | Hybrid opt-in semantics (V1 default + ZKBearing opt-in) | `tests/zk_vectors.rs::ac7_*` | `46e29fa2` (S3) |
| **AC-8** | Wire format v2 (4th segment = `proof_bundle_borsh`) + DoS guard | `tests/wire_v2_roundtrip.rs` (8 tests; 2 new DoS guards) | `46e29fa2` (S3); R3 wire DoS `4977a416` |
| **AC-9** | `PublicInputMismatch` + slot-binding drift | `tests/zk_vectors.rs::tv4 + ac9` | `46e29fa2` (S3) |
| **AC-10** | CASM drift at mint AND verify paths | `tests/zk_vectors.rs::tv5_*` | `46e29fa2` (S3) |
| **AC-11** | Proof gen <2s SelfHost 10K trace + verify <100ms | `tests/bench.rs` (3 #[ignore] gates; R3 audit fixed bench to batch path: `proof_size = 32 bytes`) | `46e29fa2` (S3); R3 audit fix-up 2026-07-31 |
| **AC-12** | Proof size 50-500KB (full feature gate) | `tests/bench.rs::proof_size_50_to_500kb` | `46e29fa2` (S3); R3 #4 declare `full` feature `2fb0a455` |
| **AC-13** | cargo-fuzz `capability_zk_verify` 24h nightly | `crates/octo-wallet/fuzz/fuzz_targets/capability_zk_verify.rs` + CI `fuzz-nightly` job (R3 #C3 fix `-p` flag → `octo-wallet-fuzz`) | `46e29fa2` (S3); R3 fix `4977a416` |
| **AC-14** | `cargo clippy --workspace --all-targets --features full -- -D warnings` clean | workspace lint | `46e29fa2` (S3); R3 dead-variant triage `b98801e0` |
| **AC-15** | Master plan §8 R12 exit criteria + single cipherocto-side PR + 8 R3 follow-ups | this doc + `git diff --stat` scope check + R3 fix-ups table | S4 closure + R3 #5 N=2 `a4594be8` + #7 dead variants `b98801e0` |
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
| **G1** (AC-11) | Proof gen <2s on 10K trace (SelfHost reference HW) | `tests/bench.rs::proof_gen_latency_self_host_under_2s_10k_trace` | sub-ms on stub; real-STWO gate under `--features full` |
| **G2** (AC-11) | Verify <100ms | `tests/bench.rs::verify_latency_under_100ms` | sub-ms on stub |
| **AC-12** | Proof size 50-500KB | `tests/bench.rs::proof_size_50_to_500kb` | stub = 32B structural smoke; 50-500KB gate under `--features full` |

**Reference HW:** 4-core x86_64, 16GB RAM, NVMe SSD (per RFC-0958 §Performance Targets). STWO build flags: STWO_CAIRO=1 STWO_USE_BUNDLED=0. OS: Ubuntu 22.04 LTS. Round 4 review F-56 closure: the prior "RFC-0958 §Implementation Reference" cross-reference was a phantom section (no such heading exists in RFC-0958); corrected to point at the actual `§Performance Targets` section.

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

### CASM Rotation (N=2 grace) — RFC-0958 §CASM Drift N=2 retention

**When does CASM rotate?** Only when `cairo/src/lib.cairo` changes in a way that affects proof binding (circuit structure, public input field, verification logic). Documentation-only changes do NOT trigger rotation.

**Operator runbook (per RFC-0958 §CASM Drift, mission 0958-a R3 #5 fix-up):**

1. **Compute new CASM BLAKE3.** Run `scarb build` in `cairo/` and read the in-process CASM BLAKE3 hash from `bundled_casm_hash()`. The hash is computed at runtime by `crates/zk-circuit/src/lib.rs::compile_source_inner`; no manual CASM file step is required.
   ```bash
   cd cairo && scarb build
   cargo test -p zk-circuit --test casm_snapshot
   # → assert_eq!(casm_hash, <EXPECTED_CASM_BLAKE3_HASH>)
   ```

2. **Deploy N=2 grace verifier config.** Update the verifier's `accepted_casm_blake3_hashes` config to include both old and new:
   ```rust
   verify_capability_zk(&proof, &expected_public_inputs, &[old_casm, new_casm], now);
   ```
   Proofs bound to EITHER hash verify for the duration of the grace period.

3. **Banner the grace period in release notes.** Operators see the N=2 acceptance in the dev guide + the verifier config.

4. **Reissue mints in flight** (operator-driven, not automatic). Capabilities minted under the old CASM continue to verify during grace; new mints automatically bind to the new CASM (since `bundle.casm_hash = bundled_casm_hash()` reads from the current compiled binary).

5. **After 7-day grace** (RFC-0958 default), remove the old hash from the verifier config:
   ```rust
   verify_capability_zk(&proof, &expected_public_inputs, &[new_casm], now);
   ```
   Capabilities minted under the old hash now return `CasmHashMismatch`.

**Test coverage:** `r3_casm_n2_rotation_accepts_either_v1_or_v2_hash` in `crates/octo-wallet/tests/zk_vectors.rs` exercises:
- v1 proof rejected when accepted-set = [v2]
- v2 proof NOT CasmHashMismatch when accepted-set = [v1, v2]
- empty accepted-set fails closed

**Migration runbook (future):** When `cairo/src/lib.cairo` changes:
1. Bump `casm_version` on each new mint (currently hardcoded to 1)
2. Track per-version CASM BLAKE3 separately
3. Cap-verifier accepts `[v_N_casm] + [v_{N-1}_casm]` for grace, then `[v_N_casm]` alone
4. Document the migration in this file (track all past CASM hashes)

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
4. **`cargo fuzz` invocation:** the `cargo fuzz` binary is not
   pre-installed in CI; the `fuzz-nightly` job runs
   `cargo install cargo-fuzz` before `cargo fuzz run`. Note: the
   fuzz package is `octo-wallet-fuzz`, so the correct invocation
   is `cargo fuzz run capability_zk_verify -p octo-wallet-fuzz`
   (NOT `-p octo-wallet`, which would fail with "package not a
   fuzz package"). See commit `46e29fa2` workaround fix in
   `.github/workflows/zk-capability-circuit.yml`.
5. **Workspace duplicate warning:** the cipherocto workspace prints
   `warning: skipping duplicate package octo-determin` on every build.
   This is a benign duplicate `Cargo.toml` location (workspace root +
   `determin/` subdirectory). Ignored — does NOT affect correctness.

## Cross-mission dependencies

- **Depends on:** RFC-0957 — substrate;
  RFC-0630 — `ExecutionTrace` type;
  RFC-0009 — DID + NodeType;
  RFC-0102 — wallet substrate;
  RFC-0853 — BLAKE3 primitive.
- **Optional consumers:** mission `0957-b` (provider-boundary exercise path)
  may consume ZK-bearing caps in optional ZK flag (Phase F extension; not
  required for S04 acceptance); sibling mission `zk-proof-verification.md`
  shares `crates/zk-vendor/` STWO substrate.
- **Non-overlapping:** Phase C (mission `0959-a-ask-pricing-stoolap`) and
  Phase F (mission `0957-b-provider-boundary-exercise-path`) per master
  plan §8 Risk #10 R12 mitigation. The cipherocto-side PR for 0958-a is
  the only cipherocto-side PR for this mission; the cross-repo
  stoolap fork PR (AC-16) was CANCELED in v0.3.
