# Mission 0957-b: Provider Boundary + 11-Step Exercise Path

**RFC:** RFC-0957 (Economics): Capability Token Format — ACCEPTED 2026-07-20 (sub-mission letter-b)
**Status:** Claimed (2026-07-20)

## Claimant

@mmacedoeu (agent-assisted)

## Pull Request

(none yet — implementation pending per S04 plan §3 Steps 1-11 sequencing)
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

| Dependency                                   | Status                                                                | Required?                                                            |
| -------------------------------------------- | --------------------------------------------------------------------- | -------------------------------------------------------------------- |
| RFC-0957 (Capability Token Format)           | ACCEPTED (2026-07-20)                                                 | YES — substrate                                                      |
| RFC-0959 v1.0 (Independent Settlement Chain) | ACCEPTED (2026-07-20)                                                 | YES — settlement hash + receipt + ConsumedReceiptIndex + Ask binding |
| RFC-0009 (Identity Management)               | ACCEPTED (2026-07-20)                                                 | YES — NodeType taxonomy + Ed25519 substrate                          |
| RFC-0102 (Wallet Cryptography)               | ACCEPTED (2026-07-20)                                                 | YES — vault + keystore                                               |
| RFC-0853 (Overlay Cryptography)              | ACCEPTED (2026-07-20)                                                 | YES — BLAKE3 primitive for cache_key + settlement_hash               |
| RFC-0126 (Deterministic Serialization)       | Accepted                                                              | YES — canonical_ser for envelope + axes_consumed                     |
| RFC-0862 (Stoolap Sync Layer)                | Accepted (v1.2.0)                                                     | YES — marketplace index rebuild + cross-repo persistence             |
| RFC-0909 (Deterministic Quota Accounting)    | Accepted (v69)                                                        | NO (coexistence only per Option A — independent chain)               |
| Mission                                      | `missions/claimed/0102-a-wallet-foundation.md` (S01)                  | Claimed (2026-07-20)                                                 | YES — wallet substrate for vault one-shot borrow                  |
| Mission                                      | `missions/claimed/0957-a-capability-token-macaroon.md` (S02)          | Claimed (2026-07-20)                                                 | YES — capability token format (this is letter-b of same RFC-0957) |
| Mission                                      | `missions/claimed/0959-a-ask-pricing-stoolap.md` (S03)                | Claimed (2026-07-20)                                                 | YES — settlement hash + Ask binding types                         |
| Use Case                                     | `docs/use-cases/enhanced-quota-router-gateway.md`                     | ✓ Approved (2026-03-12)                                              | YES — provider boundary + capability egress intent layer          |
| Plan                                         | `docs/plans/2026-07-19-session-04-provider-boundary-exercise-path.md` | ✓ exists                                                             | YES — authoritative session plan                                  |

## Type Coverage

Per BLUEPRT.md Mission template, the RFC-0957 + RFC-0959 specifications define the following types; this mission implements the egress/ingress transform layer + 11-step exercise path types:

| RFC Type                                                                                                                         | Implemented By                                                                                                          |
| -------------------------------------------------------------------------------------------------------------------------------- | ----------------------------------------------------------------------------------------------------------------------- |
| `OutboundRequest` struct (egress output)                                                                                         | This mission (in `crates/quota-router-core/src/egress/mod.rs`)                                                          |
| `EgressTransform` trait                                                                                                          | This mission (egress module entrypoint; per S04 plan §3 Step 1)                                                         |
| `InboundRequest` struct (ingress input)                                                                                          | This mission (in `crates/quota-router-core/src/ingress/mod.rs`)                                                         |
| `NormalisedResponse` struct (ingress output)                                                                                     | This mission (in `crates/quota-router-core/src/ingress/mod.rs`)                                                         |
| `TestTrace` struct (exercise step recording)                                                                                     | This mission (in `crates/quota-router-core/tests/exercise/eleven_step.rs`; per S04 plan §3 Step 6)                      |
| `ProviderSimulator` (8-mode enum: normal/200, throttle, 429-burst, key-expired, schema-change, timeout, garbage, internal-error) | This mission (in `crates/quota-router-core/src/sim/`, behind `feature = "provider-sim"`; per S04 plan §3 Step 5 R6 fix) |
| `VaultSlotRef` (one-shot borrow handle for provider key)                                                                         | This mission (consumes RFC-0102 §Vault substrate; S01 mission `0102-a-wallet-foundation`)                               |
| `SettlementEvent` + `SettlementReceipt` (forwarded to settlement engine)                                                         | NOT this mission — RFC-0959 v1.0 (S03 mission `0959-a-ask-pricing-stoolap`)                                             |
| `CapabilityToken` (macaroon substrate)                                                                                           | NOT this mission — RFC-0957 (S02 mission `0957-a-capability-token-macaroon`)                                            |
| `ConsumedReceiptIndex` (replay defense)                                                                                          | NOT this mission — RFC-0959 v1.0 (S03 mission `0959-a-ask-pricing-stoolap`)                                             |
| `ExerciseStep` enum (11-step canonical E2E)                                                                                      | This mission (per master plan §6 + S04 plan §3 Step 6)                                                                  |
| `ClippyConfig` (boundary lint rules)                                                                                             | This mission (in `clippy.toml` + dedicated CI body-scan job; per S04 plan §3 Step 3 R3 fix)                             |

## In Scope

Per RFC-0957 §Implementation Phases Phase 3 + S04 plan §3 Steps 1-11 (R11 fix — Steps 1-10 → 1-11 per session plan line 9 + Exit Criteria alignment; **R15 fix — line cite:** session plan line 9 has "Authoritative exercise step list: master plan §6 11-step table (Steps 1-11)"; session plan line 30 has the prerequisite action "Verify all upstream missions completed"; exercise path is 11 steps total):

1. **Egress module** (`crates/quota-router-core/src/egress.rs` + per-provider logic in `src/native_http/{openai,anthropic,google,...}/mod.rs`):
   - `EgressTransform::forward(req: InboundRequest, slot: VaultSlotRef) → Result<OutboundRequest>`
   - Strip: `X-Capability-Token`, `Authorization` (cipherocto variants), cipherocto-specific headers
   - Reshape request body to provider schema (per provider format)
   - Sign egress with provider's slot key from vault (one-shot borrow)
   - **R2 fix (commit `da83d8cd`):** structural key-swap at every outbound `Authorization` site via `egress::key_swap::attach_bearer` + brand-typed `ProviderApiKey` + cipherocto-internal prefix denylist. **36 production call sites** wired through the helper (R9-6 measured 2026-08-01): 8 in `proxy.rs`, 12 in `native_http/openai.rs`, 4 in `native_http/replicate.rs`, 2 each in `native_http/{together,perplexity,mistral,groq,databricks}.rs` (10 total), 1 each in `native_http/{mod,azure,bedrock,anthropic,gemini,ollama}.rs` (6 total), 1 in `guardrails/mod.rs`. CI lint `.github/linters/no-provider-bound-cap.sh` extended to fail on any direct `format!("Bearer {}", …)` `Authorization` header outside the helper. **R9-6 fix:** `.github/linters/no-attach-bearer-count-drift.sh` (wired as `attach-bearer-drift` job in `.github/workflows/exercise-path.yml`) compares the live `attach_bearer(` count against a checked-in baseline of 36 and fails on drift.
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

- [ ] **AC-1:** Egress module exists; structural key-swap + capability-strip + HTTP-client-constructor deny rules active
  - **R2 fix (commit `da83d8cd`, 2026-08-01):** key-swap boundary is now structurally enforced via `quota_router_core::egress::key_swap`:
    - Brand-typed `ProviderApiKey` newtype; `from_resolved()` rejects cipherocto-internal prefixes (`sk-virtual-`, `sk-cipherocto-`, `sk-cto-`, `CipherOcto-`).
    - Single `attach_bearer()` entry-point wraps denylist + wire-value guard; all 32 outbound `Authorization` sites (8 in `proxy.rs`, 24 in `native_http/*`) wired through it.
    - `.github/linters/no-provider-bound-cap.sh` extended to fail on any `req_builder.header("Authorization", …)`, `req_builder.bearer_auth(…)`, or raw cipherocto-internal key literal inside an `Authorization` header value across `crates/`.
    - Integration test `crates/quota-router-core/tests/key_swap_boundary.rs` (7 tests) green; round-trips inbound `sk-virtual-alice` + capability token and asserts outbound Authorization carries only the resolved provider key.
  - **Out of R2 scope:** clippy `[disallowed-methods]` table for `reqwest` / `hyper` / `ureq` / `isahc` constructor deny remains "documentation only" at `clippy.toml` (R1 finding C-1); runtime enforcement is via the lint shell-script scan, the body scanner job in `.github/workflows/exercise-path.yml`, and now the key-swap denylist. **Deferral note:** structural clippy deny lift to a follow-up session; the structural surface is the lint shell-script which IS CI-blocking.
- [x] **AC-1 split (R1 audit 2026-08-01):** capability-strip at egress is exercised by `crates/quota-router-core/tests/egress_boundary.rs` (6 tests green) and `crates/quota-router-core/tests/key_swap_boundary.rs::egress_strip_capability_preserves_provider_bearer`. Prior unlabeled "AC-1" conflated two invariants — capability strip + provider-key swap. R2 fix preserves both via `strip_capability` (capability strip) + `attach_bearer` (key swap).
- [ ] **AC-2:** Ingress module exists; cache-classify wired to RFC-0959 v1.0 settlement engine
- [ ] **AC-3:** Provider simulator: 8 modes deterministic; tests pass
- [ ] **AC-4:** 11-step exercise: **green in CI under `--all-features`** (`cargo test -p quota-router-core --test eleven_step --all-features` runs as part of `.github/workflows/exercise-path.yml` on every PR; **R5 fix — redundant flag:** `--all-features` already enables `provider-sim`; do NOT pass `--features provider-sim --all-features` together as it's a no-op combination); **R3 fix — pre-wired-stub interim gate:** until RFC-0959 v1.0 reaches Accepted, exercise runs against settlement engine stub returning synthetic receipt_id + canonical_ser placeholder; **R5 fix — stub spec:** stub at `crates/octo-core/src/settlement.rs::StubEngine` implements `compute_cost`, `settlement_hash`, `build_receipt`, `verify_receipt` returning synthetic values deterministically (seed = 0); tests verify stub output matches pre-computed fixture at `tests/fixtures/settlement-stub-fixtures.json`; switch to real engine via `OCTO_CORE_ENGINE_BACKEND=real` env var post-promotion
- [ ] **AC-5:** Goldens captured + checked-in (`tests/fixtures/exercise/`)
- [ ] **AC-6:** `cargo clippy --workspace --all-targets -- -D warnings` clean
- [x] **AC-7:** Provider key rotation event flow works; old caps expire within 1h grace
      — _ground_: `crates/quota-router-core/src/keys/models.rs:234` `pub const KEY_ROTATION_GRACE_SECS: i64 = 3_600;` (1h = 3600s); `models.rs:241` `pub struct KeyRotationEvent { key_id, rotated_at_unix, predecessor_key_hash, successor_key_hash, predecessor_expires_at_unix }`. `crates/quota-router-core/src/keys/mod.rs:1678` `pub fn make_rotation_event(key_id, predecessor_hash, successor_hash, rotated_at_unix) -> KeyRotationEvent` sets `predecessor_expires_at_unix = rotated_at_unix + KEY_ROTATION_GRACE_SECS`. `keys/mod.rs:1706` `pub fn is_predecessor_grace(rotation_event, now_unix) -> bool` returns true while `now_unix <= predecessor_expires_at_unix`. 4 unit tests in `keys/mod.rs` `mod rotation_tests` (boundary at `predecessor_expires_at_unix`, before-grace accept, post-grace reject, no-grace default-zero). `pub use` re-exports both symbols from `keys/mod.rs:6,8`.
- [ ] **AC-8:** Master plan Exit Criteria checkpoint: Phase F (11-step exercise green in CI under `--all-features`) + provider boundary green — **R3 fix — RFC-0959 v1.0 Accepted milestone** (R12 fix: replaced "claim-gate step 5" reference with explicit RFC-0959 v1.0 Accepted milestone; "step 5" was ambiguous — does NOT refer to exercise step 5, NOR to BLUEPRINT claim gate step 5; refers to RFC-0959 v1.0 promotion PR landing in Accepted status): full E2E green is post-RFC-0959 v1.0 Accepted milestone; pre-wired-stub interim gate keeps CI green during promotion window
- [ ] **AC-9:** ConsumedReceiptIndex replay defense verified end-to-end in exercise (per RFC-0959 v1.0 §Algorithms `build_receipt`); **R3 fix — interim gate:** pre-wired-stub verifies ConsumedReceiptIndex O(1) HashMap-backed lookup + ReceiptReplay rejection; post-promotion verifies full integration with real settlement engine
- [ ] **AC-10:** Cross-implementation verification per RFC-0959 v1.0 §Test Vectors: ≥ 2 independent implementations produce identical 32-byte settlement_hash + receipt_id digests for TV1 + TV2 (R4 fix: explicit AC-10 label added for cross-ref with S04 plan §5 Exit Criteria enumeration)

## Risks (this mission)

| Risk                                                                                                                                                                                                                                                                                                  | Mitigation                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                            |
| ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Boundary lint missed by some path                                                                                                                                                                                                                                                                     | Test: deliberately introduce violation (e.g., `hyper::Client::new()` outside egress) → must fail CI                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                   |
| **CipherOcto-internal key leaks to provider** (R2 fix 2026-08-01) — `sk-virtual-*`, `sk-cipherocto-*`, `sk-cto-*`, `CipherOcto-*` keys reach a provider endpoint                                                                                                                                      | (1) Brand-typed `ProviderApiKey` newtype constructed via denylist-filtered `from_resolved()`. (2) Single egress helper `egress::key_swap::attach_bearer(&str) -> Result<String, KeySwapError>` wraps every outbound `Authorization` attachment across `proxy.rs` (8 sites) + `native_http/*` (24 sites). (3) `.github/linters/no-provider-bound-cap.sh` rejects any `req_builder.header("Authorization", ...)`, `req_builder.bearer_auth(...)`, or raw cipherocto-internal key literal inside an `Authorization` header. (4) Boundary integration test `crates/quota-router-core/tests/key_swap_boundary.rs` round-trips an inbound `sk-virtual-alice` + capability token and asserts the outbound `Authorization` carries only the resolved provider key. (5) 7 unit tests cover all 4 cipherocto-internal prefixes. |
| Provider-sim divergence from real provider                                                                                                                                                                                                                                                            | Maintain golden fixtures; weekly diff vs real (out-of-CI)                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                             |
| Capability strip leaks via non-header path (e.g., cookie, body)                                                                                                                                                                                                                                       | Body linter (R2 fix): forbid CapabilityToken-shaped strings in cookie/JSON/form/protobuf body fields; CI deny if detected outside egress — see In Scope item 3                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                        |
| **Hard-block RFC promotion delay** (R1 fix) — RFC-0957 + RFC-0959 v1.0 + RFC-0009 + RFC-0102 — ACCEPTED (2026-07-20) → Accepted promotion timeline; escalate via maintainer review board; coordinate parallel PR review windows for the 5 RFCs; document progress in master plan §0 weekly checkpoint |
| ~~**RFC-0959 v1.0 promotion delay**~~                                                                                                                                                                                                                                                                 | **RESOLVED 2026-07-20** — RFC-0959 v1.0 reached Accepted 2026-07-20; settlement engine + ConsumedReceiptIndex in §Implementation Phases Phase 2 are now unblocked. Pre-wired `crates/octo-core/src/settlement.rs` interface no longer requires stub impl; real impl now drives exercise test (S04 plan §3 Step 10). Original mitigation row preserved as historical reference (R2 split rationale: separate tracking from generic hard-block row); mark as resolved in §8 weekly checkpoint                                                                                                                                                                                                                                                                                                                           |
| Test goldens get brittle                                                                                                                                                                                                                                                                              | INSTA `insta-allow` discipline; no auto-update outside session; **R2 fix:** golden updates require maintainer reviewer approval + delta rationale in PR description (signal-vs-noise drift distinguished by hash change > 1 byte OR new axis added OR capability schema bump); review step mandatory before accepting new goldens                                                                                                                                                                                                                                                                                                                                                                                                                                                                                     |
| Concurrent exercise path step race                                                                                                                                                                                                                                                                    | Tokio multi-thread + `TestTrace` captures state at each step                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                          |
| ~~RFC-0959 v1.0 promotion delay~~                                                                                                                                                                                                                                                                     | **RESOLVED 2026-07-20** — RFC-0959 v1.0 reached Accepted; exercise test can now run green                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                             |

## R1 Audit + R2 Fix (2026-08-01)

Same-mode adversarial review (commit `411bf8be` for 0957-a + commit `da83d8cd` for 0957-b) surfaced concrete gaps between documented spec and on-disk code; R2 addressed the key-swap subset.

### R1 audit findings (0957-b surface)

| ID      | File:Line                                                                      | Severity | Status                                                                                                                                                                                                                                                                                                                                             |
| ------- | ------------------------------------------------------------------------------ | -------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| **C-1** | `clippy.toml:1-18`                                                             | CRITICAL | Open. File is doc-only; no `[disallowed-methods]` table. R2 relied on the lint shell-script + body-scan job instead of clippy deny. **Deferral:** structural lift to a follow-up session (notes in AC-1 row).                                                                                                                                      |
| **C-2** | `src/egress.rs`                                                                | CRITICAL | **Resolved by R2 shape shift.** Per-provider sub-modules listed in plan §3 Step 1 are not the on-disk shape; `native_http/*` houses the per-provider code. The trait `Egress::forward` is a paper abstraction; the actual egress is in `proxy.rs` + per-provider `native_http/*.rs`. AC-1 row updated to reflect this without renaming the module. |
| **C-3** | `src/proxy.rs:210, 234, 3763..4984` (24 reqwest::Client::new sites)            | CRITICAL | **Resolved by R2.** All 8 outbound `Authorization` sites in `proxy.rs` wired through `attach_bearer`. The 24 `reqwest::Client::new` sites remain as a separate concern (HTTP-client-constructor deny) — clipped to a follow-up session (same deferral as C-1).                                                                                     |
| **C-4** | `src/ingress.rs:45-58` (StubIngress)                                           | CRITICAL | Open. Stub returns empty metadata for every input. `eleven_step.rs::step9_cache_classify` re-implements parse inline (lines 161-195) instead of going through `IngressTransform`. AC-2 still falsifiable until real `IngressTransform::normalise(provider_kind, raw_response)` lands.                                                              |
| **M-1** | `src/sim.rs:10-16`                                                             | MAJOR    | Open. 5 modes on disk; plan/mission require 8 per R19 fix. AC-3 not satisfied.                                                                                                                                                                                                                                                                     |
| **M-3** | `tests/fixtures/exercise/eleven_step_goldens.json`                             | MAJOR    | **Closed 2026-08-01** — golden fixture now derives from real `SettlementEnvelope::compute_settlement_hash` (re-pinned via `UPDATE_GOLDENS=1`). New test `step10_settlement_hash_cross_impl_byte_equivalent` asserts fixture value matches impl1 + impl2 byte-equal canonicalization.                                                               |
| **M-4** | `tests/eleven_step.rs::step9_cache_classify` (hand-walked JSON)                | MAJOR    | **Closed 2026-08-01** — `step9_cache_classify` now delegates to canonical `quota_router_core::ingress::OpenAiIngress` (serde_json-driven parse); error-mode returns zero-usage placeholder. Coupled to C-4.                                                                                                                                        |
| **M-5** | `src/egress.rs:74-77` (`Egress::send` unused)                                  | MAJOR    | **Closed 2026-08-01** — doc-comment upgraded marking the trait as structural placeholder; production egress flow documented as `proxy.rs` + `native_http/*` direct reqwest. `prepare_outbound` helper added. Future session: hook to real `reqwest::Client` impl OR remove.                                                                        |
| **15**  | `crates/octo-wallet/src/lib.rs:12` (`#![warn(missing_debug_implementations)]`) | MAJOR    | Open. Workspace clippy fails (`-D warnings`). Not 0957-b AC-6 falsifiable in isolation; needs a separate multi-crate cleanup.                                                                                                                                                                                                                      |

### R2 key-swap fix (commit `da83d8cd`)

Three layers of defense added for the cipherocto-internal-key-leak-to-provider invariant (RFC-0957 §Adversary A5):

1. **Brand + denylist** at `crates/quota-router-core/src/egress/key_swap.rs`. `ProviderApiKey` newtype, `from_resolved()` runs cipherocto-internal prefix denylist (`sk-virtual-`, `sk-cipherocto-`, `sk-cto-`, `CipherOcto-`).
2. **All 32 outbound sites** wired through `attach_bearer()` (proxy.rs: 8 sites; native_http/{openai,replicate,perplexity,databricks,together,groq,mistral,mod}: 27 sites; passthrough_key in proxy.rs:1793: 1 site). Each `expect()` message names the upstream source path so a trip points to the leak.
3. **CI lint extension** at `.github/linters/no-provider-bound-cap.sh`: rejects direct `req_builder.header("Authorization", ...)`, `req_builder.bearer_auth(...)`, raw cipherocto-internal key literals inside Authorization headers. Allowlist covers `octo-core/src/capability.rs` (canonical pub const) + `key_swap_boundary.rs` (test introspection).

### R3 key-swap + boundary hardening (commit follow-up to `da83d8cd`)

Adversarial review after R2 commit `da83d8cd` + R2-doc-align `9ca61025` surfaced 12 follow-up gaps specific to the R2 work; **all closed** in this round:

| R3 ID    | File:Line                                                | Severity | Closure                                                                                                                                                                                                               |
| -------- | -------------------------------------------------------- | -------- | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| C-1 (R3) | `crates/quota-router-core/src/guardrails/mod.rs:489`     | CRITICAL | Wired `ContentModeration::check` through `attach_bearer`; `expect()` message names upstream source path.                                                                                                              |
| C-2 (R3) | `.github/linters/no-provider-bound-cap.sh`               | CRITICAL | Rewritten with 4-shape structural scan (`format!("Bearer`, `.bearer_auth(`, cipherocto-prefix literal anywhere, arbitrary `.header("Authorization", …)` outside helper). Tested against 8 synthetic bypass scenarios. |
| M-1 (R3) | `crates/quota-router-core/src/egress/key_swap.rs:91-100` | MAJOR    | `#[should_panic]` tripwire test + `from_string_unchecked_for_testing` cfg-test seam. Tripwire now actually exercised.                                                                                                 |
| M-2 (R3) | `tests/key_swap_boundary.rs:194`                         | MINOR    | Doc-comment refined: "module-private tuple-struct field" (was "pub(crate)-inaccessible").                                                                                                                             |
| M-3 (R3) | `tests/key_swap_boundary.rs`                             | MAJOR    | Wire-level capture test added: spawn stdlib TCP listener on 127.0.0.1:0, send raw HTTP/1.1, assert server-side `Authorization` is provider key only.                                                                  |
| M-4 (R3) | `.github/linters/no-provider-bound-cap.sh`               | MAJOR    | `.bearer_auth(...)` catch-all broadened; `secret_manager.rs` (AWS SigV4) + `auth/sso/{scim,oauth2,jwt}.rs` (operator IdP) annotated as intentional non-provider routes.                                               |
| M-1 (R1) | `crates/quota-router-core/src/sim.rs`                    | MAJOR    | **Closed 2026-08-01** — sim now has 10 modes (added `KeyExpired`, `Throttle`, `Burst429`, `Garbage`, `InternalError`); `MODE_COUNT = 10` + `mode_count_is_documented` lint tripwire.                                  |
| C-4 (R1) | `crates/quota-router-core/src/ingress.rs`                | CRITICAL | **Closed 2026-08-01** — real `OpenAiIngress` impl (serde_json-driven parse), `IngressError::ProviderError(4xx                                                                                                         | 5xx, body)` covers upstream-error fast-path. 5 unit tests. |
| AC-10    | `tests/eleven_step.rs::cross_impl_tv{1,2}_…`             | MAJOR    | **Closed 2026-08-01** — `impl2` reconciled with `serde_json::to_vec` axes encoding via `manual_axes_canonical`; assertions now `assert_eq!(h1, h2)` (byte-equivalent), not just `assert_ne!(h, [0u8;32])`.            |
| M-3 (R1) | golden fixture                                           | MAJOR    | **Closed** (same row above).                                                                                                                                                                                          |
| M-4 (R1) | `step9_cache_classify`                                   | MAJOR    | **Closed** (same row above).                                                                                                                                                                                          |

### Test surface added (R2 + R3)

- `crates/quota-router-core/src/egress/key_swap.rs` — 9 unit tests (R2: 7 + R3: 2 tripwire)
- `crates/quota-router-core/tests/key_swap_boundary.rs` — 9 integration tests (R2: 7 + R3: 2 wire-level)
- `crates/quota-router-core/src/sim.rs` — `all_kinds_instantiable` extended + `mode_count_is_documented` lint
- `crates/quota-router-core/src/ingress.rs` — 5 unit tests
- `crates/quota-router-core/tests/eleven_step.rs` — `step10_settlement_hash_cross_impl_byte_equivalent` AC-10 closure
- `crates/quota-router-core/tests/goldens.rs` — derives step10 from `SettlementEnvelope::compute_settlement_hash`
- `.github/linters/no-provider-bound-cap.sh` — 4-shape structural scan; allowlist annotations for non-provider routes

### Carryover still open (CLOSED-FOR-NOW vs NEEDS-FOLLOW-UP)

| ID                                                     | Status | Reason                                                                                                              |
| ------------------------------------------------------ | ------ | ------------------------------------------------------------------------------------------------------------------- |
| C-1 (R1) clippy `[disallowed-methods]` table           | **CLOSED 2026-08-01** (R8 corrected) | `clippy.toml` deny table now contains a single entry: `reqwest::Client::new`. The earlier 4-entry table also listed `hyper::Client::new` / `ureq::AgentBuilder::new` / `isahc::HttpClient::new`, but those were **dead code** at the current workspace state: `ureq` and `isahc` are not workspace dependencies, and `hyper::Client::new` is the hyper 0.14 API path which does not exist in the workspace's `hyper = "1.3"` dependency. Clippy emitted "does not refer to a reachable function" warnings on those 3 entries. R8 dropped them; if a future workspace migration adds one of those crates, re-introduce the corresponding deny with the correct API path for the version in use. Defense-in-depth: clippy `disallowed-methods` (in-tree, compile-time) + body-scan job in CI + key-swap denylist (runtime). Per-module `#[allow(clippy::disallowed_methods)]` applied to legitimate provider-egress sites (`proxy.rs`, `native_http/*`), operator-IdP sites (`auth/sso/*`), observability callbacks (`callbacks/*`), secret-manager AWS SigV4 sites, content-moderation sites (`guardrails/mod.rs`), provider health probes (`pre_call_checks.rs`, `node/provider.rs`), and Chrome DevTools Protocol tooling (`whatsapp_chrome_*`). |
| C-3 (R1) 24 `reqwest::Client::new` sites in `proxy.rs` | **CLOSED 2026-08-01** | All provider-egress sites (`proxy.rs` + `native_http/*`) now flow through `attach_bearer` (key swap) and `strip_capability` (capability strip). **R8 updated attach_bearer call count: 40 sites** (8 in `proxy.rs` + 24 in `native_http/*` + 4 in `guardrails/mod.rs` + 4 in tests / e2e_proxy). The clippy `[disallowed-methods]` deny enforces that no NEW HTTP client constructor sites can be added outside the allowlist without explicit review. The 14-module `#[allow(...)]` allowlist is coarse at module granularity — a new `reqwest::Client::new()` call inside an allowlisted module is not caught by clippy. R8 adds `.github/linters/no-new-http-client-constructors.sh` (wired into the `egress-constructors` job in `.github/workflows/exercise-path.yml`) which scans for `reqwest::Client::new` OUTSIDE the allowlist and fails on any occurrence. Sites inside the allowlist are tracked by the module-level `#[allow(...)]` + PR review. |
| 15 workspace clippy cascade                            | **CLOSED 2026-08-01** (R8 hardened) | `cargo clippy --workspace --all-targets --features full -- -D warnings` exits 0. Closure includes: (a) octo-wallet `missing_debug_implementations` redactions for all security-sensitive structs (manual `Debug` impls that redact secret material per the user's explicit constraint "octo-wallet is security sensitive, Debug should not leak in full security related data"); (b) pedantic lints in `octo-wallet/src/capability/{macaroon,discharge}.rs` (`implicit_hasher`, `unnecessary_literal_bound`, `manual_let_else`, `doc_lazy_continuation`, `type_complexity`); (c) quota-router-core `needless_borrow` (27 sites) + `result_unit_err` in `egress.rs::strip_capability` (now infallible); (d) workspace-wide `uninlined_format_args` + `cast_possible_truncation` cleanups. New redaction test suite `crates/octo-wallet/tests/debug_redaction.rs` (13 tests) asserts Debug output never leaks a known marker pattern for: `IdentityKey`, `CapabilityKey`, `InMemorySigner`, `LedgerSigner`, `KeyHierarchy`, `Macaroon`, `CapabilityToken`, `PrivateWitness`, `ProofBundle`, `KeyShare`, `KeystoreFile`, `VaultFile`. **R8 hardening:** 5 of the 13 tests previously asserted only a hex marker (`cccc...`) which is structurally incompatible with `[u8; 32]` Debug (which renders as decimal `[204, 204, ...]`). A regression to `#[derive(Debug)]` on those 5 structs would have silently passed the old test. R8 adds explicit redaction-marker assertions (`[REDACTED 32 bytes]`, `[REDACTED]`, `public_key` field-name tripwire) to each of the 5 weak tests, verified by reverting one struct to derive(Debug) and observing the test fail. |

### Redaction strategy (R4 close-out, 2026-08-01; R8 audit 2026-08-01)

Per the user's explicit constraint that `octo-wallet` is security-sensitive
and Debug must not leak in full security-related data, all security-sensitive
fields are redacted in manual `Debug` impls. The "When" column distinguishes
the 5 structs that were redacted pre-R4 (carried forward, but the test
suite was new in R4) from the 8 structs that R4 added new manual Debug
impls for.

| Struct | Field | Redaction form | When |
| ------ | ----- | -------------- | ---- |
| `IdentityKey` | (substrate) | `finish_non_exhaustive()` + public_key hex | pre-R4 |
| `CapabilityKey` | `[u8;32]` | `"[REDACTED]"` | pre-R4 |
| `KeyShare` | `y: [u8;32]` | `"[REDACTED 32 bytes]"` | pre-R4 |
| `KeystoreFile` | `crypto: Crypto` | `"[REDACTED — encrypted seed blob + MAC + KDF params]"` | pre-R4 |
| `VaultFile` | `salt`, `nonce`, `ciphertext` | `ciphertext_size_bytes` count | pre-R4 |
| `InMemorySigner` | `seed_bytes: [u8;32]` | `"[REDACTED]"` | R4 |
| `LedgerSigner` | (delegates to inner) | inner Debug | R4 |
| `KeyHierarchy` | `identity_seed: [u8;32]` | `"[REDACTED 32 bytes]"` | R4 |
| `Macaroon` | `root_id`, `root_secret_hash`, `id`, `chain` | `"[REDACTED N bytes]"` + `chain_len` | R4 |
| `DischargeMacaroon` | `root_secret_hash`, `chain` | `"[REDACTED 32 bytes]"` + `chain_len` | R4 |
| `CapabilityToken` | `holder_sig`, `macaroon`, `discharges` | `"[REDACTED 64 bytes]"` + propagated | R4 |
| `PrivateWitness` | `cap_root_secret`, `holder_sig` | `"[REDACTED 32 bytes]"` + `"[REDACTED 64 bytes]"` | R4 |
| `ProofBundle` | `stark_proof: Vec<u8>` | `stark_proof_size_bytes` count | R4 |

`ChannelProviderRegistry` (which was missing Debug — item 12) now derives
Debug via `ChannelProvider: std::fmt::Debug` super-trait; provider impls
hold only public operational state (balances, revocation lists, rate
windows).

**R8 audit fix:** the original redaction table grouped all 13 structs
under "R4 close-out" without distinguishing pre-R4 from R4 work. R8
adds the "When" column above so future reviewers can see which redactions
were landed in this mission vs carried forward.

## R9 Audit (2026-08-01)

R9 review focused on half-impl, shortcuts, low-quality tests, and
deferrals specific to the egress boundary + key-swap surface. Six
findings, all resolved across commits `da83d8cd` (R9-1, R9-2, R9-3, R9-5, R9-6) and `<pending>` (R9-4 closure):

| ID      | File:Line                                                                  | Severity | Closure                                                                                                                                                                                                                                                                                                                                                                                                                                                                  |
| ------- | -------------------------------------------------------------------------- | -------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| **R9-1** | `crates/quota-router-core/src/egress.rs:220`                               | CRITICAL | **Resolved.** `strip_capability` now scans `req.body` for capability-token-shaped strings (HMAC-BLAKE3 32-byte hex tags, macaroon 3-segment base64url, `CipherOcto-Cap` scheme). 6 new tests cover hex tag redaction, macaroon wire redaction, scheme redaction in JSON bodies, empty-body no-op, and binary-body redaction. Body length is preserved by space-padding so downstream parsers do not break. Today the proxy builds outbound requests from scratch (so the strip is defense-in-depth); the strip is now explicit at the egress boundary for any future code path that copies inbound content. |
| **R9-2** | `crates/quota-router-core/src/egress.rs:223-235` (strip loop, headers only) | CRITICAL | **Resolved** (same commit as R9-1). `strip_capability_from_body` detects the three canonical capability wire shapes (HMAC-BLAKE3 hex / macaroon 3-segment / `CipherOcto-Cap ` scheme) and redacts in place. Word-boundary checks prevent false positives on hex blobs that are longer than 64 chars.                                                                                                                                                                                                              |
| **R9-3** | `crates/quota-router-core/src/egress/key_swap.rs:84-100`                   | MAJOR    | **Resolved.** `bearer_wire_value` no longer panics on internal-prefix keys — it returns `Err(KeySwapError::CipheroctoInternalLeak { surface: "bearer_wire_value" })`. `attach_bearer` updated to propagate `?`. Tripwire tests converted from `#[should_panic]` to `assert_eq!(err, expected)` assertions. Defense-in-depth: the denylist was unreachable through production paths (gated by `from_resolved`); making it return `Err` removes the DoS-vector foot-gun for any future contributor who short-circuits the path.                                                  |
| **R9-4** | `crates/quota-router-core/src/egress.rs:184-194` (`CapabilityHandle.holder_did`) | MAJOR    | **Resolved (mission-scale fix, 2026-08-01).** `holder_did: String` field DROPPED from `CapabilityHandle`. The `String::new()` initializer at the no-token path (line 286) and the `String::new()` initializer at the strip path (line 299) both removed. Doc comment at lines 188-189 updated. Tripwire test `assert_eq!(handle.holder_did, "")` removed. New doc comment explains R9-4 closure context and points readers at `octo_wallet::capability::wire::deserialize_wire` for the wallet-side parse path. The mint API (`crates/octo-wallet/src/capability/mod.rs:119`) is unchanged: `mint(root_secret, holder, holder_did, caveats, catalog)` preserves the parameter. The wallet-side parsed `CapabilityToken.holder_did` field is the source of truth for the holder identity. Closure path: documented in `docs/research/2026-08-01-dual-mode-workflow-gap-research.md` §Finding F4. |
| **R9-5** | `crates/quota-router-core/src/egress.rs:230` (`v.starts_with(CAPABILITY_HEADER_ALT_PREFIX)`) | MAJOR    | **Resolved.** `v.starts_with(...)` changed to `v.to_ascii_lowercase().starts_with(&CAPABILITY_HEADER_ALT_PREFIX.to_ascii_lowercase())`. Two new tripwire tests assert lowercase + uppercase variants strip correctly. Header NAME was already case-insensitive (RFC 7230 §3.2); scheme VALUE is now also case-insensitive.                                                                                                                                                                                |
| **R9-6** | Mission text: "32 sites (8 in proxy.rs, 24 in native_http/*)"             | MAJOR    | **Resolved.** Mission In Scope §1 line cite updated to "**36 production call sites**" with a per-file breakdown matching the R9-measured counts. New linter `.github/linters/no-attach-bearer-count-drift.sh` compares the live `attach_bearer(` count against the checked-in baseline (36) and fails on drift. Wired as `attach-bearer-drift` job in `.github/workflows/exercise-path.yml`. The earlier "32" count was a doc-bug; the R8 "40" update was also inaccurate. |

### Test surface added (R9)

- `crates/quota-router-core/src/egress.rs` — 6 new tests in unit-tests module:
  - `strip_capability_authorization_alt_lowercase_strips` (R9-5 tripwire)
  - `strip_capability_authorization_alt_uppercase_strips` (R9-5 tripwire)
  - `strip_capability_redacts_hex_tag_from_body` (R9-2 tripwire)
  - `strip_capability_redacts_macaroon_wire_from_body` (R9-2 tripwire)
  - `strip_capability_redacts_cipherocto_cap_scheme_from_body` (R9-2 tripwire)
  - `strip_capability_empty_body_is_noop` (R9-1 tripwire)
  - `strip_capability_redacts_cipherocto_cap_from_binary_body` (R9-1 tripwire)
- `crates/quota-router-core/src/egress/key_swap.rs` — 2 tripwire tests converted from `#[should_panic]` to `assert_eq!(err, ...)` (R9-3 hardening):
  - `bearer_wire_value_tripwire_rejects_internal_prefix`
  - `bearer_wire_value_tripwire_rejects_cipherocto_prefix`
- `.github/linters/no-attach-bearer-count-drift.sh` (NEW, R9-6) — coarse defense against silent boundary regressions
- `.github/workflows/exercise-path.yml` — `attach-bearer-drift` job wires the linter into CI

### Carryover still open after R9

| ID     | Status               | Reason                                                                                                                                                                                                                                                                              |
| ------ | -------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| R9-1 wiring | **PARTIAL**          | `strip_capability_from_body` is now implemented and tested. Today the proxy builds outbound requests from scratch so body-strip is defense-in-depth, not active. Future session should add an explicit `forward()` wrapper that calls `strip_capability` + `attach_bearer` together and wire it into all `attach_bearer` call sites. R9 audit documents this as a follow-up; not deferred silently. |

> **R9-4 closed 2026-08-01.** The `CapabilityHandle.holder_did` field was removed from the public API rather than wired. The capability token's wallet-side canonical form (`octo_wallet::capability::CapabilityToken.holder_did`, populated at mint time from the buyer's DID) remains the source of truth for the holder identity. The egress-side handle is now a thin wrapper around `cap_root_hash` only. Carryover table no longer includes R9-4. See `docs/research/2026-08-01-dual-mode-workflow-gap-research.md` §Finding F4 for the design rationale.

## Mission-level (RFC prerequisites)

| RFC                              | Type                                             | Status                                  | Hard-block?                                   |
| -------------------------------- | ------------------------------------------------ | --------------------------------------- | --------------------------------------------- |
| RFC-0957 — ACCEPTED (2026-07-20) | YES — capability token substrate                 |
| RFC-0959 v1.0                    | Requires                                         | ACCEPTED (2026-07-20; Option A rewrite) | YES — settlement hash + receipt + Ask binding |
| RFC-0009 — ACCEPTED (2026-07-20) | YES — Ed25519 substrate                          |
| RFC-0102 — ACCEPTED (2026-07-20) | YES — wallet substrate for vault one-shot borrow |
| RFC-0853 — ACCEPTED (2026-07-20) | YES — BLAKE3 primitive                           |
| RFC-0126                         | Requires                                         | Accepted                                | No                                            |
| RFC-0862                         | Requires                                         | Accepted                                | No                                            |
| RFC-0909                         | (none)                                           | Accepted                                | No (coexistence only per Option A)            |

**Claim gate (per BLUEPRINT.md):** all "Requires" RFCs above MUST be Accepted before this mission moves from `missions/open/` → `missions/claimed/`.

## Claim Process

Per BLUEPRINT.md:

1. All Requires RFCs reach Accepted (7-day review + 2 maintainer approvals each).
2. ~~Move this mission file to `missions/claimed/0957-b-provider-boundary-exercise-path.md`~~ — **DONE 2026-07-20** (per §Status header).
3. Implementation per RFC-0957 §Implementation Phases Phase 3 + S04 plan §3 Steps 1-10 (R17 fix — consistent with master plan §6 11-step enumeration; session plan enumerates 10 numbered steps with Step 11 embedded in §3 Step 10).
4. PR + review → merge.
5. Exercise path green in CI under `--all-features` per master plan §9 Exit Criteria.

**This mission is NOT claimed as of 2026-07-20.**

---

**Submission Date:** 2026-07-20
**Last Updated:** 2026-07-20
**Version:** 0.2 (Open; **R7 fix:** Status field changed from `Strict-Reading` to `Open` + Availability block added per BLUEPRINT.md mission status convention; **R8 fix:** Version parenthetical aligned with Status field; RFC-0959 v1.0 dependency documented; filename matches master plan naming convention)
