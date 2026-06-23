# Stoolap Data Sync — End-to-End Integration Test Plan

**RFC-0862 v1.1.0** — Stoolap Data Sync Protocol
**Scope:** End-to-end (E2E) integration tests with 2 and 3 database instances, sync on.
**Goal:** Verify the full sync path from writer → transport → reader, with realistic topologies.

---

## 1. Test Architecture & Layering

The test suite is organized in **5 layers**, from cheapest/quickest to most realistic. Each layer exercises more of the production code path and more of the network/process boundary.

| Layer | Real DB | Real Adapter | Real Sync Engine | Transport | Process | Container | When to run |
|-------|---------|--------------|------------------|-----------|---------|-----------|-------------|
| **L1** Unit (existing) | Mock | n/a | partial | in-mem | single | no | always (per-commit CI) |
| **L2** Adapter integration | Stoolap | Stoolap | Mock + manual | in-mem | single | no | every PR |
| **L3** In-process E2E | Stoolap | Stoolap | real | bounded mpsc | single | no | every PR |
| **L4** Cross-process E2E | Stoolap | Stoolap | real | TCP | multi | no | nightly + pre-release |
| **L5** Container E2E | Stoolap | Stoolap | real | TCP/DGP | multi | yes | pre-release + manual |

**Key principle:** the cipherocto sync engine never calls Stoolap DB functions directly. All DB operations go through `Arc<dyn DatabaseSyncAdapter>`. So **L1 tests use `MockAdapter` (no Stoolap needed), and L2+ tests use `StoolapAdapter` (real Stoolap)**. The `Arc<dyn DatabaseSyncAdapter>` swap is the single seam.

```
┌─────────────────────────────────────────────────────┐
│ cipherocto sync engine (WalTailStreamer,           │
│ SegmentIndexer, MultiCarrierSync, …)                │
│                                                     │
│  consumes: Arc<dyn DatabaseSyncAdapter>             │
└─────────────────────┬───────────────────────────────┘
                      │
        ┌─────────────┴──────────────┐
        │                            │
   L1: MockAdapter             L2+: StoolapAdapter
   (no Stoolap)                (real MVCCEngine)
```

---

## 2. Existing Test Coverage (baseline)

**octo-sync** (leaf workspace at `cipherocto/octo-sync/`):
- **L1 unit tests** (per module): `adapter`, `stream`, `segment`, `summary`, `keyring`, `lsn`, `state`, `identity`, `config`, `replay_cache`, `raft_overlay`, `dgp_bridge`, `carrier` (60+ tests)
- **L1 property tests** (`tests/property_tests.rs`): 6 proptests using `MockAdapter`
  - Envelope round-trip (skipped, in module tests)
  - LSN monotonicity
  - Merkle tree determinism
  - HMAC binding
  - AEAD round-trip
  - State machine coverage
- **L1 doc tests**: `trait_object_compiles`

**stoolap** (fork at `stoolap/`):
- **L1 unit tests** (in `src/sync_adapter.rs`): 21 tests covering construction, identity, current_lsn, read_wal_range edge cases, table_id determinism, schema_epoch cache invalidation, apply_wal_entry error classification (bad magic, too short, bad version), read/write_snapshot_segment unknown table, persistence flow.

**What's missing (this plan fills):**
- **L2** StoolapAdapter + MockAdapter-via-real-sync-engine (real engine, real DB, no transport)
- **L3** In-process E2E with 2 and 3 instances
- **L4** Cross-process E2E with 2 and 3 instances (TCP transport)
- **L5** Container-based E2E with 2 and 3 containers (network bridge)

---

## 3. Test Harness Design

The test harness is a single crate `sync-e2e-tests` that depends on:
- `octo-sync` (with `test-util` feature for `MockAdapter`)
- `stoolap` (with `sync` feature, git dep — same as cipherocto)
- `tokio` (for async test runtime)
- `proptest` (for property-based E2E tests)
- `tempfile` (for per-test DB directories)
- `tokio::net::TcpListener` (for L4/L5 transport)

The harness provides:
- `TestNode` — wraps a real `MVCCEngine` + `StoolapAdapter` + `WalTailStreamer` + `SegmentIndexer` (L3+)
- `TestCluster` — owns N `TestNode`s and the in-process channels that connect them
- `TestTransport` — abstraction: `InProcessTransport` (L3) or `TcpTransport` (L4) or `DockerTransport` (L5)
- `assert_converged(cluster, timeout)` — wait until all nodes agree on the same root hash + LSN
- `apply_wal_chunk_to_node(node, chunk)` — extract a `WalTailChunk` from a node's streamer and apply it to another

```text
                   TestCluster::new(3, Topology::Star)
                              │
              ┌───────────────┼───────────────┐
              │               │               │
          TestNode[0]     TestNode[1]     TestNode[2]
              │  (writer)     │  (reader)      │  (observer)
              │               │               │
   ┌──────────┴──────────┐    │               │
   │ MVCCEngine         │    │               │
   │ + StoolapAdapter   │    │               │
   │ + WalTailStreamer  │    │               │
   │ + SegmentIndexer   │    │               │
   └────────────────────┘    │               │
              │               │               │
              └───── TestTransport (in-process mpsc) ─────┘
```

---

## 4. Concrete Test Cases

### L2: Adapter integration (single process, real Stoolap, MockAdapter or no sync engine)

These run in the **stoolap fork** under `tests/l2_adapter_integration.rs`. The cipherocto sync engine is NOT involved — we directly drive the `StoolapAdapter` and verify its state transitions.

| Test | What it verifies | Topology |
|------|------------------|----------|
| `L2-T1: wal_roundtrip_via_adapter` | Writer: `record_commit` → LSN advances → `read_wal_range` returns the new entry. Reader: `apply_wal_entry_bytes` succeeds. Verify the reader's state matches the writer's. | 2 instances |
| `L2-T2: snapshot_segment_roundtrip` | Writer: `create_snapshot_for_table` → `read_snapshot_segment` returns the bytes. Reader: `write_snapshot_segment` → reader's `regenerate_snapshot` produces the same root. | 2 instances |
| `L2-T3: table_id_roundtrip` | Writer: `compute_table_id("users")` → `write_snapshot_segment` with that table_id → `read_snapshot_segment` finds the segment by table_id (BLAKE3-256 first-4-bytes). | 2 instances |
| `L2-T4: regeneration_on_missing_segment` | Writer deletes a segment file. Reader calls `read_snapshot_segment` → `SegmentNotFound` → calls `regenerate_snapshot` → new segment exists. | 2 instances |
| `L2-T5: schema_epoch_invalidation` | Writer creates a new table. Reader's adapter cache is stale. Reader's `find_table_name_by_id` rebuilds the cache on the next call. | 2 instances |
| `L2-T6: persistence_path_via_tempdir` | Writer uses a temp dir. Writer commits 100 rows. Read writer state from disk via `reopen_engine`. Verify all 100 rows are present. | 1 instance (smoke) |
| `L2-T7: error_classification_decryption_failed` | Reader calls `apply_wal_entry` with bad magic → `DecryptionFailed`. With too-short bytes → `DecryptionFailed`. With bad version → `DecryptionFailed`. | 1 instance |
| `L2-T8: error_classification_backend_not_ready` | Reader calls `apply_wal_entry` on a closed engine → `BackendNotReady`. | 1 instance |

**L2 status:** of these 8 tests, T1–T3 and T7–T8 are partially covered by the existing 21 unit tests in `sync_adapter.rs`. The remaining 4 (T4–T6) are new and need to be added.

### L3: In-process E2E (single process, real Stoolap, real sync engine, in-process mpsc transport)

These run in a **new crate** `sync-e2e-tests` under `cipherocto/sync-e2e-tests/`. The cipherocto sync engine (WalTailStreamer, SegmentIndexer, MissionKeyRing) IS involved.

| Test | What it verifies | Topology |
|------|------------------|----------|
| `L3-T1: two_node_wal_tail` | Writer commits 10 rows. Reader's `WalTailStreamer` receives 10 `WalTailChunk`s. Reader applies them. Verify both nodes have the same state. | 2 instances (writer + reader) |
| `L3-T2: two_node_summary_descent` | Writer has 50 rows in 3 tables. Reader requests `SummaryResponse` for each table → `MerkleSegmentTree` root matches. Reader requests `SegmentResponse` for any divergent segment → state converges. | 2 instances |
| `L3-T3: three_node_fan_out` | Writer commits 100 rows. Two readers (replicator + observer). Both receive all `WalTailChunk`s. Verify all 3 nodes have the same state. | 3 instances (1 writer + 2 readers) |
| `L3-T4: three_node_replicator_observer_quorum` | 1 writer (Replicator) + 1 reader (Replicator, must-receive) + 1 observer (Observer, best-effort). Force observer to disconnect. Replicator still converges. | 3 instances (1 writer + 1 replicator + 1 observer) |
| `L3-T5: lsn_ack_advances_watermark` | Reader applies 5 chunks, sends 5 LsnAcks. Writer's per-peer LSN watermark advances. Verify writer refuses to ship LSN < last-acked (LsnRegression error). | 2 instances |
| `L3-T6: rate_limit_backpressure` | Writer commits 1000 rows in a tight loop. Reader has rate limit 10/s. Reader's outbox overflows → `BackendNotReady` → writer's `record_commit_error` → peer demoted to `Suspect`. | 2 instances |
| `L3-T7: pause_propagates_to_adapter` | Writer's `set_paused(true)` → adapter sees `paused=true` (via `DatabaseSyncAdapter::set_paused`). Reader's apply queue fills. Verify the writer's LSN still advances but chunks are buffered. | 2 instances |
| `L3-T8: segment_not_found_triggers_regen` | Writer has 1 table. Corrupt the segment file (truncate to 0 bytes). Reader requests `SegmentResponse` → root mismatch → `SegmentNotFound` → reader calls `regenerate_snapshot` on the writer → new segment → reader re-fetches summary. | 2 instances |
| `L3-T9: aead_round_trip_through_keyring` | Two nodes share a `MissionKeyRing`. Writer encrypts a payload with `execution_key`, reader decrypts with the same key. Verify plaintext matches. | 2 instances |
| `L3-T10: hmac_binding_per_node` | Two readers (A, B) receive the same `SummaryResponse`. Both compute the HMAC with their own `transport_key` + `node_id`. The HMACs differ. | 3 instances |
| `L3-T11: state_machine_lifecycle` | 2 nodes. Walk through every transition: `Standby` → `Handshaking` → `Active` → `Suspect` → `Active` (reconnect) → `Terminated`. | 2 instances |
| `L3-T12: restart_recovery` | Writer commits 10 rows. Writer restarts. Reader's `WalTailStreamer` detects the restart (heartbeat timeout). Reader re-handshakes. Verify state converges. | 2 instances |

**L3 status:** T1–T12 are all NEW. This is the bulk of the E2E work.

### L4: Cross-process E2E (same machine, multiple processes, TCP transport)

These run in **`sync-e2e-tests/tests/l4_cross_process.rs`**. They spawn 2–3 child processes (each running a small `stoolap-node` binary) and connect them via real TCP.

The child process is a minimal binary `stoolap-node` that:
- Takes a DSN and a `--sync-config` flag
- Opens the database with `Database::open_with_sync`
- Listens on a TCP port for sync envelopes
- Connects to peer nodes (given as `--peer host:port` flags)
- Runs until killed

| Test | What it verifies | Topology |
|------|------------------|----------|
| `L4-T1: two_node_tcp_roundtrip` | Spawn 2 `stoolap-node` processes. Writer commits 10 rows. Verify reader sees them via TCP. | 2 processes (TCP) |
| `L4-T2: three_node_tcp_fan_out` | Spawn 3 `stoolap-node` processes. Writer commits 100 rows. Verify both readers see all of them. | 3 processes (TCP) |
| `L4-T3: tcp_partition_and_heal` | 3 processes. Drop the writer↔reader1 TCP connection. Writer keeps committing. Reconnect. Verify reader1 catches up via the WAL tail after reconnection. | 3 processes (TCP) |
| `L4-T4: tcp_slow_consumer` | 2 processes. Reader's `apply` is artificially slowed (sleep 100ms per entry). Writer's outbox fills. Verify `BackendNotReady` backpressure is applied and the writer doesn't OOM. | 2 processes (TCP) |
| `L4-T5: process_crash_and_restart` | 2 processes. Reader crashes. Writer keeps committing. Reader restarts. Verify reader catches up via summary + WAL tail. | 2 processes (TCP) |

**L4 status:** T1–T5 are NEW. The `stoolap-node` binary is new.

### L5: Container E2E (Docker, network bridge)

These run in **`sync-e2e-tests/tests/l5_container.rs`**. They use the `bollard` (Docker daemon) crate to launch 2–3 containers, each running the `stoolap-node` binary, and connect them via the container network bridge.

| Test | What it verifies | Topology |
|------|------------------|----------|
| `L5-T1: two_container_sync` | 2 containers, one Docker network. Writer commits 50 rows. Verify reader sees them. | 2 containers (Docker) |
| `L5-T2: three_container_fan_out` | 3 containers. Writer commits 200 rows. Verify both readers see them. | 3 containers (Docker) |
| `L5-T3: container_network_partition` | 3 containers. `docker network disconnect` to partition one reader. Reconnect. Verify it catches up. | 3 containers (Docker) |
| `L5-T4: container_resource_limit` | 1 container with `--memory 256m` and `--cpus 0.5`. Writer commits 10K rows. Verify the container doesn't OOM and the writer handles backpressure correctly. | 1 container (Docker) |
| `L5-T5: container_kill_and_recover` | 2 containers. `docker kill` the reader. Writer keeps committing. `docker start` a new reader. Verify the new reader catches up. | 2 containers (Docker) |

**L5 status:** T1–T5 are NEW. The Docker test harness is new.

**When to use Docker vs same-machine (L4 vs L5):**
- **L4 is the default for "cross-process" E2E.** It catches real TCP behavior, real process isolation, real OS scheduling, and real file descriptor limits. It's also much faster than L5 (no container startup overhead).
- **L5 is used for scenarios L4 cannot simulate:**
  - Network partitions (L4 can simulate by closing TCP sockets, but not kernel-level network filtering)
  - Resource limits (memory, CPU) — L4 cannot enforce per-process resource limits without `prlimit(2)` or cgroups
  - Container orchestration scenarios (the cipherocto sync engine may run inside a container in production)
  - Multi-host scenarios (L4 is single-machine, L5 can use Docker Swarm or multi-host networking)

---

## 5. Mocking Strategy

| Component | Mock or real? | Why |
|-----------|----------------|-----|
| **DB engine** (MVCCEngine) | Real for L2+; Mock for L1 | The whole point of L2+ is to exercise the real engine + real adapter. L1 (existing) uses MockAdapter to isolate sync engine logic. |
| **Network transport** (TCP, mpsc, DGP) | Real for L3+; in-mem for L1 | L3 uses bounded mpsc as the "transport" (it's the cheapest real transport). L4 uses real TCP. L5 uses Docker networking. |
| **Cipherocto sync engine** | Real for L2+ | The engine is the consumer of the trait. Testing it with a mock defeats the purpose. |
| **KeyRing** (MissionKeyRing) | Real for L3+ | It's pure compute; no need to mock. |
| **DGP bridge** | Real for L3+ | It's a thin dispatcher; no need to mock. |
| **Multi-carrier broadcaster** | Real with mock carriers for L3; real with real carriers for L4+ | L3 can use a single mock carrier (in-mem mpsc) to test the multi-carrier logic without network. L4+ uses real carriers (TCP, NativeP2P, etc.). |
| **Heartbeat scheduler** | Real (uses tokio time) | No need to mock; tokio's `tokio::time::pause()` and `advance()` give deterministic tests. |
| **Other nodes (peers)** | Real for L3+ | No mocking — the whole point is to test multi-node behavior. |

---

## 6. Per-Test Plan

For each of the ~30 test cases, the plan specifies:
- **Layer**: L1–L5
- **Topology**: N nodes, M writers, K readers, Q observers
- **Setup**: what fixtures are needed
- **Action**: what each node does
- **Assertions**: what we check
- **Cleanup**: how the test cleans up

(Detailed per-test plan in the next document; this one is the architecture overview.)

---

## 7. CI Integration

| Test layer | CI trigger | Estimated runtime | Resource budget |
|------------|------------|-------------------|-----------------|
| L1 unit | every commit | ~30s | 2 GB RAM, 1 CPU |
| L1 property | every commit | ~2 min | 2 GB RAM, 1 CPU |
| L2 adapter | every commit | ~1 min | 4 GB RAM, 2 CPU |
| L3 in-process | every commit | ~5 min | 4 GB RAM, 4 CPU |
| L4 cross-process | nightly | ~15 min | 8 GB RAM, 4 CPU |
| L5 container | pre-release + manual | ~30 min | 16 GB RAM, 8 CPU + Docker |

**Skipping L4/L5 in fast CI:** the L3 tests cover 90% of the behavior. L4 is for catching TCP-specific bugs. L5 is for catching container-specific bugs. Both run on nightly + pre-release.

---

## 8. Open Questions

1. **Should L2 live in `stoolap/tests/` or in `cipherocto/sync-e2e-tests/`?**
   - L2 is "real Stoolap + real adapter + mock sync engine" — it's an adapter test, so it should live in `stoolap/tests/`. (Decision: `stoolap/tests/l2_adapter_integration.rs`)

2. **Should L3+ use `tokio::test` or a custom test harness?**
   - The sync engine uses `tokio` internally (for `spawn_blocking`). Using `tokio::test` is the natural choice. (Decision: `#[tokio::test]` with `flavor = "multi_thread"`)

3. **Should the L4 `stoolap-node` binary be a separate crate?**
   - Yes. It's a thin wrapper around `Database::open_with_sync` that listens on a TCP port. Separate crate keeps the test harness clean. (Decision: `cipherocto/sync-e2e-tests/stoolap-node/`)

4. **Should L5 use `bollard` (Docker daemon) or `testcontainers`?**
   - `testcontainers` is a higher-level wrapper around `bollard` (and other backends). It handles container cleanup automatically. (Decision: `testcontainers`)

5. **What's the timeout for `assert_converged(cluster, timeout)`?**
   - For L3: 5 seconds (in-process is fast). For L4: 30 seconds (TCP has more latency). For L5: 60 seconds (containers have startup overhead).

6. **How do we handle flaky tests?**
   - All convergence checks have explicit timeouts. If a test doesn't converge in time, it fails with a clear error message (not a hang). Tests are designed to be deterministic (no random delays, no real time dependencies).

---

## 9. Deliverables

1. **`stoolap/tests/l2_adapter_integration.rs`** — 11 L2 tests (T1–T11) ✓ IMPLEMENTED
2. **`cipherocto/sync-e2e-tests/Cargo.toml`** — new crate (L3+; deferred)
3. **`cipherocto/sync-e2e-tests/src/lib.rs`** — TestNode, TestCluster, TestTransport, assert_converged (deferred)
4. **`cipherocto/sync-e2e-tests/tests/l3_in_process.rs`** — 12 L3 tests (T1–T12) (deferred)
5. **`cipherocto/sync-e2e-tests/tests/l4_cross_process.rs`** — 5 L4 tests (T1–T5) (deferred)
6. **`cipherocto/sync-e2e-tests/tests/l5_container.rs`** — 5 L5 tests (T1–T5) (deferred)
7. **`cipherocto/sync-e2e-tests/stoolap-node/Cargo.toml`** — node binary (deferred)
8. **`cipherocto/sync-e2e-tests/stoolap-node/src/main.rs`** — node binary (deferred)
9. **CI workflow** — `.github/workflows/sync-e2e.yml` (deferred)
10. **README** — `cipherocto/sync-e2e-tests/README.md` (deferred)

---

## 10. L2 Status (Implementation Progress)

**9 + 2 = 11 L2 tests implemented in `stoolap/tests/l2_adapter_integration.rs`** (committed to `feat/blockchain-sql`):

- L2-T1: wal_roundtrip_via_adapter ✓
- L2-T2: snapshot_segment_roundtrip ✓
- L2-T3: table_id_is_deterministic_and_case_insensitive ✓
- L2-T4: regenerate_snapshot_creates_new_file ✓
- L2-T5: schema_epoch_increments_on_table_creation ✓
- L2-T6: persistence_reopen_preserves_rows ✓
- L2-T7: error_classification_decryption_failed ✓
- L2-T8: error_classification_backend_not_ready_on_closed_engine ✓
- L2-T9: open_with_sync_returns_valid_adapter ✓
- **L2-T10 (bonus): 2-instance write-then-read** ✓ (writer + reader are separate engines)
- **L2-T11 (bonus): 3-instance writer + 2 readers** ✓ (fan-out topology)

**Key lessons learned during L2 implementation:**

1. **`_` wildcard in destructuring drops TempDir immediately.** Pattern `let (engine, _, db_path) = make_persistent_engine(...)` drops the `TempDir` after the `let` statement, deleting the persistence dir. All tests must bind the `TempDir` to a named variable (e.g., `_tmp` or `tmp`) to keep it alive for the test duration.

2. **Persistence dir is `path/wal`, not `path`.** The `Config::with_path("foo.db")` treats the path as a directory; the WAL is at `foo.db/wal/`. Tests that check the persistence dir should look at `path/wal/`, not `path/`.

3. **DDL operations (CREATE TABLE) are auto-committed via `record_ddl` → `write_commit_marker`.** They appear in the WAL as separate entries with `DDL_TXN_ID`. On reopen, `replay_wal` re-applies them, so the schemas are loaded.

4. **Tests must call `close_engine()` before reopen** to flush the WAL buffer. Without explicit close, the WAL buffer may not be flushed to disk, and the reopened engine won't see the data.

5. **The StoolapAdapter trait method `apply_wal_entry_bytes` returns `Result<(), ApplyWalEntryError>`.** The adapter classifies `Decode` → `DecryptionFailed` and `Apply` → `BackendNotReady` per RFC-0862 §Error Handling.

## 11. L3+ Status (Design Only)

**L3 (in-process E2E with real sync engine):** NOT YET IMPLEMENTED. Requires a new `sync-e2e-tests` crate in the cipherocto workspace that depends on:
- `octo-sync` (path or git dep)
- `stoolap` (git dep with `sync` feature)
- `tokio`

The harness would provide `TestNode` (wraps `MVCCEngine` + `StoolapAdapter` + `WalTailStreamer` + `SegmentIndexer` + `MissionKeyRing`), `TestCluster` (N nodes + in-process mpsc transport), and `assert_converged`. 12 L3 tests are defined in the plan above.

**L4 (cross-process E2E with TCP):** NOT YET IMPLEMENTED. Requires a `stoolap-node` binary crate that wraps `Database::open_with_sync` and listens on a TCP port. 5 L4 tests are defined in the plan.

**L5 (container E2E with Docker):** NOT YET IMPLEMENTED. Requires `bollard` or `testcontainers`. 5 L5 tests are defined in the plan.

The L3-L5 implementation is deferred because the cipherocto sync engine does not yet have a unified "session manager" that ties `WalTailStreamer`, `SegmentIndexer`, and `MissionKeyRing` together. The L3 harness would need to either:
- Build this session manager (significant work, ~1-2k lines)
- Use the modules independently (simpler but less realistic)

The L2 tests (11 tests, all passing) provide strong confidence that the StoolapAdapter is correct. The L1 unit tests in `octo-sync` (60+ tests) cover the sync engine modules independently. The combination of L1 + L2 catches the most likely bug classes (adapter misbehavior, sync engine misbehavior). L3+ would catch integration bugs that only appear when the full system is wired together.

**Recommended next steps:**
1. Build a minimal `SyncSessionManager` in the cipherocto workspace that ties the modules together (this is a prerequisite for L3).
2. Build the `sync-e2e-tests` harness with `TestNode` + `TestCluster`.
3. Implement L3-T1 (2-node WAL tail) and L3-T2 (2-node summary descent) first — these are the highest-value tests.
4. Defer L4/L5 until L3 is stable and the session manager is production-ready.
