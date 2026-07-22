# Mission 0958-a: ZK Capability Circuit (Cairo + STWO Production in Stoolap Fork)

**RFC:** RFC-0958 (Proof Systems): ZK Capability Subclass — Draft (sub-mission letter-a; authored 2026-07-20; promotion to Accepted pending 7-day review + 2 maintainer approvals)
**Status:** Open
**Phase:** B.2 (CASM compilation — S05 unique deliverable) + Phase C.2 (STWO plugin stable-rust vendoring)
**Master plan:** `docs/plans/2026-07-19-identity-master-plan.md`
**Session plan:** `docs/plans/2026-07-19-session-05-zk-capability-circuit.md`

> **Availability (initial authoring):** Mission Requires RFCs: RFC-0958 (Draft, authored 2026-07-20 — own RFC, not yet Accepted), RFC-0957 (ACCEPTED 2026-07-20), RFC-0630 (ACCEPTED 2026-07-20 — self-host PoI), RFC-0009 (ACCEPTED 2026-07-20), RFC-0102 (ACCEPTED 2026-07-20), RFC-0853 (ACCEPTED 2026-07-20). 5 of 6 Requires RFCs now Accepted; RFC-0958 self-promotion pending. Claim gate per BLUEPRT Mission Lifecycle: mission claim DEFERRED until RFC-0958 reaches Accepted (per BLUEPRT "Missions REQUIRE an approved RFC"). Implementation coverage (RFC-0958 §Specification + RFC-0958 §Data Structures + §Algorithms + §Test Vectors) ships the spec; the implement transition is held until RFC-0958 is Accepted.

---

## Summary

Sub-mission letter-a of RFC-0958. Implements the ZK capability subclass: Cairo 2.6.0 circuit (`cairo/capability_zk.cairo`) + STWO integration + CASM production + NodeType gating (Wholesale fail-closed / SelfHost default ZK / Hybrid opt-in) + Rust verification wrapper in `crates/quota-router-core/src/zk_verify/capability.rs`. Phase B.2 owns CASM compilation; Phase C.2 owns STWO plugin stable-rust vendoring. Both phases unique to S05 (non-overlapping with C = stoolap ASK table owned by S03, per master plan §8 Risk #10 R12 mitigation).

**Why letter-a (not letter-b):** RFC-0958 §Implementation Phases enumerates Phases B.2 → C.2 → D → E → F → G. Each phase ships a cohesive crypto/scaffolding unit; the base mission implements all phases as one atomic claim since CASM + STWO + verifier + gating are interdependent (CASM compilation feeds verifier; STWO plugin drives CASM compile; gating depends on verifier). Future decomposition (e.g., letter-b = Phase F self-host integration only) tracked as F8 amendment if PR becomes unwieldy.

## Dependencies

**R3 clarification — DAG acyclicity:** No circular dependency exists. RFC-0958 subclasses RFC-0957 (adds optional `proof_bundle` field, no breaking change); RFC-0958 requires RFC-0630 (PoI) only for self-host mode (`ExecutionTrace` consumed by private witness). Promotion order: RFC-0957 reaches Accepted → RFC-0958 reaches Accepted (gated on RFC-0957 + RFC-0630) → this mission 0958-a claimable (gated on both). Mission 0957-a (S02) does NOT require RFC-0958; only mission 0957-b (S04) might consume RFC-0958 (optional ZK-bearing cap in exercise path).

| Type | Artifact | Status (2026-07-20) | Required? |
|------|----------|----------------------|-----------|
| RFC | RFC-0958 (ZK Capability Subclass) | Draft (authored 2026-07-20) | YES — substrate |
| RFC | RFC-0957 (Capability Token Format) | Draft (authored 2026-07-19, S02) | YES — `CapabilityToken` extended |
| RFC | RFC-0630 (Proof-of-Inference Consensus) | Draft (authored 2026-07-20) | YES (self-host mode only) — `ExecutionTrace` type |
| RFC | RFC-0009 (Identity Management) | Draft (authored 2026-07-19, S01) | YES — DID + NodeType |
| RFC | RFC-0102 (Wallet Cryptography) | Draft (authored 2026-07-19, S01) | YES — wallet substrate for `cap_root_secret` |
| RFC | RFC-0853 (Overlay Cryptography) | Draft (authored 2026-07-19, S03 propagation) | YES — BLAKE3 primitive |
| RFC | RFC-0126 (Deterministic Serialization) | Accepted (v2.5.1, 2026-07-20 status check) | Optional (referenced — `canonical_ser` only) |
| RFC | RFC-0909 (Deterministic Quota Accounting) | Accepted (v69, 2026-07-20 status check; folder `final/`) | Optional (coexistence only — symmetry reference) |
| Mission | `missions/open/0957-a-capability-token-macaroon.md` (S02) | Open | YES — `CapabilityToken` base struct |
| Mission | `missions/open/0957-b-provider-boundary-exercise-path.md` (S04) | Open | Optional (exercise path may consume ZK-bearing caps in optional ZK flag) |
| Mission | `missions/open/0959-a-ask-pricing-stoolap.md` (S03) | Open | Optional (related; settlement hash + Ask binding types referenced; not Required for S05 claim per `## Out of Scope` cross-ref to S03/S04) |
| Mission | `missions/open/zk-proof-verification.md` (sibling, generic STWO marketplace) | Open (relocated to lifecycle dir 2026-07-20) | RELATED — sibling; STWO plugin substrate shared (Phase C.2) |
| Use Case | `docs/use-cases/enhanced-quota-router-gateway.md` | ✓ Approved | YES — provider boundary + ZK trust reduction |
| Use Case | `docs/use-cases/hybrid-ai-blockchain-runtime.md` | ✓ Approved | YES — ZK PoI for self-host inference |
| Plan | `docs/plans/2026-07-19-session-05-zk-capability-circuit.md` | ✓ exists | YES — authoritative session plan |
| External | `/home/mmacedoeu/_w/databases/stoolap` fork `feat/blockchain-sql` branch | active | YES — CASM + STWO substrate |

## In Scope

Per RFC-0958 §Implementation Phases B.2 + C.2 + D + E + F + G + S05 plan §3 Steps 1-8 (Steps 1-8 cover CASM + circuit + verifier + gating + self-host integration + RFC authorship + CASM stability test + cross-feature CI):

1. **Cairo circuit** (`cairo/capability_zk.cairo` in stoolap fork `feat/blockchain-sql` branch):
   - Public input struct: `CapabilityClaim { ask_id, axes_consumed, cap_root_hash, invocation_hash, holder_did, current_unix_time }` (per S05 plan §3 Step 2)
   - Private witness: full macaroon chain + discharges + capability caveats + (optionally) inference trace
   - Verify HMAC-BLAKE3 chain of macaroon
   - Verify holder signature (Ed25519 via RFC-0009)
   - Evaluate first-party caveats (amount, model, before, jurisdiction, axis caps)
   - Verify discharges' HMAC chains
   - Sum axes_consumed and bound against `max_total`
   - (Self-host only) Verify inference trace hash matches output hash via Poseidon
   - Output: `1` (proof valid) or panic (proof builder error)

2. **CASM compilation** (`cairo/build.rs` in stoolap fork):
   - Invoke `cairo-compile >=2.6.0` (not marker write — per S05 plan §3 Step 1)
   - Pin cairo-compile via scarb/asdf in CI
   - Compute real CASM BLAKE3 hash; regenerate `bundled.rs` constants
   - Replace stub bytes with actual compiled CASM

3. **STWO plugin stable-rust vendoring** (`stwo-plugin/Cargo.toml` in stoolap fork — Phase C.2):
   - Replace nightly dep with stable rustc stwo fork
   - Vendoring strategy: git subtree from `keep-stwo/stwo` patched branch, `cipherocto-stable` tag
   - Bench: `stwo-bench/stwo_proof.rs` measure proof gen + verify latency

4. **Verification wrapper** (`crates/quota-router-core/src/zk_verify/capability.rs`):
   - `verify_capability_zk(stark_proof: &StarkProof, public_inputs: &CapabilityClaim) -> Result<()>`
   - Bind to STWO plugin's `stark_verify_proof_with_metadata`
   - PublicInputMismatch check
   - CASM hash re-check at verify time

5. **Mint API + NodeType gating** (`crates/octo-wallet/src/cap/zk_mint.rs`):
   - `mint_with_zk(witness, public_inputs, casm_hash) -> Result<ProofBundle, ZkMintError>`
   - Wholesale → REJECT (`NodeTypeCannotMintZKCap`)
   - SelfHost → DEFAULT ZK (mint requires `inference_trace` in witness)
   - Hybrid → OPT-IN (explicit `mint_with_zk()` call)
   - `CapabilityClass` registry enforces `Wholesale → V1 only`

6. **Wire format extension** (modify `crates/octo-wallet/src/cap/wire.rs`):
   - Add optional 4th segment after 3rd dot: `proof_bundle_borsh`
   - Borsh-serialized `ProofBundle` (deterministic encoding)
   - v1 verifiers split on first 3 dots and ignore the rest (forward-compat per RFC-0957 §Compatibility)

7. **CI integration** (extend `.github/workflows/exercise-path.yml` or new `.github/workflows/zk-capability-circuit.yml`):
   - Jobs: build / test / clippy / fuzz-24h / cross-impl / casm-snapshot
   - `cargo clippy --workspace --all-targets -- -D warnings` mandatory gate
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

## Implementation Guide

Authoritative session plan: `docs/plans/2026-07-19-session-05-zk-capability-circuit.md` §3 Steps 1-8.

Master plan S05 row (§5 line 103): "Cairo ZK capability circuit + STWO production in stoolap fork ... owns Phase B.2 (CASM compilation as unique deliverable) + Phase C.2 (STWO plugin stable-rust vendoring); depends on S02 (capability token format) for ZK binding semantic + S03 (Ask/settlement types) + S04 (exercise path) for end-to-end integration."

**Cross-repo coordination (per RFC-0958 §Cross-Repo Coordination):** S05 spans 2 repos:
1. `cipherocto/` — `crates/quota-router-core/src/zk_verify/capability.rs` + `crates/octo-wallet/src/cap/zk_mint.rs` + CI workflow
2. `/home/mmacedoeu/_w/databases/stoolap` fork on `feat/blockchain-sql` branch — `cairo/capability_zk.cairo` + `cairo/build.rs` + `stwo-plugin/`

Recommended PR ordering: cipherocto-side PR first (defines interfaces), stoolap fork PR second (implements CASM + STWO). Both PRs MUST be reviewed together for atomic landing.

**Filename consistency note (per master plan naming):** mission file `0958-a-zk-capability-circuit.md` (without `-cairo` suffix per master plan §0 line 32; S04 mission Out of Scope cited `-cairo` suffix but master plan naming omits it; using master plan naming for consistency with `0102-a-`, `0957-a-`, `0957-b-`, `0959-a-` pattern).

**Stoolap fork branch pin:** commit hash recorded in S05 plan §1 line 8 + master plan §8 Risk #6 mitigation "Pin commit hash; weekly diff vs upstream."

## Acceptance Criteria

- [ ] **AC-1:** `cairo/capability_zk.cairo` authored + compiles via `cairo-compile >=2.6.0`
- [ ] **AC-2:** CASM hash matches check-in (snapshot test); `cargo test --features zk` in stoolap fork passes with real CASM (no stub bytes)
- [ ] **AC-3:** `stwo-plugin` builds on stable rust (no `+nightly` in CI); `stwo` vendored from `keep-stwo/stwo` patched branch at `cipherocto-stable` tag
- [ ] **AC-4:** `verify_capability_zk` accepts RFC-0958 §Test Vectors TV1 (SelfHost + inference trace) and TV2 (Hybrid + no trace) for ≥2 independent prover implementations (cross-impl verification)
- [ ] **AC-5:** Wholesale mint attempt returns `NodeTypeCannotMintZKCap` 100% of time; CI lint forbids `mint_with_zk` calls in `NodeType::Wholesale` code paths
- [ ] **AC-6:** Self-host mint requires `inference_trace` in witness; integration test asserts error `MissingInferenceTrace` if absent
- [ ] **AC-7:** Hybrid mint opt-in works (explicit `mint_with_zk()` call); test asserts Hybrid without explicit call returns v1 token (no ZK)
- [ ] **AC-8:** Wire format v2 parses correctly: v1 verifiers ignore 4th segment (forward-compat); v2 verifiers extract `proof_bundle_borsh` and verify
- [ ] **AC-9:** PublicInputMismatch detected; integration test corrupts `ask_id` and asserts `ZkVerifyError::PublicInputMismatch`
- [ ] **AC-10:** CASM drift detected; integration test mutates compiled CASM hash and asserts `ZkVerifyError::CasmHashMismatch` at mint AND verify
- [ ] **AC-11:** Proof gen latency <2s for SelfHost (10K trace steps reference HW); verify latency <100ms (per RFC-0958 §Performance Targets G1 + G2)
- [ ] **AC-12:** Proof size 50-500KB (measured against fixture set)
- [ ] **AC-13:** Fuzz `capability_zk_verify` 24h no crash (cargo-fuzz nightly job)
- [ ] **AC-14:** `cargo clippy --workspace --all-targets --all-features -- -D warnings` clean
- [ ] **AC-15:** Master plan Exit Criteria checkpoint: Phase B.2 (CASM production) + Phase C.2 (STWO plugin stable-rust) green; non-overlapping with Phase C (S03 owns) per master plan §8 Risk #10 R12 mitigation
- [ ] **AC-16:** Cross-repo coordination: cipherocto-side PR + stoolap fork PR both reviewed together; commit hash pinned in mission file `## Cross-Repo Coordination` section

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
| Cairo 2.6.0 + STWO integration | This mission (in `/home/mmacedoeu/_w/databases/stoolap` fork `feat/blockchain-sql` branch; Phase B.2 + C.2) |
| `zk_verify::capability::verify_capability_zk` function | This mission |
| `ZkMintError` + `ZkVerifyError` enums | This mission |
| `CapabilityClass` registry + NodeType gating | This mission |

## Risks (this mission)

| Risk | Mitigation |
|------|-----------|
| STWO unstable on stable rust | Vendor fork; pin commit hash; weekly diff vs upstream (per master plan §8 Risk #6) |
| Cairo 2.6 + STWO version mismatch | Pin both; document compatibility matrix in `crates/quota-router-core/src/zk_verify/README.md` |
| ZK circuit soundness bug | Property tests + 24h fuzz; snapshot vectors per RFC-0958 §Test Vectors |
| Wholesale path bypassing ZK rejection | 3-layer defense: mint API + registry + CI lint; integration test asserts error |
| Capability claim leak (private inputs in trace) | Public-input table documented in RFC-0958 §Data Structures; CI lint prohibits private field in logs (`PrivateWitness` is `Debug`-redacted) |
| STWO proof size growth makes exercise path slow | Test: 11-step finishes within 5s including ZK prove (if S04 consumes ZK flag); cap proof size if needed |
| Cairo non-determinism | Pin cairo-compile 2.6.0; deterministic-layout flag in `cairo/build.rs` |
| **Cross-repo PR ordering confusion** | Document ordering in PR description; recommended cipherocto-first then stoolap fork; both PRs reviewed together |
| **Stoolap fork drift** | Pin commit hash in mission file `## Cross-Repo Coordination`; weekly diff vs upstream per master plan §8 Risk #6 |
| **Hard-block RFC promotion delay** — RFC-0958 + RFC-0957 + RFC-0009 + RFC-0102 + RFC-0853 + RFC-0630 all Draft as of 2026-07-20; ALL are Requires per Mission-level table; BLUEPRINT.md: "Missions REQUIRE an approved RFC" | Track each RFC's Draft → Accepted promotion timeline; escalate via maintainer review board; coordinate parallel PR review windows for the 6 RFCs; document progress in master plan §0 weekly checkpoint |

## Mission-level (RFC prerequisites)

| RFC | Type | Status | Hard-block? |
|-----|------|--------|-------------|
| RFC-0958 | Requires | Draft (authored 2026-07-20) | YES — substrate |
| RFC-0957 | Requires | Draft (authored 2026-07-19, S02) | YES — `CapabilityToken` extended |
| RFC-0630 | Requires | Draft (authored 2026-07-20) | YES (self-host mode only) — `ExecutionTrace` |
| RFC-0009 | Requires | Draft (authored 2026-07-19, S01) | YES — DID + NodeType |
| RFC-0102 | Requires | Draft (authored 2026-07-19, S01) | YES — wallet substrate |
| RFC-0853 | Requires | Draft (authored 2026-07-19, S03 propagation) | YES — BLAKE3 primitive |
| RFC-0126 | Optional (referenced — `canonical_ser` only) | Accepted | No |
| RFC-0909 | Optional (coexistence only — symmetry reference) | Accepted | No (coexistence only) |

**Claim gate (per BLUEPRINT.md):** all "Requires" RFCs above MUST be Accepted before this mission moves from `missions/open/` → `missions/claimed/`. **6 Requires RFCs total** — RFC-0958, RFC-0957, RFC-0630, RFC-0009, RFC-0102, RFC-0853 (RFC-0126 + RFC-0909 referenced, not Requires).

## Cross-Repo Coordination

Stoolap fork substrate pin (record commit hash here at claim time, e.g., `cipherocto/stable-rust-stwo@<commit-hash>`):

- **Repo:** `/home/mmacedoeu/_w/databases/stoolap`
- **Branch:** `feat/blockchain-sql`
- **Commit hash (at claim time):** TBD — pin when mission claimed
- **PR review:** both PRs (cipherocto-side + stoolap fork) MUST be reviewed together for atomic landing

## Claim Process

Per BLUEPRINT.md:
1. All 6 Requires RFCs reach Accepted (7-day review + 2 maintainer approvals each). **R1 fix:** Claimed mission has 14-day timeout per BLUEPRINT.md §Mission Lifecycle ("Claimed mission: 14 days → Return to open"); if implementation not substantially progressing by day 14, mission returns to `missions/open/`.
2. Move this mission file to `missions/claimed/0958-a-zk-capability-circuit.md`.
3. Pin stoolap fork commit hash in `## Cross-Repo Coordination` section.
4. Implementation per RFC-0958 §Implementation Phases B.2 + C.2 + D + E + F + G + S05 plan §3 Steps 1-8.
5. PR + review → merge (**R1 fix:** cipherocto-side PR + stoolap fork PR reviewed together for atomic landing per `## Cross-Repo Coordination`).
6. CASM + STWO + verifier + gating all green per AC-1 through AC-16.

**This mission is NOT claimed as of 2026-07-20.**

## Related Artifacts

- **Sibling mission:** `missions/open/zk-proof-verification.md` (relocated 2026-07-20 from root level; generic STWO marketplace ZK verification, RFC-0100/0102 base; shares Phase C.2 STWO plugin vendoring)
- **Downstream consumers (optional):** mission `0957-b-provider-boundary-exercise-path.md` may consume ZK-bearing caps via S04 exercise path's optional ZK flag (Phase F extension; not required for S04 acceptance)

---

**Submission Date:** 2026-07-20
**Last Updated:** 2026-07-20
**Version:** 0.2 (Open; Availability block per BLUEPRINT.md mission status convention; **R1 fixes applied (10 total):** (1) Availability block RFC-0959 mis-cite removed; (2) 6 RFC Requires set clarified; (3) Phase C vs Phase C.2 disambiguation; (4) "Depends on" → "Dependencies" header; (5) 14-day Claim Process timeout; (6) "parent" → "sibling" for zk-proof-verification.md; (7) atomic PR landing requirement added; (8) RFC-0958 status = Draft authored 2026-07-20; (9) 6 Requires RFCs documented; (10) cross-repo coordination section added)