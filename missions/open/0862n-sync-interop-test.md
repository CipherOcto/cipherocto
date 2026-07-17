# Mission: 0862n — Sync Interop Test

## Status

Planned

## RFC

RFC-0862 v1.1.0 (Networking): Stoolap Data Sync Protocol — §Implementation Phases Phase 4

## Summary

Verify that two independent implementations (Rust + the eventual Cairo/Move ports) reach identical state when syncing the same data. This is the definitive interop test for the sync protocol.

This is a Phase 4 requirement per RFC-0862 §Implementation Phases Phase 4: "Interop test: two implementations (Rust + the eventual Cairo / Move ports) reach identical state".

## Design

### Test architecture

```text
Rust Node (StoolapAdapter)     Mock Cairo Node (test double)
  ├── Open DB                    ├── Open DB (mock)
  ├── Commit 1000 rows           ├── Connect to Rust node
  ├── Serve WAL entries          ├── Apply WAL entries
  └── Verify state matches       └── Verify state matches
```

### Why a mock Cairo node?

The actual Cairo/Move port (F5 in RFC-0862 Future Work) does not exist yet. This mission creates a **mock Cairo node** that:
- Accepts the same WAL V2 binary format
- Applies entries to a simplified in-memory store
- Computes BLAKE3-256 hashes for comparison

This allows protocol-level interop testing NOW, even before the real Cairo port exists. When the real port is implemented, the mock is replaced.

### Verification method

Both nodes compute `BLAKE3-256(SELECT * FROM each_table)` and compare. Per RFC-0862 §Determinism, the same operations on the same data must produce the same hash.

### Prerequisites

- Mock Cairo node (this mission creates it)
- Both implementations must agree on:
  - WAL V2 binary format (magic, header, CRC32) — already specified in Stoolap
  - Table serialization format — already specified in Stoolap
  - BLAKE3-256 hashing semantics — already specified in RFC-0126

### What already exists

- `stoolap-node` binary (`sync-e2e-tests/stoolap-node/`) can serve as the Rust node
- `MockAdapter` (`octo-sync/src/test_util.rs`) provides in-memory adapter for testing
- BLAKE3-256 is already used throughout the sync protocol

### What this mission creates

- `MockCairoNode` — a test double that applies WAL entries to an in-memory store
- Interop test harness — commits data to Rust node, syncs to mock Cairo, compares hashes
- Regression test — detects when protocol changes break interop

## Acceptance Criteria

- [ ] `MockCairoNode` struct that applies WAL V2 entries to in-memory store
- [ ] Test harness commits data to Rust node, syncs to mock Cairo
- [ ] Both nodes verify state via BLAKE3-256 hash comparison
- [ ] Test passes when implementations agree
- [ ] Test fails when mock is intentionally broken (regression test)
- [ ] Mock implements the same WAL V2 decode path as StoolapAdapter

## Dependencies

- **Requires:** `0862-base`, `0862f` (multi-peer)
- **Required by:** none
- **Future:** Cairo/Move port (F5) replaces mock with real implementation

## Complexity

Medium (~250 lines). Mock Cairo node is the main work; test harness is straightforward.

## Changelog

- **Round 1** (2026-06-23): Clarified that Cairo port doesn't exist yet — mission creates mock Cairo node for protocol-level testing. Added details on what already exists vs what's new. Removed dependency on F5 (future).
