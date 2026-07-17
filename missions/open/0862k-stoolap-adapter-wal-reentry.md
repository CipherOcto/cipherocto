# Mission: 0862k — StoolapAdapter WAL Re-entry for Chain Relay

## Status

Completed

## RFC

RFC-0862 v1.1.0 (Networking): Stoolap Data Sync Protocol — §4.3.3.1 Chain Relay Topology, §DatabaseSyncAdapter Durability/LSN Advancement requirements

## Summary

Fix the `StoolapAdapter::apply_wal_entry` implementation to persist received WAL entries to the local WAL and advance the LSN counter. Currently, `apply_wal_entry` only applies to in-memory MVCC state (via `MVCCEngine::apply_wal_entry_bytes`), which means:

1. `current_lsn()` stays at 0 (or whatever it was from local writes)
2. `read_wal_range()` returns empty (reads from WAL files on disk, which were never written)
3. Chain relay fails silently — downstream peers receive no entries

This is the root cause of the L4/L5 chain relay test failures identified during multi-peer E2E testing.

## Design

### Current behavior (broken for chain relay)

```rust
// StoolapAdapter::apply_wal_entry (current)
fn apply_wal_entry(&self, entry: &[u8]) -> Result<(), SyncError> {
    self.engine.lock().apply_wal_entry_bytes(entry)
    // ↑ applies to in-memory MVCC state only
    // ↑ does NOT write to WAL files
    // ↑ does NOT advance LSN counter
}
```

### Required behavior (per RFC-0862 §DatabaseSyncAdapter)

```rust
// StoolapAdapter::apply_wal_entry (fixed)
fn apply_wal_entry(&self, entry: &[u8]) -> Result<(), SyncError> {
    // 1. Apply to in-memory MVCC state (existing behavior)
    self.engine.lock().apply_wal_entry_bytes(entry)?;
    
    // 2. Re-enter into local WAL (NEW — required for chain relay)
    // This makes the entry visible to read_wal_range() and advances LSN
    if let Some(pm) = self.persistence.as_ref().as_ref() {
        if pm.is_enabled() {
            // Decode the entry back to WALEntry for append_entry
            let decoded = WALEntry::decode(entry)?;
            pm.wal.append_entry(decoded)?;
            // ↑ writes to wal-*.log files on disk
            // ↑ advances current_lsn counter via fetch_add(1)
        }
    }
    
    Ok(())
}
```

### Key details

1. **WAL re-entry**: After applying to in-memory state, decode the raw bytes back to `WALEntry` and call `WALManager::append_entry()` to persist to WAL files and advance LSN.

2. **Persistence check**: Only re-enter if persistence is enabled (`pm.is_enabled()`). In-memory-only mode (no WAL files) doesn't need re-entry.

3. **Idempotency**: `append_entry` assigns a new LSN via `fetch_add(1)`. For idempotency, the adapter should check if the entry's LSN is already ≤ `current_lsn()` before re-entering. If so, skip the re-entry (it's a replay).

4. **LSN conflict**: The received entry has the writer's LSN. Re-entering assigns a new local LSN. This is correct — each node has its own LSN namespace. The sync engine tracks per-peer LSN watermarks, so LSN values are peer-scoped.

### Implementation location

- **File**: `/home/mmacedoeu/_w/databases/stoolap/src/sync_adapter.rs`
- **Method**: `StoolapAdapter::apply_wal_entry` (line 219)
- **Dependency**: `WALManager::append_entry` (wal_manager.rs:1287)

### Testing

1. **Unit test**: Verify `apply_wal_entry` writes to WAL and `read_wal_range` returns the entry
2. **Unit test**: Verify `current_lsn()` advances after `apply_wal_entry`
3. **Unit test**: Verify idempotency (replay doesn't double-advance LSN)
4. **L3 E2E**: Chain relay test (A→B→C) passes with real StoolapAdapter
5. **L4 E2E**: `L4-T6: tcp_chain_relay` passes

## Acceptance Criteria

- [x] `StoolapAdapter::apply_wal_entry` persists entries to WAL files
- [x] `StoolapAdapter::apply_wal_entry` advances `current_lsn()` counter
- [x] `StoolapAdapter::read_wal_range` returns entries applied via `apply_wal_entry`
- [x] Idempotency: replay of same entry does not double-advance LSN
- [x] L3 chain relay test passes (A→B→C)
- [x] L4-T6 chain relay test passes
- [x] All existing L1-L4 tests still pass
- [x] `cargo clippy -D warnings` clean (stoolap-node, sync-e2e-tests)
- [x] `cargo fmt` clean

## Complexity

Low (~30-50 lines change in `sync_adapter.rs`). The heavy lifting is in the existing `WALManager::append_entry` — this mission just wires it into the apply path.

## Prerequisites

- RFC-0862 v1.1.0 accepted with §4.3.3.1 Chain Relay and §DatabaseSyncAdapter Durability requirements (✅)
- 0862-base implemented (✅)
- 0862j (network layer integration) implemented (✅)
- Multi-peer E2E tests added (✅)

## Implementation Notes

- The `WALManager::append_entry` method (wal_manager.rs:1287) already handles LSN assignment, WAL file writing, and buffer management. The fix just needs to call it after the in-memory apply.
- The `WALEntry::decode` method is needed to convert raw bytes back to a `WALEntry` struct for `append_entry`. This is the inverse of `WALEntry::encode`.
- The persistence check (`pm.is_enabled()`) is important — in-memory-only mode (no WAL files) should not attempt WAL re-entry.
- The idempotency check (`entry.lsn <= current_lsn()`) prevents double-advance on replay. This is critical for the `apply_wal_entry` MUST be idempotent requirement.
