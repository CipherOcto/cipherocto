# Mission 0957-b: Provider Boundary + 11-Step Exercise Path

**RFC:** RFC-0957 (Economics): Capability Token Format — ACCEPTED 2026-07-20 (sub-mission letter-b)
**Status:** Claimed (2026-07-20)
**Phase:** F (11-step exercise integration) + provider boundary (Phase D adjacent)
**Master plan:** `docs/plans/2026-07-19-identity-master-plan.md`
**Session plan:** `docs/plans/2026-07-19-session-04-provider-boundary-exercise-path.md`

> **Availability (R7 fix):** Mission is now CLAIMABLE per BLUEPRT Mission Lifecycle (Requires RFC-0957 + RFC-0959 v1.0 + RFC-0009 + RFC-0102 + RFC-0853 all reached Accepted 2026-07-20). Claim filed 2026-07-20.

---

## Summary

Sub-mission letter-b of RFC-0957. Implements the provider boundary egress/ingress transforms (single egress module + CI lint; single ingress module) plus the 11-step exercise path as executable CI spec. Phase F owns green-build signal; depends on RFC-0957 letter-a (S02 capability token core) + RFC-0959 v1.0 (S03 independent settlement chain) + RFC-0009 (Ed25519 substrate) + RFC-0102 (wallet crypto).

**Why letter-b (not letter-c):** letter-a (`0957-a-capability-token-macaroon.md`, S02) covers macaroon core + attenuation + discharge + egress stub. letter-b (this mission) wires the full egress/ingress transforms + provider simulator + 11-step exercise path. RFC-0957 §Implementation Phases §Phase 3 explicitly defers "full egress/ingress transform (vault borrow, provider key attachment, ingress response classify) [to] S04."

## Depends on (RFC + upstream missions)

**R3 clarification — DAG acyclicity:** No circular dependency exists despite reviewer's R3 finding. Actual DAG: `RFC-0957 → RFC-0009 (substrate) → RFC-0102 (vault) → RFC-0853 (BLAKE3) → RFC-0959 (settlement) → RFC-0957-b (this mission)` is acyclic per R3 audit; depends_on relation is linear + ancestor-walk.

| Dependency | Status | Required? |
|------------|--------|-----------|
| RFC-0957 (Capability Token Format) | ACCEPTED (2026-07-20) | YES — substrate |
| RFC-0959 v1.0 (Independent Settlement Chain) | ACCEPTED (2026-07-20) | YES — settlement hash + receipt + ConsumedReceiptIndex + Ask binding |
| RFC-0009 (Identity Management) | ACCEPTED (2026-07-20) | YES — NodeType taxonomy + Ed25519 substrate |
| RFC-0102 (Wallet Cryptography) | ACCEPTED (2026-07-20) | YES — vault + keystore |
| RFC-0853 (Overlay Cryptography) | ACCEPTED (2026-07-20) | YES — BLAKE3 primitive for cache_key + settlement_hash |
| RFC-0126 (Deterministic Serialization) | Accepted | YES — canonical_ser for envelope + axes_consumed |
| RFC-0862 (Stoolap Sync Layer) | Accepted (v1.2.0) | YES — marketplace index rebuild + cross-repo persistence |
| RFC-0909 (Deterministic Quota Accounting) | Accepted (v69) | NO (coexistence only per Option A — independent chain) |
| Mission | `missions/open/0102-a-wallet-foundation.md` (S01) | open | YES — wallet substrate for vault one-shot borrow |
| Mission | `missions/open/0957-a-capability-token-macaroon.md` (S02) | open | YES — capability token format (this is letter-b of same RFC-0957) |
| Mission | `missions/open/0959-a-ask-pricing-stoolap.md` (S03) | open | YES — settlement hash + Ask binding types |
| Use Case | `docs/use-cases/enhanced-quota-router-gateway.md` | ✓ Approved (2026-03-12) | YES — provider boundary + capability egress intent layer |
| Plan | `docs/plans/2026-07-19-session-04-provider-boundary-exercise-path.md` | ✓ exists | YES — authoritative session plan |

## In Scope

Per RFC-0957 §Implementation Phases Phase 3 + S04 plan §3 Steps 1-11 (R11 fix — Steps 1-10 → 1-11 per session plan line 9 + Exit Criteria alignment; **R15 fix — line cite:** session plan line 9 has "Authoritative exercise step list: master plan §6 11-step table (Steps 1-11)"; session plan line 30 has the prerequisite action "Verify all upstream missions completed"; exercise path is 11 steps total):

1. **Egress module** (`crates/quota-router-core/src/egress/{openai,anthropic,google}/mod.rs`):
   - `EgressTransform::forward(req: InboundRequest, slot: VaultSlotRef) → Result<OutboundRequest>`
   - Strip: `X-Capability-Token`, `Authorization` (cipherocto variants), cipherocto-specific headers
   - Reshape request body to provider schema (per provider format)
   - Sign egress with provider's slot key from vault (one-shot borrow)
2. **Ingress module** (`crates/quota-router-core/src/ingress/mod.rs`):
   - `IngressTransform::normalise(provider_kind, raw_response) → NormalisedResponse`
   - Detect cache-hit per provider response metadata
   - Call `cache::classify(prompt)` against local cache; reconcile with provider metadata (per RFC-0959 §Cache classification approach: provider flag wins, local cache as hint)
   - Forward to RFC-0959 v1.0 settlement engine (`crates/octo-core/src/settlement.rs`)
   - Attach `cap_root_hash` + `ask_id` + `invocation_hash` metadata for receipt build (per RFC-0959 §Data Structures `SettlementReceiptEnvelope`)
3. **Provider boundary lint** (`clippy.toml` + dedicated CI body-scan job):
   - Forbid `reqwest::Client::new()` outside `crates/quota-router-core/src/egress/` (S04 plan §3 Step 1)
   - **R1 fix per RFC-0957 §Adversary A5 line 685:** lint must also cover `hyper`, `ureq`, `isahc` client constructors (NOT just `reqwest`); runtime backtrace assertion in `egress::client()` constructor
   - **R2 fix:** body-field linter scans request/response bodies for CapabilityToken-shaped strings (HMAC-BLAKE3 32-byte tags + macaroon caveat structure) in cookie, JSON, form-encoded, and protobuf fields; CI deny if detected outside egress — corresponds to risk "Capability strip leaks via non-header path"
   - **R3 fix — explicit CI mechanism:** body-field linter implemented as dedicated body-scan CI job in `.github/workflows/exercise-path.yml` (separate from `cargo clippy` since `clippy.toml` cannot scan request/response bodies); job runs `cargo run -p quota-router-cli -- body-scan --diff main` on every PR; CI deny (`exit 1`) if body linter detects CapabilityToken-shaped strings in non-egress module bodies. Custom proc-macro escape hatch per S04 plan §3 Step 3 if `cargo clippy` cannot enforce
4. **Provider key rotation handling**:
   - `octo-wallet::vault::on_rotation(slot_id) → RotationEvent`
   - `quota_router_core::marketplace` listens; invalidates ASKs referencing old slot
   - Active capabilities bound to old slot: 1h grace; new mints rejected post-grace
5. **Provider simulator** (`crates/quota-router-core/src/sim/`, behind `feature = "provider-sim"`):
   - 8 modes: normal/200, throttle, 429-burst, key-expired, schema-change, timeout, garbage, **internal-error** (R6 fix — added to enumeration; per S04 plan Step 5 R1 fix: `internal-error` for provider 500 responses with provider-specific error schema; matches RFC-0959 v1.0 §Adversary A5 mitigation surface)
   - Seeded RNG for reproducibility
   - CLI: `quota-router-cli sim --provider openai --mode throttle --seed 42`
6. **Exercise path spec** (`crates/quota-router-core/tests/exercise/eleven_step.rs`):
   - 11-step canonical E2E (master plan §6): SSO → virtual API key → capability mint → POST → marketplace lookup → OCTO-W escrow pre-auth → egress transform → provider response → cache-classify + axes_consumed → receipt build → reputation delta + ledger append
   - `TestTrace` records every step into JSON fixture; passes through `quota-router-cli settle-replay --expected-hash` for determinism check
7. **Test fixtures** (`tests/fixtures/exercise/`):
   - JSON of expected outputs per step (goldens)
   - `INSTA` for snapshot assertions
   - `MESSY_YAML_FOR_ASK` fixture = 10 ASKs across providers
8. **CI integration** (`.github/workflows/exercise-path.yml`):
   - Jobs: build / test / **body-scan (R5 fix — from In Scope item 3)** / clippy / fuzz-min-5min / exercise-goldens / **cross-impl (R5 fix — from AC-10)**
   - `cargo clippy --workspace --all-targets -- -D warnings` mandatory gate
   - cache-poison regression test (preventing `cache_hit_rate > 0.90` false-positive; per RFC-0959 §Adversary A5 mitigation)
9. **Adapter self-tests**: each egress sub-module ships `adapter-self-test` (spin up local provider-sim on random port; round-trip 5 calls; compare signatures and token counts); rate-limit backoff integration test (sim returns 429 then 200; egress must respect backoff)
10. **Adjudication** (Phase G; R1 fix — Step 10 omitted from v0.1 In Scope; R10 fix — RFC-0850ab cross-ref removed as ambiguous): document each adapter test in `crates/quota-router-core/tests/exercise/README.md`; **deliverable spec (R10):** `tests/exercise/README.md` MUST contain (i) per-step expected hash table (11 rows × SHA-256 / BLAKE3 columns); (ii) assertion contract template (which assertion fails → which log lines to inspect); (iii) per-adapter self-test contract (5-call round-trip + signature + token count assertions); (iv) cross-link table mapping step number → RFC-0959 v1.0 §Algorithm (settlement_hash → Step 9/10; ConsumedReceiptIndex → Step 10; build_receipt → Step 10); NO vague cross-ref to RFC-0850ab (interface mismatch: PlatformAdapter vs EgressTransform::forward)

## Out of Scope (this mission only)

- ZK capability circuit exercise → mission `0958-a-zk-capability-circuit-cairo.md` (S05)
- On-chain settlement discharge flow → RFC-0955 future
- Hardware wallet integration → Phase H (future)
- MPC threshold keys → Phase I (future)
- RFC-0909 v69 ↔ v70 upgrade migration → N/A (Option A independent chain; no v70 bump)

## Implementation Guide

Authoritative session plan: `docs/plans/2026-07-19-session-04-provider-boundary-exercise-path.md` §3 Steps 1-10 (R17 fix — consistent with master plan §6 11-step enumeration; session plan enumerates 10 numbered steps with Step 11 embedded in §3 Step 10; mission In Scope § AC-4/AC-8 + master plan §6 cite authoritative 11-step list).

Master plan S04 row (§5 line 102): "Provider egress/ingress + 11-step exercise as executable CI spec (canonical test path: `crates/quota-router-core/tests/exercise/eleven_step.rs`; CI workflow: `.github/workflows/exercise-path.yml`)."

**Pre-existing crate note (per S04 plan §1 line 41):** `crates/quota-router-core/` exists with 30+ modules. S04 ADDS the missing modules under the same crate: `src/egress/`, `src/ingress/`, `src/marketplace/`, `src/settle/`, `src/receipt/`, `src/sim/`. No `cargo new`; just `mod` declarations + feature flags.

**Filename consistency note (R1 fix per S04 audit):** master plan names `0957-b-provider-boundary-exercise-path.md`. S04 plan §0 line 21 names bare `provider-boundary-exercise-path.md` (no RFC prefix; cited pattern `missions/open/quota-market-integration.md` does not exist per `ls`). This mission file uses master-plan naming for consistency with `0102-a-`, `0957-a-`, `0959-a-` pattern.

## Acceptance Criteria

- [ ] **AC-1:** Egress module exists; clippy deny rule active (covers `reqwest` + `hyper` + `ureq` + `isahc` per RFC-0957 §Adversary A5)
- [ ] **AC-2:** Ingress module exists; cache-classify wired to RFC-0959 v1.0 settlement engine
- [ ] **AC-3:** Provider simulator: 8 modes deterministic; tests pass
- [ ] **AC-4:** 11-step exercise: **green in CI under `--all-features`** (`cargo test -p quota-router-core --test eleven_step --all-features` runs as part of `.github/workflows/exercise-path.yml` on every PR; **R5 fix — redundant flag:** `--all-features` already enables `provider-sim`; do NOT pass `--features provider-sim --all-features` together as it's a no-op combination); **R3 fix — pre-wired-stub interim gate:** until RFC-0959 v1.0 reaches Accepted, exercise runs against settlement engine stub returning synthetic receipt_id + canonical_ser placeholder; **R5 fix — stub spec:** stub at `crates/octo-core/src/settlement.rs::StubEngine` implements `compute_cost`, `settlement_hash`, `build_receipt`, `verify_receipt` returning synthetic values deterministically (seed = 0); tests verify stub output matches pre-computed fixture at `tests/fixtures/settlement-stub-fixtures.json`; switch to real engine via `OCTO_CORE_ENGINE_BACKEND=real` env var post-promotion
- [ ] **AC-5:** Goldens captured + checked-in (`tests/fixtures/exercise/`)
- [ ] **AC-6:** `cargo clippy --workspace --all-targets -- -D warnings` clean
- [ ] **AC-7:** Provider key rotation event flow works; old caps expire within 1h grace
- [ ] **AC-8:** Master plan Exit Criteria checkpoint: Phase F (11-step exercise green in CI under `--all-features`) + provider boundary green — **R3 fix — RFC-0959 v1.0 Accepted milestone** (R12 fix: replaced "claim-gate step 5" reference with explicit RFC-0959 v1.0 Accepted milestone; "step 5" was ambiguous — does NOT refer to exercise step 5, NOR to BLUEPRINT claim gate step 5; refers to RFC-0959 v1.0 promotion PR landing in Accepted status): full E2E green is post-RFC-0959 v1.0 Accepted milestone; pre-wired-stub interim gate keeps CI green during promotion window
- [ ] **AC-9:** ConsumedReceiptIndex replay defense verified end-to-end in exercise (per RFC-0959 v1.0 §Algorithms `build_receipt`); **R3 fix — interim gate:** pre-wired-stub verifies ConsumedReceiptIndex O(1) HashMap-backed lookup + ReceiptReplay rejection; post-promotion verifies full integration with real settlement engine
- [ ] **AC-10:** Cross-implementation verification per RFC-0959 v1.0 §Test Vectors: ≥ 2 independent implementations produce identical 32-byte settlement_hash + receipt_id digests for TV1 + TV2 (R4 fix: explicit AC-10 label added for cross-ref with S04 plan §5 Exit Criteria enumeration)

## Risks (this mission)

| Risk | Mitigation |
|------|-----------|
| Boundary lint missed by some path | Test: deliberately introduce violation (e.g., `hyper::Client::new()` outside egress) → must fail CI |
| Provider-sim divergence from real provider | Maintain golden fixtures; weekly diff vs real (out-of-CI) |
| Capability strip leaks via non-header path (e.g., cookie, body) | Body linter (R2 fix): forbid CapabilityToken-shaped strings in cookie/JSON/form/protobuf body fields; CI deny if detected outside egress — see In Scope item 3 |
| **Hard-block RFC promotion delay** (R1 fix) — RFC-0957 + RFC-0959 v1.0 + RFC-0009 + RFC-0102 — ACCEPTED (2026-07-20) → Accepted promotion timeline; escalate via maintainer review board; coordinate parallel PR review windows for the 5 RFCs; document progress in master plan §0 weekly checkpoint |
| **RFC-0959 v1.0 promotion delay** (R2 fix — intentionally split from generic hard-block row for actionable tracking; **R11 fix — clarifies intentional separation, not double-counting:** hard-block row tracks ALL 5 RFCs at meta level for BLUEPRT.md "Missions REQUIRE an approved RFC" gate; this row adds RFC-0959-specific operational detail (settlement engine + ConsumedReceiptIndex) needed for actionable tracking. Both rows remain; S04 plan §7 Risks (the session plan document's own §7 Risks table) is the same source as this row — no separate mirror needed.) — settlement engine + receipt build + ConsumedReceiptIndex in §Implementation Phases Phase 2 are gated on RFC-0959 v1.0 reaching Accepted | Designate RFC-0959 v1.0 parallel-track owner (per S03 mission `0959-a-ask-pricing-stoolap.md` claimant); set target Accepted date (TBD; coord with maintainer review board); pre-wire `crates/octo-core/src/settlement.rs` interface so exercise test compiles against stub impl + switches to real impl on RFC-0959 promotion; weekly status in master plan §0 checkpoint |
| Test goldens get brittle | INSTA `insta-allow` discipline; no auto-update outside session; **R2 fix:** golden updates require maintainer reviewer approval + delta rationale in PR description (signal-vs-noise drift distinguished by hash change > 1 byte OR new axis added OR capability schema bump); review step mandatory before accepting new goldens |
| Concurrent exercise path step race | Tokio multi-thread + `TestTrace` captures state at each step |
| RFC-0959 v1.0 promotion delay | Settlement exercise test cannot run green until RFC-0959 reaches Accepted; document in §Exit Criteria as blocker |

## Mission-level (RFC prerequisites)

| RFC | Type | Status | Hard-block? |
|-----|------|--------|-------------|
| RFC-0957 — ACCEPTED (2026-07-20) | YES — capability token substrate |
| RFC-0959 v1.0 | Requires | ACCEPTED (2026-07-20; Option A rewrite) | YES — settlement hash + receipt + Ask binding |
| RFC-0009 — ACCEPTED (2026-07-20) | YES — Ed25519 substrate |
| RFC-0102 — ACCEPTED (2026-07-20) | YES — wallet substrate for vault one-shot borrow |
| RFC-0853 — ACCEPTED (2026-07-20) | YES — BLAKE3 primitive |
| RFC-0126 | Requires | Accepted | No |
| RFC-0862 | Requires | Accepted | No |
| RFC-0909 | (none) | Accepted | No (coexistence only per Option A) |

**Claim gate (per BLUEPRINT.md):** all "Requires" RFCs above MUST be Accepted before this mission moves from `missions/open/` → `missions/claimed/`.

## Claim Process

Per BLUEPRINT.md:
1. All Requires RFCs reach Accepted (7-day review + 2 maintainer approvals each).
2. Move this mission file to `missions/claimed/0957-b-provider-boundary-exercise-path.md`.
3. Implementation per RFC-0957 §Implementation Phases Phase 3 + S04 plan §3 Steps 1-10 (R17 fix — consistent with master plan §6 11-step enumeration; session plan enumerates 10 numbered steps with Step 11 embedded in §3 Step 10).
4. PR + review → merge.
5. Exercise path green in CI under `--all-features` per master plan §9 Exit Criteria.

**This mission is NOT claimed as of 2026-07-20.**

---

**Submission Date:** 2026-07-20
**Last Updated:** 2026-07-20
**Version:** 0.2 (Open; **R7 fix:** Status field changed from `Strict-Reading` to `Open` + Availability block added per BLUEPRINT.md mission status convention; **R8 fix:** Version parenthetical aligned with Status field; RFC-0959 v1.0 dependency documented; filename matches master plan naming convention)