# Seven-Gap Spec→Impl Conformance Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Close 7 gaps between RFCs and `crates/` per `docs/reviews/mission-0909-i-adversarial-review-r1.md` style gap reports.

**Architecture:** TDD, bite-sized commits, one gap per gap_*. Each task = fail test → minimal impl → green → commit. RFC numbers only (no version pins) per referencing convention.

**Tech Stack:** Rust 2024, blake3, ed25519-dalek, stoolap (cipherocto fork), stwo/stwo-sys (libloading), cipherocto-{encoding,policy}, octo-wallet, quota-router-{core,sm-engine,storage}, zk-{circuit,verifier}.

**Reference paths (verified):**
- `crates/cipherocto-policy/src/lib.rs:524` — `is_subgraph(child, parent)` helper exists.
- `crates/octo-wallet/src/capability/caveat.rs:162` — `WrappedOnly { parent_capability }` variant.
- `crates/octo-wallet/src/capability/macaroon.rs:117` — `attenuate(&self, Caveat) -> Self`.
- `crates/quota-router-sm-engine/src/envelope.rs:349` — `check_nesting_depth` scaffold (explicit "not yet supported" comment at line 353).
- `crates/quota-router-core/src/marketplace.rs` — `Marketplace` + `PolicyAttachment`; `cheapest()` BTreeMap stub.
- `crates/quota-router-core/src/zk_verify/capability.rs:37` — `verify_capability_zk` (verify only, no prove).
- `crates/octo-wallet/src/capability/zk_mint.rs` — `mint_with_zk` API leaves NodeType gating.
- `crates/zk-circuit/src/lib.rs` — `CairoProgram` + `compile`.
- RFCs: `rfcs/accepted/economics/0960`, `0962`, `0964`, `0965`, `0967`; `rfcs/draft/economics/0900`, `0918`.

---

## Gap 1 — Capability subgraph check at redeem (RFC-0967 §5)

**Why:** `is_subgraph` exists in policy crate but envelope verifier never calls it. Capability without `capability ⊆ policy` check is unsafe.

**Files:**
- New: `crates/octo-wallet/src/capability/redemption.rs`
- New: `crates/octo-wallet/tests/redemption_subgraph.rs`
- Modify: `crates/octo-wallet/src/capability/mod.rs` (export redemption API)
- Modify: `crates/octo-wallet/Cargo.toml` (add `cipherocto-policy` dep if absent)

### Task 1.1 — Define `RedemptionError` enum

**Files:** `crates/octo-wallet/src/capability/redemption.rs` (new)

**Step 1:** Fail test.

```rust
// crates/octo-wallet/tests/redemption_subgraph.rs
use octo_wallet::capability::redemption::{RedemptionError, redeem_capability};

#[test]
fn redemption_error_has_policy_not_superseded_variant() {
    let _err = RedemptionError::PolicyNotSuperseded { cap_id: [0u8;32], policy_id: [1u8;32] };
}
```

**Step 2:** `cargo test -p octo-wallet --test redemption_subgraph -- redemption_error_has_policy_not_superseded_variant` → FAIL (module missing).

**Step 3:** Implement.

```rust
// crates/octo-wallet/src/capability/redemption.rs
use thiserror::Error;

#[derive(Debug, Error, PartialEq, Eq)]
pub enum RedemptionError {
    #[error("capability {cap_id:?} not a subgraph of policy {policy_id:?}")]
    PolicyNotSuperseded { cap_id: [u8;32], policy_id: [u8;32] },
    #[error("missing PolicyReference caveat on capability")]
    MissingPolicyReference,
    #[error("policy {policy_id:?} not found in catalog")]
    PolicyNotFound { policy_id: [u8;32] },
}
```

**Step 4:** `cargo test -- redemption_error_has_policy_not_superseded_variant` → PASS.

**Step 5:** Commit `feat(octo-wallet): redemption error enum`.

### Task 1.2 — Implement `redeem_capability` with subgraph check

**Files:** `crates/octo-wallet/src/capability/redemption.rs`

**Step 1:** Fail test.

```rust
#[test]
fn redeem_rejects_capability_exceeding_policy() {
    use cipherocto_policy::{PolicyObject, PolicyCatalog, ConstraintSet};
    let cap_policy = build_cap_policy_over_limit(); // 1M tokens
    let org_policy = build_org_policy_under_limit(); // 100k tokens
    let catalog = PolicyCatalog::in_memory();
    let org_id = catalog.insert(org_policy);
    let cap = sample_capability(Some(org_id));
    let err = redeem_capability(&cap, &catalog).unwrap_err();
    assert_eq!(err, RedemptionError::PolicyNotSuperseded { cap_id: cap.id, policy_id: org_id });
}
```

**Step 2:** Run → FAIL (unimplemented).

**Step 3:** Implement.

```rust
pub fn redeem_capability(
    cap: &CapabilityToken,
    catalog: &PolicyCatalog,
) -> Result<(), RedemptionError> {
    let policy_ref = cap.caveats.iter()
        .find_map(|c| match c { Caveat::PolicyReference { policy_id, version } => Some((*policy_id, *version)), _ => None })
        .ok_or(RedemptionError::MissingPolicyReference)?;
    let policy = catalog.get(&policy_ref.0).ok_or(RedemptionError::PolicyNotFound { policy_id: policy_ref.0 })?;
    if !is_subgraph(&cap.to_policy_object(), &policy) {
        return Err(RedemptionError::PolicyNotSuperseded { cap_id: cap.id, policy_id: policy_ref.0 });
    }
    Ok(())
}
```

**Step 4:** Test → PASS.

**Step 5:** Commit `feat(octo-wallet): enforce capability ⊆ policy subgraph at redeem`.

### Task 1.3 — Wire into envelope verify hot-path — **DEFERRED**

**Status (2026-07-24):** **Deferred to a follow-up gap/PR.** No production code in
`crates/octo-network` currently invokes `redeem_capability` or
`CapabilityToken::redeem`. The only `verify_capability` function in
`octo-network` (`mon/nostr_bootstrap.rs:161`) is a Nostr-specific
`DotCapabilityClaim` checker (different type, different namespace).

The `CapabilityToken::redeem` method exists (added in commit `827754a6`)
and is the canonical entry point. A `// TODO: wire into ...` comment in
`crates/octo-wallet/src/capability/redemption.rs` documents the deferral.

**Resume trigger:** when an envelope verifier lands in
`crates/octo-network/src/dot/pce/` that takes `&CapabilityToken` (likely
alongside Gap 2 — MultiEnvelope nesting — per RFC-0962 §7 R8-F5), the
verifier should call `cap.redeem(catalog)` after the structural proof
checks. Until then, callers invoking the new API directly from any new
verifier path will get the subgraph check.

**Files:** Modify `crates/octo-network/src/dot/pce/verify.rs` (new if absent) or wherever `verify_capability` lives.

**Step 1:** Fail test exercising integration. **Step 2:** Run. **Step 3:** Call `redeem_capability` after `verify_holder_sig`. **Step 4:** Pass. **Step 5:** Commit `feat(pce): invoke subgraph check on capability redemption`.

---

## Gap 2 — MultiEnvelope nesting (RFC-0962 §7 R8-F5)

**Why:** `check_nesting_depth` is scaffold only; multi-hop envelopes fail closed.

**Files:** Modify `crates/quota-router-sm-engine/src/envelope.rs` (lines 349–360 + service tree).

### Task 2.1 — Replace scaffold with recursive walk

**Step 1:** Fail test.

```rust
// in crates/quota-router-sm-engine/src/envelope.rs tests
#[test]
fn check_nesting_depth_accepts_two_level_envelope() {
    let child = build_envelope(...);
    let parent = MultiEnvelope { envelopes: vec![child.clone()], nested: Some(Box::new(child)) };
    check_nesting_depth(&parent, 0).unwrap();
}
```

**Step 2:** Run → FAIL.

**Step 3:** Implement.

```rust
pub fn check_nesting_depth(multi: &MultiEnvelope, current_depth: u8) -> Result<(), EnvelopeError> {
    if current_depth >= MAX_NESTING_DEPTH {
        return Err(EnvelopeError::NestingDepthExceeded(current_depth));
    }
    for env in &multi.envelopes {
        check_nesting_depth(env, current_depth + 1)?;
    }
    if let Some(nested) = &multi.nested {
        check_nesting_depth(nested, current_depth + 1)?;
    }
    Ok(())
}
```

**Step 4:** Pass. **Step 5:** Commit `feat(sm-engine): recursive MultiEnvelope nesting depth check (RFC-0962 R8-F5)`.

### Task 2.2 — Update serialization round-trip

**Step 1:** Test nested encode/decode preserves `nested` field. **Step 2-4:** Implement + pass. **Step 5:** Commit.

---

## Gap 3 — ZK batch signature circuit

> **Spec correction (2026-07-24):** the original plan + commits cited
> "RFC-0962 §6" but RFC-0962 §6 is the Lifecycle section. The
> ZK batch signature surface actually targets:
>
> - **RFC-0958** (capability ZK subclass) — `ProofBundle.stark_proof`
>   shape. This is where the capability ZK surface lives; the Gap 3
>   implementation extends `ProofBundle.stark_proof` with a
>   batch-aggregated commitment.
> - **RFC-0962 §9** (ZK proof integration) — `EnvelopeProof` shape on
>   the ExecutionEnvelope side. Not directly modified by Gap 3 (Gap 3
>   is capability-side only); cross-referenced for the downstream
>   envelope that consumes the capability ZK proof.
>
> The original commits' "RFC-0962 §6" reference should be read as
> "RFC-0958 (capability ZK subclass) + RFC-0962 §9 (ZK proof
> integration)". Cannot rewrite git history per project convention; the
> live source comments have been updated to cite RFC-0958.

**Why:** `verify_capability_zk` exists but no proof generation; fuzz harness runs smoke only.

**Files:**
- Modify: `crates/zk-circuit/src/lib.rs` (add batch signature program)
- Modify: `crates/octo-wallet/src/capability/zk_mint.rs` (call prover)
- Modify: `crates/quota-router-core/src/zk_verify/capability.rs` (batch verify)
- Modify: `crates/octo-wallet/Cargo.toml` (enable stwo-sys feature)

### Task 3.1 — Define `BatchSigPublicInputs`

**Files:** `crates/zk-circuit/src/lib.rs`

**Step 1:** Fail test: `pub struct BatchSigPublicInputs { signer_roots: Vec<[u8;32]>, message_root: [u8;32] }`. **Step 3:** Add struct, derive `Serialize/Deserialize`. **Step 5:** Commit `feat(zk-circuit): batch signature public inputs`.

### Task 3.2 — Mock prover for tests

**Step 1:** Fail test calling `prove_batch_signature(Program::BatchSig, inputs, witnesses) -> Proof`. **Step 3:** Implement `prove_batch_signature` that delegates to `stwo-sys` `prove` via libloading when `feature = "full"`, else returns deterministic mock bytes. **Step 5:** Commit `feat(zk-circuit): mock batch signature prover`.

### Task 3.3 — Wire `mint_with_zk` to prover

**Files:** `crates/octo-wallet/src/capability/zk_mint.rs`

**Step 1:** Fail test: `mint_with_zk(N=11 signers)` returns token with attached batch proof. **Step 3:** Generate `BatchSigPublicInputs` from capability caveats, call prover, embed proof in token. **Step 5:** Commit `feat(octo-wallet): 11-step batch ZK mint`.

### Task 3.4 — Extend 11-step test to verify ZK proof

**Files:** `crates/octo-wallet/tests/eleven_step_zk.rs` (new, was 11-step)

Step 1: Assert `verify_capability_zk(token, &verifier).is_ok()` after mint. **Step 5:** Commit `test(octo-wallet): exercise ZK proof generation in 11-step flow`.

---

## Gap 4 — WrappedOnly depth check (RFC-0960 §8)

**Why:** `WrappedOnly { parent_capability }` exists but `attenuate` appends no parent-walk/cycle guard.

**Files:** Modify `crates/octo-wallet/src/capability/macaroon.rs` (line 117 `attenuate`).

### Task 4.1 — `MacaroonError::WrappedCycle`

**Step 1:** Fail test: `WrappedOnly` referencing ancestor triggers `WrappedCycle`. **Step 3:** Add variant. **Step 5:** Commit.

### Task 4.2 — Walk parent chain on attenuate

**Step 1:** Fail test: chain of 3 WrappedOnly → `check_max_depth` (cap=8) ok; chain length 9 → fail. **Step 3:**

```rust
fn check_wrapped_depth(macaroon: &Macaroon, count: u8) -> Result<(), MacaroonError> {
    if count >= MAX_WRAPPED_DEPTH { return Err(MacaroonError::WrappedDepthExceeded(count)); }
    if let Some(parent_id) = macaroon.parent_capability() {
        if parent_id == macaroon.id { return Err(MacaroonError::WrappedCycle); }
        // caller re-resolves parent from catalog; this fn takes count only
    }
    Ok(count + 1)
}
```

**Step 5:** Commit `feat(octo-wallet): enforce WrappedOnly depth + cycle guard`.

### Task 4.3 — Cycle detection across catalog

**Step 1:** A→B→A chain fails. **Step 3:** Maintain visited set in `attenuate`. **Step 5:** Commit.

---

## Gap 5 — Marketplace full order book + escrow + slashing (RFC-0900)

**Why:** `cheapest(model)` is BTreeMap stub; §Order Book, §Escrow Flow, §Slashing Model unimplemented.

**Files:**
- Modify: `crates/quota-router-core/src/marketplace.rs`
- New: `crates/quota-router-core/src/marketplace/orderbook.rs`
- New: `crates/quota-router-core/src/marketplace/escrow.rs`
- New: `crates/quota-router-core/src/marketplace/slashing.rs`
- New: `crates/quota-router-core/tests/marketplace_e2e.rs`

### Task 5.1 — `OrderBook` data structure

**Step 1:** Fail test: `place_bid`/`place_ask` + `match_top()`. **Step 3:** Use `BTreeMap<(Price, Ts), Order>` for price-time priority. **Step 5:** Commit.

### Task 5.2 — Escrow state machine

**Step 1:** State enum `EscrowState { Pending, Locked, Settled, Disputed, Slashed }`. **Step 3:** Transitions per RFC-0900 §Escrow Flow. **Step 5:** Commit.

### Task 5.3 — Slashing model

**Step 1:** Fail test: provider misses SLA → slash `stake * miss_rate`. **Step 3:** Implement `slash(provider_id, amount, reason)`. **Step 5:** Commit.

### Task 5.4 — End-to-end match+escrow+settle test

**Step 1:** buyer places bid, provider places ask, book matches, escrow locks, settle releases funds. **Step 5:** Commit `test(marketplace): end-to-end order book + escrow + settlement`.

### Task 5.5 — Disconnect `cheapest()` from BTreeMap-only path

**Step 1:** Replace `cheapest` with `OrderBook::best_ask()` lookup. **Step 5:** Commit.

---

## Gap 6 — Inference Task Market (RFC-0918)

**Why:** Full RFC, no impl.

**Files:**
- New: `crates/quota-router-core/src/task_market.rs`
- New: `crates/quota-router-core/tests/task_market.rs`
- New: RFC-0918 → `crates/quota-router-core/src/task_market/{orders,escrow,slashing,dispute}.rs`

### Task 6.1 — `TaskType` enum + `TaskSpec`

**Step 1:** Test constructors. **Step 3:** Enum `TaskType::{Inference, Embedding, FineTune, Eval}`. **Step 5:** Commit.

### Task 6.2 — Order book (reuse Gap 5)

**Step 1:** Task market wraps `OrderBook<TaskSpec>`. **Step 3:** Implement. **Step 5:** Commit.

### Task 6.3 — Escrow + dispute resolution

**Step 1:** Tests cover happy path + dispute path. **Step 3:** Implement. **Step 5:** Commit.

### Task 6.4 — Slashing (reuse Gap 5.3)

**Step 1:** Provider underperforms → slash. **Step 5:** Commit.

### Task 6.5 — Acceptance test

**Step 1:** Full inference task: place → match → execute → settle → release. **Step 5:** Commit `test(task-market): full RFC-0918 inference flow`.

### Task 6.6 — Promote RFC-0918 to Accepted

After impl lands + green tests + review, move `rfcs/draft/economics/0918-inference-task-market.md` → `rfcs/accepted/economics/`.

---

## Gap 7 — Quota marketplace depth (RFC-0900)

**Why:** Partial; only index lookup.

**Files:** Modify `crates/quota-router-core/src/marketplace.rs` (extend `Marketplace`).

### Task 7.1 — Provider scoring circuit-breaker

**Step 1:** Fail test: provider falls below reputation threshold → excluded from `cheapest`. **Step 3:** Add `ProviderScore` + circuit breaker. **Step 5:** Commit.

### Task 7.2 — Latency-aware ranking

**Step 1:** Fail test: lower latency beats cheaper price when `prefer_latency=true`. **Step 3:** Weighted score. **Step 5:** Commit.

### Task 7.3 — Promote RFC-0900 to Accepted

After gaps 5+7 land + tests pass, move `rfcs/draft/economics/0900-ai-quota-marketplace.md` → `rfcs/accepted/economics/`.

---

## Cross-Cutting

### Task X.1 — `cargo clippy --all-targets --all-features -- -D warnings`

Run per gap close. Zero tolerance.

### Task X.2 — `cargo fmt`

Run before any commit per repo memory.

### Task X.3 — Update `docs/grand-design.md`

Note conformant features + link RFCs (no version pins in prose).

### Task X.4 — `docs/reviews/mission-XXX-i-adversarial-review-r1.md` → close

After implementation, mark gaps 1-7 resolved. New round = `r2.md`.

---

## Execution Order

Recommended pipeline (each stage's tests must be green before next starts):

1. **Gap 4** (WrapppedOnly) — smallest, unblocks grand-design §8.
2. **Gap 1** (subgraph) — verifier hardening.
3. **Gap 2** (MultiEnvelope) — RFC-0962 §7 R8-F5.
4. **Gap 3** (ZK batch) — depends on Gap 2 envelope integrity.
5. **Gap 5** (marketplace core) — unblocks gaps 6 + 7.
6. **Gap 6** (task market) — depends on Gap 5.
7. **Gap 7** (quota depth) — depends on Gap 5.

After each gap: run full `cargo test -p <changed>`, then `cargo clippy --all-targets --all-features -- -D warnings`, then `cargo fmt --check`.

## Out-of-Scope

- RFC-0955 on-chain binding (separate Phase J, see `quota-router-risk-closures-2026-07-23.md`).
- LLM-provider adapters (RFC-0902-0907).
- LiteLLM parity beyond marketplace pricing axes.

## Risks

- **Gap 3 ZK real circuit:** stwo-sys may not yet ship on stable arch. Default to mock prover behind `full` feature; ship feature-off. Acceptance = mock path end-to-end green.
- **Gap 5/6 lock-in:** OrderBook BTreeMap is fine ≤1k providers; revisit if perf gate fails.
- **Gap 7 RFC promotion:** RFC-0900 acceptance requires all stakeholder sign-offs; this plan only implements the code.

## Done When

- All 7 gaps closed + tasks committed.
- `cargo test --workspace` green.
- `cargo clippy --all-targets --all-features -- -D warnings` clean.
- `cargo fmt --check` clean.
- RFCs 0900 + 0918 promoted to `rfcs/accepted/economics/`.
- `docs/grand-design.md` updated to reflect new conformance.
- Adversarial review round 2 closed.
