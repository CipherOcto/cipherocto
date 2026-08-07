# Mission: Production CapabilityCatalog → octo-transport::NodeTransport Wiring (RFC-0862 follow-up)

## Status

Open (filed 2026-08-06 by mission `0959-c2-cross-node-delivery.md` Band A closure). Per [[deferred-vs-unspecified]] named-owner rule, this follow-up mission owns the deferred production wiring of `CapabilityCatalog::gossip_to_buyer` to the canonical `octo_transport::NodeTransport::broadcast`.

**Sub-mission of:** `missions/claimed/0959-c2-cross-node-delivery.md` (Band A closed 2026-08-06).

## RFC

RFC-0862 (Networking): Stoolap Data Sync Protocol — Gossip Substrate
RFC-0959-A1 (Economics): Market Delivery Envelope (Amendment) — Accepted 2026-08-02
RFC-0957-A1 (Economics): Capability Holder Registry — Accepted

## Summary

The 0959-c2 Band A closure proved the cross-node delivery contract end-to-end (TV7: seller builds `MarketDeliveryEnvelope` → gossip retry → buyer inbox → `StoolapHolderRegistry::lookup_by_ask`) using an in-process `InProcessDeliveryCatalog` test harness. The production `CapabilityCatalog` impl that delegates to `octo_transport::NodeTransport::broadcast` was deferred because:

1. **Dep inversion**: `octo-wallet` does NOT depend on `octo-transport` (wallet would need to add `octo-transport = { path = "../../octo-transport" }` to `crates/octo-wallet/Cargo.toml`). The current `crates/octo-wallet/Cargo.toml` is a consumer of `octo-network` (sync envelope types) and `quota-router-storage` (registry types) but not transport. Adding the dep is straightforward but the production catalog must thread an `Arc<NodeTransport>` + a `SendContext` builder through the constructor (no global state).

2. **SendContext construction**: `NetworkSender::send(payload, &SendContext)` requires `mission_id`, `priority`, `source_peer`, `origin_gateway` — fields that the wallet currently has no clean source for. The seller side needs to derive them from the `MarketDeliveryEnvelope` (mission_id from `ask_id`, priority from `role_tag`, source_peer from `seller_did` hex-decode, origin_gateway from a node-config slot).

3. **Cross-node test fixture gap**: the 0959-c2 test uses a single-process `InProcessDeliveryCatalog`. The production wiring needs an in-process `NodeTransport` configured with a buyer-side `MockSender` that captures outbound payloads + a `MockReceiver` that dispatches inbound payloads to the buyer's `StoolapHolderRegistry`. This is a non-trivial harness (the `quota-router-e2e-tests/src/lib.rs::InProcessSender` pattern is the reference).

## Acceptance Criteria

### Cargo dep + public API

- [ ] `crates/octo-wallet/Cargo.toml` — add `octo-transport = { path = "../../octo-transport" }` dep
- [ ] `crates/octo-wallet/src/capability/macaroon.rs` — new `pub struct TransportDeliveryCatalog { transport: Arc<octo_transport::NodeTransport>, source_peer: [u8; 32], origin_gateway: [u8; 32] }` impls `CapabilityCatalog`; `gossip_to_buyer(buyer_did, payload)` builds a `SendContext { mission_id: derive_from_payload(payload), priority: DEFAULT_PRIORITY, source_peer, origin_gateway }` and calls `self.transport.broadcast(payload, &ctx).await`. Block-on via `tokio::runtime::Handle::current().block_on()` IF wallet is currently sync-only; otherwise accept `async` on the trait (changes the public API — see "API surface decision" below).
- [ ] `TransportDeliveryCatalog::new(transport, source_peer, origin_gateway) -> Self` constructor.

### Async API decision (trade-off)

- [ ] **Option A (preferred):** change `CapabilityCatalog::gossip_to_buyer` to `async fn`. Wallet is already on `tokio` (`Cargo.toml` line ~XX). Async signature propagates cleanly: `gossip_envelope_to_buyer` becomes async, callers in `crates/octo-wallet/src/capability/` get `await`. Pros: idiomatic tokio; matches `NodeTransport::broadcast` shape exactly. Cons: breaking API change for any external consumer of `CapabilityCatalog` (none currently exist per Band A audit 2026-08-06).
- [ ] **Option B:** keep `gossip_to_buyer` sync; use `tokio::runtime::Handle::current().block_on()` inside `TransportDeliveryCatalog::gossip_to_buyer`. Pros: no API break. Cons: panics if called outside a tokio runtime; double-await risk; harder to test.

The mission defaults to **Option A** unless review surfaces a blocking concern.

### Cross-node integration test (production wiring)

- [ ] `crates/octo-wallet/tests/cross_node_delivery_transport.rs` (NEW) — uses `octo_transport::NodeTransport::new(vec![Arc::new(InProcessCapturingSender { inbox: shared })])` for seller side + `register_receiver(Arc::new(InProcessRegistryReceiver { registry }))` for buyer side. Drives `gossip_envelope_to_buyer(&env, &buyer, &TransportDeliveryCatalog::new(...))` end-to-end. Asserts buyer `StoolapHolderRegistry::lookup_by_ask(ask_id)` returns the persisted record with `holder_did = buyer_did`.
- [ ] Asserts: sender inbox captures the same `MarketDeliveryEnvelope` JSON bytes that the 0959-c2 harness produced (cross-crate wiring parity test).

### Cross-crate compat

- [ ] `cargo build -p octo-wallet` green (new dep compiles)
- [ ] `cargo test -p octo-wallet --test cross_node_delivery` green (4/4 pre-existing tests still pass — proves the in-process harness still works alongside the new production wiring)
- [ ] `cargo test -p octo-wallet --test cross_node_delivery_transport` green (new production-wiring test)
- [ ] `cargo clippy -p octo-wallet --all-targets -- -D warnings` clean (per [[feedback_clippy_zero_warnings]])
- [ ] `cargo fmt --check -p octo-wallet` clean

## Dependencies

**Requires (RFC gates):**
- RFC-0862 — Gossip Substrate (provides `octo_transport::NodeTransport`)
- RFC-0959-A1 — Market Delivery Envelope (Amendment)
- RFC-0957-A1 — Capability Holder Registry

**Requires (mission gates):**
- `missions/claimed/0959-c2-cross-node-delivery.md` (Band A closed 2026-08-06) — provides the in-process harness pattern + TV7 contract; production wiring builds on top

```yaml
depends_on:
  - 0959-c2-cross-node-delivery # in-process harness pattern + TV7 contract
  - RFC-0862 # octo_transport::NodeTransport substrate
```

## Location

- `crates/octo-wallet/Cargo.toml` (MODIFY) — `octo-transport` dep
- `crates/octo-wallet/src/capability/macaroon.rs` (MODIFY) — `TransportDeliveryCatalog` impl
- `crates/octo-wallet/src/capability/gossip.rs` (MODIFY if Option A) — async `gossip_envelope_to_buyer`
- `crates/octo-wallet/tests/cross_node_delivery_transport.rs` (NEW) — production-wiring test

## Claimant

TBD (claim 2026-08-06+)

## Notes

- The 0959-c2 in-process harness (`InProcessDeliveryCatalog` struct in `tests/cross_node_delivery.rs`) STAYS — it remains the unit-test substrate for the retry loop. This mission adds the production wiring as a parallel impl, not a replacement.
- The async API change (Option A) is a breaking change for any external consumer of `CapabilityCatalog`. Per the 2026-08-06 audit, no external consumers exist; wallet is the sole producer. Confirm via `git grep -l "CapabilityCatalog" crates/ docs/` before flipping to Option A.
- The `SendContext` field derivation (mission_id from payload) is fragile. Prefer to encode `mission_id` as a constructor arg if the deployment topology is stable; fall back to payload-derivation only if the deployment requires it.
