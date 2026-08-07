# Mission: Production CapabilityCatalog → octo-transport::NodeTransport Wiring (RFC-0862 follow-up)

## Status

Closed (Band A — 2026-08-07). Claimed (2026-08-07) by @mmacedoeu.

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

- [x] `crates/octo-wallet/Cargo.toml` — added `octo-transport = { path = "../../octo-transport" }` dep, plus `tokio = { version = "1", features = ["macros", "rt-multi-thread", "sync", "time"] }` and `async-trait = "0.1"` to runtime deps (required for `CapabilityGossip::gossip_to_buyer`'s `Box<dyn Future>` return type so the trait stays dyn-compatible with `&dyn CapabilityGossip`)
- [x] `crates/octo-wallet/src/capability/macaroon.rs` — `pub struct TransportDeliveryCatalog { transport: Arc<octo_transport::NodeTransport>, source_peer: [u8; 32], origin_gateway: [u8; 32] }`; constructor `TransportDeliveryCatalog::new(transport, source_peer, origin_gateway) -> Self`; `implements_gossip() -> true`; `CapabilityGossip::gossip_to_buyer` builds a `SendContext { mission_id: BLAKE3("cipherocto:market-delivery:mission" || payload)[:32], priority: 128, source_peer, origin_gateway }` and calls `self.transport.broadcast(payload, &ctx).await`
- [x] Async API surface decision: **Option A** (chosen). `CapabilityCatalog::gossip_to_buyer` was split into a separate `CapabilityGossip` async trait so the primary `CapabilityCatalog` stays object-safe (`&dyn CapabilityCatalog` is used throughout `capability::mod` for caveat attenuation + macaroon storage). The split preserves 0959-c2's sync-test harness (sync shim `gossip_to_buyer_sync` retained for backwards compat) while adding the new async production path. `CapabilityGossip` is `#[async_trait]` so it's invokable through `&dyn CapabilityGossip`.

### Async API decision (trade-off)

- [x] **Option A** implemented. `CapabilityCatalog::gossip_to_buyer` was NOT directly turned into `async fn` (would break `&dyn CapabilityCatalog` object-safety). Instead:
  - A new `CapabilityGossip` async trait was added (gated by `#[async_trait]`) to preserve `&dyn CapabilityGossip` dispatch
  - `CapabilityCatalog::gossip_to_buyer_sync` (sync shim returning `Err(Unsupported)` for legacy catalogs) remains for existing callers
  - `CapabilityCatalog::implements_gossip() -> bool` flag routes production callers to `&dyn CapabilityGossip`
  - New `gossip_envelope_to_buyer_async(env, buyer_did, catalog: &dyn CapabilityGossip)` retry loop in `gossip.rs` mirrors the sync version but uses `tokio::time::sleep`

### Cross-node integration test (production wiring)

- [x] `crates/octo-wallet/tests/cross_node_delivery_transport.rs` (NEW) — 6 tests:
  - `xp01_transport_delivers_same_bytes_as_test_harness` — cross-crate wiring parity: production `NodeTransport` delivers byte-equal envelope bytes vs the 0959-c2 in-process harness
  - `xp02_mission_id_derivation_is_deterministic_and_distinct` — `mission_id_for(payload)` is BLAKE3 domain-separated + produces deterministic + distinct mission_ids per payload
  - `xp03_transport_catalog_implements_gossip` — `CapabilityCatalog::implements_gossip()` returns `true` for `TransportDeliveryCatalog`
  - `xp04_node_transport_zero_senders_returns_zero_not_error` — `NodeTransport::broadcast` with no senders returns 0 (not an error); defends against "no peers reachable" mistakes
  - `xp05_end_to_end_pipeline_through_production_transport` — full TV7 contract driven by production `TransportDeliveryCatalog` (seller → NodeTransport → captured inbox → deserialize → `StoolapHolderRegistry::lookup_by_ask` → assert `holder_did = buyer_did`)
  - `xp06_catalog_gossip_error_is_send_sync` — `CatalogGossipError` is `Send + Sync` (required for `tokio::spawn` boundaries in production wiring)
- [x] Asserts: sender inbox captures the same `MarketDeliveryEnvelope` JSON bytes that the 0959-c2 harness produced (cross-crate wiring parity test). `xp01` is the canonical assertion.

### Cross-crate compat

- [x] `cargo build -p octo-wallet` green (new deps compile)
- [x] `cargo test -p octo-wallet --test cross_node_delivery` green (4/4 pre-existing tests still pass — proves the in-process harness still works alongside the new production wiring)
- [x] `cargo test -p octo-wallet --test cross_node_delivery_transport` green (6/6 new production-wiring tests)
- [x] `cargo clippy -p octo-wallet --all-targets -- -D warnings` clean (per [[feedback_clippy_zero_warnings]])
- [x] `cargo fmt --check -p octo-wallet` clean

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

- `crates/octo-wallet/Cargo.toml` (MODIFY) — `octo-transport` + `tokio` + `async-trait` runtime deps
- `crates/octo-wallet/src/capability/macaroon.rs` (MODIFY) — `TransportDeliveryCatalog` struct + `CapabilityGossip` trait
- `crates/octo-wallet/src/capability/gossip.rs` (MODIFY) — async `gossip_envelope_to_buyer_async` retry loop + sync→async rename of `gossip_to_buyer` to `gossip_to_buyer_sync`
- `crates/octo-wallet/tests/cross_node_delivery.rs` (MODIFY) — `gossip_to_buyer` → `gossip_to_buyer_sync` rename (synced with the new trait method name)
- `crates/octo-wallet/tests/cross_node_delivery_transport.rs` (NEW) — production-wiring test (6 tests)

## Claimant

@mmacedoeu (claimed 2026-08-07, closed 2026-08-07)

## Notes

- The 0959-c2 in-process harness (`InProcessDeliveryCatalog` struct in `tests/cross_node_delivery.rs`) STAYS — it remains the unit-test substrate for the retry loop. This mission adds the production wiring as a parallel impl, not a replacement.
- The async API change (Option A) was implemented by splitting `gossip_to_buyer` into a sync shim (preserves object-safety on `CapabilityCatalog`) + a separate `CapabilityGossip` async trait (production-wired catalogs implement BOTH). Per the 2026-08-06 audit, no external consumers exist; wallet is the sole producer.
- The `SendContext` field derivation (mission_id from payload via `BLAKE3(b"cipherocto:market-delivery:mission" || payload)`) is the canonical mission-scoped binding per RFC-0959-A1 §Algorithms + RFC-0862 §Gossip Substrate. Future work can swap to a constructor-supplied `mission_id` if deployment topology requires it.
- `TransportDeliveryCatalog` does not implement macaroon storage (`get` returns `None`); production wallets compose a `CompositeCapabilityCatalog` that delegates `get` to the storage catalog + `gossip_to_buyer` to this struct. Out of scope for Band A — future mission can add a `Composite` impl.

## Closure (2026-08-07)

**Status:** 12/12 ACs green. Production `TransportDeliveryCatalog` + 6 production-wiring tests landed in single commit.

**Implementation commit (local on `next`):**

`feat(octo-wallet): 0959-c3 production TransportDeliveryCatalog wiring — RFC-0862 gossip channel binding for MarketDeliveryEnvelope via octo_transport::NodeTransport::broadcast with Option-A async split (CapabilityGossip trait via async_trait for object-safety, sync shim gossip_to_buyer_sync retained for 0959-c2 harness parity); 6/6 production wiring tests pass (xp01 cross-crate parity test asserts byte-equal delivery vs 0959-c2 harness, xp02 mission_id derivation, xp03 implements_gossip, xp04 zero-senders Ok, xp05 full TV7 end-to-end through NodeTransport, xp06 Send+Sync); 4/4 pre-existing TV7 tests still green; clippy -D warnings clean; fmt clean`

**Substrate touched:**

- `crates/octo-wallet/Cargo.toml` — added `octo-transport = { path = "../../octo-transport" }`, `tokio = { version = "1", features = ["macros", "rt-multi-thread", "sync", "time"] }`, `async-trait = "0.1"` to `[dependencies]` (runtime, not dev-deps — library code consumes them)
- `crates/octo-wallet/src/capability/macaroon.rs` — added `CapabilityGossip` async trait (with `#[async_trait]` macro) + `TransportDeliveryCatalog { transport, source_peer, origin_gateway }` struct + debug-redacting `Debug` impl (per RFC-0957-A1 §Security) + `CapabilityCatalog` + `CapabilityGossip` impls (proves `implements_gossip()` returns `true`)
- `crates/octo-wallet/src/capability/gossip.rs` — added `gossip_envelope_to_buyer_async` (async bounded-retry loop, mirrors sync version with `tokio::time::sleep`) + sync `gossip_envelope_to_buyer` now delegates to `catalog.gossip_to_buyer_sync(...)` (preserves 0959-c2 TV7 contract)
- `crates/octo-wallet/tests/cross_node_delivery.rs` — renamed `gossip_to_buyer` → `gossip_to_buyer_sync` on both test catalog impls (4 TV7 tests unchanged, no behavior delta)
- `crates/octo-wallet/tests/cross_node_delivery_transport.rs` (NEW) — 6 production-wiring tests + `InProcessCapturingSender` (`NetworkSender` impl) + `make_catalog` helper

**Verification output:**

```text
cargo build -p octo-wallet                                                    # clean
cargo test -p octo-wallet --test cross_node_delivery                          # 4/4 pass (0959-c2 TV7 preserved)
cargo test -p octo-wallet --test cross_node_delivery_transport                # 6/6 pass (NEW)
cargo test -p octo-wallet --lib capability::gossip                             # 7/7 pass (retry loop)
cargo test -p octo-wallet                                                     # full suite passes (18+ tests across ZK + TV7 + gossip)
cargo clippy -p octo-wallet --all-targets -- -D warnings                      # clean
cargo fmt -p octo-wallet -- --check                                           # clean
```

**Design rationale (post-implementation):**

- **Async trait split (Option A chosen, with refinement)**: original Option A proposed making `CapabilityCatalog::gossip_to_buyer` itself `async fn`. That broke object-safety — `&dyn CapabilityCatalog` is used throughout `capability::mod` for caveat attenuation + macaroon storage, so making it async would force a refactor of every callsite. The refined split — `CapabilityCatalog::gossip_to_buyer_sync` (sync shim, returns `Unsupported` for production-wired catalogs) + separate `#[async_trait] CapabilityGossip` trait — keeps both surfaces object-safe while allowing async gossip production wiring. `CapabilityCatalog::implements_gossip()` flag lets the retry loop short-circuit unsupported catalogs before trait-dispatch.
- **Manual `Debug` redaction (RFC-0957-A1 §Security)**: `TransportDeliveryCatalog::Debug` prints `[REDACTED 32B]` for `source_peer` and `origin_gateway` (defense in depth — same pattern as `BridgeError` in `octo-network::dc::slash_bridge`). The `transport` field is `Arc<NodeTransport>` and prints as `<Arc<NodeTransport>>` (no inner byte leak).
- **`mission_id` domain separation**: `BLAKE3(b"cipherocto:market-delivery:mission" || payload)[:32]`. Domain string `b"cipherocto:market-delivery:mission"` separates this `mission_id` derivation from other BLAKE3-using code paths (e.g., `capability_id` in `macaroon.rs` uses `CAPABILITY_ID_DOMAIN = 0x05`). RFC-0862 §Gossip Substrate downstream consumers (DC reputation stores) can rely on the 32-byte mission ID being payload-derived + mission-scoped.
- **`NodeTransport::broadcast` returns `usize`, not `Result`**: a zero count is **not** an error (every peer can be offline simultaneously); the production wiring surfaces `Ok(())` so the bounded retry loop doesn't mistake "no peers reachable right now" for "transient failure." Caller can read the count via custom `NetworkSender::send` impls if they need delivery accounting.
- **Receiver dispatch not implemented in Band A**: production gates (`octo-reputation::SlashReputationStoreCompat`) consume `mission_id`-scoped envelopes out-of-band from the gossip substrate (per RFC-0862 §Gossip Substrate delivery contract). The receiver-side `NodeTransport::register_receiver` path that fans inbound `GossipObject`s into the buyer's `StoolapHolderRegistry` lives in `crates/octo-network/src/sync/dgp_integration.rs` (0862j substrate — already shipped). Future mission can add a `register_buyer_registry(registry)` convenience method to `TransportDeliveryCatalog`; out of scope for Band A.

**Version History:**

| Version | Date       | Change                                                                                                                                                                                                                                                                |
| ------- | ---------- | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| v0.1    | 2026-08-06 | Filed open by mission `0959-c2` Band A closure. 12 ACs.                                                                                                                                                                                                              |
| v0.2    | 2026-08-07 | Claimed + closed Band A same-session. 12/12 ACs green. Production `TransportDeliveryCatalog` + 6 production-wiring tests landed. Async API surface decision: Option A with refined split (sync shim + `CapabilityGossip` async trait via `async_trait`). 0959-c2 4/4 TV7 preserved. Clippy + fmt clean. Status header flipped Open→Claimed→Closed (Band A — 2026-08-07). |

Last Updated: 2026-08-07
Version: 0.2
