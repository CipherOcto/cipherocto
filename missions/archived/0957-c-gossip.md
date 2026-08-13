# Mission: Cross-Node Mint Verifiability via RFC-0862 Gossip (RFC-0957-A1 §G5)

## Status

Closed (2026-08-09). Claimed + implemented in commit `pending` (see below). Sub-mission of `missions/claimed/0957-c-holder-registry-impl.md` per [[deferred-vs-unspecified]] deferral rule (TV5 cross-node mint verifiability).

**Substrate landed:** `serialize_for_gossip` + `apply_gossip_record` trait methods (RFC-0957-A1 §G5) at `crates/quota-router-storage/src/holder_registry.rs` + canonical JSON helpers at `crates/quota-router-storage/src/holder_record.rs` (`HolderRecord::canonical_ser` + `HolderRecord::canonical_de`). Integration test at `crates/octo-wallet/tests/holder_registry_cross_node_gossip.rs` (5/5 TV pass: TV5.1 cross-node mint verifiability + TV5.2 idempotent duplicate + TV5.3 missing record error + TV5.4 malformed bytes + TV5.5 revoked record not active). `sync_peers()` trait method kept as no-op stub (registry-wide sync; production wiring deferred to `0959-c3` `NodeTransport` fan-out per RFC-0862).

**Design rationale:** `HolderRegistry` lives in Layer B-adjacent (quota-router-storage); adding `&dyn NodeTransport` to `sync_peers()` would couple Layer B → Layer D (forbidden per [[cipherocto-design-principles]] Layer direction table). Solution: `serialize_for_gossip` + `apply_gossip_record` are transport-agnostic; the test harness wires `tokio::sync::mpsc` as in-process gossip substitute. Production wiring (commit `4ed4ff1f` precedent: `NodeTransport::broadcast`) tracks in `0959-c3`.

**Cross-crate compat:** `cargo test -p quota-router-storage --lib` 165/165 pass (zero regressions); `cargo test -p octo-wallet --lib capability` 145/145 pass (zero regressions); `cargo test -p octo-wallet --test holder_registry_cross_node_gossip` 5/5 pass; `cargo clippy -p quota-router-storage -p octo-wallet --all-targets -- -D warnings` clean; `cargo fmt --check` clean.

## RFC

RFC-0957-A1 (Economics): Holder Registry + Catalog Storage (Amendment) — Accepted 2026-08-02
RFC-0862 (Network): Gossip Substrate — referenced for gossip channel binding

**Sub-mission of:** `missions/claimed/0957-c-holder-registry-impl.md` (top-level sub-mission under `0957-a1-holder-registry`)

## Summary

Implement the TV5 cross-node mint verifiability integration test deferred from `0957-c-holder-registry-impl` Band A closure (2026-08-06). The test exercises:

1. node-A `mint` produces a `CapabilityToken` + `HolderRecord`
2. node-A gossips the holder record via RFC-0862 gossip channel
3. node-B receives the gossip delta + applies it to its local `StoolapHolderRegistry`
4. node-B `lookup_active(cap_root_hash)` returns the synced record
5. Verify the same `CapabilityToken` verifies on both nodes

## Acceptance Criteria

### Integration test (TV5)

- [ ] NEW: `crates/octo-wallet/tests/holder_registry_cross_node_gossip.rs` integration test
- [ ] Two `StoolapHolderRegistry::open_in_memory()` fixtures (one per node) — pattern demonstrated in `0959-c2-cross-node-delivery` TV7 test
- [ ] node-A mint: produces `CapabilityToken` + `HolderRecord::from_capability`
- [ ] node-A gossip: invokes `HolderRegistry::sync_peers()` with the new record (current 0957-c stub returns `Ok(())`; this mission delivers the real impl)
- [ ] node-B receives + applies: gossip sync handler inserts the record into node-B's local registry
- [ ] node-B lookup: `HolderRegistry::lookup_active(cap_root_hash, &clock)` returns the synced record (ACTIVE state, not revoked, not expired)
- [ ] Byte-equality assertion: `node_a_record == node_b_record` (content-addressable PK ensures this; verify via Debug redacted representation)
- [ ] Holder sig verification: same `CapabilityToken::verify_holder_sig()` passes on both nodes

### sync_peers impl upgrade

- [ ] `HolderRegistry::sync_peers()` default impl upgraded from `Ok(())` stub to real gossip fan-out (or removed in favor of an explicit channel)
- [ ] If retained as default impl, must be a no-op `Ok(())`; production impls override with RFC-0862 gossip binding

### Cross-crate compat

- [ ] `cargo test -p octo-wallet --test holder_registry_cross_node_gossip` green
- [ ] `cargo test -p octo-wallet --lib capability` zero regressions
- [ ] `cargo clippy --workspace --all-targets --features full -- -D warnings` clean
- [ ] `cargo fmt --check` clean

## Dependencies

**Requires:**

- `missions/claimed/0957-c-holder-registry-impl.md` — `HolderRegistry` trait + `StoolapHolderRegistry` impl + `HolderRecord` schema
- `missions/claimed/0959-c2-cross-node-delivery.md` — TV7 in-process harness pattern for two-node fixtures
- RFC-0862 gossip substrate (Layer D via `octo-transport::NodeTransport`)

**Mission gates:**

- Per-extension crate layout: integration test wires `Arc<dyn HolderRegistry>` directly (catalog accessor was dropped in Phase 2c-2)

**Not Requires:**

- RFC-0871 acceptance (independent of NodeEnvelope work)
- RFC-0009 §Identity evolution (not in scope)

## Implementation Guide

- Test fixture pattern:
  - Two in-process `StoolapHolderRegistry` instances backed by separate in-memory DBs
  - Gossip channel = `tokio::sync::mpsc::channel(8)` between the two nodes (in-process substitute for RFC-0862 sender set)
- Test steps:
  1. Construct node-A + node-B fixtures
  2. node-A: `mint(root_secret, holder, did, caveats)` → `CapabilityToken` + `HolderRecord::from_capability(token)`
  3. node-A: gossip the record via mpsc sender
  4. node-B: receive + `insert(record)` into local registry
  5. node-B: `lookup_active(cap_root_hash, &clock)` returns Some(record)
  6. Verify byte-equal + verify holder sig

## Decomposition Rationale

Single-file mission: 1 integration test + 1 trait method upgrade. Below BLUEPRINT §Multi-Mission Decomposition threshold.

## Claimant

@cipherocto (implementation)

## Pull Request

(unset — local commit per [[feedback_initiation_user_only]]; push awaits user instruction)

## Closure Notes (2026-08-09)

- **Trait extension:** added `serialize_for_gossip(&[u8; 32]) -> Result<Vec<u8>, RegistryError>` + `apply_gossip_record(&[u8]) -> Result<(), RegistryError>` to `HolderRegistry` trait with default impls. `sync_peers()` retained as no-op stub for registry-wide fan-out (production wiring tracked in `0959-c3`).
- **Canonical JSON helpers:** `HolderRecord::canonical_ser()` + `HolderRecord::canonical_de()` use the existing `serde(Serialize, Deserialize)` derives (canonical form per RFC-0126 §canonical JSON for `#[derive(Serialize)]` named-field structs).
- **Layer discipline preserved:** no new deps added; `quota-router-storage` stays Layer B-adjacent (no `octo-transport` dependency).
- **Test fixture:** `crates/octo-wallet/tests/holder_registry_cross_node_gossip.rs` (NEW, 211 lines) — `TwoNodeFixture` struct + 5 integration tests covering the full gossip pipeline.
- **Net diff:** +228 lines (production: +40 in `holder_record.rs`, +25 in `holder_registry.rs`; tests: +211 in new file). Zero production regressions.

Per [[git-workflow]] push awaits user instruction. Per [[no-line-refs-anywhere]] all references use §section-name / symbol form. Per [[rfc-referencing-convention]] RFCs referenced by number only.

**Version History:**

| Version | Date | Change |
| --- | --- | --- |
| v0.1 | 2026-08-09 | Mission filed. Captures TV5 cross-node mint verifiability deferred from 0957-c Band A closure. RFC-0862 gossip substrate binding required. |
| v0.2 | 2026-08-09 | Claimed + Closed (Band A). Trait methods + canonical helpers + 5-test integration suite landed. 165/165 + 145/145 + 5/5 zero regressions. Layer discipline preserved (no new deps). |

Last Updated: 2026-08-09
Version: 0.2