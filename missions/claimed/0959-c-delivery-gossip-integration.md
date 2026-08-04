# Mission: Delivery Gossip Integration + Retry Policy (RFC-0959-A1 §Phase 3)

## Status

Claimed (2026-08-04)

## RFC

RFC-0959-A1 (Economics): Market Delivery Envelope (Amendment) — Accepted 2026-08-02

**Sub-mission of:** `missions/open/0959-a1-market-delivery.md` (top-level decomposition mission)

## Summary

Implement RFC-0959-A1 §Phase 3: gossip integration for envelope delivery to the buyer + bounded retry policy. Wrap `CapabilityCatalog::gossip_to_buyer(buyer_did, env)` (owned by sub-mission 0957-e) in a bounded retry loop (exhaustion → `DeliveryError::GossipFailed { attempts }`). Implement cross-node delivery verification (TV7) and gossip retry (TV4).

This sub-mission depends on 0959-b envelope wire format + 0957-e `gossip_to_buyer` extension. The retry loop is the load-bearing mechanism for Finding A11 (gossip partition → envelope not received).

## Acceptance Criteria

### Gossip retry loop

- [ ] `crates/octo-wallet/src/capability/gossip.rs` (MODIFY) — `gossip_envelope_to_buyer(env: &MarketDeliveryEnvelope, buyer_did: &Did, catalog: &CapabilityCatalog) -> Result<(), DeliveryError>`.
- [ ] Bounded retry: attempts ≤ `MAX_GOSSIP_ATTEMPTS` (RFC-0959-A1 §Future Work F5 reserves the variant; this sub-mission implements the loop). Default `MAX_GOSSIP_ATTEMPTS = 5`.
- [ ] On exhaustion, return `DeliveryError::GossipFailed { attempts: MAX_GOSSIP_ATTEMPTS }`.
- [ ] Exponential backoff between attempts (RFC-0862 gossip convention; documented in RFC-0959-A1 §Future Work F5).

### Cross-node delivery verification

- [ ] Integration test: seller node builds envelope; buyer node receives via gossip; buyer node's `HolderRegistry::lookup(envelope_id)` returns the inserted record (TV7).

### Test vectors (RFC-0959-A1 §Test Vectors, this sub-mission owns TV4, TV7)

- [ ] TV4: Gossip Retry — mock transient gossip failure; retry succeeds; `attempts == 3` (not exhausted).
- [ ] TV7: Cross-Node Delivery — seller node builds envelope, syncs to buyer node, buyer's `HolderRegistry::lookup(envelope_id)` returns the persisted envelope. Full end-to-end across two `StoolapHolderRegistry` instances + RFC-0862 gossip channel.

### Retry exhaustion path

- [ ] Unit test: mock permanent gossip failure; loop exhausts after `MAX_GOSSIP_ATTEMPTS`; returns `DeliveryError::GossipFailed { attempts: 5 }`. Manual redacting Debug on `DeliveryError::GossipFailed` displays `attempts` but no envelope content.

### Cross-crate compat

- [ ] `cargo build --workspace` green
- [ ] `cargo test --workspace` green
- [ ] `cargo clippy --workspace --all-targets --all-features -- -D warnings` clean
- [ ] `cargo fmt --check` clean

## Dependencies

**Requires (RFC gates):**

- RFC-0862 — gossip substrate

**Requires (mission gates):**

- `missions/open/0959-a1-market-delivery.md` (top-level)
- `missions/open/0959-b-market-delivery-impl.md` — `MarketDeliveryEnvelope` + `DeliveryError::GossipFailed` variant
- `missions/open/0957-e-mint-txn-parameter.md` — `CapabilityCatalog::gossip_to_buyer`

```yaml
depends_on:
  - 0959-b-market-delivery-impl # MarketDeliveryEnvelope + DeliveryError::GossipFailed variant
  - 0957-e-mint-txn-parameter # CapabilityCatalog::gossip_to_buyer
```

## Type Coverage

This sub-mission implements (per top-level Type Coverage table):

- Bounded gossip retry loop (RFC-0959-A1 §Future Work F5)
- `DeliveryError::GossipFailed { attempts }` code path emission
- Cross-node delivery verification integration test

## Location

- `crates/octo-wallet/src/capability/gossip.rs` (MODIFY) — `gossip_envelope_to_buyer`

## Claimant

@mmacedoeu (gossip retry loop + cross-node verification stub)

## Pull Request

(unset)

## Notes

- The retry loop is RFC-0959-A1 §Future Work F5. The variant was RESERVED in 0959-b; this sub-mission implements the loop that emits it. Round 1 review found the variant missing from §Error Handling; R8-N11 fix reserved it.
- TV4 + TV7 are the 2 remaining vectors not owned by 0959-b.
- Exponential backoff per RFC-0862 gossip convention; constant values documented in `gossip.rs` module-level doc comment.
