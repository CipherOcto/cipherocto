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
Rust Node (StoolapAdapter)     Cairo Node (future port)
  ├── Open DB                    ├── Open DB
  ├── Commit 1000 rows           ├── Connect to Rust node
  ├── Serve WAL entries          ├── Apply WAL entries
  └── Verify state matches       └── Verify state matches
```

### Verification method

Both nodes compute `BLAKE3-256(SELECT * FROM each_table)` and compare. Per RFC-0862 §Determinism, the same operations on the same data must produce the same hash.

### Prerequisites

- Cairo/Move port of Stoolap (F5 in RFC-0862 Future Work)
- Both implementations must agree on:
  - WAL V2 binary format (magic, header, CRC32)
  - Table serialization format
  - BLAKE3-256 hashing semantics

## Acceptance Criteria

- [ ] Test harness supports two implementations (Rust + mock Cairo)
- [ ] Both nodes commit identical data
- [ ] Both nodes verify state via BLAKE3-256 hash comparison
- [ ] Test passes when implementations agree
- [ ] Test fails when implementations disagree (regression test)

## Dependencies

- **Requires:** `0862-base`, `0862f` (multi-peer), F5 (Cairo port — future)
- **Required by:** none

## Complexity

Medium (~300 lines). Blocked on Cairo/Move port (F5). Can be partially implemented with a mock Cairo node for protocol-level testing.
