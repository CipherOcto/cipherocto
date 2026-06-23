# Mission: 0862j — Network Layer Integration (wire sync into octo-network)

## Status

Open

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

- [ ] `octo-network/src/sync/mod.rs` exists with `SyncNode` struct
- [ ] `SyncNode::open` calls `Database::open_with_sync` and creates `SyncSessionManager`
- [ ] `SyncNode::on_snapshot_fragment` routes DGP subtypes to the sync engine
- [ ] `SyncNode::send_sync_envelope` packages sync payloads as DGP `GossipObject`
- [ ] Unit tests pass: `cargo test -p octo-network`
- [ ] Clippy clean: `cargo clippy -p octo-network -- -D warnings`

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
