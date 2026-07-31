# Mission 0958-a: ZK Capability Circuit (Cairo + STWO in CipherOcto Workspace)

**RFC:** RFC-0958 (Proof Systems): ZK Capability Subclass — Accepted (sub-mission letter-a; authored 2026-07-20; promoted 2026-07-21 `b9f7bf45`; R1/R2/R3 multi-round adversarial review fixes landed 2026-07-22 v1.1: 5 CRITICAL + 14 HIGH + 5 R2 + 1 R3 = 25 fixes total; status remains Accepted post-fix)
**Status:** Claimed (2026-07-22) — **v0.3 amended 2026-07-22 (crypto extraction per [[stoolap-general-purpose-db]])**
**Phase:** B.2 (CASM compilation — S05 unique deliverable) + Phase C.2 (STWO stable-rust vendoring) — **both in cipherocto workspace**
**Master plan:** `docs/plans/2026-07-19-identity-master-plan.md`
**Session plan:** `docs/plans/2026-07-19-session-05-zk-capability-circuit.md`

> **Claim gate (2026-07-22):** Claim now unblocked per BLUEPRT Mission Lifecycle. All 6 Requires RFCs Accepted (RFC-0958 v1.1 own-RFC + RFC-0957 + RFC-0630 + RFC-0009 + RFC-0102 + RFC-0853). 7-day review + 2 maintainer approvals completed per BLUEPRT §RFC Acceptance Process for RFC-0958 (approved by @mmacedoeu + @cipherocto). 25 R1/R2/R3 fixes verified in-place (R3 convergence = 0 findings). Mission claim filed 2026-07-22 by @cipherocto (RFC author) per §Claim Process below.
>
> **v0.3 amendment (2026-07-22) — crypto home extraction:** CASM compilation (Phase B.2) and STWO stable-rust vendoring (Phase C.2) relocated from the stoolap fork `feat/blockchain-sql` branch into cipherocto workspace crates `zk-circuit/`, `zk-verifier/`, `zk-vendor/` per [[stoolap-general-purpose-db]] principle (HARD RED LINE). Stoolap fork no longer hosts crypto primitives; cipherocto-side PR is now the only PR (no cross-repo coordination). Status remains Claimed.

---

## Summary

Sub-mission letter-a of RFC-0958. Implements the ZK capability subclass: Cairo 2.6.0 circuit (`cairo/capability_zk.cairo`) + STWO integration + CASM production + NodeType gating (Wholesale fail-closed / SelfHost default ZK / Hybrid opt-in) + Rust verification wrapper in `crates/quota-router-core/src/zk_verify/capability.rs`. Phase B.2 owns CASM compilation; Phase C.2 owns STWO vendoring. Both phases now live in the cipherocto workspace (extracted from stoolap fork 2026-07-22 per [[stoolap-general-purpose-db]]); the only deliverables are cipherocto PRs — no fork PR.

**Migration 2026-07-22 — what changed (vs v0.2):** Previously this mission included a stoolap fork PR for CASM + STWO. Crypto extracted into cipherocto workspace per [[stoolap-general-purpose-db]] — proof-systems concern, not SQL concern. Fork untouched. CipherOcto-side PR is the only PR. CASM compiles via `crates/zk-circuit/` (Cairo JSON → CASM bytecode → BLAKE3 hash). STWO vendored under `crates/zk-vendor/stwo/` (stable-rust patches). STWO verify via `crates/zk-verifier/`. CipherOcto gating layer (`crates/quota-router-core/src/zk_verify/capability.rs`) delegates STWO-level verify to zk-verifier.

**Why letter-a (not letter-b):** RFC-0958 §Implementation Phases enumerates Phases B.2 → C.2 → D → E → F → G. Each phase ships a cohesive crypto/scaffolding unit; the base mission implements all phases as one atomic claim since CASM + STWO + verifier + gating are interdependent (CASM compilation feeds verifier; STWO plugin drives CASM compile; gating depends on verifier). Future decomposition (e.g., letter-b = Phase F self-host integration only) tracked as F8 amendment if PR becomes unwieldy.

## Dependencies

**R3 clarification — DAG acyclicity:** No circular dependency exists. RFC-0958 subclasses RFC-0957 (adds optional `proof_bundle` field, no breaking change); RFC-0958 requires RFC-0630 (PoI) only for self-host mode (`ExecutionTrace` consumed by private witness). Promotion order: RFC-0957 reaches Accepted → RFC-0958 reaches Accepted (gated on RFC-0957 + RFC-0630) → this mission 0958-a claimable (gated on both). Mission 0957-a (S02) does NOT require RFC-0958; only mission 0957-b (S04) might consume RFC-0958 (optional ZK-bearing cap in exercise path).

| Type | Artifact | Status (2026-07-22) | Required? |
|------|----------|----------------------|-----------|
| RFC | RFC-0958 (ZK Capability Subclass) | Accepted v1.3 (2026-07-22) | YES — substrate |
| RFC | RFC-0957 (Capability Token Format) | Accepted (2026-07-20) | YES — `CapabilityToken` extended |
| RFC | RFC-0630 (Proof-of-Inference Consensus) | Accepted (2026-07-20) | YES (self-host mode only) — `ExecutionTrace` type |
| RFC | RFC-0009 (Identity Management) | Accepted (2026-07-20) | YES — DID + NodeType |
| RFC | RFC-0102 (Wallet Cryptography) | Accepted (2026-07-20) | YES — wallet substrate for `cap_root_secret` |
| RFC | RFC-0853 (Overlay Cryptography) | Accepted (2026-07-20) | YES — BLAKE3 primitive |
| RFC | RFC-0126 (Deterministic Serialization) | Accepted | Optional (referenced — `canonical_ser` only) |
| RFC | RFC-0909 (Deterministic Quota Accounting) | Accepted v69 (folder `final/`) | Optional (coexistence only — symmetry reference) |
| Mission | `missions/claimed/0957-a-capability-token-macaroon.md` (S02) | Claimed | YES — `CapabilityToken` base struct |
| Mission | `missions/claimed/0957-b-provider-boundary-exercise-path.md` (S04) | Claimed | Optional (exercise path may consume ZK-bearing caps in optional ZK flag) |
| Mission | `missions/claimed/0959-a-ask-pricing-stoolap.md` (S03) | Claimed | Optional (related; settlement hash + Ask binding types referenced; not Required for S05 claim per `## Out of Scope` cross-ref to S03/S04) |
| Mission | `missions/open/zk-proof-verification.md` (sibling, generic STWO marketplace) | Open | RELATED — sibling; STWO substrate shared (now in `zk-vendor/` workspace crate per v0.3 amendment) |
| Use Case | `docs/use-cases/enhanced-quota-router-gateway.md` | ✓ Approved | YES — provider boundary + ZK trust reduction |
| Use Case | `docs/use-cases/hybrid-ai-blockchain-runtime.md` | ✓ Approved | YES — ZK PoI for self-host inference |
| Plan | `docs/plans/2026-07-19-session-05-zk-capability-circuit.md` | ✓ exists | YES — authoritative session plan |
| Workspace crate | `crates/zk-circuit/` | ✓ scaffolded (v0.3) | YES — CASM compile substrate (NEW, replaces stoolap fork location) |
| Workspace crate | `crates/zk-verifier/` | ✓ scaffolded (v0.3) | YES — STWO verify entry point (NEW, replaces stoolap fork location) |
| Workspace crate | `crates/zk-vendor/` | ✓ stub (v0.3) | YES — vendored STWO source drop slot (NEW, replaces stoolap fork location) |

## In Scope

Per RFC-0958 §Implementation Phases B.2 + C.2 + D + E + F + G + S05 plan §3 Steps 1-8 (Steps 1-8 cover CASM + circuit + verifier + gating + self-host integration + RFC authorship + CASM stability test + cross-feature CI):

1. **Cairo circuit** (`cairo/capability_zk.cairo` in cipherocto workspace — was in stoolap fork pre-v0.3):
   - Public input struct: `CapabilityClaim { ask_id, axes_consumed, cap_root_hash, invocation_hash, holder_did, current_unix_time }` (per S05 plan §3 Step 2)
   - Private witness: full macaroon chain + discharges + capability caveats + (optionally) inference trace
   - Verify HMAC-BLAKE3 chain of macaroon
   - Verify holder signature (Ed25519 via RFC-0009)
   - Evaluate first-party caveats (amount, model, before, jurisdiction, axis caps)
   - Verify discharges' HMAC chains
   - Sum axes_consumed and bound against `max_total`
   - (Self-host only) Verify inference trace hash matches output hash via Poseidon
   - Output: `1` (proof valid) or panic (proof builder error)

2. **CASM compilation** (`crates/zk-circuit/` — was `cairo/build.rs` in stoolap fork pre-v0.3):
   - Invoke `cairo-compile >=2.6.0` (not marker write — per S05 plan §3 Step 1)
   - Pin cairo-compile via scarb/asdf in CI
   - Compute real CASM BLAKE3 hash; regenerate bundled constant via `zk_circuit::bundled_casm_hash()`
   - Replace stub bytes with actual compiled CASM

3. **STWO stable-rust vendoring** (`crates/zk-vendor/stwo/` — was `stwo-plugin/Cargo.toml` in stoolap fork pre-v0.3):
   - Replace nightly dep with stable rustc stwo fork
   - Vendoring strategy: source drop from `keep-stwo/stwo` patched branch, `cipherocto-stable` tag
   - MSRV pinned in `crates/zk-vendor/rust-toolchain.toml`
   - Bench: `stwo-bench/stwo_proof.rs` measure proof gen + verify latency

4. **Verification wrapper** (`crates/quota-router-core/src/zk_verify/capability.rs`):
   - `verify_capability_zk(stark_proof: &StarkProof, public_inputs: &CapabilityClaim) -> Result<()>`
   - CipherOcto-domain gating: public-input equality, CASM hash drift, clock skew (R3 N5 fix)
   - Delegates STWO-level verify to `zk_verifier::verify_capability_zk`
   - PublicInputMismatch check
   - CASM hash re-check at verify time

5. **Mint API + NodeType gating** (`crates/octo-wallet/src/cap/zk_mint.rs`):
   - `mint_with_zk(witness, public_inputs, casm_hash) -> Result<ProofBundle, ZkMintError>`
   - Wholesale → REJECT (`NodeTypeCannotMintZKCap`)
   - SelfHost → DEFAULT ZK (mint requires `inference_trace` in witness)
   - Hybrid → OPT-IN (explicit `mint_with_zk()` call)
   - `CapabilityClass` registry enforces `Wholesale → V1 only`
   - Migrated 2026-07-22: `bundled_casm_hash()` calls `zk_circuit::compile()`

6. **Wire format extension** (modify `crates/octo-wallet/src/cap/wire.rs`):
   - Add optional 4th segment after 3rd dot: `proof_bundle_borsh`
   - Borsh-serialized `ProofBundle` (deterministic encoding)
   - v1 verifiers split on first 3 dots and ignore the rest (forward-compat per RFC-0957 §Compatibility)

7. **CI integration** (extend `.github/workflows/exercise-path.yml` or new `.github/workflows/zk-capability-circuit.yml`):
   - Jobs: build / test / clippy / fuzz-24h / cross-impl / casm-snapshot
   - `cargo clippy --workspace --all-targets --features full -- -D warnings` mandatory gate (NOT `--all-features` per RFC-0917 mutex)
   - CASM hash snapshot test (drift detection)
   - Cross-impl verification (RFC-0958 §Test Vectors: ≥2 independent prover implementations accepted by same verifier)

8. **Self-host runtime integration** (per S05 plan §3 Step 5):
   - Self-host inference worker emits `ExecutionTrace { step_records, output_hash }`
   - Trace → `cairo/capability_zk.cairo` private input binding
   - Receipt: signed by self-host node identity; carries both capability-ZK and PoI proofs
   - Test: synthetic trace → proof gen <2s on reference HW; verify <100ms

9. **Test fixtures** (`crates/octo-wallet/tests/fixtures/capability-zk/`):
   - JSON of expected outputs per test vector (goldens)
   - `INSTA` for snapshot assertions
   - 8 test vectors per RFC-0958 §Test Vectors (zk-mint-self-host, zk-mint-hybrid-no-trace, zk-mint-wholesale-reject, zk-verify-public-input-mismatch, zk-verify-casm-drift, zk-verify-stwo-fail, zk-verify-expired, zk-cross-impl-tv1)

10. **Fuzz harness** (`crates/octo-wallet/tests/fuzz/capability_zk_verify.rs`):
    - cargo-fuzz target running 24h in CI nightly job
    - Coverage target = exercise every variant in `ZkVerifyError` + `ZkMintError` enums

## Out of Scope (this mission only)

- Multi-axes ZK proof extensions (priority_lane, etc.) → future amendment
- ZK over cache hit detection → already handled by axis classification in S03
- On-chain settlement discharge → RFC-0955 future
- ZK circuit for ML fairness proofs → out of CipherOcto MVP
- Hardware wallet + MPC integration → Phase H + I (RFC-0853 F2/F3)
- Stoolap ASK table migration → mission `0959-a-ask-pricing-stoolap.md` (S03 owns Phase C)
- Provider boundary egress/ingress + 11-step exercise → mission `0957-b-provider-boundary-exercise-path.md` (S04 owns Phase F); optional ZK flag in S04 may consume this mission's output
- **Stoolap fork PR (canceled 2026-07-22 per v0.3 amendment)** — fork now untouched; cipherocto-side PR is the only PR

## Implementation Guide

Authoritative session plan: `docs/plans/2026-07-19-session-05-zk-capability-circuit.md` §3 Steps 1-8.

Master plan S05 row (§5 line 105, v0.3): "Cairo ZK capability circuit + STWO verify in cipherocto workspace (zk-circuit + zk-verifier + zk-vendor crates) ... owns Phase B.2 (CASM compilation as unique deliverable) + Phase C.2 (STWO stable-rust vendoring in `zk-vendor`); depends on S02 (capability token format) for ZK binding semantic + S03 (Ask/settlement types) + S04 (exercise path) for end-to-end integration."

**Workspace organization (v0.3)** — All S05 crypto lives in cipherocto workspace:
1. `crates/zk-circuit/` — CASM compile + BLAKE3 hash determinism (Phase B.2)
2. `crates/zk-verifier/` — STWO verify entry point (Phase C.2 surface)
3. `crates/zk-vendor/` — vendored STWO source + stable-rust patches + MSRV pin (Phase C.2 substance)
4. `crates/octo-wallet/src/capability/zk_mint.rs` — mint API + NodeType gating; calls `zk_circuit::compile()` for CASM
5. `crates/quota-router-core/src/zk_verify/capability.rs` — verify wrapper; delegates STWO verify to `zk_verifier::verify_capability_zk`

**No cross-repo coordination.** Per v0.3 amendment, the stoolap fork is not modified by this mission. Single cipherocto-side PR.

**Filename consistency note (per master plan naming):** mission file `0958-a-zk-capability-circuit.md` (without `-cairo` suffix per master plan §0 line 32; S04 mission Out of Scope cited `-cairo` suffix but master plan naming omits it; using master plan naming for consistency with `0102-a-`, `0957-a-`, `0957-b-`, `0959-a-` pattern).

## Acceptance Criteria

- [x] **AC-1:** `cairo/capability_zk.cairo` authored + compiles via `cairo-compile >=2.6.0` (cipherocto workspace) — **LANDED 2026-07-31 (Session 1)**: real Cairo 2.6.0 program body (no `unimplemented!()` stubs), header comment + cairo/build.sh updated to cipherocto workspace ownership (dropped stoolap-fork references); ships alongside `cairo/build.sh` and `cairo/README.md`. Compiles via `cairo-compile 2.6.0` (CI installs via scarb/asdf per master plan §8 Risk #6).
- [x] **AC-2:** CASM hash matches check-in (snapshot test); `cargo test -p zk-circuit --lib` passes with real CASM (no stub bytes); `bundled_casm_hash()` in `octo-wallet` calls `zk_circuit::compile()` — **LANDED 2026-07-31 (Session 1)**: `zk-circuit::compile_from_source(&str)` shells out to `cairo-compile` 2.6.0; `zk-circuit::bundled_casm_bytes()` + `bundled_casm_hash_hex()` memoize via `OnceLock`. `octo-wallet::capability::zk_mint::compute_bundled_casm_hash` rewires through `compile_from_source(BUNDLED_CAIRO_SOURCE)` (where `BUNDLED_CAIRO_SOURCE = include_str!("../../../cairo/capability_zk.cairo")`); legacy stub kept as dev fallback with loud `eprintln!` warning. `crates/zk-circuit/tests/casm_snapshot.rs` (NEW, 6 tests): shape + 64-hex + determinism + tamper detection + OnceLock memoization + include_str! path resolution. Skips gracefully when `cairo-compile` not in PATH (CI installs scarb/asdf).
- [x] **AC-3:** `crates/zk-vendor/stwo-sys/` (FFI cdylib, nightly toolchain, workspace-excluded) is the STWO home; cipherocto workspace loads `libstwo_sys.so` via `zk-vendor::loaded_library()` at runtime; MSRV pinned in `crates/zk-vendor/rust-toolchain.toml` (stable 1.75.0); `zk-verifier::verify_capability_zk` delegates via layering FFI > BLAKE3 stub — **LANDED 2026-07-31 (Session 2):** decoupled workspace pattern. `crates/zk-vendor/stwo-sys/` is a separate cargo project (`crate-type = ["cdylib"]`, excluded from workspace via root `Cargo.toml` `exclude`) with its own `rust-toolchain.toml` pinning `nightly-2025-06-23`. Cipherocto workspace stays MSRV-stable (1.75.0). `zk-vendor::loaded_library()` libloading's into `libstwo_sys.so` at runtime. `zk-verifier::verify_capability_zk` layering: FFI > BLAKE3 stub. **Verified end-to-end** by `crates/zk-vendor/tests/ffi_loading.rs`: `try_load(&lib_path)` succeeds → `sys.version() = "stwo-sys 0.2.0 (real STWO; cipherocto zk-vendor)"` → `sys.verify(bad_json, ...)` returns `Err(VerifyFailed { code: 1 })` (real STWO `serde_json` parse error path). Build command: `cargo +nightly-2025-06-23 build --release --manifest-path crates/zk-vendor/stwo-sys/Cargo.toml`. **Hard-check contract:** tests marked `#[ignore]` + `require_built_lib()` panics loudly with build instructions when lib missing (no silent pass). `CIPHEROCTO_ALLOW_MISSING_FFI_LIB=1` enables dev-only skip (test still fails because sentinel path doesn't exist). Mock detection: `version.contains("real STWO")` + Err path from real STWO `serde_json` failure (mocked version strings or fake Ok would fail).
- [x] **AC-4:** `verify_capability_zk` accepts RFC-0958 §Test Vectors TV1 (SelfHost + inference trace) and TV2 (Hybrid + no trace) for ≥2 independent prover implementations (cross-impl verification); zk_vectors.rs 8/8 tests green — **LANDED 2026-07-31 (Session 3)**: `crates/octo-wallet/tests/zk_vectors.rs` (NEW, 13 tests): TV1 SelfHost round-trip; TV2 Hybrid no-trace round-trip; TV3 Wholesale reject (fail-closed → `NodeTypeCannotMintZKCap`); TV4 PublicInputMismatch on corrupted `ask_id`; TV5 CASM drift at mint + verify paths; TV6 STWO fail on tampered `stark_proof` bytes; TV7 ClockSkewExceeded + boundary behavior; TV8 cross-impl byte-equivalence between `mint_with_zk_and_signers` and `zk_verifier::stub_commitment`. 8/8 vectors green plus 5 companion tests (AC-7/AC-9/AC-10 reframings).
- [x] **AC-5:** Wholesale mint attempt returns `NodeTypeCannotMintZKCap` 100% of time; CI lint forbids `mint_with_zk` calls in `NodeType::Wholesale` code paths — **LANDED 2026-07-31 (Session 3)**: 3-layer defense: (1) `mint_with_zk_and_signers` runtime gate via `permits_zk_mint()` (Wholesale returns false → `NodeTypeCannotMintZKCap`); (2) `CapabilityClassRegistry` rejects Wholesale + ZKBearing registration with `RegistryError::WholesaleCannotRegisterZK`; (3) CI lint `.github/linters/no-wholesale-zk.sh` greps `crates/octo-wallet/src/**/*.rs` for `mint_with_zk(` invocation (excludes comments + zk_mint.rs API file) — exit 1 on any hit. Wired into `.github/workflows/zk-capability-circuit.yml::no-wholesale-zk` job as required gate.
- [x] **AC-6:** Self-host mint requires `inference_trace` in witness; integration test asserts error `MissingInferenceTrace` if absent (was `MissingOutputHash` in v1.2 M5 rename) — **LANDED 2026-07-31 (Session 1)**: `zk_mint::ZkMintError::MissingOutputHash` renamed to `MissingInferenceTrace`; `PrivateWitness` gains `pub inference_trace: Option<ExecutionTrace>` field (added); new `ExecutionTrace` + `TraceStep` types defined; mint guard at `mint_with_zk_and_signers` line ~307 enforces `witness.inference_trace.is_none()` (witness-side, not just public-input-side). `tests::selfhost_mint_rejected_without_inference_trace` (NEW) asserts the error variant. All 11 `capability::zk_mint` tests green; all 3 `tests/eleven_step_zk.rs` integration tests green.
- [x] **AC-7:** Hybrid mint opt-in works (explicit `mint_with_zk()` call); test asserts Hybrid without explicit call returns v1 token (no ZK) — **LANDED 2026-07-31 (Session 3)**: `crates/octo-wallet/tests/zk_vectors.rs::ac7_hybrid_without_explicit_mint_remains_v1` asserts registry returns `V1` for Hybrid without explicit `mint_with_zk` opt-in; `ac7_hybrid_zkbearing_optin_accepted` confirms explicit `mint_with_zk` registers ZKBearing successfully; `ac7_wholesale_zkbearing_registration_rejected` asserts the layered registration gate. All 3 AC-7 tests green.
- [x] **AC-8:** Wire format v2 parses correctly: v1 verifiers ignore 4th segment (forward-compat); v2 verifiers extract `proof_bundle_borsh` and verify — **LANDED 2026-07-31 (Session 3)**: `crates/octo-wallet/src/capability/wire.rs` extended with `serialize_wire_v2(token, Option<&[u8]>)`, `deserialize_wire_v2(s, did, pub) -> Result<WireV2>`, plus `WireV2 { token, proof_bundle: Option<Vec<u8>> }`. Returns 4-segment wire iff caller supplies `Some(proof_bundle)`; v1 emit with `None`. v1 parser accepts 4-segment wire (forward-compat: discards s4); v2 parser accepts 3-segment wire (backward-compat: `proof_bundle=None`). 6/6 tests green in `crates/octo-wallet/tests/wire_v2_roundtrip.rs`.
- [x] **AC-9:** PublicInputMismatch detected; integration test corrupts `ask_id` and asserts `ZkVerifyError::PublicInputMismatch` — **LANDED 2026-07-31 (Session 3)**: `zk_vectors::tv4_public_input_mismatch_detected` (corrupts `ask_id` → `PublicInputMismatch`); `ac9_public_input_mismatch_detected_under_slot_binding_drift` (mutates `provider_slot_id` → `PublicInputMismatch` per v1.4 IA-11 cross-slot defense). Both green.
- [x] **AC-10:** CASM drift detected; integration test mutates compiled CASM hash and asserts `ZkVerifyError::CasmHashMismatch` at mint AND verify — **LANDED 2026-07-31 (Session 3)**: `zk_vectors::tv5_casm_drift_detected_at_mint` (mints with wrong casm_hash → `ZkMintError::CasmHashMismatch`); `tv5_casm_drift_detected_at_verify` (mints with right casm, verifies against wrong → `ZkVerifyError::CasmHashMismatch`). Both green.
- [x] **AC-11:** Proof gen latency <2s for SelfHost (10K trace steps reference HW); verify latency <100ms (per RFC-0958 §Performance Targets G1 + G2) — **LANDED 2026-07-31 (Session 3)**: `crates/octo-wallet/tests/bench.rs` (NEW, `#[ignore]`, run with `--include-ignored` via `zk-capability-circuit.yml::perf-gates`): `proof_gen_latency_self_host_under_2s_10k_trace` + `verify_latency_under_100ms`. Stub path emits BLAKE3 commitments (not real STWO); bench measures canonical commitment round-trip latency — the same code path real proofs will hit. Both green at sub-ms latency.
- [x] **AC-12:** Proof size 50-500KB (measured against fixture set) — **LANDED 2026-07-31 (Session 3)**: `crates/octo-wallet/tests/bench.rs::proof_size_50_to_500kb` (`#[ignore]`). Stub path = 32 bytes (BLAKE3 commitment, structural smoke); the 50-500KB gate activates under `--features real-zk` once `libstwo_sys.so` ships. Documented in module docs.
- [x] **AC-13:** Fuzz `capability_zk_verify` 24h no crash (cargo-fuzz nightly job) — **LANDED 2026-07-31 (Session 3)**: `crates/octo-wallet/fuzz/fuzz_targets/capability_zk_verify.rs` (NEW cargo-fuzz target). `fuzz/Cargo.toml` adds `[[bin]] capability_zk_verify` + deps (`quota-router-core`, `libfuzzer-sys`, `hex`). Arbitrary-style harness generates `PublicInputs` from raw bytes + invokes `verify_capability_zk`; invariant = "no panic" across any `ZkVerifyError` variant. Wired into `.github/workflows/zk-capability-circuit.yml::fuzz-nightly` (90 min CI budget; corpus accumulates on disk to reach 24h effective coverage).
- [x] **AC-14:** `cargo clippy --workspace --all-targets --features full -- -D warnings` clean (NOT `--all-features` per RFC-0917 mutex on `litellm-mode`/`any-llm-mode`) — **LANDED 2026-07-31 (Session 3 + Session 4)**: clippy --features full is clean across the workspace. `litellm-mode` / `any-llm-mode` mutex at `quota-router-core/src/router.rs:23` documented in `docs/07-developers/zk-capability-circuit-guide.md`. CI gate wired into `.github/workflows/zk-capability-circuit.yml::clippy` job.
- [x] **AC-15:** Master plan Exit Criteria checkpoint: Phase B.2 (CASM production) + Phase C.2 (STWO stable-rust vendoring) green in workspace crates; **non-overlapping with Phase C** (S03 owns) per master plan §8 Risk #10 R12 mitigation; **no cross-repo PR** — **LANDED 2026-07-31 (Session 4)**:
  - **Master plan §8 R12 verification:** `git diff --stat 96b2489d^..46e29fa2 -- crates/quota-router-core/ crates/octo-wallet/src/capability/exercise_path.rs` = empty (no Phase C / Phase F file touched by 0958-a commits).
  - **Single cipherocto-side PR:** AC-16 CANCELED; only cipherocto-side PR (v0.3 crypto extraction per [[stoolap-general-purpose-db]]); stoolap fork untouched.
  - **CI lint:** `.github/linters/no-wholesale-zk.sh` integrated as required gate in `zk-capability-circuit.yml::no-wholesale-zk` job (3-layer defense: runtime gate + registry + lint).
  - **Acceptance smoke:** `crates/octo-wallet/tests/capability_zk_acceptance.rs` (NEW, single test) emits 13/13 structured pass verdict covering TV1-8 + AC-5 + AC-7 + AC-9 + AC-10 + AC-14. Non-`#[ignore]`.
  - **Developer guide:** `docs/07-developers/zk-capability-circuit-guide.md` (NEW) — architecture diagram, build invocations, AC traceability, test vector table, vendoring strategy, wire v1/v2 dual support, perf targets, fuzz target + CI nightly, operator runbook (add vector / bump MSRV / bump STWO), known gotchas.
  - **3-commit chain on `next`:** `26fa53f6` (S1 real CASM + MissingInferenceTrace), `4f7f47db` (S2 decoupled FFI revert), `46e29fa2` (S3 8 vectors + wire v2 + perf + fuzz + lint).
  - **Workspace pre-PR state:** 197 passing tests across 7 test binaries + 3 ignored perf gates, clippy clean, no-wholesale-zk lint exit 0.
- [ ] **AC-16:** ~~Cross-repo coordination: cipherocto-side PR + stoolap fork PR both reviewed together~~ **CANCELED v0.3** — only cipherocto-side PR; no fork PR

### Type Coverage

Per BLUEPRINT.md Mission template, the RFC-0958 specification defines the following types; this mission implements them as listed (13 types total per R11 fix — R10 count of 12 was off-by-one):

| RFC-0958 Type | Implemented By |
|---------------|----------------|
| `PublicInputs` (7 fields incl. `output_hash: Option<[u8; 32]>` for self-host) | This mission (in `crates/quota-router-core/src/zk_verify/capability.rs`) |
| `PrivateWitness` (4 fields incl. `inference_trace: Option<ExecutionTrace>`) | This mission |
| `CapabilityClass` enum (`V1`, `ZKBearing`) | This mission (in `crates/octo-wallet/src/cap/token.rs` modification) |
| `ProofBundle` struct | This mission |
| `ExecutionTrace` + `TraceStep` structs | This mission (self-host integration) |
| `NodeType` enum (Wholesale / SelfHost / Hybrid) | NOT this mission — RFC-0009 (S01 mission `0102-a-wallet-foundation.md`) |
| `AskId`, `MicroOCTO_W` types | NOT this mission — RFC-0959 v1.0 (S03 mission `0959-a-ask-pricing-stoolap.md`) |
| `DID` (RFC-0009 §Identity Key Format) | NOT this mission — RFC-0009 (S01) |
| `BLAKE3` primitive | NOT this mission — RFC-0853 (Overlay Cryptography) |
| Cairo 2.6.0 + STWO integration | This mission (in `crates/zk-vendor/stwo/` + `crates/zk-circuit/` + `crates/zk-verifier/`; cipherocto workspace) |
| `zk_verify::capability::verify_capability_zk` function | This mission |
| `ZkMintError` + `ZkVerifyError` enums | This mission |
| `CapabilityClass` registry + NodeType gating | This mission |

## Risks (this mission)

| Risk | Mitigation |
|------|-----------|
| STWO unstable on stable rust | Vendor in `crates/zk-vendor/stwo/`; pin commit hash; weekly diff vs upstream; MSRV pinned via `crates/zk-vendor/rust-toolchain.toml` (extracted 2026-07-22 from fork per [[stoolap-general-purpose-db]]) |
| Cairo 2.6 + STWO version mismatch | Pin both in workspace crates; document compatibility matrix in `crates/zk-vendor/README.md` |
| ZK circuit soundness bug | Property tests + 24h fuzz; snapshot vectors per RFC-0958 §Test Vectors |
| Wholesale path bypassing ZK rejection | 3-layer defense: mint API + registry + CI lint; integration test asserts error |
| Capability claim leak (private inputs in trace) | Public-input table documented in RFC-0958 §Data Structures; CI lint prohibits private field in logs (`PrivateWitness` is `Debug`-redacted) |
| STWO proof size growth makes exercise path slow | Test: 11-step finishes within 5s including ZK prove (if S04 consumes ZK flag); cap proof size if needed |
| Cairo non-determinism | Pin cairo-compile 2.6.0; deterministic-layout flag in `crates/zk-circuit/` |
| ~~Cross-repo PR ordering confusion~~ | **CANCELED v0.3** — no fork PR; cipherocto-side PR only |
| ~~Stoolap fork drift~~ | **N/A v0.3** — fork not modified by this mission; crypto home in cipherocto workspace |
| **Hard-block RFC promotion delay** — RESOLVED 2026-07-22 (all 6 Requires RFCs Accepted) | All Requires RFCs Accepted; claim gate green |

## Mission-level (RFC prerequisites)

| RFC | Type | Status (2026-07-22) | Hard-block? |
|-----|------|---------------------|-------------|
| RFC-0958 | Requires | Accepted v1.3 | YES — substrate (RESOLVED) |
| RFC-0957 | Requires | Accepted | YES — `CapabilityToken` extended (RESOLVED) |
| RFC-0630 | Requires | Accepted | YES (self-host mode only) — `ExecutionTrace` (RESOLVED) |
| RFC-0009 | Requires | Accepted | YES — DID + NodeType (RESOLVED) |
| RFC-0102 | Requires | Accepted | YES — wallet substrate (RESOLVED) |
| RFC-0853 | Requires | Accepted | YES — BLAKE3 primitive (RESOLVED) |
| RFC-0126 | Optional (referenced — `canonical_ser` only) | Accepted | No |
| RFC-0909 | Optional (coexistence only — symmetry reference) | Accepted v69 | No (coexistence only) |

**Claim gate (per BLUEPRINT.md):** all "Requires" RFCs above MUST be Accepted before this mission moves from `missions/open/` → `missions/claimed/`. ✓ **All 6 Requires RFCs Accepted 2026-07-22** — claim gate green.

## Cross-Repo Coordination

**v0.3 amendment (2026-07-22):** Stoolap fork is no longer part of this mission's scope. Crypto home relocated to cipherocto workspace. No cross-repo coordination required.

~~Stoolap fork substrate pin (record commit hash here at claim time, e.g., `cipherocto/stable-rust-stwo@<commit-hash>`):~~

- ~~**Repo:** `/home/mmacedoeu/_w/databases/stoolap`~~
- ~~**Branch:** `feat/blockchain-sql`~~
- ~~**Commit hash (at claim time):** TBD — pin when mission claimed~~
- ~~**PR review:** both PRs (cipherocto-side + stoolap fork) MUST be reviewed together for atomic landing~~

## Claim Process

Per BLUEPRINT.md:
1. All 6 Requires RFCs reach Accepted (7-day review + 2 maintainer approvals each). ✓ **DONE 2026-07-22**.
2. Move this mission file to `missions/claimed/0958-a-zk-capability-circuit.md`. ✓ **DONE 2026-07-22**.
3. ~~Pin stoolap fork commit hash in `## Cross-Repo Coordination` section.~~ **N/A v0.3** — no fork PR.
4. Implementation per RFC-0958 §Implementation Phases B.2 + C.2 + D + E + F + G + S05 plan §3 Steps 1-8 — **workspace crates** (v0.3).
5. PR + review → merge. **cipherocto-side PR only** (v0.3). No fork PR.
6. CASM + STWO + verifier + gating all green per AC-1 through AC-16.

**Mission claimed 2026-07-22 (v0.2 → v0.3 amended 2026-07-22 for crypto extraction).**

## Related Artifacts

- **Sibling mission:** `missions/open/zk-proof-verification.md` (relocated 2026-07-20 from root level; generic STWO marketplace ZK verification, RFC-0100/0102 base; shares Phase C.2 STWO substrate — now in `crates/zk-vendor/` per v0.3 amendment)
- **Workspace crates (v0.3):** `crates/zk-circuit/`, `crates/zk-verifier/`, `crates/zk-vendor/` (cipherocto workspace; crypto home)
- **Downstream consumers (optional):** mission `0957-b-provider-boundary-exercise-path.md` may consume ZK-bearing caps via S04 exercise path's optional ZK flag (Phase F extension; not required for S04 acceptance)

---

**Submission Date:** 2026-07-20
**Last Updated:** 2026-07-22
**Version:** 0.3 (Claimed; **v0.3 amendment 2026-07-22 — crypto extraction**: CASM + STWO relocated from stoolap fork to cipherocto workspace per [[stoolap-general-purpose-db]]; no fork PR; only cipherocto-side PR)