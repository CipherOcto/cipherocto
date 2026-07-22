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

- [ ] **AC-1:** `cairo/capability_zk.cairo` authored + compiles via `cairo-compile >=2.6.0` (cipherocto workspace)
- [ ] **AC-2:** CASM hash matches check-in (snapshot test); `cargo test -p zk-circuit --lib` passes with real CASM (no stub bytes); `bundled_casm_hash()` in `octo-wallet` calls `zk_circuit::compile()`
- [ ] **AC-3:** `crates/zk-vendor/stwo/` builds on stable rust (no `+nightly`); MSRV pinned in `crates/zk-vendor/rust-toolchain.toml`; `zk-verifier::verify_capability_zk` delegates to vendored STWO
- [ ] **AC-4:** `verify_capability_zk` accepts RFC-0958 §Test Vectors TV1 (SelfHost + inference trace) and TV2 (Hybrid + no trace) for ≥2 independent prover implementations (cross-impl verification); zk_vectors.rs 8/8 tests green
- [ ] **AC-5:** Wholesale mint attempt returns `NodeTypeCannotMintZKCap` 100% of time; CI lint forbids `mint_with_zk` calls in `NodeType::Wholesale` code paths
- [ ] **AC-6:** Self-host mint requires `inference_trace` in witness; integration test asserts error `MissingInferenceTrace` if absent (was `MissingOutputHash` in v1.2 M5 rename)
- [ ] **AC-7:** Hybrid mint opt-in works (explicit `mint_with_zk()` call); test asserts Hybrid without explicit call returns v1 token (no ZK)
- [ ] **AC-8:** Wire format v2 parses correctly: v1 verifiers ignore 4th segment (forward-compat); v2 verifiers extract `proof_bundle_borsh` and verify
- [ ] **AC-9:** PublicInputMismatch detected; integration test corrupts `ask_id` and asserts `ZkVerifyError::PublicInputMismatch`
- [ ] **AC-10:** CASM drift detected; integration test mutates compiled CASM hash and asserts `ZkVerifyError::CasmHashMismatch` at mint AND verify
- [ ] **AC-11:** Proof gen latency <2s for SelfHost (10K trace steps reference HW); verify latency <100ms (per RFC-0958 §Performance Targets G1 + G2)
- [ ] **AC-12:** Proof size 50-500KB (measured against fixture set)
- [ ] **AC-13:** Fuzz `capability_zk_verify` 24h no crash (cargo-fuzz nightly job)
- [ ] **AC-14:** `cargo clippy --workspace --all-targets --features full -- -D warnings` clean (NOT `--all-features` per RFC-0917 mutex on `litellm-mode`/`any-llm-mode`)
- [ ] **AC-15:** Master plan Exit Criteria checkpoint: Phase B.2 (CASM production) + Phase C.2 (STWO stable-rust vendoring) green in workspace crates; **non-overlapping with Phase C** (S03 owns) per master plan §8 Risk #10 R12 mitigation; **no cross-repo PR**
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