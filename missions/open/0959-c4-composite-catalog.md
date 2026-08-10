# 0959-c4 — CompositeCapabilityCatalog (gossip + storage delegation)

**Status:** unassigned (wave 3a step 1; gap surfaced 2026-08-10)
**Substrate:** RFC-0862 gossip + RFC-0959-A1 market delivery + RFC-0957-A1 holder registry
**Parent:** 0959-c3 (closed Band A) per [[mission-0959-c3-octo-transport-wiring]]

## Scope

Closes 0959-c3 Notes "out of scope for Band A" (gossip + storage delegation in one catalog). The `CapabilityCatalog` trait today requires implementers to provide BOTH `lookup_by_ask` (storage) AND `gossip_to_buyer` (transport). A storage-only OR gossip-only backend must implement both (production want: composite = storage backend for lookups + transport backend for gossip).

1. `crates/octo-wallet/src/capability/catalog.rs` — NEW `CompositeCapabilityCatalog { storage: Arc<dyn CapabilityCatalog>, gossip: Arc<dyn CapabilityGossip> }`. Dispatches:
   - `lookup_by_ask` → `self.storage.lookup_by_ask(...)`
   - `lookup` → `self.storage.lookup(...)`
   - `lookup_active` → `self.storage.lookup_active(...)`
   - `attenuate_chain` → `self.storage.attenuate_chain(...)`
   - `implements_gossip() -> true` always (the gossip slot is always present)
   - NEW method `gossip_to_buyer_via(env, buyer_did)` → `self.gossip.gossip_to_buyer(env, buyer_did)`
2. `CapabilityCatalog` trait (existing) — add `pub fn gossip_to_buyer_via(...)` default impl that returns `Err(Unsupported)` for non-composite catalogs (preserves `&dyn CapabilityCatalog` object-safety).
3. `crates/octo-wallet/tests/composite_catalog.rs` (NEW) — integration test:
   - `composite_storage_hits_only_storage_lookup` — `lookup_by_ask` delegates to storage backend
   - `composite_gossip_hits_only_gossip_delivery` — `gossip_to_buyer_via` delegates to gossip backend
   - `composite_implements_gossip` — `implements_gossip()` returns `true`
   - `composite_lookup_active_propagates_revocation` — `lookup_active` reflects revocation in storage

## Test vector discipline

- 4 new TV covering dispatch + delegation.
- Reuses 0959-c3 TV7 (cross-node delivery through production transport) + 0959-c2 TV5 (in-process harness).

## Depends on

- 0959-c3 closed Band A (done — `missions/claimed/0959-c3-octo-transport-wiring.md`)
- 0959-c2 closed Band A (done)
- `CapabilityGossip` trait + `TransportDeliveryCatalog` impl (shipped 0959-c3)

## Blocks

- 0957-f F1 catalog federation (needs composite catalog for "federated = composite of local + remote")
- 0871e Phase 5 follow-on — wallet-node `CapabilityCatalog` slot in `CapabilityIssuerNode`

## Layer direction

- `octo-wallet` (Layer B) owns the composite + trait extension
- `Storage` backend lives in `quota-router-storage` (Layer B-substrate)
- `Gossip` backend lives in `octo-transport` (Layer D) via `TransportDeliveryCatalog`
- Composite lives in Layer B and depends on both (allowed: B → B-substrate, B → D)

## Validation

- `cargo fmt --all -- --check`
- `cargo clippy -p octo-wallet --all-targets --features full -- -D warnings`
- `cargo test --lib -p octo-wallet capability`
- `cargo test -p octo-wallet --test composite_catalog`

## Cross-references

- [[wave-3-gaps-2026-08-10]] — gap surface context
- [[mission-gap-closure-priorities-2026-08-10]] — Wave 2 plan (origin reference)
- [[cipherocto-design-principles]] — Layer B→D boundary rules
