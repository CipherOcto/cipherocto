# Quota Router — Clean Inbound Architecture & Fake-Binary Removal

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Fix the broken `NodeTransport` inbound path with a clean, layered, symmetric design; remove the fake `quota-router-node` binary and its fake L3 cross-process TCP test; codify a "no fake tests, no workarounds" policy across RFCs, missions, implementation, and the test suite.

**Architecture:** `QuotaRouterNode` owns its `QuotaRouterHandler` as an internal member. `QuotaRouterNodeBuilder::build()` constructs both and wires the handler into `NodeTransport::register_receiver` so callers receive a single, fully-wired node. Inbound and outbound both flow through `QuotaRouterNode` (`route()` for outbound, `receive()` for inbound), with `NodeTransport` as the boundary into the wire.

**Tech Stack:** Rust (tokio, async-trait, blake3), `octo-transport`, `quota-router` library, `quota-router-e2e-tests` integration test crate.

---

## 0. New Policy (codified across RFCs, missions, and code)

This policy must be stated in every relevant RFC and in the implementation as a doc-comment at the module level. Test fixtures and test-only binaries that exist solely to make tests "appear" to exercise production code are forbidden.

### Policy: Test honesty

> Tests must target the production library. A test that constructs a separate binary, subprocess, or fixture only to verify behavior the library itself owns is **not a valid test** — it is theatre. If a test reveals that production code is missing, untestable, or unreachable from the public API, **stop and raise the design concern**. Do not paper over the gap with a hack.

### Policy: Symmetric data flow

> Every node has two API surfaces: `route()` (outbound) and `receive()` (inbound). Both flow through `NodeTransport`. Both are reachable directly from the public `QuotaRouterNode` API. Internal layering (`NodeTransport` → `NetworkReceiver` → handler) is an implementation detail; the public API is the node.

### Policy: One source of truth

> There is exactly one definition of `QuotaRouterNode` in the codebase. Duplicates (e.g., `lib.rs` vs `mod.rs`) are bugs and must be resolved before this plan is considered complete.

---

## Architectural Decisions (made in this plan — confirm before executing)

| # | Decision | Rationale |
|---|---|---|
| D1 | `QuotaRouterNode` owns a `handler: Arc<QuotaRouterHandler>` field | Symmetric inbound/outbound, single builder call, no caller-side wiring |
| D2 | `QuotaRouterNodeBuilder::build()` returns `Result<QuotaRouterNode, RouterNodeError>` (single value) | Already the case in code; aligns implementation with truth |
| D3 | Add `pub async fn QuotaRouterNode::receive(&self, payload: &[u8], ctx: &ReceiveContext) -> Result<(), TransportError>` | Public inbound API; symmetric to `route()` |
| D4 | DELETE the `quota-router-node` binary crate (`quota-router-e2e-tests/quota-router-node/`) | Production binary does not exist; it was a hack |
| D5 | DELETE `quota-router-e2e-tests/tests/l3_tcp_basic.rs` | It tested a fake binary; not a real test |
| D6 | DEFER Mission 0870g (L3 cross-process TCP) and Mission 0870i (TCP adapter) to `missions/open/` with a "needs design discussion" note | Cross-process TCP is a real future concern, but not solved by a fake binary |
| D7 | Resolve `lib.rs` vs `mod.rs` duplicate `QuotaRouterNode` (delete `mod.rs`, keep `lib.rs`) | One source of truth |
| D8 | Tests come AFTER implementation, not before | User directive; avoids the previous mistake of writing tests against unfinished APIs |
| D9 | Implement `GovernedTransport::receive()` to call `inner.dispatch()` after governance checks | Plan §3b of the prior iteration was skipped; this iteration completes it |

---

## Phase 1: RFC Amendments

All RFC cross-references must use the bare number (per CLAUDE.md), never version pins in prose. Version-history tables are the documented exception.

### Task 1.1: Update RFC-0870 — remove fake binary, fix builder semantics

**Files:**
- Modify: `rfcs/accepted/networking/0870-distributed-quota-router-network.md`

**Step 1:** In the "Architecture" / "Components" section, DELETE any mention of a standalone `quota-router-node` binary or process model that depends on it. Replace with: "The library `quota-router` is the production artifact. It is consumed by the Python SDK (via PyO3) and the quota-router CLI. There is no separate `quota-router-node` binary."

**Step 2:** In the builder section (§ around line 1074-1119), REWRITE the example to show:
```rust
let node = QuotaRouterNode::builder()
    .node_id(id)
    .network_id(nid)
    .provider(...)
    .peer(...)
    .policy(...)
    .forwarding(...)
    .build()?;
// node.transport is wired to node.handler internally.
// Outbound: node.route(...).
// Inbound: node.receive(payload, ctx).
```
Replace the previous `(QuotaRouterNode, QuotaRouterHandler)` tuple claim — the builder returns a single, fully-wired node.

**Step 3:** Add a "Public API" subsection listing exactly:
- `pub async fn route(&self, request: RequestContext) -> Result<RouteDecision, RouterNodeError>`
- `pub async fn receive(&self, payload: &[u8], ctx: &ReceiveContext) -> Result<(), TransportError>`
- `pub fn builder() -> QuotaRouterNodeBuilder`

**Step 4:** Add a "Test Policy" subsection (mirror Section 0 above) referencing `octo-transport` and `quota-router` library tests only.

**Step 5:** Update the Version History table to add a row: v1.13 — "Removed fictitious `quota-router-node` binary from architecture. Builder returns single `QuotaRouterNode` (handler is internal member). Added public `QuotaRouterNode::receive()` API. Added test policy."

**Step 6:** Search the file for `RFC-0863 v` and `RFC-0870 v` in prose. Strip the version pin in each occurrence. Version-history rows may keep numbers.

**Step 7:** Commit: `git add rfcs/accepted/networking/0870-distributed-quota-router-network.md && git commit -m "docs(rfc-0870): remove fake binary, fix builder semantics, add public API"`

### Task 1.2: Update RFC-0863 — NodeTransport ownership semantics

**Files:**
- Modify: `rfcs/accepted/networking/0863-general-purpose-network-integration.md`

**Step 1:** In the `NodeTransport` section, add a sentence: "Typical callers register a single receiver that owns the inbound dispatcher (e.g., `QuotaRouterNode`'s internal handler). `NodeTransport` does not assume one receiver or many; both are supported."

**Step 2:** Search the file for `RFC-0863 v` in prose and strip version pins.

**Step 3:** Update Version History: add row v1.8 — "Clarified NodeTransport receiver-ownership semantics: any number of receivers, typical usage is one owned by the consumer (e.g., QuotaRouterNode)."

**Step 4:** Commit: `git add rfcs/accepted/networking/0863-general-purpose-network-integration.md && git commit -m "docs(rfc-0863): clarify NodeTransport receiver ownership"`

### Task 1.3: Update RFC-0863p-a — confirm GovernedTransport.receive() contract

**Files:**
- Modify: `rfcs/accepted/networking/0863p-a-domain-governed-transport.md`

**Step 1:** Find the `GovernedTransport::receive()` section. Rewrite the spec to state:
> `pub async fn receive(&self, payload: &[u8], ctx: &ReceiveContext) -> Result<(), TransportError>` — runs governance checks (kick detection, domain binding). On pass, calls `self.inner.dispatch(payload, ctx)`. On fail, returns `TransportError::GovernanceViolation(reason)`.

**Step 2:** Search for `RFC-0863p-a v` in prose and strip version pins.

**Step 3:** Update Version History: add row v0.1.3 — "Confirmed `GovernedTransport::receive()` contract: governance checks → `inner.dispatch()`."

**Step 4:** Commit: `git add rfcs/accepted/networking/0863p-a-domain-governed-transport.md && git commit -m "docs(rfc-0863p-a): confirm GovernedTransport.receive() contract"`

---

## Phase 2: Mission Updates

### Task 2.1: Move Mission 0870g to `missions/deferred/` and rewrite

**Files:**
- Move: `missions/claimed/0870g-l3-cross-process-tcp-e2e.md` → `missions/deferred/0870g-l3-cross-process-tcp-e2e.md`
- Modify: same file (now in deferred/)

**Rationale:** Mission was based on a fake binary. Move out of `claimed/` (no longer in active work). Do not delete — preserve as record of the design discussion that needs to happen.

**Step 1:** `git mv missions/claimed/0870g-l3-cross-process-tcp-e2e.md missions/deferred/0870g-l3-cross-process-tcp-e2e.md`

**Step 2:** Rewrite the mission content to: (a) explain why it is deferred — "this mission previously relied on a `quota-router-node` binary that does not exist in production; cross-process TCP needs a real design discussion before re-scoping", (b) list open design questions: cross-process trust boundary, sender-id wire framing, deployment model, (c) leave acceptance criteria blank.

**Step 3:** Commit: `git add missions/deferred/0870g-l3-cross-process-tcp-e2e.md && git commit -m "missions: defer 0870g cross-process TCP pending real design"`

### Task 2.2: Move Mission 0870i to `missions/deferred/` and rewrite

**Files:**
- Move: `missions/open/0870i-tcp-adapter-for-quota-router.md` → `missions/deferred/0870i-tcp-adapter-for-quota-router.md`

**Step 1:** `git mv missions/open/0870i-tcp-adapter-for-quota-router.md missions/deferred/0870i-tcp-adapter-for-quota-router.md`

**Step 2:** Rewrite: "TCP adapter is part of the cross-process TCP design discussion (see Mission 0870g). Until that design is settled, this mission is deferred."

**Step 3:** Commit: `git add missions/deferred/0870i-tcp-adapter-for-quota-router.md && git commit -m "missions: defer 0870i TCP adapter pending 0870g"`

### Task 2.3: Rewrite Mission 0870c — match new builder semantics

**Files:**
- Modify: `missions/claimed/0870c-consumer-integration-bootstrap.md`

**Step 1:** Update the "Wiring" example to show the new builder pattern (single value return, no caller-side handler wiring). Show both directions:
```rust
let node = QuotaRouterNode::builder()...build()?;
// Outbound: node.route(ctx).await?;
// Inbound: node.receive(payload, &recv_ctx).await?;
// (handler is internal — no manual registration required)
```

**Step 2:** Update the `QuotaRouterHandler` struct definition shown in the mission: `pub struct QuotaRouterHandler { node: Arc<QuotaRouterNode>, provider: Arc<dyn LocalProvider>, network_key: [u8; 32] }`. Remove the standalone `transport` field (already done in the diff).

**Step 3:** Strip any remaining version pins in prose (`RFC-0863 v`, `RFC-0870 v`).

**Step 4:** Update acceptance criteria: add `QuotaRouterNode::receive()` is reachable as `pub async fn`.

**Step 5:** Commit: `git add missions/claimed/0870c-consumer-integration-bootstrap.md && git commit -m "missions(0870c): rewrite wiring to single-node builder"`

### Task 2.4: Update Mission 0863b — register_receiver + dispatch acceptance

**Files:**
- Modify: `missions/claimed/0863b-node-transport.md`

**Step 1:** Add acceptance criteria:
- `NodeTransport::register_receiver()` appends to the `receivers` vec
- `NodeTransport::dispatch()` with empty receivers returns `Ok(())`
- `NodeTransport::dispatch()` iterates receivers in registration order
- `NodeTransport::dispatch()` fails fast on the first receiver error (returns first `Err`, does not invoke subsequent receivers)

**Step 2:** Strip version pins in prose.

**Step 3:** Commit: `git add missions/claimed/0863b-node-transport.md && git commit -m "missions(0863b): add dispatch acceptance criteria"`

### Task 2.5: Update Mission 0863d — register_receiver is implemented

**Files:**
- Modify: `missions/claimed/0863d-dotgateway-fanout-receiver.md`

**Step 1:** Remove the "Handlers register with NodeTransport (future)" note. Replace with: "Handlers register with `NodeTransport` via `register_receiver()`. This is implemented in `QuotaRouterNode::builder().build()` and is no longer a future concern."

**Step 2:** Strip version pins.

**Step 3:** Commit: `git add missions/claimed/0863d-dotgateway-fanout-receiver.md && git commit -m "missions(0863d): register_receiver is implemented"`

### Task 2.6: New Mission 0870m — `QuotaRouterNode::receive` public API

**Files:**
- Create: `missions/claimed/0870m-quota-router-receive-public-api.md`

**Step 1:** Write the mission content:

```markdown
# Mission 0870m — QuotaRouterNode::receive() Public API

## Goal
Define and test the public inbound API of `QuotaRouterNode`.

## Public API
- `pub async fn QuotaRouterNode::receive(&self, payload: &[u8], ctx: &ReceiveContext) -> Result<(), TransportError>`
- Behavior: delegates to `self.transport.dispatch(payload, ctx)` (the production seam).
- Symmetric to `pub async fn QuotaRouterNode::route(...)`.

## Acceptance Criteria
- [ ] `QuotaRouterNode::receive()` exists and is `pub async`.
- [ ] `QuotaRouterNode::receive(payload, ctx)` returns the same result as `self.transport.dispatch(payload, ctx)`.
- [ ] Handler is registered automatically by `QuotaRouterNodeBuilder::build()` (no caller-side wiring required).
- [ ] Empty receivers → `dispatch()` returns `Ok(())` and so does `receive()`.
- [ ] Documented in RFC-0870 v1.13 "Public API" subsection.

## Tests (added in Phase 4)
- `tests/inbound_api_happy_path.rs`
- `tests/inbound_api_hmac_failure.rs`
- `tests/inbound_api_ttl_exceeded.rs`
- `tests/inbound_api_capacity_exhausted.rs`
```

**Step 2:** Commit: `git add missions/claimed/0870m-quota-router-receive-public-api.md && git commit -m "missions(0870m): define QuotaRouterNode::receive() public API"`

---

## Phase 3: Implementation (TDD; tests come at the end but we verify with focused unit tests during impl)

### Task 3.1: Resolve the `lib.rs` / `mod.rs` duplicate

**Files:**
- Delete: `quota-router/src/mod.rs`
- Verify: `quota-router/src/lib.rs` is canonical

**Step 1:** `diff -u quota-router/src/lib.rs quota-router/src/mod.rs > /tmp/libmod.diff`. Identify which file is more complete (longer, more fields, more methods).

**Step 2:** Read both top-to-bottom to confirm which is the source of truth. Expected: `lib.rs` (956 lines) is the canonical one based on the `QuotaRouterNode` struct including `handler.rs` imports the new code from.

**Step 3:** `git rm quota-router/src/mod.rs` (or `rm` if not tracked).

**Step 4:** Run `cargo check -p quota-router` (with the workspace exclusion still in place, build via `-p`). Expect: errors if `mod.rs` defined items referenced from `lib.rs`. Resolve by porting missing items into `lib.rs`.

**Step 5:** Run `cargo check -p quota-router-e2e-tests` (if tests reference `mod.rs` items). Resolve any compile errors by aligning with `lib.rs`.

**Step 6:** Commit: `git add -A quota-router/src/ && git commit -m "refactor(quota-router): remove duplicate mod.rs, lib.rs is canonical"`

### Task 3.2: Add `handler` field to `QuotaRouterNode` (TDD)

**Files:**
- Modify: `quota-router/src/lib.rs:80-101`
- Test: `quota-router/src/lib.rs:645+` (existing test module — add a new test)

**Step 1:** Write the failing test inside the existing `#[cfg(test)] mod tests` block (lib.rs ~line 645):
```rust
#[test]
fn node_has_internal_handler_after_build() {
    let node = QuotaRouterNode::builder()
        .node_id(RouterNodeId([1u8; 32]))
        .network_id(NetworkId([2u8; 32]))
        .provider(test_provider())
        .build()
        .unwrap();
    // Verify node has a registered handler. We assert by sending an
    // inbound forward request envelope and checking the peer cache is
    // updated (proves dispatch went through the handler).
    // For now, simpler: assert node.transport.dispatch_count > 0 after
    // a synthetic dispatch — OR add a `handler_registered()` accessor.
    assert!(node.handler.is_some(), "QuotaRouterNode must own its handler");
}
```

**Step 2:** Run the test:
```bash
cd /home/mmacedoeu/_w/ai/cipherocto && cargo test -p quota-router --lib node_has_internal_handler_after_build -- --nocapture
```
Expected: FAIL — `node.handler` does not exist yet.

**Step 3:** Modify `QuotaRouterNode` (lib.rs:80):
- Add `pub(crate) handler: Arc<QuotaRouterHandler>` field.
- (For testability, we may want a `pub fn handler_registered(&self) -> bool` accessor or expose `handler` as `pub`. Decide: `pub` for the same reason `transport` is `pub` — tests need to inspect it. Confirm with reviewer.)

**Step 4:** Run the test again. Expected: still FAIL — builder does not set `handler`. Move to Task 3.3.

### Task 3.3: Wire handler in `QuotaRouterNodeBuilder::build` (TDD)

**Files:**
- Modify: `quota-router/src/lib.rs:599-643`

**Step 1:** Inside `build()`, after creating the `transport`, construct the handler:
```rust
let provider: Arc<dyn LocalProvider> =
    Arc::new(provider::HttpLocalProvider::new(self.providers[0].clone()));
let handler = Arc::new(QuotaRouterHandler::new(
    Arc::new(node_arc), // need a self-reference; see Step 2
    provider.clone(),
    network_key,
));
transport.register_receiver(handler.clone() as Arc<dyn NetworkReceiver>);
```

**Step 2:** Solve the self-reference problem. `QuotaRouterHandler::new` needs `Arc<QuotaRouterNode>` but `QuotaRouterNode` doesn't exist yet. Two options:
- **(a)** Build `node` as `Arc<QuotaRouterNode>` first, construct handler from `Arc::clone(&node)`, then populate `handler` field on `node` (requires `Arc<Mutex<...>>` for that one field, OR using `Arc::new_cyclic`).
- **(b)** Refactor `QuotaRouterHandler::new` to take `Arc<QuotaRouterNode>` constructed via `Arc::new_cyclic` inside `build()`.

Recommended: option (b). Use `Arc::new_cyclic`:
```rust
let node = Arc::new_cyclic(|weak| {
    let transport_clone = transport.clone();
    let primary_provider = primary_provider.clone();
    let node_inner = QuotaRouterNode {
        config: RouterNodeConfig { ... },
        state: RouterNodeLifecycle::Init,
        transport: transport_clone,
        gossip_cache: GossipCache::new(),
        peer_cache: PeerCache::new(),
        pending: PendingRequests::new(),
        identity_key,
        primary_provider,
        rate_limiter: ratelimit::RateLimiter::new(100, 500),
        metrics: Some(metrics::QuotaRouterMetrics::new()),
        active_forwards: ...::new(0),
        request_seq: ...::new(0),
        handler: weak.clone().upgrade().map(Arc::new).unwrap_or(/* see below */),
    };
    // Replace `handler` after constructing with Arc to self
    node_inner
});
let handler = Arc::new(QuotaRouterHandler::new(Arc::clone(&node), primary_provider.clone(), network_key));
node.handler = Arc::clone(&handler);
transport.register_receiver(handler.clone() as Arc<dyn NetworkReceiver>);
```

Note: `handler` field type must be `Arc<QuotaRouterHandler>`. Since `QuotaRouterHandler::new` takes `Arc<QuotaRouterNode>`, the cyclic construction above gives the handler a weak Arc that it upgrades when needed. Alternative cleaner refactor: make `handler` lazy — store an `OnceCell<Arc<QuotaRouterHandler>>` and fill it after both are constructed.

**Step 3:** Choose the cleanest implementation. The author MUST show the chosen pattern with full code in the implementation, not in this plan.

**Step 4:** Run unit test from Task 3.2. Expected: PASS.

**Step 5:** Run full `cargo test -p quota-router --lib`. Expected: all pass (no regressions).

**Step 6:** Commit: `git add quota-router/src/lib.rs && git commit -m "feat(quota-router): wire handler internally via builder"`

### Task 3.4: Add `QuotaRouterNode::receive()` public method

**Files:**
- Modify: `quota-router/src/lib.rs:200+`

**Step 1:** Add the method:
```rust
impl QuotaRouterNode {
    /// Public inbound API: dispatch a payload through NodeTransport
    /// to all registered receivers (which includes this node's own
    /// handler, registered by `QuotaRouterNodeBuilder::build()`).
    pub async fn receive(
        &self,
        payload: &[u8],
        ctx: &octo_transport::receiver::ReceiveContext,
    ) -> Result<(), octo_transport::sender::TransportError> {
        self.transport.dispatch(payload, ctx).await
    }
}
```

**Step 2:** Add unit test:
```rust
#[tokio::test]
async fn receive_delegates_to_transport_dispatch() {
    let node = QuotaRouterNode::builder()
        .node_id(RouterNodeId([1u8; 32]))
        .network_id(NetworkId([2u8; 32]))
        .provider(test_provider())
        .build()
        .unwrap();
    let ctx = octo_transport::receiver::ReceiveContext {
        source_transport: "test".into(),
        mission_id: [0u8; 32],
        sender_id: None,
    };
    // Empty receivers → Ok
    let r = node.receive(&[0xFF], &ctx).await;
    assert!(r.is_ok(), "empty payload path returned error: {:?}", r);
}
```

**Step 3:** Run `cargo test -p quota-router --lib receive_delegates_to_transport_dispatch -- --nocapture`. Expected: PASS.

**Step 4:** Commit: `git add quota-router/src/lib.rs && git commit -m "feat(quota-router): add QuotaRouterNode::receive() public API"`

### Task 3.5: Implement `GovernedTransport::receive()`

**Files:**
- Modify: `octo-transport/src/governed_transport.rs`

**Step 1:** Read the current file to understand the structure.

**Step 2:** Find any placeholder / TODO around `receive`. Replace with:
```rust
pub async fn receive(
    &self,
    payload: &[u8],
    ctx: &octo_transport::receiver::ReceiveContext,
) -> Result<(), octo_transport::sender::TransportError> {
    if !self.passes_governance(ctx).await {
        return Err(octo_transport::sender::TransportError::GovernanceViolation(
            "kick detected or domain mismatch".into(),
        ));
    }
    self.inner.dispatch(payload, ctx).await
}
```

**Step 3:** Add a unit test in `octo-transport` that: (a) constructs a `GovernedTransport` wrapping a `NodeTransport` with a `MockReceiver` registered, (b) calls `receive()` with a valid context, (c) asserts the mock receiver was called, (d) calls `receive()` with a kicked context, (e) asserts `GovernanceViolation` error.

**Step 4:** Run `cargo test -p octo-transport --lib`. Expected: new test PASS, no regressions.

**Step 5:** Commit: `git add octo-transport/src/governed_transport.rs && git commit -m "feat(octo-transport): implement GovernedTransport::receive()"`

### Task 3.6: DELETE the fake `quota-router-node` binary crate

**Files:**
- Delete: `quota-router-e2e-tests/quota-router-node/` (entire directory)

**Step 1:** Confirm the binary is not imported anywhere else:
```bash
grep -rn "quota-router-node\|quota_router_node" --include="*.rs" --include="*.toml" /home/mmacedoeu/_w/ai/cipherocto/
```
Expected: only the binary's own files match.

**Step 2:** `git rm -r quota-router-e2e-tests/quota-router-node/`

**Step 3:** Commit: `git commit -m "chore: remove fake quota-router-node binary crate (never existed in production)"`

### Task 3.7: DELETE the fake L3 cross-process TCP test

**Files:**
- Delete: `quota-router-e2e-tests/tests/l3_tcp_basic.rs`

**Step 1:** `git rm quota-router-e2e-tests/tests/l3_tcp_basic.rs`

**Step 2:** Run `cargo test -p quota-router-e2e-tests --test l2_basic_routing` (one of the L2 tests). Expected: passes (unaffected by L3 deletion).

**Step 3:** Commit: `git commit -m "chore: remove fake L3 cross-process TCP test (relied on fake binary)"`

### Task 3.8: Refactor L2 test harness to use `node.receive()` instead of manual `transport.dispatch`

**Files:**
- Modify: `quota-router-e2e-tests/src/lib.rs:220-285`

**Step 1:** Update `dispatch_with_sender` to call `node.receive(payload, ctx)` instead of `node.transport.dispatch(payload, ctx)`. This exercises the public API (not the internals), making the harness independent of `transport`'s exact surface.

**Step 2:** Remove the manual `register_receiver()` call (lines 232-236) — the builder now does this.

**Step 3:** Remove the `handler` field from `TestNode` struct if it is no longer used by tests directly. (Inspect usage: if tests use `node.handler` directly, keep it as a borrowed accessor; otherwise remove.)

**Step 4:** Update the `MockLocalProvider` plumbing if the test was constructing it externally — the builder should handle provider construction. OR keep the test's manual provider injection if that's needed for test isolation.

**Step 5:** Run `cargo test -p quota-router-e2e-tests`. Expected: all L2 tests pass with no behavioral change.

**Step 6:** Commit: `git add quota-router-e2e-tests/src/lib.rs && git commit -m "refactor(e2e-tests): use node.receive() public API, remove manual handler wiring"`

### Task 3.9: Update `Cargo.toml` workspace exclude list (if needed)

**Files:**
- Modify: `Cargo.toml` (workspace root)

**Step 1:** Confirm `quota-router-e2e-tests/quota-router-node/` was not in the workspace members (it was excluded). No change needed unless the deletion requires cleanup.

**Step 2:** Run `cargo metadata --format-version=1 --no-deps` to confirm workspace is consistent.

**Step 3:** Commit only if changes were made.

---

## Phase 4: E2E Tests (added AFTER implementation lands)

These tests target the production `quota-router` library via the public `QuotaRouterNode::receive()` API. No fake binaries, no subprocesses, no special test harnesses beyond the existing L2 `TestNode`.

### Task 4.1: Inbound API happy path

**Files:**
- Create: `quota-router-e2e-tests/tests/l2_inbound_happy_path.rs`

**Step 1:** Write the test:
```rust
//! Verify QuotaRouterNode::receive() dispatches a valid envelope
//! through the production inbound path: payload → NodeTransport.dispatch
//! → registered receiver (handler) → handle_forward_request.

use quota_router::builder_support::test_node; // or use TestNode from crate
use quota_router_e2e_tests::TestCluster;

#[tokio::test]
async fn receive_dispatches_forward_request_through_handler() {
    let cluster = TestCluster::new(2).await;
    let node = &cluster.nodes[0];
    // Build a valid 0xC3 (ForwardRequest) envelope with a known peer id.
    let payload = build_forward_request_envelope(node.node_id, peer_id);
    let sender = Some(peer_id.0);
    node.receive(&payload, &ctx_with_sender(sender)).await.unwrap();
    // Assert: the dispatch reached the handler — peer cache updated.
    assert!(node.node.peer_count() >= 1);
}
```

**Step 2:** Run `cargo test -p quota-router-e2e-tests --test l2_inbound_happy_path -- --nocapture`. Expected: PASS.

**Step 3:** Commit: `git add quota-router-e2e-tests/tests/l2_inbound_happy_path.rs && git commit -m "test(e2e): inbound API happy path"`

### Task 4.2: HMAC failure path

**Files:**
- Create: `quota-router-e2e-tests/tests/l2_inbound_hmac_failure.rs`

**Step 1:** Write a test that sends a gossip envelope (0xC6) with a tampered HMAC, asserts the handler returns an error and the gossip cache is unchanged.

**Step 2:** Run. Expected: PASS.

**Step 3:** Commit.

### Task 4.3: TTL exceeded path

**Files:**
- Create: `quota-router-e2e-tests/tests/l2_inbound_ttl_exceeded.rs`

**Step 1:** Send a forward request with `ttl == 0`, assert `ForwardRejectPayload { reason: TTLExceeded }` is generated.

**Step 2:** Run. Expected: PASS.

**Step 3:** Commit.

### Task 4.4: Capacity exhausted path

**Files:**
- Create: `quota-router-e2e-tests/tests/l2_inbound_capacity_exhausted.rs`

**Step 1:** Saturate the local provider's quota, send a forward request, assert `ForwardRejectPayload { reason: CapacityExhausted }` and that pull-gossip is triggered.

**Step 2:** Run. Expected: PASS.

**Step 3:** Commit.

### Task 4.5: Multi-receiver dispatch (NodeTransport semantics)

**Files:**
- Create: `quota-router-e2e-tests/tests/l2_inbound_multi_receiver.rs`

**Step 1:** Register an additional `NetworkReceiver` (a `TestObserver`) on the node's transport after building. Send a payload. Assert BOTH the handler AND the test observer receive the payload.

**Step 2:** Run. Expected: PASS.

**Step 3:** Commit.

### Task 4.6: GovernedTransport dispatch

**Files:**
- Create: `quota-router-e2e-tests/tests/l2_governed_dispatch.rs` (or in `octo-transport/tests/` if more natural)

**Step 1:** Construct a `GovernedTransport` wrapping a `NodeTransport` with a mock receiver. Call `receive()` with a valid context, assert mock was called. Call with a kicked context, assert `GovernanceViolation`.

**Step 2:** Run. Expected: PASS.

**Step 3:** Commit.

### Task 4.7: L2 dispatch counter assertion (regression guard)

**Files:**
- Modify: `quota-router-e2e-tests/src/lib.rs` — add `dispatch_call_count: AtomicUsize` to `TestNode`
- Modify: all `l2_*.rs` tests — assert `node.dispatch_call_count.load() >= 1` after `node.receive(...)`

**Step 1:** Increment counter inside `dispatch_with_sender` (or inside the new `receive` shim).

**Step 2:** Update each L2 test to assert the counter is non-zero after `receive()`. This guards against future regressions where someone bypasses `receive()` and calls `handler.on_receive()` directly.

**Step 3:** Run `cargo test -p quota-router-e2e-tests`. Expected: all PASS.

**Step 4:** Commit: `git add quota-router-e2e-tests/ && git commit -m "test(e2e): assert dispatch counter to guard public API seam"`

---

## Phase 5: Verification

### Task 5.1: Lint and format

**Step 1:** `cd /home/mmacedoeu/_w/ai/cipherocto && cargo fmt -- --check`
If diff: `cargo fmt`

**Step 2:** `cargo clippy --all-targets --all-features -- -D warnings`
For crates not in workspace: `cargo clippy -p quota-router --all-targets --all-features -- -D warnings` and similarly for `quota-router-e2e-tests`, `octo-transport`.

**Step 3:** Commit any auto-fixes: `git commit -am "style: apply cargo fmt and clippy fixes"`

### Task 5.2: Run full test suite

**Step 1:** `cargo test -p quota-router --lib --all-features`
**Step 2:** `cargo test -p quota-router-e2e-tests --all-features`
**Step 3:** `cargo test -p octo-transport --lib --all-features`

**Step 4:** Confirm zero failures. If failures, fix or document in this plan as additional tasks.

### Task 5.3: Cross-reference consistency check

**Step 1:** For each amended RFC, grep for `RFC-0XXX v` in prose and confirm none remain. Version-history tables may keep numbers.
```bash
grep -nE "RFC-0[0-9]+[a-z-]* v[0-9]" rfcs/accepted/networking/*.md
```
(Expected to match only version-history rows.)

**Step 2:** For each mission, same grep. Fix any matches.

**Step 3:** For each RFC, confirm version-history table has the new row (v1.13 / v1.8 / v0.1.3 as appropriate).

### Task 5.4: Confirm no fake binaries / fake tests remain

**Step 1:** `find . -name "Cargo.toml" -path "*/quota-router-node/*"` → expected: empty
**Step 2:** `find . -name "l3_*"` → expected: empty (L3 tests deleted)
**Step 3:** `grep -rn "register_receiver" quota-router-e2e-tests/src/` → expected: only in documentation comments, not in test fixture wiring (the builder handles it)

### Task 5.5: Final commit

If any verification step surfaced changes:
`git commit -am "chore: verification pass — fmt, clippy, cross-refs"`

---

## Open Questions (require user confirmation before executing)

1. **D3 field visibility**: Should `QuotaRouterNode::handler` be `pub` (testable, like `transport`) or `pub(crate)` (encapsulated)? Default proposal: `pub` for symmetry with `transport`. Confirm.

2. **D6 mission status**: After deferring Missions 0870g and 0870i, should they remain in `missions/deferred/` (preserved for future design discussion) or be deleted entirely? Default proposal: preserve in `deferred/` so the design discussion has a starting point.

3. **Arc::new_cyclic vs OnceCell**: For the handler-node circular reference in `build()`, which pattern? Default proposal: `Arc::new_cyclic` (cleaner, no lazy-init race). Confirm.

4. **`QuotaRouterNode::receive()` error type**: Should it return `Result<(), RouterNodeError>` (richer, includes handler-level errors) or `Result<(), TransportError>` (matches `transport.dispatch()`)? Default proposal: `TransportError` for now to avoid breaking the public API; add a richer variant later if needed.

5. **L2 test counter**: Should the regression-guard dispatch counter be on `TestNode` (private to test harness) or on `QuotaRouterNode` itself (production observability)? Default proposal: private to test harness — production observability is a separate concern.

---

## Decision Summary (sign-off needed before executing)

Before I dispatch subagents to execute this plan, please confirm:
- [ ] Policy wording (Section 0) is acceptable.
- [ ] Architectural decisions D1-D9 are correct.
- [ ] Open questions 1-5 above have your preferred answers.
- [ ] The order (RFCs → Missions → Implementation → Tests → Verification) is correct.
- [ ] You want me to execute this plan in this session (subagent-driven) or in a separate session (parallel).

Once you sign off, I'll execute via `superpowers:subagent-driven-development` — one task at a time, with review between each.