# Mission: Cross-Node Mint Verifiability via RFC-0862 Gossip (RFC-0957-A1 §G5)

## Status

Open (2026-08-09). Sub-mission of `missions/claimed/0957-c-holder-registry-impl.md` per [[deferred-vs-unspecified]] deferral rule (TV5 cross-node mint verifiability, owner @cipherocto, target 2026-08-28).

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

@unassigned (per `[[feedback_initiation_user_only]]` — user initiates the claim)

## Pull Request

(unset)

## Notes

- Mission captured in `0957-c-holder-registry-impl.md` §Cross-node mint verifiability (G5) deferral note 2026-08-06
- Per `[[no-phantom-mission-pointers]]`: mission file now exists; the phantom pointer that existed in 0957-c status note is now resolved
- Per `[[cargo-fmt-workflow]]` + `[[feedback_clippy_zero_warnings]]`: `cargo fmt` + `cargo clippy -D warnings` green before commit

**Version History:**

| Version | Date | Change |
| --- | --- | --- |
| v0.1 | 2026-08-09 | Mission filed. Captures TV5 cross-node mint verifiability deferred from 0957-c Band A closure. RFC-0862 gossip substrate binding required. |

Last Updated: 2026-08-09
Version: 0.1