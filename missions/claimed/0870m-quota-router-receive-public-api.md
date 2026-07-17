# Mission: 0870m — QuotaRouterNode::receive() Public Inbound API

## Status

Claimed (2026-06-30) — first-draft specification; acceptance criteria below drive the implementation and tests in Phase 4 of the cleanup plan.

## RFCs

- RFC-0870 — Distributed Quota Router Network (Public API, Test Policy)
- RFC-0863 — General-Purpose Network Integration (`NodeTransport::dispatch` is the underlying mechanism)

## Dependencies

Missions that must be completed before this one lands in production:

- 0863b (must be completed) — provides `NodeTransport::dispatch` and `register_receiver`
- 0870c (must be completed) — provides `QuotaRouterNode::builder().build()` and the internal `QuotaRouterHandler` registration step
- 0870d (must be completed) — provides HMAC verification path on inbound forward requests

## Summary

Define and lock down the public inbound API of `QuotaRouterNode`. The API is the inbound counterpart of `QuotaRouterNode::route()`: callers invoke `node.receive(payload, ctx)` with a wire payload and a `ReceiveContext`, and the implementation fans the payload through `NodeTransport::dispatch()` to the node's registered receivers (chiefly the internal `QuotaRouterHandler`).

This mission is the consumer-facing contract. The implementation lives in `quota-router/src/lib.rs`; the tests land in Phase 4 (Inbound API tests) of the cleanup plan.

## Design

### Public API

```rust
impl QuotaRouterNode {
    /// Inbound API: dispatch a payload through `NodeTransport` to all
    /// registered receivers. The internal `QuotaRouterHandler` (a
    /// `NetworkReceiver` impl) is one of those receivers, registered
    /// automatically by `QuotaRouterNodeBuilder::build()`. Symmetric
    /// to `route()` for outbound traffic.
    pub async fn receive(
        &self,
        payload: &[u8],
        ctx: &ReceiveContext,
    ) -> Result<(), TransportError> {
        self.transport.dispatch(payload, ctx).await
    }
}
```

### Behavior contract

1. **Delegation.** `receive(payload, ctx)` returns the same `Result` as `self.transport.dispatch(payload, ctx).await`. Any semantics changes to inbound behavior happen in `dispatch()` (e.g., adding a receiver, changing iteration order, returning errors), not in `receive()` itself.
2. **Symmetry.** `receive()` is the inbound counterpart of `route()`. Same error-type convention (`Result<_, TransportError>` for `receive()`, `Result<_, RouterNodeError>` for `route()` — the inbound path deals with wire-level errors, the outbound path with business-level errors).
3. **No caller-side wiring.** Callers do not need to construct or register a `QuotaRouterHandler`. The builder does that. (See Mission 0870c.) Optional additional receivers (for example, an observability sink) can be added via `node.transport.register_receiver(...)` after `build()` — this is an opt-in extension, not part of the consumer contract.
4. **Receiver registration order.** Receivers run in the order they were registered (per RFC-0863 v1.8, "Registration order" and "Receiver ownership"). The internal `QuotaRouterHandler` is registered first (during `build()`), so any opt-in additional receivers run after it.

### Use cases

| Use case | Caller | Pattern |
|----------|--------|---------|
| L2 in-process e2e tests | `TestNode::drive()` calls `node.receive(payload, &ctx)` with `ctx.sender_id = Some(sender.0)` for HMAC enforcement | inbound via API |
| Future `PlatformAdapter` integration | Adapter polling loop: `node.receive(&payload, &ctx).await?` | inbound via API |
| Custom layered inbound | Code that wants fine-grained control over `NodeTransport` directly | `node.transport.dispatch(...)` |

The L2 test harness (Mission 0870f) is the only current caller of `receive()`. After Mission 0870i lands (cross-process adapter, deferred), additional callers will appear.

## Acceptance Criteria

### API contract

- [ ] `QuotaRouterNode::receive()` exists and is `pub async`
- [ ] Signature is `pub async fn receive(&self, payload: &[u8], ctx: &ReceiveContext) -> Result<(), TransportError>`
- [ ] `receive(payload, ctx)` returns the same `Result` as `self.transport.dispatch(payload, ctx).await`
- [ ] Internal `QuotaRouterHandler` is registered automatically by `QuotaRouterNodeBuilder::build()` (no caller-side wiring)
- [ ] Empty receivers (e.g., in test isolation) → `dispatch()` returns `Ok(())`, so does `receive()`
- [ ] `receive()` is documented in the doc-comment as the inbound counterpart of `route()`

### Documentation

- [ ] Added to `quota-router/src/lib.rs` `impl QuotaRouterNode` block
- [ ] Cross-referenced from RFC-0870 §Public API (already updated in v1.13)
- [ ] Listed in Mission 0870c's Wiring subsection (already updated)

### Tests (Phase 4 of the cleanup plan)

The following tests live in `quota-router-e2e-tests/tests/`:

- [ ] `l2_inbound_happy_path.rs` — valid `ForwardRequest` (0xC3) envelope delivers through `receive()` and updates peer cache
- [ ] `l2_inbound_hmac_failure.rs` — gossip envelope (0xC6) with tampered HMAC returns an error and does not mutate the gossip cache
- [ ] `l2_inbound_ttl_exceeded.rs` — `ForwardRequest` with `ttl == 0` produces a `ForwardReject { reason: TTLExceeded }`
- [ ] `l2_inbound_capacity_exhausted.rs` — saturated local provider produces `ForwardReject { reason: CapacityExhausted }` and triggers pull-gossip
- [ ] `l2_inbound_multi_receiver.rs` — registering an additional `NetworkReceiver` (a `TestObserver`) after `build()` causes both handlers to run on `receive()`
- [ ] `l2_dispatch_counter_regression.rs` — `TestNode` increments a `dispatch_call_count` counter inside `receive()`; tests assert the counter is non-zero after `receive()` (guards against future regressions where someone bypasses `receive()` and calls `handler.on_receive()` directly)

These tests are TDD-driven; they are added in Phase 4 after the implementation lands in Phase 3 of the cleanup plan.

## Type Coverage

| RFC Type | Implemented By |
|----------|---------------|
| `QuotaRouterNode::receive` | This mission (Phase 3 implementation + Phase 4 tests) |

## Complexity

Low for the implementation (~30 lines of `pub async fn` body). Medium for the test suite (~6 test files, ~150 lines each, exercising happy path and failure modes of inbound dispatch).

## Implementation Notes

- The implementation is a one-liner: delegate to `self.transport.dispatch(payload, ctx).await`. The real work is in `NodeTransport::dispatch()` and `QuotaRouterHandler::on_receive()` (both already implemented in earlier missions).
- Tests target the public API. They must not call `handler.on_receive()` directly — that would bypass the production seam and the regression counter would not protect the contract.
- Document `receive()` as the canonical inbound API in the `QuotaRouterNode` doc-comment block. Mention that callers who want to add observability-side receivers can call `node.transport.register_receiver(...)` directly.

## Reference

- RFC-0870 v1.13 — §Public API, §Test Policy, §Inbound Path
- Mission 0870c — Wiring example showing the symmetric `route()` / `receive()` pair
- Mission 0863b — `NodeTransport::dispatch()` semantics
- Cleanup plan — `docs/plans/2026-06-30-quota-router-clean-inbound-architecture.md`, Phase 3 (Task 3.4) for implementation, Phase 4 (Tasks 4.1–4.7) for tests
