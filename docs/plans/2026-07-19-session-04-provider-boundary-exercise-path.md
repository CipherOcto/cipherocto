# Session 04 — Provider Boundary + 11-Step Exercise Path

**Date:** 2026-07-19 (authored)
**R7 fix (2026-07-20):** updated to reflect S04 audit + RFC-0959 v1.0 Option A rewrite + mission file authorship
**R2 fix (2026-08-01):** key-swap boundary structurally enforced (commit `da83d8cd`). §3 Step 1 + §2 Decisions + §5 Exit Criteria updated. Mission 0957-b §R1 Audit + R2 Fix block carries the per-finding status table.
**Phase coverage:** D (boundary), G (test scaffolding)
**Master plan:** `docs/plans/2026-07-19-identity-master-plan.md` §0 (naming convention per §0 line 18 mission file pattern)
**Depends on:** session 01 (wallet), session 02 (capability token), session 03 (Ask + pricing)
**Unblocks:** session 05 (ZK circuit verification via exercise path), Phase G/H/I start
**Authoritative exercise step list:** master plan §6 11-step table (Steps 1-11); session plan §3 below has Steps 1-10 (Step 11 = reputation delta + ledger append documented in §3 Step 10 per R14 fix)

---

## 0. BLUEPRINT Workflow Gate (mandatory per docs/BLUEPRINT.md)

Per master plan §0, this session requires:

| Gate                                                                              | Required (target state for mission claim per BLUEPRINT.md)                                                                                                                                                                          | Current status (2026-07-21)                                                                                                                                                                    |
| --------------------------------------------------------------------------------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Use Case                                                                          | `docs/use-cases/enhanced-quota-router-gateway.md` (provider boundary + capability-token egress transform)                                                                                                                           | ✓ exists                                                                                                                                                                                       |
| RFC-0957 (Capability Token Format) status                                         | **Accepted**                                                                                                                                                                                                                        | ✓ Accepted 2026-07-20 (`rfcs/accepted/economics/0957-capability-token-format.md`, commit `b1bf0ed7`); 7-day review + 2 maintainer approvals completed                                          |
| RFC-0959 v1.0 (Independent Settlement Chain — Option A rewrite 2026-07-20) status | **Accepted**                                                                                                                                                                                                                        | ✓ Accepted 2026-07-20 (`rfcs/accepted/economics/0959-ask-settlement-chain.md`, commit `9385d98c`); independent chain (NOT RFC-0909 amendment); 7-day review + 2 maintainer approvals completed |
| RFC-0903 (Virtual API Key System) status                                          | **Accepted** (gate requires)                                                                                                                                                                                                        | ✓ Accepted                                                                                                                                                                                     |
| Mission file                                                                      | `missions/claimed/0957-b-provider-boundary-exercise-path.md` (R4 fix: filename matches master plan §4 naming; was originally listed as `provider-boundary-exercise-path.md` per S04 plan v0; corrected during S04 audit 2026-07-20) | ✓ Authored 2026-07-20 + Claimed 2026-07-20 (commit `d4f25531`); S04 mission v0.1; multi-round adversarial review R1-R5 convergence (2 consecutive 0-finding rounds: R4 refined + R5 re-run)    |
| Claim                                                                             | Move mission to `missions/claimed/`                                                                                                                                                                                                 | ✓ Done 2026-07-20                                                                                                                                                                              |
| Implement                                                                         | `crates/quota-router-core/src/egress/` + `src/ingress/` + `src/sim/` + `tests/exercise/eleven_step.rs`                                                                                                                              | ⏳ S04 Steps 1-10 pending execution                                                                                                                                                            |

**Required prerequisite actions:**

1. ✓ S01, S02, S03 RFCs Accepted 2026-07-20 (RFC-0102, RFC-0009, RFC-0957, RFC-0959 v1.0).
2. ✓ Upstream missions verified completed (S01 + S02 + S03 missions all claimed 2026-07-20).
3. ✓ Mission file `missions/claimed/0957-b-provider-boundary-exercise-path.md` authored 2026-07-20 + claimed 2026-07-20; cites RFC-0957 + RFC-0959 v1.0 as Requires + upstream missions as Depends on.
4. ⏳ Implement per §3 Steps 1-10 of this session plan (Step 11 = reputation + ledger append documented in §3 Step 10; authoritative 11-step list per master plan §6 table).
5. ⏳ After PR merges, capture 11-step exercise as `tests/exercise/eleven_step.rs` running in CI per `.github/workflows/exercise-path.yml`.

**Cross-crate edit warning:** S04 edits `crates/quota-router-core/` (existing) — does NOT create a new crate. Workspace already lists it. No workspace `Cargo.toml` edit required; just `mod` declarations in `crates/quota-router-core/src/lib.rs` + feature flags.

---

## 1. Goal

Wire the **provider boundary** (CipherOcto internal ↔ provider opaque) and turn the **11-step exercise path** into an executable CI spec. When this session closes:

**Pre-existing crate note:** `crates/quota-router-core/` **already exists** as of 2026-07-19 (sibling crates are `octo-core`, `octo-adapter-*`, `quota-router-cli`, `quota-router-pyo3`, `quota-router-integration-tests`). Current src layout: `lib.rs`, `admin.rs`, `auth/`, `balance.rs`, `cache.rs`, `callbacks/`, `config.rs`, `fallback.rs`, `guardrails/`, `health.rs`, `key_rate_limiter.rs`, `keys/`, `logging.rs`, `metrics.rs`, `middleware.rs`, `model.rs`, `mode.rs`, `native_http/`, `node/`, `pre_call_checks.rs`, `pricing.rs`, `prompts/`, `providers.rs`, `proxy.rs`, `py_bridge/`, `python_sdk_entry/`, `rate_limit.rs`, `router.rs`, `schema.rs`, `secret_manager.rs`, `shared_types.rs`, `storage.rs`, `testing/`, `tracing.rs`, `types.rs`. Session 04 **adds** the missing modules under the same crate: `src/egress/`, `src/ingress/`, `src/marketplace/`, `src/settle/`, `src/receipt/`, `src/sim/`. No `cargo new`; just `mod` declarations + feature flags in `Cargo.toml`. Session 01's `octo-wallet/` crate is a new sibling created from scratch (also not pre-existing).

- Single egress module — only place provider HTTP can run
- Single ingress module — only place provider response gets internalised
- Provider boundary lint forbids any other HTTP egress to provider hosts
- Provider simulator produces realistic responses (200/429/401/timeout/schema-change)
- The 11-step exercise runs green in CI on every PR
- **R2 fix:** cipherocto-internal key (admin master_key / virtual API key / capability token) NEVER egresses to a provider. Single egress helper `egress::key_swap::attach_bearer` wraps every outbound `Authorization` attachment with brand-typed `ProviderApiKey` + cipherocto-internal prefix denylist.

---

## 2. Decisions Locked This Session

| #   | Decision                                                                              | Rationale                                                                                                                                                                                                                                                                                                                                                                                                                |
| --- | ------------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| 1   | Egress point = `crates/quota-router-core/src/egress/{openai,anthropic,google}/mod.rs` | Lint-enforced; prevents cap_token egress                                                                                                                                                                                                                                                                                                                                                                                 |
| 2   | Ingress point = `crates/quota-router-core/src/ingress/`                               | Single handler for provider response normalisation                                                                                                                                                                                                                                                                                                                                                                       |
| 3   | Capability token strip in egress                                                      | `X-Capability-Token` header dropped; cap_root_hash logged                                                                                                                                                                                                                                                                                                                                                                |
| 4   | Provider key fetch = one-shot borrow from `octo-wallet::vault`                        | Plaintext never held long                                                                                                                                                                                                                                                                                                                                                                                                |
| 5   | Provider simulator = in-repo feature                                                  | `provider-sim` feature flag; gate prod paths                                                                                                                                                                                                                                                                                                                                                                             |
| 6   | Exercise path = `tests/exercise/eleven_step.rs`                                       | All 11 steps traced; outputs to JSON fixture for replay                                                                                                                                                                                                                                                                                                                                                                  |
| 7   | Provider key rotation = capability mint rejects stale                                 | Wallet event → cap caveat propagation                                                                                                                                                                                                                                                                                                                                                                                    |
| 8   | Boundary lint = `clippy.toml` deny rule                                               | Forbid any `reqwest::Client::new()` not inside egress module                                                                                                                                                                                                                                                                                                                                                             |
| 9   | **R2 fix (2026-08-01):** Key-swap boundary = `egress::key_swap::attach_bearer` helper | Brand-typed `ProviderApiKey` newtype + cipherocto-internal prefix denylist; every outbound `Authorization` attachment (32 sites: 8 in `proxy.rs`, 24 in `native_http/*`) routes through the helper. CI lint `.github/linters/no-provider-bound-cap.sh` extended to reject `req_builder.header("Authorization", …)` / `req_builder.bearer_auth(...)` / raw cipherocto-internal key literals inside Authorization headers. |

---

## 3. Steps

### Step 1 — Egress module

- [ ] `crates/quota-router-core/src/egress/mod.rs` defining `EgressTransform`
- [ ] Per-provider sub-modules: `egress::openai`, `egress::anthropic`, `egress::google`
- [ ] `EgressTransform::forward(req: InboundRequest, slot: VaultSlotRef) → Result<OutboundRequest>`
- [ ] Strip: `X-Capability-Token`, `Authorization` (cipherocto variants), cipherocto-specific headers
- [ ] Reshape request body to provider schema (per provider format)
- [ ] Sign egress with provider's slot key from vault (one-shot borrow)
- [ ] Lint: `clippy.toml` rule `[disallowed-methods]` covering ALL HTTP client constructors outside egress: `reqwest::Client::new`, `hyper::Client::new`, `ureq::Agent::new`, `isahc::HttpClient::new` (R1 fix: per RFC-0957 §Adversary A5 line 685, deny list MUST cover all HTTP crates, not only reqwest); runtime backtrace assertion in `egress::client()` constructor as escape hatch
- [x] **R2 fix (commit `da83d8cd`):** key-swap boundary structurally enforced. `egress::key_swap::attach_bearer(&str) -> Result<String, KeySwapError>` is the single egress entry-point. 32 sites wired through it (`proxy.rs` ×8 + `native_http/*` ×24). Brand-typed `ProviderApiKey` + cipherocto-internal prefix denylist (`sk-virtual-`, `sk-cipherocto-`, `sk-cto-`, `CipherOcto-`). CI lint shell-script `.github/linters/no-provider-bound-cap.sh` rejects any `format!("Bearer {}", …)` Authorization attachment outside the helper. Test surface: 7 unit tests in `egress/key_swap.rs` + 7 integration tests in `tests/key_swap_boundary.rs`.

### Step 2 — Ingress module

- [ ] `crates/quota-router-core/src/ingress/mod.rs`
- [ ] `IngressTransform::normalise(provider_kind, raw_response) → NormalisedResponse`
- [ ] Detect cache-hit per provider response metadata
- [ ] Call `cache::classify(prompt)` against local cache; reconcile with provider metadata
- [ ] Forward to settlement engine (**RFC-0959 v1.0 independent settlement chain**; `crates/octo-core/src/settlement.rs::settlement_hash` + `build_receipt`) (R1 fix: anchor to RFC-0959 v1.0, not just "session 03")
- [ ] Attach cap_root_hash + ask_id + invocation_hash metadata for receipt building (RFC-0959 v1.0 §Data Structures `SettlementEvent` fields)

### Step 3 — Provider boundary lint

- [ ] `clippy.toml` with `[disallowed-methods]` (R1 finding C-1: file is doc-only at HEAD; structural lift deferred — see Exit Criteria R2 row)
- [ ] CI step: `cargo clippy --workspace --all-targets -- -D warnings` includes lint
- [ ] Custom deny via `proc-macro2` if needed (escape hatch if clippy lints not enough)
- [x] **R2 fix:** `.github/linters/no-provider-bound-cap.sh` extended with key-swap boundary structural scan:
  - Rejects any `req_builder.header("Authorization", format!("Bearer {}", …))` outside `crates/quota-router-core/src/egress/key_swap.rs`
  - Rejects any `req_builder.bearer_auth(...)` outside the helper
  - Rejects any raw cipherocto-internal key literal (`sk-virtual-`, `sk-cipherocto-`, `sk-cto-`, `CipherOcto-`) inside an `Authorization` header across `crates/`
  - Allowlist: `octo-core/src/capability.rs` (canonical `CAPABILITY_HEADER` pub const), `key_swap_boundary.rs` (test introspection), `egress.rs`/`egress/` (the strip point + helper itself)
- [ ] **Body-field linter (R3 fix per mission 0957-b In Scope item 3):** scan request/response bodies for CapabilityToken-shaped strings (HMAC-BLAKE3 32-byte tags + macaroon caveat structure) in cookie, JSON, form-encoded, and protobuf fields; CI deny if detected outside egress
- [ ] Test: introduce a `reqwest::Client::new()` outside egress in a test PR; CI must fail

### Step 4 — Provider key rotation handling

- [ ] `octo-wallet::vault::on_rotation(slot_id) → RotationEvent`
- [ ] `quota_router_core::marketplace` listens; invalidates ASKs referencing old slot
- [ ] Active capabilities bound to old slot: 1h grace; new mints rejected post-grace
- [ ] Audit log: rotation event + grace expiry

### Step 5 — Provider simulator

- [ ] `crates/quota-router-core/src/sim/` module behind `feature = "provider-sim"`
- [ ] Toggle modes: `200-ok` (success), `throttle`, `429-burst`, `key-expired`, `schema-change`, `timeout`, `garbage`, **`internal-error`** (R1 fix: 8th mode = `internal-error` for provider 500 responses with provider-specific error schema; matches RFC-0959 v1.0 §Adversary A5 mitigation surface; **R19 fix:** mode names kebab-case to avoid "normal / 200" parse ambiguity; **exactly 8 modes**)
- [ ] Seeded RNG for reproducibility
- [ ] CLI: `quota-router-cli sim --provider openai --mode throttle --seed 42`
- [ ] Tests: 8 modes × 100 calls each; assertions on behavior
- [ ] **Cross-implementation verification (R3 fix per RFC-0959 v1.0 §Test Vectors §Property test matrix):** implement ≥ 2 independent verification impls (e.g., Rust reference + Python `pyca/cryptography` + `blake3` PyPI) producing identical 32-byte settlement_hash + receipt_id digests for TV1 + TV2; owner = RFC-0959 v1.0 promotion reviewer; deadline = RFC-0959 acceptance PR merge

### Step 6 — Exercise path specification

Create the executable spec:

```rust
// crates/quota-router-core/tests/exercise/eleven_step.rs

// R11 + R16 + R17 fix: import statements at module top bring all R11+R16+R17-flagged identifiers into scope.
// **R17 fix — forward-looking import scope:** The use statements below reference modules
// (quota_router_core::egress, quota_router_core::ingress, quota_router_core::settlement,
// quota_router_core::wallet, quota_router_core::provider_kind, crate::testing) that do NOT
// yet exist in the repo. These are forward-looking imports for the implementation that
// will be created when S04 mission 0957-b is claimed per §Implementation Phases Phase 1.
// The current quota-router-core crate has the skeleton modules listed in master plan §4
// Phase D + Phase F but not the egress/ingress/settlement/wallet/provider_kind/testing
// submodules referenced below. Each missing submodule is part of the S04 mission
// implementation work (not spec). **R18 fix:** egress + ingress added to the list
// (created by this session per Step 1 + Step 2 + pre-existing crate note line 43).
use std::result::Result;
use time::OffsetDateTime;
use quota_router_core::settlement::{
    AxesConsumed, ConsumedReceiptIndex, SettlementEvent,
    build_receipt, canonical_ser, compute_cost,
};
use quota_router_core::egress::{EgressTransform, InboundRequest};
use quota_router_core::ingress::IngressTransform;
use quota_router_core::provider_kind;
use quota_router_core::wallet::{
    AskBinding, Before, CachePolicy, DID_ALICE, Jurisdiction, OptIn, ProviderKeyRef,
};
use crate::testing::{BuyerJuris, TestCtx, TestTrace, marketplace_lookup, sim_sso_login, test_ctx};

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn exercise_path_11_steps() -> Result<()> {
    let ctx: TestCtx = test_ctx().await?;
    let trace = TestTrace::new();

    // R8+R10 fix: declare all helper bindings before use (per RFC-0959 v1.0 §Algorithms + S04 plan §3 Step 6 fixture requirements)
    let wallet = ctx.wallet();
    let alice_did = DID_ALICE;
    let identity_key = wallet.identity_key(&alice_did)?;
    let ask = marketplace_lookup(...).await?;   // fixture
    let router_identity = ctx.router_identity();
    let router_did = router_identity.did();
    let ingress = IngressTransform::new();
    let reputation = ctx.reputation();
    let ledger = ctx.ledger();
    // R10 fix: declare provider_sim + escrow (used in Steps 4 + 6)
    let provider_sim = ctx.provider_sim().await?;
    let escrow = ctx.escrow();
    // R12 fix: declare buyer_juris (used in Step 3 mint_capability call)
    let buyer_juris = BuyerJuris::default();  // default fixture; real impl per RFC-0957 §3.5.7 jurisdiction handling
    fn current_unix() -> u64 { OffsetDateTime::now_utc().unix_timestamp() as u64 }

    // Step 1: SSO login (mock IdP)
    let sso_token = sim_sso_login(&ctx, "alice@example.com").instrument(trace.step(1)).await?;
    assert_eq!(sso_token.subject_did, DID_ALICE);

    // Step 2: Mint virtual API key (RFC-0903)
    let vkey = wallet.mint_virtual_key(alice_did, /*...*/).instrument(trace.step(2)).await?;
    assert!(vkey.prefix.starts_with("sk-"));

    // Step 3: Mint capability token (macaroon ::AskBinding)
    let cap = wallet.mint_capability(
        identity_key,
        AskBinding { ask_id: ask.id, model: ask.model, axes: ask.axes, max_total: 50_000_000, cache_policy: OptIn },
        vec![
            ProviderKeyRef { provider: provider_kind::OpenAI, slot: "openai-prod" },
            Before::now_plus(3600),
            Jurisdiction::intersect(buyer_juris, ask.jurisdictions),
        ],
    ).instrument(trace.step(3)).await?;

    // ... etc

    // R9 fix: expand Steps 4-8 (previously condensed to "// ... etc") so `response` is declared.
    // Step 4: POST /v1/chat/completions + Authorization + X-Capability-Token
    // R13 fix: EgressTransform::new() and IngressTransform::new() are constructors per RFC-0957 §Implementation Phases Phase 3; both added to ingress.rs / egress.rs module spec alongside ::forward / ::normalise / ::cache_classify methods
    let egress = EgressTransform::new();
    let outbound = egress.forward(InboundRequest::new(/*...*/), wallet.vault_slot("openai-prod")?)?;
    let raw_response = provider_sim.openai().call(&outbound).await?;
    // Step 5: Marketplace lookup cheapest matching Ask (fixture; already declared at top)
    let _cheapest_ask = marketplace_lookup(...).await?;
    // Step 6: OCTO-W escrow pre-auth
    let _escrow_id = escrow.pre_auth(&cap, ask.axes.values().sum()).await?;
    // Step 7: Egress transform (strip cap, attach provider key, send — done in Step 4)
    // Step 8: Provider returns (HTTP, opaque — `raw_response` from Step 4)
    let response = ingress.normalise(provider_kind::OpenAI, raw_response).instrument(trace.step(8)).await?;

    // Step 9: Cache-classify + axes_consumed (RFC-0959 v1.0 §Specification)
    let axes_consumed = ingress.cache_classify(&response).instrument(trace.step(9)).await?;
    // R1 fix: include settlement_hash + receipt_id build in exercise (RFC-0959 v1.0 §Algorithms).

    // Step 10: Receipt build — RFC-0959 v1.0 settlement hash + envelope signature + ConsumedReceiptIndex insert
    // R6 fix: declare consumed_index in scope before build_receipt call (per RFC-0959 v1.0 §Algorithms signature).
    let mut consumed_index = ConsumedReceiptIndex::new();
    // R7 fix: declare csprng in scope before nonce construction (per RFC-0959 v1.0 §Data Structures `nonce = csprng.next_u64().to_le_bytes() ++ current_unix.to_le_bytes()`).
    let csprng = rand::rngs::StdRng::from_entropy(); // seeded CSPRNG; for test determinism, use seeded RNG per fixture
    let event = SettlementEvent {
        cap_root_hash: cap.root_hash,
        ask_id: ask.id,
        invocation_hash: response.invocation_hash,
        axes_consumed: axes_consumed.clone(),
        cost: compute_cost(&ask, &axes_consumed)?,
        settled_at_unix: current_unix(),
    };
    // R2 fix: nonce MUST be exactly 16 bytes per RFC-0959 v1.0 §Data Structures:
    // `nonce: [u8; 16] = csprng.next_u64().to_le_bytes() ++ current_unix.to_le_bytes()` (8 + 8 = 16).
    let settled_at = current_unix();
    let nonce: [u8; 16] = {
        let mut n = [0u8; 16];
        n[..8].copy_from_slice(&csprng.next_u64().to_le_bytes());
        n[8..].copy_from_slice(&settled_at.to_le_bytes());
        n
    };
    let receipt = build_receipt(&router_identity, &router_did, event, nonce, settled_at, &mut consumed_index)
        .instrument(trace.step(10)).await?;
    assert_eq!(receipt.envelope.receipt_id, blake3::hash(&canonical_ser((&receipt.envelope.event, &receipt.envelope.nonce, receipt.envelope.settled_at_unix)).unwrap()).into());
    // Replay defense: second insert of same receipt_id must fail
    assert!(consumed_index.try_insert(&router_did, receipt.envelope.receipt_id).is_err());

    // Step 11: Reputation delta + ledger append
    reputation.apply_settlement(&receipt).instrument(trace.step(11)).await?;
    ledger.append(&receipt).await?;

    Ok(())
}
```

`TestTrace` records every step into a JSON fixture; same fixture passes through `quota-router-cli settle-replay --expected-hash` for determinism check.

### Step 7 — Test fixtures

- [ ] `tests/fixtures/exercise/` = JSON of expected outputs per step (goldens)
- [ ] `INSTA` for snapshot assertions
- [ ] `MESSY_YAML_FOR_ASK` fixture = 10 ASKs across providers

### Step 8 — CI integration

- [ ] `.github/workflows/exercise-path.yml` runs on every PR
- [ ] Jobs: build / test / clippy / fuzz-min-5min / exercise-goldens
- [ ] `cargo clippy --workspace --all-targets -- -D warnings` mandatory gate
- [ ] cache-poison regression test (preventing cache_hit_rate > 90% false-positive)

### Step 9 — Adapters self-test

- [ ] Each egress sub-module ships an `adapter-self-test` test: spin up local provider-sim on random port; round-trip 5 calls; compare signatures and token counts
- [ ] Rate-limit backoff integration: sim returns 429 then 200; egress must respect backoff

### Step 10 — Adjudication

- [ ] Document each adapter test in `quota-router-core/tests/exercise/README.md`

---

## 4. Validation

```bash
# Run the exercise
cargo test -p quota-router-core --test eleven_step --features provider-sim

# Lint the boundary
cargo clippy --workspace --all-targets -- -D warnings

# Replay determinism
cargo run -p quota-router-core -- settle-replay \
    --log-path tests/fixtures/exercise/last_run.json \
    --expected-hash $(jq -r '.final_hash' tests/fixtures/exercise/last_run.json)

# Provider-sim sweep
cargo test -p quota-router-core --features provider-sim -- sim::all_modes --nocapture
```

## 5. Exit Criteria

- [ ] Egress module exists; structural key-swap + capability-strip + HTTP-client-constructor deny rules active (covers reqwest+hyper+ureq+isahc per RFC-0957 A5)
  - **R2 fix:** key-swap is structurally enforced via `egress::key_swap::attach_bearer` (32 sites wired; brand-typed `ProviderApiKey`; cipherocto-internal prefix denylist). Capability-strip enforced by `egress::strip_capability` + `egress_boundary.rs` test (6/6 green). HTTP-client-constructor deny lifted to a follow-up session (clippy.toml is documentation-only at HEAD; CI enforcement is via the lint shell-script scan + key-swap denylist).
- [ ] Ingress module exists; cache-classify wired to RFC-0959 v1.0 settlement engine (`crates/octo-core/src/settlement.rs`)
- [ ] Provider simulator: 8 modes deterministic; tests pass
- [ ] 11-step exercise: green at HEAD (Steps 1-11 incl. Step 10 settlement_hash + receipt_id + ConsumedReceiptIndex replay detection per R1 fix)
- [ ] Goldens captured + checked-in
- [ ] `clippy --workspace --all-targets -- -D warnings` clean
- [ ] Provider key rotation event flow works; old caps expire within grace
- [x] **R2 fix:** Key-swap boundary enforced; `cargo test -p quota-router-core --test key_swap_boundary` green at HEAD with 7/7 integration tests; `cargo test --lib egress::key_swap` green with 7/7 unit tests; `bash .github/linters/no-provider-bound-cap.sh` exits 0.
- [ ] Master plan Exit Criteria checkpoint: Phase F (11-step green in CI) + Provider boundary green — **aligns with mission `0957-b-provider-boundary-exercise-path.md` Acceptance Criteria items 1-10** (R1 fix: explicit mission AC cross-reference):
  - AC-1: Egress module exists; clippy deny active (reqwest+hyper+ureq+isahc)
  - AC-2: Ingress module exists; cache-classify wired to RFC-0959 v1.0
  - AC-3: Provider simulator 8 modes deterministic
  - AC-4: 11-step exercise green at HEAD
  - AC-5: Goldens captured + checked-in
  - AC-6: `clippy --workspace --all-targets -- -D warnings` clean
  - AC-7: Provider key rotation event flow works
  - AC-8: Master plan Phase F + provider boundary checkpoint green
  - AC-9: ConsumedReceiptIndex replay defense verified end-to-end
  - AC-10: Cross-implementation verification per RFC-0959 v1.0 §Test Vectors (≥2 independent implementations, TV1+TV2)

## 6. Out-of-Scope (this session only)

- ZK capability circuit exercise → session 05
- On-chain settlement discharge flow → RFC-0955 future
- Hardware wallet integration → Phase H
- MPC threshold keys → Phase I

## 7. Risks for This Session

| Risk                                                                                                                                                                                                                                                            | Mitigation                                                                                                                                                                                                                                                                                                            |
| --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Boundary lint missed by some path                                                                                                                                                                                                                               | Test: deliberately introduce violation → must fail CI                                                                                                                                                                                                                                                                 |
| Provider-sim divergence from real provider (R3 fix)                                                                                                                                                                                                             | Owner = S04 claimant; format = JSON diff of sim response vs real OpenAI/Anthropic response schema; weekly diff runs out-of-CI; escalation = if delta > 5% on any field, file `quota-router-cli/sim-drift.md` issue + notify maintainer review board                                                                   |
| Capability strip leaks via non-header path (e.g., cookie, body)                                                                                                                                                                                                 | Body linter (per Step 3 sub-item): forbid CapabilityToken-shaped strings in body fields; CI deny if detected outside egress                                                                                                                                                                                           |
| Test goldens get brittle                                                                                                                                                                                                                                        | INSTA `insta-allow` discipline; no auto-update outside session; golden updates require maintainer reviewer approval + delta rationale in PR description (signal-vs-noise drift distinguished by hash change > 1 byte OR new axis added OR capability schema bump); review step mandatory before accepting new goldens |
| Concurrent exercise path step race (R3 fix)                                                                                                                                                                                                                     | Synchronization: shared state (ConsumedReceiptIndex, marketplace index, Anti-Fraud Monitor sliding-window) wrapped in `parking_lot::Mutex`; per-step isolation via `tokio::sync::mpsc` channel pass-through; TestTrace captures state at each step boundary via `Arc<Mutex<>>` snapshots                              |
| **Hard-block RFC promotion delay** (R1 fix) — RFC-0957, RFC-0959 v1.0, RFC-0009, RFC-0102, RFC-0853 all Draft as of 2026-07-20; mission claim blocked on ALL reaching Accepted per BLUEPRINT.md "Missions REQUIRE an approved RFC" rule + master plan §0 gate 5 | Track each RFC's Draft → Accepted promotion timeline; escalate via maintainer review board; coordinate parallel PR review windows; document progress in master plan §0 weekly checkpoint                                                                                                                              |

## 8. Post-Session

- Tag `session-04-provider-boundary-and-exercise-path`
- Update master plan
- Create follow-up: priority list of missing capabilities surfaced by exercise
- Hand off to session 05 (ZK capability circuit + STWO production)

## 9. R1 Audit + R2 Key-Swap Fix (2026-08-01)

Same-mode adversarial review (commit `411bf8be` for 0957-a, then `da83d8cd` for 0957-b). R1 surfaced concrete gaps between documented spec and on-disk code; R2 addressed the key-swap subset. Full per-finding status table lives in mission `0957-b-provider-boundary-exercise-path.md` §"R1 Audit + R2 Fix (2026-08-01)". This section tracks which R1 findings the R2 commit moved.

### R2 (key-swap boundary) — `da83d8cd`

| R1 ID       | On-disk before R2                                                                                                 | After R2                                                                                                                                                                          |
| ----------- | ----------------------------------------------------------------------------------------------------------------- | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| C-1 partial | `clippy.toml` empty; `.github/linters/no-provider-bound-cap.sh` only scanned capability-token shape, not key swap | Lint script extended to scan key-swap bypass; runtime denylist at `egress::key_swap::attach_bearer` is the actual enforcement. `clippy [disallowed-methods]` table lift deferred. |
| C-2 partial | `egress.rs` is a single 406-line file; per-provider subdirs missing                                               | Egress boundary is **structurally** in `proxy.rs` + `native_http/*.rs`; `attach_bearer` is the canonical swap entry-point. No file rename — `egress.rs` is now the types module.  |
| C-3 partial | 24 `reqwest::Client::new` sites in `proxy.rs`                                                                     | Egress-side swap is hard-enforced; HTTP-constructor deny is the parallel concern (clippy `[disallowed-methods]`) deferred.                                                        |

### R1 carryover status (post-R3)

| R1 ID | Status @ R3 (2026-08-01)                                                                                                                                                                                                                             |
| ----- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| C-1   | OPEN. Lint shell-script + key-swap denylist are the current defense; clippy is the durable closure.                                                                                                                                                  |
| C-4   | **Closed 2026-08-01** — real `OpenAiIngress::parse` impl (serde_json-driven) lives at `crates/quota-router-core/src/ingress.rs`; `eleven_step.rs::step9_cache_classify` re-wired to delegate.                                                        |
| M-1   | **Closed 2026-08-01** — `sim.rs` now has 10 modes (5 baseline + 5 R3-added: `KeyExpired`, `Throttle`, `Burst429`, `Garbage`, `InternalError`). `MODE_COUNT` constant + `mode_count_is_documented` lint tripwire.                                     |
| M-3   | **Closed 2026-08-01** — golden fixture re-pinned to real `SettlementEnvelope::compute_settlement_hash` (was stub `b"settlement-mock"`); `step10_settlement_hash_cross_impl_byte_equivalent` test asserts fixture matches canonical canonicalization. |
| M-4   | **Closed 2026-08-01** — coupled to C-4; `step9_cache_classify` now delegates to `OpenAiIngress`. Error-mode (4xx/5xx/malformed) returns zero-usage placeholder.                                                                                      |
| M-5   | **Closed 2026-08-01** — `Egress::send` trait doc-comment upgraded (structural placeholder); production egress flow documented as `proxy.rs` + `native_http/*` direct reqwest. `prepare_outbound` helper added.                                       |
| AC-10 | **Closed 2026-08-01** — `impl2` reconciled with `serde_json::to_vec` axes encoding via `manual_axes_canonical`. `cross_impl_tv{1,2}` assertions now `assert_eq!(h1, h2)` (byte-equivalent), not `assert_ne!(h, [0u8;32])` (non-zero only).           |
| 15    | OPEN. Workspace clippy fails (octo-wallet `#![warn(missing_debug_implementations)]` cascade to 12 errors). Not 0957-b-specific; needs multi-crate cleanup.                                                                                           |

### R3 follow-ups (post-R2 review) — all closed

| R3 ID    | File:Line                                                | Severity | Closure                                                                                                                                                                                                  |
| -------- | -------------------------------------------------------- | -------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| C-1 (R3) | `crates/quota-router-core/src/guardrails/mod.rs:489`     | CRITICAL | Wired `ContentModeration::check` through `attach_bearer`; expect message names upstream source path.                                                                                                     |
| C-2 (R3) | `.github/linters/no-provider-bound-cap.sh`               | CRITICAL | Rewritten with 4-shape structural scan. Tested against 8 synthetic bypass scenarios (all caught).                                                                                                        |
| M-1 (R3) | `crates/quota-router-core/src/egress/key_swap.rs:91-100` | MAJOR    | `#[should_panic]` tripwire + `from_string_unchecked_for_testing` cfg-test seam.                                                                                                                          |
| M-2 (R3) | `tests/key_swap_boundary.rs:194`                         | MINOR    | Doc-comment refined: "module-private tuple-struct field".                                                                                                                                                |
| M-3 (R3) | `tests/key_swap_boundary.rs` (new wire-level tests)      | MAJOR    | Stdlib TCP capture server on 127.0.0.1:0; raw HTTP/1.1 send; assert server-side Authorization is provider key only.                                                                                      |
| M-4 (R3) | `.github/linters/no-provider-bound-cap.sh`               | MAJOR    | `.bearer_auth(...)` catch-all broadened (no `req_builder` prefix needed); `secret_manager.rs` (AWS SigV4) + `auth/sso/{scim,oauth2,jwt}.rs` (operator IdP) annotated as intentional non-provider routes. |
