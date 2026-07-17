# sync-e2e-tests

End-to-end integration tests for the CipherOcto Stoolap Data Sync Protocol (RFC-0862).

## Test Layers

| Layer | What | Processes | Transport | When |
|-------|------|-----------|-----------|------|
| **L1** Unit | octo-sync modules + stoolap adapter | single | in-memory | every commit |
| **L2** Adapter | StoolapAdapter with real DB | single | in-memory | every commit |
| **L3** In-process | Full sync engine with MockAdapter | single | in-process | every commit |
| **L4** Cross-process | Real Stoolap DBs over TCP | multi | TCP | every commit |
| **L5** Container | Docker containers on network bridge | multi | Docker network | manual |

## Running Tests

```bash
# L1 (octo-sync)
cd octo-sync && cargo test

# L2 (stoolap adapter)
cd /path/to/stoolap && cargo test --features sync

# L3 (in-process E2E)
cd sync-e2e-tests && cargo test --test l3_in_process

# L4 (cross-process TCP)
cd sync-e2e-tests/stoolap-node && cargo build
cd sync-e2e-tests && cargo test --test l4_cross_process

# L5 (Docker containers)
cd sync-e2e-tests && cargo test --test l5_container
```

## Architecture

```
sync-e2e-tests/
├── src/lib.rs              # TestNode, TestCluster, assert_converged
├── tests/
│   ├── l3_in_process.rs    # 12 tests (MockAdapter, in-process)
│   ├── l4_cross_process.rs # 5 tests (real Stoolap, TCP)
│   └── l5_container.rs     # 5 tests (Docker containers)
└── stoolap-node/
    ├── Cargo.toml
    └── src/main.rs          # Minimal binary wrapping Database::open_with_sync
```

## Key Design Decisions

- **L3 uses `MockAdapter`** — no real Stoolap DB, just the sync engine logic
- **L4 writer uses `file://` DSN** — `memory://` has no WAL so LSN stays at 0
- **L4 reader uses `memory://` DSN** — verification via `--status-file` (live db query)
- **L5 builds Docker image** — copies pre-built `stoolap-node` binary into `ubuntu:20.04`
- **`SyncSessionManager`** — orchestrates all sync modules (WalTailStreamer, SegmentIndexer, MissionKeyRing, ReplayCacheManager, per-peer state machines)

## Test Count

- L1: 125 tests (117 unit + 7 proptest + 1 doc)
- L2: 11 tests (in stoolap fork)
- L3: 12 tests
- L4: 5 tests
- L5: 5 tests
- **Total: 158 tests**
