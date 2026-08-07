# Mission: Cross-Node Delivery + RFC-0862 Gossip Binding (RFC-0959-A1 §Phase 3 follow-up)

## Status

Open (filed 2026-08-06 by mission `0959-c-delivery-gossip-integration.md` Band A closure). Per [[deferred-vs-unspecified]] named-owner rule, this follow-up mission owns the deferred cross-node delivery test infrastructure (RFC-0862 gossip channel + two `StoolapHolderRegistry` instances + TV7 end-to-end).

**Sub-mission of:** `missions/claimed/0959-c-delivery-gossip-integration.md` (Band A closed 2026-08-06; commits `2f078974` + `323a115f`-prior).

## RFC

RFC-0959-A1 (Economics): Market Delivery Envelope (Amendment) — Accepted 2026-08-02

RFC-0862 (Networking): Gossip Substrate — Accepted (per master plan §Networking)

## Summary

Bind RFC-0862 gossip channel to `CapabilityCatalog::gossip_to_buyer` and author an end-to-end cross-node delivery integration test (TV7): seller node builds `MarketDeliveryEnvelope`, syncs to buyer node via RFC-0862 gossip, buyer node's `HolderRegistry::lookup(envelope_id)` returns the persisted envelope record. Requires two `StoolapHolderRegistry` instances (seller-side + buyer-side) and the RFC-0862 gossip channel binding.

The `0959-c` Band A closure deferred this work because (a) the cross-node integration test infrastructure (two StoolapHolderRegistry instances + RFC-0862 gossip channel harness) is not present on disk today, and (b) the test exercises substrate owned by other missions (0957-c for `StoolapHolderRegistry`; RFC-0862 for the gossip channel). This mission owns the cross-mission wiring + TV7.

## Acceptance Criteria

### Cross-node test harness

- [ ] Test fixture: `crates/octo-wallet/tests/cross_node_delivery.rs` (NEW) — bootstraps two `StoolapHolderRegistry` instances (seller-side + buyer-side) bound to a local RFC-0862 gossip channel.
- [ ] Fixture uses in-process gossip (loopback) so CI runs deterministically without real network.

### RFC-0862 gossip binding

- [ ] `crates/zk-verifier` / `crates/octo-wallet/src/capability/gossip.rs` — `gossip_envelope_to_buyer` delegates to the RFC-0862 gossip channel (currently calls `CapabilityCatalog::gossip_to_buyer` directly, which is a local stub; cross-node binding deferred here).

### Test vector

- [ ] TV7: Cross-Node Delivery — seller builds `MarketDeliveryEnvelope` via `mint_dual` (or equivalent RFC-0959-A1 envelope build); envelope gossips to buyer node via RFC-0862; buyer `HolderRegistry::lookup(envelope_id)` returns the persisted envelope record. Assert: lookup result matches what the seller built (canonical bytes identical).

### Cross-crate compat

- [ ] `cargo build --workspace` green
- [ ] `cargo test --workspace --lib` green (existing tests + 1 new TV7 test)
- [ ] `cargo test -p octo-wallet --test cross_node_delivery` green (1/1 TV7)
- [ ] `cargo clippy --workspace --all-targets --all-features -- -D warnings` clean (per [[feedback_clippy_zero_warnings]]); workspace-level pre-existing `tdlib-rs` build error excluded from this AC (documented in 0959-c closure)
- [ ] `cargo fmt --check --workspace` clean

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

TBD (claim 2026-08-06+)

## Notes

- The fixture uses two `StoolapHolderRegistry` instances pointing at separate temp dirs (per [[stoolap-general-purpose-db]] hard red line: cipherocto business/consumer schema stays cipherocto-side; the test creates ephemeral tempdir-backed registries to avoid contaminating production state).
- RFC-0862 gossip channel binding is a cross-crate wiring operation; consult the RFC-0862 substrate owners (mission `missions/claimed/0862m-sync-peer-slashing.md`) for the canonical gossip-channel constructor.
- TV7 is the previously-deferred AC-6 from mission `0959-c` Band A closure.
