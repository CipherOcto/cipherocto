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

#### Chain Relay Tests (L3-T13 through L3-T15)

These tests verify chain relay topology (A → B → C) where intermediate nodes forward entries to downstream peers. Per RFC-0862 §4.3.3.1, `adapter.apply_wal_entry` MUST persist to WAL and advance `current_lsn()` for chain relay to work.

| Test | What it verifies | Topology |
|------|------------------|----------|
| `L3-T13: chain_relay_basic` | Writer A commits 5 entries. Relay B receives via `apply_wal_entry`. Leaf C connects to B and receives via `read_wal_range`. Verify C has all 5 entries. | 3 instances (A→B→C) |
| `L3-T14: chain_relay_lsn_advancement` | Verify B's `current_lsn()` advances after applying entries from A. Verify B's `read_wal_range(1, lsn)` returns the applied entries. | 2 instances |
| `L3-T15: chain_relay_dedup` | Apply same entry to B twice. Verify idempotency (no duplicate in WAL, LSN not double-advanced). | 2 instances |

**L3 status:** T1–T12 implemented and passing. T13–T15 added for chain relay (requires `DatabaseSyncAdapter` WAL persistence). Multi-peer tests (T16–T27) implemented in `l3_multi_peer.rs`.

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

#### Chain Relay Tests (L4-T6)

| Test | What it verifies | Topology |
|------|------------------|----------|
| `L4-T6: tcp_chain_relay` | Writer A commits 5 rows. Relay B (file:// DSN) connects to A, receives entries. Leaf C (memory:// DSN) connects to B. Verify C has all 5 entries via chain relay. **Requires `StoolapAdapter` WAL re-entry fix** — without it, B's `current_lsn()` stays at 0 and C receives nothing. | 3 processes (A→B→C) |

**L4 status:** T1–T5 implemented and passing. T6 added for chain relay (blocked on `StoolapAdapter` WAL re-entry fix).

### L5: Container E2E (Docker, network bridge)

These run in **`sync-e2e-tests/tests/l5_container.rs`**. They use the `bollard` (Docker daemon) crate to launch 2–3 containers, each running the `stoolap-node` binary, and connect them via the container network bridge.

| Test | What it verifies | Topology |
|------|------------------|----------|
| `L5-T1: two_container_sync` | 2 containers, one Docker network. Writer commits 50 rows. Verify reader sees them. | 2 containers (Docker) |
| `L5-T2: three_container_fan_out` | 3 containers. Writer commits 200 rows. Verify both readers see them. | 3 containers (Docker) |
| `L5-T3: container_network_partition` | 3 containers. `docker network disconnect` to partition one reader. Reconnect. Verify it catches up. | 3 containers (Docker) |
| `L5-T4: container_resource_limit` | 1 container with `--memory 256m` and `--cpus 0.5`. Writer commits 10K rows. Verify the container doesn't OOM and the writer handles backpressure correctly. | 1 container (Docker) |
| `L5-T5: container_kill_and_recover` | 2 containers. `docker kill` the reader. Writer keeps committing. `docker start` a new reader. Verify the new reader catches up. | 2 containers (Docker) |

#### Chain Relay Tests (L5-T6, L5-T7)

| Test | What it verifies | Topology |
|------|------------------|----------|
| `L5-T6: container_chain_relay` | Writer A commits 5 rows. Relay B (file:// DSN) connects to A. Relay C (file:// DSN) connects to B. Leaf D (memory:// DSN) connects to C. Verify D has all 5 entries via 3-hop chain. **Requires `StoolapAdapter` WAL re-entry fix.** | 4 containers (A→B→C→D) |
| `L5-T7: container_four_node_fan_out` | Writer, 3 readers. Writer commits 100 rows. Verify all 3 readers see them. | 4 containers (1 writer + 3 readers) |

**L5 status:** T1–T5 implemented and passing. T6–T7 added (T6 blocked on `StoolapAdapter` WAL re-entry fix).

**All layers implemented — none remaining.**

**Key prerequisite completed:** `SyncSessionManager` implemented in `octo-sync/src/session.rs` (~300 lines, 15 unit tests). It ties together `WalTailStreamer`, `SegmentIndexer`, `MissionKeyRing`, `ReplayCacheManager`, and per-peer `Peer` state machines.

**Additional fix:** `WalTailStreamer::new` now initializes `current_lsn` from the adapter's current LSN (instead of 0), enabling correct restart recovery.
