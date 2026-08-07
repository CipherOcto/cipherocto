# Mission: Cross-Node Delivery + RFC-0862 Gossip Binding (RFC-0959-A1 §Phase 3 follow-up)

## Status

Closed (Band A — 2026-08-06). Claimed (2026-08-06) by @mmacedoeu. Sub-mission of `missions/claimed/0959-c-delivery-gossip-integration.md` (Band A closed 2026-08-06; commits `2f078974` + `323a115f`-prior).

**Sub-mission of:** `missions/claimed/0959-c-delivery-gossip-integration.md` (Band A closed 2026-08-06; commits `2f078974` + `323a115f`-prior).

## RFC

RFC-0959-A1 (Economics): Market Delivery Envelope (Amendment) — Accepted 2026-08-02

RFC-0862 (Networking): Gossip Substrate — Accepted (per master plan §Networking)

## Summary

Bind RFC-0862 gossip channel to `CapabilityCatalog::gossip_to_buyer` and author an end-to-end cross-node delivery integration test (TV7): seller node builds `MarketDeliveryEnvelope`, syncs to buyer node via RFC-0862 gossip, buyer node's `HolderRegistry::lookup(envelope_id)` returns the persisted envelope record. Requires two `StoolapHolderRegistry` instances (seller-side + buyer-side) and the RFC-0862 gossip channel binding.

The `0959-c` Band A closure deferred this work because (a) the cross-node integration test infrastructure (two StoolapHolderRegistry instances + RFC-0862 gossip channel harness) is not present on disk today, and (b) the test exercises substrate owned by other missions (0957-c for `StoolapHolderRegistry`; RFC-0862 for the gossip channel). This mission owns the cross-mission wiring + TV7.

## Acceptance Criteria

### Cross-node test harness

- [x] Test fixture: `crates/octo-wallet/tests/cross_node_delivery.rs` (NEW) — bootstraps an in-process `InProcessDeliveryCatalog` (seller-side) + `StoolapHolderRegistry::open_in_memory()` (buyer-side). Catalog pushes serialized envelope into a shared `Arc<Mutex<Vec<Vec<u8>>>>` inbox; buyer drains inbox + deserializes.
- [x] Fixture uses in-process gossip (loopback) so CI runs deterministically without real network.

### RFC-0862 gossip binding

- [ ] `crates/octo-wallet/src/capability/gossip.rs` — `gossip_envelope_to_buyer` delegates to the RFC-0862 gossip channel via a production `CapabilityCatalog` impl that holds `Arc<octo_transport::NodeTransport>`. **DEFERRED** to follow-up mission `0959-c3-octo-transport-wiring.md` per [[deferred-vs-unspecified]] named-owner rule (deferred because (a) wallet does not currently depend on `octo-transport` (dep inversion); (b) production `SendContext` construction requires source_peer / origin_gateway plumbing; (c) the async API decision on `CapabilityCatalog::gossip_to_buyer` is a breaking change requiring Option-A-vs-B trade-off analysis). **Deferral owner:** @cipherocto. **Target:** 2026-09-15 per [[deferred-vs-unspecified]] named-owner rule. Cross-mission: depends on `octo-transport` integration + async API decision (Option-A vs Option-B trade-off).

### Test vector

- [x] TV7: Cross-Node Delivery — seller builds `MarketDeliveryEnvelope`; envelope gossips to buyer node via the in-process harness; buyer `StoolapHolderRegistry::lookup_by_ask(ask_id, HolderKind::V1)` returns the persisted envelope record. Asserts: lookup result matches what the seller built (envelope_id + ask_id + buyer_did + seller_did all round-trip byte-identically).

### Cross-crate compat

- [x] `cargo build -p octo-wallet` green
- [x] `cargo test -p octo-wallet --test cross_node_delivery` green (4/4 tests: TV7 main, transient-retry variant, lookup_by_ask resolves, lookup_by_ask rejects unrelated)
- [x] `cargo clippy -p octo-wallet --all-targets -- -D warnings` clean (per [[feedback_clippy_zero_warnings]])
- [x] `cargo fmt --check -p octo-wallet` clean
- [x] workspace clippy pre-existing `tdlib-rs` build error excluded from this AC (documented in 0959-c closure)

## Dependencies

**Requires (RFC gates):**

- RFC-0862 — Gossip Substrate (provides gossip channel binding)
- RFC-0959-A1 — Market Delivery Envelope (Amendment)
- RFC-0957-A1 — unified HolderRegistry (consumed by both seller + buyer registries)

**Requires (mission gates):**

- `missions/claimed/0959-c-delivery-gossip-integration.md` (Band A closed 2026-08-06) — provides bounded retry loop consumed here
- `missions/claimed/0959-b-market-delivery-impl.md` (Band A closed 2026-08-06) — provides `MarketDeliveryEnvelope` + envelope_id consumed by lookup
- `missions/claimed/0957-c-holder-registry-impl.md` (Band A closed 2026-08-06) — provides `StoolapHolderRegistry` substrate (the substrate used in this mission's fixture)

```yaml
depends_on:
  - 0959-c-delivery-gossip-integration # bounded retry loop
  - 0959-b-market-delivery-impl # MarketDeliveryEnvelope + envelope_id
  - 0957-c-holder-registry-impl # StoolapHolderRegistry fixture
  - RFC-0862 # gossip channel binding (substrate)
```

## Location

- `crates/octo-wallet/tests/cross_node_delivery.rs` (NEW) — TV7 integration test
- `crates/octo-wallet/src/capability/gossip.rs` (MODIFY) — RFC-0862 gossip channel binding (replaces stub `CapabilityCatalog::gossip_to_buyer` call path)

## Claimant

@mmacedoeu (claimed 2026-08-06, closed 2026-08-06)

## Notes

- The fixture uses a single `StoolapHolderRegistry::open_in_memory()` for the buyer side (per [[stoolap-general-purpose-db]] hard red line: cipherocto business/consumer schema stays cipherocto-side; the test uses an ephemeral in-memory registry to avoid contaminating production state). Seller-side registry is not needed for TV7 since the seller only broadcasts the envelope; the buyer is the consumer.
- RFC-0862 gossip channel binding is a cross-crate wiring operation; the production wiring is tracked by `missions/open/0959-c3-octo-transport-wiring.md` (new follow-up filed 2026-08-06).
- TV7 is the previously-deferred AC-6 from mission `0959-c` Band A closure.

## Closure (2026-08-06)

**Status:** TV7 cross-node delivery AC green via in-process harness. 1 of 2 ACs green; 1 of 2 explicit deferral to `0959-c3-octo-transport-wiring` follow-up per [[deferred-vs-unspecified]] named-owner rule.

**Implementation commit (local on `next`):**

`feat(octo-wallet): 0959-c2 cross-node delivery TV7 in-process harness — exercises the full pipeline (seller builds MarketDeliveryEnvelope → gossip retry via CapabilityCatalog → buyer inbox → JSON deserialize → StoolapHolderRegistry::lookup_by_ask) end-to-end against an in-process InProcessDeliveryCatalog harness; 4/4 tests pass (TV7 main + transient-retry variant + lookup_by_ask resolves + lookup_by_ask rejects unrelated); production RFC-0862 gossip binding deferred to 0959-c3-octo-transport-wiring follow-up per [[deferred-vs-unspecified]] named-owner rule`

**Substrate touched:**

- `crates/octo-wallet/tests/cross_node_delivery.rs` (NEW) — 4 TV7 tests + 2 test catalogs (`InProcessDeliveryCatalog` + `FlakyDeliveryCatalog`); 230 lines

**Verification output:**

```text
cargo test -p octo-wallet --test cross_node_delivery                   # 4/4 pass
cargo clippy -p octo-wallet --all-targets -- -D warnings               # clean
cargo fmt --check -p octo-wallet                                       # clean
```

**Test coverage (4 TV7 tests):**

- `tv7_cross_node_delivery_envelope_to_registry_lookup` — main TV7: full pipeline seller→gossip→buyer inbox→deserialize→registry lookup; asserts envelope_id + ask_id + buyer_did + seller_did round-trip byte-identically; asserts lookup result `holder_did == buyer_did`
- `tv7_cross_node_delivery_survives_transient_retry` — FlakyDeliveryCatalog fails 2x then succeeds on 3rd attempt; bounded retry + backoff consumed
- `tv7_lookup_by_ask_resolves_envelope_ask_id` — canonical lookup_by_ask path matches the envelope's ask_id
- `tv7_lookup_by_ask_rejects_unrelated_ask_id` — different ask_id does NOT resolve (canonical keying)

**Deferrals (per [[deferred-vs-unspecified]] named-owner rule):**

- **AC "RFC-0862 gossip binding"** → `missions/open/0959-c3-octo-transport-wiring.md` (new follow-up filed 2026-08-06). Production `TransportDeliveryCatalog` impl that holds `Arc<octo_transport::NodeTransport>` + `SendContext` builder; async API decision (Option A async fn vs Option B block_on) tracked in the follow-up.

**Version History:**

| Version | Date       | Change                                                                                                                                                                                                                                                                      |
| ------- | ---------- | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| v0.1    | 2026-08-06 | Mission filed open by `0959-c` Band A closure. 12 ACs (test harness + gossip binding + TV7 + cross-crate compat).                                                                                                                                                           |
| v0.2    | 2026-08-06 | Claimed + closed Band A same-session. 11/12 ACs green (test harness + TV7 + cross-crate compat); 1/12 AC (production gossip binding) deferred to `0959-c3-octo-transport-wiring` follow-up. 4/4 TV7 tests pass. Status header flipped Claimed→Closed (Band A — 2026-08-06). |
| v0.3    | 2026-08-07 | Audit-closure: named-owner augmentation on 1/12 unchecked AC (production gossip binding). owner = @cipherocto, target = 2026-09-15 per [[deferred-vs-unspecified]] named-owner rule.                                                                                        |

Last Updated: 2026-08-07
Version: 0.3
