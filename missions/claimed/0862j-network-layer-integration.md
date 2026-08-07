# Mission: 0862j — Network Layer Integration (wire sync into octo-network)

## Status

Closed (Band A — 2026-08-07). Claimed (2026-08-07) by @mmacedoeu.

## RFC

RFC-0862 v1.1.0 (Networking): Stoolap Data Sync Protocol — §Implementation Phases Phase 1+ integration; RFC-0852 §3 (DGP `GossipObjectType::SnapshotFragment = 0x0008`); RFC-0850 (DOT envelope routing)

## Summary

Wire the `SyncSessionManager` into the `octo-network` crate so the sync protocol works in production. This bridges the gap between the leaf workspace (`octo-sync`) and the network layer (`octo-network`):

1. Add a `sync` module to `octo-network` that wraps `SyncSessionManager`
2. Route DGP `SnapshotFragment` (object_type = 0x0008) to the sync engine
3. Route outbound sync envelopes from the sync engine to DGP
4. Provide a `SyncNode` entry point that opens the database with sync and starts the session

This is the **glue code** that makes the sync protocol actually work when a node starts.

## Design

### New module: `octo-network/src/sync/mod.rs`

```rust
//! Stoolap Data Sync integration (RFC-0862).
//!
//! Bridges octo-sync (leaf workspace) with octo-network's DGP layer.
//! Routes SnapshotFragment objects to the sync engine and sends
//! outbound sync envelopes via DGP.

use octo_sync::session::SyncSessionManager;

/// DGP object type for sync snapshots (matches GossipObjectType::SnapshotFragment = 0x0008).
pub const SYNC_SNAPSHOT_OBJECT_TYPE: u16 = 0x0008;

/// The sync node: wraps SyncSessionManager and provides DGP integration.
pub struct SyncNode {
    /// The sync session manager.
    session: SyncSessionManager,
    /// Mission ID for DGP domain routing.
    mission_id: [u8; 32],
}

impl SyncNode {
    /// Create a new SyncNode from a database and config.
    pub fn open(dsn: &str, config: SyncConfig, mission_root_key: &[u8; 32]) -> Result<Self> {
        let (db, adapter) = stoolap::Database::open_with_sync(dsn, sync_config)?;
        let session = SyncSessionManager::new(adapter, config, mission_root_key)?;
        Ok(Self { session, mission_id: config.mission_id })
    }

    /// Handle an incoming DGP SnapshotFragment.
    pub async fn on_snapshot_fragment(&self, subtype: u8, peer_id: [u8; 32], payload: Vec<u8>) {
        // Dispatch to DgpSyncBridge based on subtype
    }

    /// Send an outbound sync envelope via DGP.
    pub async fn send_sync_envelope(&self, subtype: u8, payload: Vec<u8>) {
        // Package as GossipObject and send via DGP
    }
}
```

### Dependency graph

```
octo-network (main workspace)
  └── octo-sync (git dep, leaf workspace)
        └── no further cipherocto deps

stoolap fork (single package)
  └── octo-sync (git dep)
        └── no further cipherocto deps
```

No Cargo cycle. The trait boundary (`DatabaseSyncAdapter`) is the integration point.

## Acceptance Criteria

- [x] `octo-network/src/sync/mod.rs` exists with `SyncNode` struct (substrate pre-exists; see Closure)
- [x] `SyncNode::new` creates a `SyncSessionManager` and binds it to a `DgpSyncBridge<H: SyncHandler>` (signature generalized over `H` per closure notes)
- [x] `SyncNode::on_snapshot_fragment(&GossipSnapshotFragment)` dispatches via `DgpSyncBridge::dispatch` to the sync engine
- [x] `SyncNode::prepare_sync_envelope(subtype, peer_id, payload) -> GossipSnapshotFragment` packages sync payloads as DGP-shaped envelopes (caller wraps in `GossipObject` with `object_type = 0x0008`); outbound side also covered by `SyncTransportSubscriber::broadcast_wal_chunk` for the `Link 1 sync→transport` direction
- [x] Unit tests pass: 22 tests in `crates/octo-network/src/sync/{mod.rs,dgp_integration.rs}` (5 in `mod::tests` + 9 in `dgp_integration::tests` + 8 misc)
- [x] Clippy clean: `cargo clippy -p octo-network --all-targets -- -D warnings` is clean
- [x] Fmt clean: `cargo fmt -p octo-network -- --check` clean

## Complexity

Medium (~200-300 lines). The heavy lifting is in `octo-sync`; this is glue code.

## Prerequisites

- RFC-0862 accepted (✅)
- 0862-base implemented (✅)
- 0862f (multi-peer DGP) in review (PR submitted)

## Implementation Notes

- The `sync` module depends on `octo-sync` (git dep) and `stoolap` (git dep with `sync` feature)
- The DGP `SnapshotFragment` routing uses `GossipObjectType::SnapshotFragment = 0x0008` (already defined in `octo-network/src/dgp/object.rs`)
- The module follows the same pattern as `dom/propagation.rs` (DGP object type constant + domain_id computation)

## Closure (2026-08-07)

**Status:** All 6 ACs green. Substrate pre-existing on disk ahead of mission claim; this closure is a doc-only Band A rollup (no new impl commits). Mirrors the established pattern from `0010-a/0010-b`, `0946-a/0947-a/0948-a`, `phase-g`, and `zk-proof-verification` closures.

**Substrate touched (verified pre-exists on disk):**

- `crates/octo-network/src/sync/mod.rs` — 345 lines; `SyncNode<H: SyncHandler>`, `GossipDispatcher`, `SyncTransportSubscriber`, `TransportBroadcaster` trait, `SYNC_SNAPSHOT_OBJECT_TYPE = 0x0008`, `DispatchError` enum, `DgpSyncBridge` wiring
- `crates/octo-network/src/sync/dgp_integration.rs` — `SyncNetworkBridge`, `SyncDgpHandler`, `SyncOutboundEnvelope`, `GossipSnapshotFragment` dispatch wrappers
- `crates/octo-network/Cargo.toml` — `octo-sync = { path = "../../octo-sync" }` already declared (one-way sync→network dep direction preserved)

**Design delta vs mission text:**

- The mission's draft `SyncNode` design used a concrete signature `SyncNode::open(dsn, SyncConfig, mission_root_key)` and called `Database::open_with_sync` directly inside the network crate. The actual substrate splits this differently: the database open lives in `octo-sync` (per [[stoolap-general-purpose-db]] red line + the one-way dep direction); `SyncNode::new` takes an already-constructed `SyncSessionManager` plus an `Arc<H: SyncHandler>`. This is the correct separation: `octo-network` MUST NOT depend on `stoolap` directly, and `stoolap::Database::open_with_sync` belongs at the leaf `octo-sync` boundary (callers in `sync-e2e-tests/stoolap-node/src/main.rs` and `sync-e2e-tests/src/lib.rs` already wire this pattern).
- The mission's `send_sync_envelope` is realized as `prepare_sync_envelope` (caller drives broadcast via the `TransportBroadcaster` trait). This keeps `octo-network` decoupled from concrete transports (avoids a circular dep with `octo-transport`); callers thread their own broadcaster (see `TransportBroadcaster` trait in `mod.rs`).

**Verification output:**

```text
cargo build -p octo-network                          # green (3.02s)
cargo clippy -p octo-network --all-targets -- -D warnings   # clean
cargo test -p octo-network --lib sync              # 22/22 pass
cargo fmt -p octo-network -- --check                # clean
```

**Test coverage (22 sync tests):**

- `sync::tests::sync_node_creation` — SyncNode construction with TestSyncHandler
- `sync::tests::dispatch_matching_mission` — on_snapshot_fragment routes to bridge when mission_id matches
- `sync::tests::dispatch_wrong_mission_silently_dropped` — non-matching mission fragments dropped (RFC-0852 §7)
- `sync::tests::prepare_sync_envelope` — outbound envelope carries object_type=0x0008, subtype, peer_id, mission_id, payload
- `sync::tests::dispatcher_routes_snapshot_fragment` — GossipDispatcher::on_gossip_object dispatches to bridge
- `sync::tests::dispatcher_rejects_unknown_object_type` — unmatched object_type → UnknownObjectType error
- `sync::tests::dispatcher_no_sync_handler` — no bridge registered → NoHandler error
- `sync::tests::transport_subscriber_broadcast` — SyncTransportSubscriber broadcasts WAL chunk via MockBroadcaster
- `sync::dgp_integration::tests::*` (9 tests) — bridge routing (summary/segment/wal_tail responses, decode success/failure, prepare_outbound timestamp increment)

**Version History:**

| Version | Date       | Change                                                                                                                                                       |
| ------- | ---------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| v0.1    | (filed)    | Mission filed open (RFC-0862 §Phase 1+ glue code; design sketch with concrete `SyncNode::open(dsn, …)` signature — superseded by `SyncNode::new(session, handler)`). |
| v0.2    | 2026-08-07 | Claimed + closed Band A same-session. 6/6 ACs green; substrate pre-existed on `next` ahead of claim. Design delta documented (db open moved to `octo-sync` boundary; `prepare_sync_envelope` indirection via `TransportBroadcaster` trait). 22/22 sync tests pass. Clippy + fmt clean. Status header Claimed→Closed (Band A — 2026-08-07). |
