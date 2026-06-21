# Research: Two-Node Data Synchronization for the Stoolap Fork via the CipherOcto Network

**Layer:** Research (Feasibility — "CAN WE?")
**Status:** Draft v2.0 (post Round 10 adversarial review — 1 pre-existing LOW from R10 resolved; see `docs/reviews/stoolap-data-sync-research-adversarial-review-r10.md`; awaiting Round 11 verification)
**Date:** 2026-06-20
**Author:** CipherOcto research
**Supersedes:** Nothing
**See also:** [BLUEPRINT.md](../BLUEPRINT.md), [Use Case: DOT Network Bootstrap](../use-cases/dot-network-bootstrap.md), [Research: Stoolap Integration](stoolap-integration-research.md), [Research: Deterministic Overlay Transport](deterministic-overlay-transport.md)

---

## Executive Summary

The **Stoolap fork** under `/home/mmacedoeu/_w/databases/stoolap` is an embedded, in-process SQL database written in pure Rust. It exposes a complete local engine — MVCC transactions, AS OF time-travel, BTree/Hash/Bitmap/HNSW indexes, a binary WAL with LSN, snapshot persistence, semantic caching, an event publisher trait, and a wealth of deterministic-arithmetic / blockchain-mode types — but **it has zero networking code**: no TCP, no UDP, no libp2p, no async runtime, no `tokio`/`reqwest`/`hyper` in `Cargo.toml` (`stoolap/Cargo.toml:36-131`). The fork's own `ROADMAP.md` lists Phase 3 "Network Protocol & Gossip" (stoolap `RFC-0303`) as **DRAFT** and unimplemented.

The **CipherOcto network** in this repository is a complete overlay transport protocol stack — Deterministic Overlay Transport (`RFC-0850`), Gateway Discovery (`RFC-0851`) and Bootstrap (`RFC-0851p-a`), the Deterministic Gossip Protocol (`RFC-0852`), Overlay Cryptography (`RFC-0853`), Mission Overlay Networks (`RFC-0855`), Coordinator lifecycle (`RFC-0855p-b`/`0855p-c` Accepted, `0855p-e` Draft), the Overlay Mempool (`RFC-0857`), Onion Routing (`RFC-0858`), Proof-Carrying Envelopes (`RFC-0859`) and Proof-of-Relay (`RFC-0860`) — but **no RFC specifies the wire-level protocol for synchronizing application-level database state between two nodes**. The closest building blocks are the DGP anti-entropy Merkle-descent sketch in `RFC-0852 §7` (scoped to *overlay* state, not application storage) and a `catch_up` pseudocode fragment in `rfcs/draft/storage/0200-production-vector-sql-storage-v2.md:1821-1997` ("Raft Log Replication Spec" body section — note: NOT in Appendix A; §A is the brief recommendation table at line 2640) with no wire format, no RFC number, and no mission.

This research investigates whether — and how — these two pieces can be combined to deliver a *first-class* two-node data synchronization feature for the Stoolap fork, in which:

1. Node A and Node B run independent copies of `stoolap::Database` against independent files (or `memory://`).
2. A dedicated **Sync transport** layer in CipherOcto — built on top of DOT envelopes, platform adapters, replay cache, and OCrypt — defines a deterministic, replay-safe, idempotent protocol for bringing the two databases into agreement.
3. The protocol is **gossip-compatible** (any node in a `GossipDomainId` may serve or receive sync), and **determinism-bounded** per `RFC-0008`: Class A for the wire protocol and the resulting state, Class B for transport selection and retry/backoff (which can affect convergence), Class C for diagnostics. See §4.4 for the full mapping.
4. The first version targets **two nodes** (the requested feature) and is **extensible to N nodes via DGP** without protocol change.

**Recommendation: YES, this is feasible and high-value.** The natural layering is:

- **Sync sub-protocol** riding on DOT envelopes, with envelope subtypes allocated from the `DOT/1/{...}` namespace (already used by RFC-0850p-c/d/e/f).
- **WAL-tail streaming** as the v1 transport (stoolap already has a self-describing V2 binary WAL with LSN + CRC32 — `src/storage/mvcc/wal_manager.rs`).
- **Anti-entropy Merkle summary** as the v1 catch-up handshake (the pattern RFC-0852 §7 already defines for *gossip* objects, applied to *table segments*).
- **Determinism** preserved by reusing `octo-determin::Dfp`/`Decimal`/`Dqa` and the `determ::DetermValue`/`DetermRow` already in the fork for any value crossing the wire.

The cost is one new RFC in the Storage/Networking range (proposed `RFC-0210` or `RFC-0862` — see §10), one base mission, and 9 sub-missions (0862a through 0862i — see §10.2), no changes to existing accepted RFCs.

---

## Problem Statement

The Stoolap fork needs a way for two nodes running separate `Database` instances to **synchronize their data** (replicate writes, converge after partitions, ship initial state, recover from a peer's WAL). The CipherOcto network already provides the *transport* that could carry such data — multi-carrier DOT envelopes, deterministic serialization, replay protection, mission-scoped key hierarchies — but it has no protocol specification for *what* the two nodes send, in *what order*, with *what conflict-resolution* semantics.

Without a Sync protocol, the operator of the fork can only synchronize data by copying files out-of-band. With one, the fork becomes a node in a CipherOcto network and gains:

- **High availability** — read replicas, failover, disaster recovery across geographies.
- **Horizontal scale** — distribute read traffic across mirrors; aggregate writes to a coordinator.
- **Disconnected operation** — nodes write locally, sync when reconnected (DGP's anti-entropy model).
- **Cryptographic provenance** — every replicated byte is OCrypt-signed and replay-protected.
- **Cross-carrier delivery** — the same sync stream can ride Telegram, Matrix, QUIC, or a NativeP2P adapter.
- **Deterministic convergence** — under the DGP anti-entropy rule, any two nodes with the same operation set will reach the same state regardless of arrival order, because the operation order is canonical (LSN, then table id, then row id, then op) and the values are DCS-encoded.

### Stakeholders

- **Primary:** Stoolap fork operators (data engineers, AI/agent backends, decentralized-app developers).
- **Secondary:** CipherOcto network operators (gateway runners), DOT adapter maintainers.
- **Affected:** Stoolap fork contributors (must integrate the Sync API without breaking the existing single-process API).

### Constraints

- **Must not** break the existing single-process `Database::open(dsn)` API or change WAL file format compatibility.
- **Must not** require `tokio` as a hard dependency (stoolap is currently a synchronous crate; the sync transport should either be an opt-in feature or live in a separate crate).
- **Must** preserve Stoolap's determinism invariants (RFC-0104): DFP arithmetic, software-emulated ordering, no FMA, fixed encoding.
- **Must** ride the existing DOT envelope wire format (`DOT/1/{base64}` / `DOT/2/{msg_id}` / `DOT/F/{base64_frag}` / `RAW/{binary}`) without modifying `RFC-0850`.
- **Must** be replay-safe across all RFC-0008 Class A boundaries (the wire protocol itself, the resulting state).
- **Must** be wire-compatible with OCrypt encryption when the mission is configured PRIVATE.

### Non-Goals (out of scope for v1)

- Multi-leader / active-active conflict resolution. v1 is single-leader (one writer node, N read replicas) with deterministic LSN ordering. CRDTs are explicitly **rejected** by `RFC-0852` for consensus-relevant state; we will not introduce them here either.
- Native browser/browser-node sync (WebRTC data channel). v1 uses DOT platform adapters (NativeP2P / QUIC / Webhook). WebRTC can be a Phase 3 platform adapter (`RFC-0850 §8.2` already allocates the type).
- Trust-anchor design for storage checkpoints (the analog of RFC-0851p-a §6 "genesis checkpoint from CipherOcto website" for peer lists, but for storage). This is mentioned in §11 of this research as F2 future work.
- Sharding across multiple Stoolap instances (different schemas per shard). v1 is whole-DB replication; sharded replication can be a follow-up.

---

## Research Scope

### Included

- Feasibility analysis of the gap: what is specified, what is implemented, what is missing.
- Five candidate sync approaches (event-driven, WAL-tail streaming, operation-log, anti-entropy Merkle, native P2P) and their trade-offs.
- A recommended architecture (sub-protocol, wire format, identity, key hierarchy, replay cache, catch-up handshake, gossip extension).
- Concrete file-level extension points in both repositories.
- Proposed RFC number, base mission, and sub-mission decomposition.
- Test strategy (replay-safety, determinism, partition healing, two-node round-trip).

### Excluded

- Full RFC text (lives in `rfcs/draft/storage/XXXX-stoolap-data-sync.md` or `rfcs/draft/networking/XXXX-stoolap-data-sync.md` — to be created *after* Use Case acceptance per the BLUEPRINT workflow).
- Implementation code (lives in missions).
- Benchmarks (created in the mission phase; rough targets proposed in §9.3).
- The two-node feature is the **first**; N-node gossip is described as a Phase 3 extension without full specification.
- Detailed cryptography review (OCrypt is the spec; the Sync protocol is a consumer).

---

## 1. Background — The Two Substrates

### 1.1 Stoolap fork (the storage)

**Identity.** `stoolap` v0.3.2, Apache-2.0, pure Rust, embedded SQL database. Crate types: `rlib` + `cdylib` (`stoolap_native`). Single binary `stoolap` behind the `cli` feature. Optional `vector`, `semantic`, `zk`, `commitment`, `parallel`, `sqlite`, `duckdb`, `mimalloc`, `wasm` features. Depends on `octo-determin` from this CipherOcto repo (branch `next`) for DFP/Decimal/Dqa/BigInt (`stoolap/Cargo.toml:55`). Source: `/home/mmacedoeu/_w/databases/stoolap`.

**Storage model.** MVCC with full row version chains (`RowVersion { txn_id, deleted_at_txn_id, data, create_time }`). Default isolation `ReadCommitted`; the only other isolation is `SnapshotIsolation` (per `core/types.rs:369-378`; the comment notes it's "equivalent to Repeatable Read"; `Serializable` is NOT supported as a separate level). AS OF time-travel via `select_as_of`. Optimistic write-conflict detection at commit. **`get_fast_timestamp()` uses `SystemTime::now()` (wall clock in nanoseconds) with monotonicity enforcement via `max(now, last_ts + 1)`** per `stoolap/src/storage/mvcc/timestamp.rs:54-71`; the monotonicity guard prevents regressions if the system clock goes backwards but the timestamps themselves are wall-clock-derived, not pure counter.

**WAL.** V2 binary format with a 32-byte fixed header (matches `WAL_HEADER_SIZE: u16 = 32` at `src/storage/mvcc/wal_manager.rs:72`, version `WAL_FORMAT_VERSION: u8 = 2` at line 69). Header fields: `Magic (4 = "WALE") | Version (1) | Flags (1) | HeaderSize (2 = 32) | LSN (8) | PreviousLSN (8) | EntrySize (4) | Reserved (4)`. Payload + CRC32 trailer. Operations: `Insert/Update/Delete/Commit/Rollback/CreateTable/DropTable/AlterTable/CreateIndex/DropIndex/CreateView/DropView/TruncateTable/VectorInsert/VectorUpdate/VectorDelete/SegmentCreate/SegmentMerge/IndexBuild/CompactionStart/CompactionFinish/SnapshotCommit` (22 variants at `wal_manager.rs:163-187`). LSN is a monotonic u64 assigned via `entry.lsn = self.current_lsn.fetch_add(1, Ordering::SeqCst) + 1` (line 1304). Files named `wal-<timestamp>-lsn-<N>.log` under `<path>/wal/`. Public API: `append_entry`, `write_commit_marker`, `write_abort_marker`, `current_lsn`, `previous_lsn`, `replay_two_phase(from_lsn, callback)`. Located in `stoolap/src/storage/mvcc/wal_manager.rs` (3,773 lines).

**Snapshots.** Two distinct artifacts:

- **Per-table snapshot files** at `<dsn-path>/snapshots/<table>/snapshot-<ts>.bin` (e.g. `snapshots/users/snapshot-1718901234.bin`). Header magic `"STSVSHD"` (8 bytes, ASCII "SToolaP VerSion Store HarD disk") at `src/storage/mvcc/snapshot.rs:38, 98`. Atomic-rename write. CRC32-verified. Per-table latest-version-per-row.
- **Snapshot metadata files** (separate, top-level). Magic `SNAP` (4 bytes, `0x50414E53` in little-endian) at `src/storage/mvcc/engine.rs:153`. Tracks per-snapshot LSN, timestamp, and CRC of the table snapshot list.

Safe-truncation logic: free function `find_safe_truncation_lsn(snapshot_dir, keep_count, active_tables)` at `engine.rs:291` requires ≥2 surviving CRC-verified snapshot metadata files per active table before WAL can be truncated. The atomic-rename semantics that guarantee a half-written segment is never observable are at `engine.rs:2828` (`std::fs::rename(temp_path, final_path)` with rollback on partial failure).

**Cross-process pub-sub (the closest existing cross-process primitive).** `src/pubsub/wal_pubsub.rs` (`WalPubSub`) writes a separate `pubsub-wal-*.log` file with entries `LSN | timestamp | event_type | event_id(32B) | channel_len | channel | payload_len | payload`. `IdempotencyTracker` (HashSet with deterministic **half-clear eviction** (not LRU — `wal_pubsub.rs:84` drops half of iteration order, which is hash-based not recency-based), default 10 000 entries) deduplicates. The doc comment is explicit: "WAL-based pub/sub for **cross-process** cache invalidation" (`stoolap/src/pubsub/wal_pubsub.rs:15`). This is the model for cross-process propagation in the fork today — but it carries *events*, not *data*, and uses a *file* on shared storage rather than a *network*.

**Higher-level cross-node types (blockchain mode, data-only).** `consensus::Operation` (variants `Insert/Update/Delete/CreateTable/DropTable/CreateIndex/DropIndex` — note: **no views, no truncate, no alter, column-level Update**) with `encode() -> Vec<u8>` / `decode(bytes) -> OpResult<Self>` / `hash() -> [u8;32]`. `consensus::Block` with `BlockHeader { block_number, parent_hash, state_root_before, state_root_after, operation_root, timestamp, gas_limit, gas_used, proposer, extra_data }`. `determ::DetermValue` and `determ::DetermRow` — deterministic, no-`Arc` value types for cross-node wire format. `rollup::RollupBatch`/`RollupState`/`RollupOperation`/`Withdrawal`/`FraudProof`. None of these are wired into the live `MVCCEngine`; they are data-only.

**Extension points for a sync layer.** From least to most invasive:

1. `pubsub::EventPublisher` trait — `publish(event: DatabaseEvent)`, `subscribe()`. Implemented by `EventBus` (intra-process), `WalPubSub` (cross-process file), `NoopPublisher`. Currently the `TransactionCommited` variant is *defined* but the executor does **not** publish it (only `TableModified` is emitted at `stoolap/src/executor/mod.rs:244` area).
2. `storage::mvcc::transaction::TransactionEngineOperations::record_commit(txn_id)` — the single commit hook. Today delegates to `PersistenceManager::record_commit` which writes a commit marker to the WAL. A sync layer can wrap this hook to capture the LSN range and ship WAL tail to peers.
3. `WALManager::append_entry` (write chokepoint), `WALManager::current_lsn()` (tail-follow), `WALManager::replay_two_phase(from_lsn, callback)` (built-in replay loop on the receive side). `WALEntry` is `Clone + Debug` and serializes to V2 binary format.
4. `MVCCEngine::create_snapshot()` (at `engine.rs:2642`) + the per-table snapshot files. The atomic-rename write is at `engine.rs:2828`.
5. `storage::traits::Engine` (whole-engine replacement), `Transaction`, `Table`, `Index`, `Scanner` (finer-grained).
6. `consensus::Operation` (extend to cover missing variants) and `consensus::Block` (batch container).

**Networking today.** Zero. `grep` for `TcpStream|TcpListener|UdpSocket|HttpClient|WebSocket|libp2p|tonic|hyper|axum|reqwest` in the fork returns no matches. `Cargo.toml` lists no network crates. The only "syscall" code is `libc::flock` / `windows-sys::Win32_Storage_FileSystem` for cross-process file locking in `src/storage/mvcc/file_lock.rs`. The CLI is single-process (`src/bin/stoolap.rs`); a postgres-server binary is commented out in `Cargo.toml:31-34` awaiting implementation.

### 1.2 CipherOcto network (the transport)

**Identity.** This repository (`/home/mmacedoeu/_w/ai/cipherocto`). Governed by the `BLUEPRINT.md` workflow (Research → Use Case → RFC → Mission → Agent). RFCs numbered 0000–0999 by category. Networking range is 0800–0899.

**Transport — `RFC-0850` (Deterministic Overlay Transport, Accepted).** `DeterministicEnvelope` with logical timestamps (NOT wall-clock). `envelope_id = BLAKE3-256(network_id || message_type || source_peer || origin_gateway || logical_timestamp || payload_hash)` (RFC-0850:363-370). Fixed field order; canonicalized via `RFC-0126` DCS. 21 platform types (Telegram 0x0001 … QUIC 0x0015) per `RFC-0850 §3.1` (Broadcast Domain table, line 195-215). Wire formats: `DOT/1/{base64}` (text), `DOT/2/{msg_id}` (native upload), `DOT/F/{base64_frag}` (fragment), `RAW/{binary}` (QUIC/WebRTC). Multi-carrier propagation per envelope. Fragmentation for IRC (512B) / LoRa (256B) / BLE (244B) per the per-adapter table. Replay cache `BTreeMap<[u8;32], u64>` (envelope_id → first_seen) with deterministic eviction (smallest first_seen; tie-broken by lexicographic envelope_id). QUIC native profile in §8.7. C ABI + WASM plugin model. Implementation status: ~53% of RFC-0850's core types in `crates/octo-network/src/dot/`; the other 10 networking RFCs are implemented at varying coverage (rough measurements from the `crates/octo-network/src/` tree: `dgp/` ~12 files, `drs/` ~8 files, `gdp/` ~11 files, `ocrypt/` ~10 files, `orr/` ~7 files, `porelay/` ~12 files, `dom/` ~9 files, `gossip/` ~2 files — only `gossip/` is genuinely minimal; the others are 30–70% implemented per RFC). (Note: 22 `octo-adapter-*` crates exist on disk, but `matrix` and `matrix-sdk` are two separate crates for the same Matrix platform type, so 21 platform types map to 22 crates.)

| **Discovery — `RFC-0851` (Gateway Discovery Protocol, Accepted) + `RFC-0851p-a` (Network Bootstrap Protocol, Accepted).** `DiscoveryScope` enum (Local 0x0001 / Regional 0x0002 / Mission 0x0003 / Global 0x0004 / Private 0x0005 / Consensus 0x0006) per `RFC-0851:107-114`. `GatewayAdvertisement` with Merkle-committed `capabilities_root/transport_root/route_root/trust_root` (`RFC-0851:172-186`). **GDP heartbeat**: 30s interval, 90s failure detection (RFC-0851 §12) — distinct from the Sync heartbeat (5s, see §6 Phase 1). **5-state `DiscoveryLifecycle`** (Bootstrap 0x0001 → Expansion 0x0002 → Stabilization 0x0003 → Degraded 0x0004 → Recovering 0x0005) per `RFC-0851:407-413` (note: the doc earlier said "6-state"; corrected — the actual enum has 5 states). Bootstrap in 3 modes: A (5 foundation nodes, 3-of-5 intersection, ≥80% peer-list overlap), B (Kademlia DHT), C (offline invite link `octo://invite?v=1&...`). **7-state `BootstrapClientLifecycle`** (`Init=0x01, Connecting=0x02, Validating=0x03, Cached=0x04, FallbackB=0x05, FallbackC=0x06, Done=0x07` per `RFC-0851p-a:469-484` — note: the RFC's prose at line 94 incorrectly says "5 states"; the enum has 7). 4-state `BootstrapNodeLifecycle` (`Registered, Active, Suspect, Revoked`). Slash code `0x000D` reserved for `bootstrap_node_misbehavior` per `RFC-0851p-a:420, 431, 726` — but **note contradiction**: `RFC-0850p-c:460` claims `0x000C-0x000D` are reserved for sub-DC delegation. The two RFCs disagree; this research uses the `0851p-a` interpretation but flags the contradiction for maintainer resolution. Use case motivation: `docs/use-cases/dot-network-bootstrap.md:1-132` (132 lines, the "genesis checkpoint from CipherOcto website" row is in §5 Mode C / §6 Sybil-Eclipse Defense, not in §6 as a section header).

**Gossip — `RFC-0852` (Deterministic Gossip Protocol, Draft).** 6 `GossipScope` values. 8 `GossipObjectType`s — `Envelope/RouteUpdate/ConsensusFragment/MissionState/VectorCommitment/ZkProof/DiscoveryAdvertisement/SnapshotFragment`. 4 modes: `Flood/Incremental/Anti-entropy/Directed`. **Anti-entropy Merkle reconciliation** with `GossipStateSummary { domain_id, state_root, object_count, watermark }`, Bloom-filter compression (BLAKE3-256), deterministic eviction via `BTreeMap`, fragmentation, retention classes (Ephemeral/Mission/Consensus/Archive). Explicitly rejects CRDTs ("Not deterministic at consensus boundary"). §11 lists Bloom filters, Merkle roots, bitmap summaries, and range commitments as compression techniques — but the bitmap-summary and range-commitment mechanisms are listed, not specified. The `SnapshotFragment` object type (0x0008) is reserved for "State synchronization — Chunk of state snapshot" but the fragment structure is generic, not snapshot-specific. Implementation: `crates/octo-network/src/gossip/` exists but is minimal.

**Crypto — `RFC-0853` (Overlay Cryptography, Draft).** BLAKE3-256, Ed25519, X25519, ChaCha20-Poly1305, HKDF-BLAKE3. `OverlayIdentity`. `EncryptedEnvelope` with AAD `(envelope_id || sender_ephemeral_public || mission_id || logical_timestamp || sequence)`. `MissionKeyHierarchy { mission_root_key, transport_keys_root, relay_keys_root, execution_keys_root }`. Replay cache per-mission (1h or 10K entries). Onion layers (primitive; consumed by `RFC-0858`). Deterministic randomness `HKDF-BLAKE3(seed, context, epoch)`. 24h revocation grace.

**Mission Overlay Networks — `RFC-0855` (Accepted) + `0855p-b`/`0855p-c` (Accepted) + `0855p-e` (Draft, early-stage).** 8-state mission lifecycle (Created 0x0001 → Discovering 0x0002 → Forming 0x0003 → Active 0x0004 → Degraded 0x0005 → Recovering 0x0006 → Terminated 0x0007 → Archived 0x0008) per `RFC-0855:287-307`. `MissionId { network_id: u32, mission_hash: [u8;32], version: u16 }` per `RFC-0855:179-186` (note: 3 fields, not 2). `MissionDescriptor`. `MissionNode` with `role_flags` bitmask. **6 topology models** (Mesh, Hierarchical, Star, Swarm, Ring, Hybrid) per `RFC-0855:485-495` (note: the doc earlier said "8"; corrected — 6). 5 governance models (Centralized, DAO, Federated, AI-Assisted, Autonomous) per `RFC-0855:859-872`. 8 membership roles (Coordinator, Executor, Relay, Validator, Observer, Archivist, Prover, Aggregator) per `RFC-0855:397-406` (defined in §4.2 Roles and Authorities, not §6). 8-state `CoordinatorLifecycle` (`Designated, Elected, Active, Suspect, Handover, Demoting, Resigned, Inactive`) per `RFC-0855p-b:153-170`. Dual-stake requirements per `RFC-0855:431-444`.

**Other accepted networking RFCs.** `0850ab-a` (Telegram auth onboarding), `0850p-a` (WhatsApp auth), `0850p-c` (Transport Group Binding Ceremony, with `BIND/BIND_ACK/REBIND/UNBIND` envelopes and 4-state `GroupState` at `RFC-0850p-c:133-141`), `0861` (CoordinatorAdmin trait refinements, 17 findings closed: H1, H2, H6, M1, M2, M3, M4, M5, M7, M8, M10, M11, M12, M13, M14, M15, M16 = 17 total, 1,373 tests passing per `RFC-0861:386`).

**Other draft networking RFCs relevant to sync.** `0856` (DRS — deterministic route selection; canonical scoring `score = trust*w_t + bandwidth*w_b + latency*w_l + censorship*w_c − cost*w_cost` — all u64 saturating, Class A) per `RFC-0856:365-383`. `0857` (DOM — overlay mempool; canonical ordering `(execution_class ASC, economic_weight DESC, logical_timestamp ASC, sequence ASC, intent_id ASC)`; "Mempool sync <5s for 10K intents" per `RFC-0857:291`). `0858` (ORR — onion relay routing; layered ChaCha20-Poly1305 inside-out, session keys `HKDF-BLAKE3(shared, "ocrypt:onion:v1", hop_index || route_id)` per `RFC-0858:315` and `RFC-0858:330`; the HKDF context "ocrypt:onion:v1" is also documented in `RFC-0853:299`). `0859` (PCE — proof-carrying envelopes; recursive aggregation via `parent_proof_commitment` per `RFC-0859:186-208`). `0860` (PoRelay — proof-of-relay; actual composite scoring at `RFC-0860:408` with weights table at line 417-421: `composite = (forwarding * WF + availability * WA + bandwidth * WB + uptime * WU + diversity * WD) * stake_multiplier / 1000` with default weights `WF=300, WA=250, WB=200, WU=150, WD=100` (total = 1000 basis points). **Note: the formula `trust_score * 10 + utility_score * 5 + recency_score * 2` is the GDP cache eviction formula in `RFC-0851 §M-GDP-2` (line 435) — it is NOT the PoRelay trust scoring formula. It is included here for disambiguation only.**).

**Existing storage replication sketch.** `rfcs/draft/storage/0200-production-vector-sql-storage-v2.md:1821-1997` contains the most concrete existing sketch: a `RaftEntry` enum (`VectorInsert/VectorDelete/VectorUpdate/CreateTable/CreateIndex/SnapshotInstall/AddReplica/RemoveReplica`), a `ReplicationState` struct (`leader_id, term, commit_index, last_applied, log`), `append_entries(follower, entries)`, `install_snapshot(snapshot)`, `catch_up(follower)` (snapshot if `log.len() - follower_index > snapshot_threshold`, else append entries), `compact_log(checkpoint_index)`, and a failure-handling table (leader crash → election timeout; follower crash → reconnect; partition → majority quorum step-down; duplicate → idempotent apply). This is **pseudocode with no wire format, no RFC number, no mission, and no relationship to DOT/DGP/OCrypt defined**. The brief recommendation table in `RFC-0200 §A` "Replication Model" (line 2640-2680) says: "Start with Raft for strong consistency. Gossip for large-scale deployments (future)." — the `catch_up` pseudocode is in the body section, not §A.

### 1.3 Prior cipherocto research on stoolap

| File | Status | What it covers |
| --- | --- | --- |
| `docs/research/stoolap-research.md` | Complete (March 2026) | Original stoolap capabilities catalogue (DFP, MVCC, persistence, pub-sub, rollup, gas metering). No sync content. |
| `docs/research/stoolap-integration-research.md` | Complete | Stoolap as verifiable state backend for the AI Quota Marketplace (RFC-0900/0901). Verifiable quote execution, compressed proof marketplace, confidential queries, decentralized listing registry, L2 rollup. Does **not** cover data sync between nodes. |
| `docs/research/stoolap-determinism-analysis.md` | Complete | Stoolap determinism properties (RFC-0104 compliance). Directly relevant: any sync must produce identical state across nodes. |
| `docs/research/stoolap-blob-dispatcher-compliance.md` | Complete | BLOB dispatcher. |
| `docs/research/stoolap-agent-memory-gap-analysis.md` | Complete | Agent memory over Stoolap. |
| `docs/research/stoolap-sum-aggregate-transaction-research.md` | Complete | SUM aggregate across MVCC transactions. |
| `docs/research/stoolap-luminair-comparison.md` | Complete | Stoolap vs Luminair. |
| `docs/research/stoolap-rfc0903-sql-feature-gap-analysis.md` | Complete | SQL feature gap. |
| `docs/research/turboquant-stoolap-enhancement.md` | Complete | Quantization enhancements. |
| `docs/research/deterministic-overlay-transport.md` | 6,273 lines | The "scratch pad" that is the design source for the entire networking RFC family. Convergent with the formal RFCs but contains additional ideas (StealthMission, DeniableRelay, RelayIncentiveEconomics, Anti-SpamSybilResistance) and the explicit notes: "Mission state synchronization SHOULD use [Merkle anti-entropy]" and "Large state synchronization SHOULD use Bloom filters, Merkle roots, bitmap summaries, range commitments." |
| `docs/research/networking-rfc-cross-reference-analysis.md` | 517 lines | Audit of the 11 networking RFCs. Lists dependencies, fan-in/fan-out, contradictions, gaps from the scratch pad, over-specification risk, and implementation status (9/17 files for RFC-0850, 0% for the rest). |
| `docs/research/9router-architecture.md`, `mimocode-architecture.md`, `jcode-architecture.md`, `ironclaw-architecture.md`, `openclaw-architecture.md`, `zeroclaw-architecture.md`, `memos-research.md` | Various | Adjacent architectures. |

**Existing stoolap use cases.**

- `docs/use-cases/stoolap-only-persistence.md` — Stoolap as a persistence commitment.
- `docs/use-cases/stoolap-mvcc-transaction-aggregate-support.md` — MVCC aggregate support.
- `docs/use-cases/verifiable-agent-memory-layer.md` — Memory layer.
- `docs/use-cases/data-marketplace.md` — Data trading.

**No use case exists for two-node (or N-node) data sync of Stoolap via CipherOcto.** This research proposes that one should be created next.

---

## 2. Findings — The Gap, Precisely

The two substrates are almost-but-not-quite complementary. Each has half of what is needed:

| Need | Stoolap has | CipherOcto has | Gap |
| --- | --- | --- | --- |
| **Local change log** | V2 binary WAL with LSN, CRC32, all DML/DDL/vector operations. `append_entry`, `current_lsn`, `replay_two_phase`. | — (DGP objects are not row data) | None on this side. |
| **Deterministic value wire format** | `determ::DetermValue`, `determ::DetermRow` (no-`Arc`, fixed inline/heap layout, SHA-256 MerkleHasher). `consensus::Operation` with big-endian fixed-width encoding. `octo_determin::Dfp/Decimal/Dqa/BigInt`. | RFC-0126 (DCS) + BLAKE3-256 for hashes. RFC-0853 (OCrypt) for encryption. | None on this side. |
| **Cross-process event propagation** | `WalPubSub` writes a separate pubsub-WAL file. | `DatabaseEvent` enum, `EventPublisher` trait. | Carrier is local file; needs network carrier. |
| **Commit hook** | `TransactionEngineOperations::record_commit(txn_id)` is the single chokepoint. | — | None. |
| **Replay-safe network transport** | — | `DeterministicEnvelope`, `ReplayCache` (BTreeMap with deterministic eviction), 21 platform adapters, multi-carrier propagation, fragmentation, 4 wire formats. | None on this side. |
| **Anti-entropy reconciliation** | — | `GossipStateSummary` + binary Merkle descent in `RFC-0852 §7`. Reserved `GossipObjectType::SnapshotFragment = 0x0008`. | Scoped to *overlay objects*, not to *Stoolap table segments*. Bitmap summaries and range commitments are listed but not specified. |
| **Per-mission encryption & key hierarchy** | — | `MissionKeyHierarchy` in `RFC-0853`. AAD = `(envelope_id \|\| sender_ephemeral_public \|\| mission_id \|\| logical_timestamp \|\| sequence)`. 24h revocation grace. | None. |
| **Bootstrap of a new node** | — | `RFC-0851p-a` (3 modes). | Handles peer-list only, not storage checkpoint. |
| **Catch-up pseudocode** | `RFC-0200` body section "Raft Log Replication Spec" (line 1821-1997): `catch_up(follower)` at line 1956. | — | No wire format, no RFC number, no mission. |
| **Identity types** | `consensus::BlockHeader::proposer: [u8;32]`, `rollup::Address([u8;20])`, `pubsub::DatabaseEvent::event_id: [u8;32]`. | `OverlayIdentity` in `RFC-0853`, `GatewayIdentity` in `RFC-0850`. | No `NodeId`, `PeerId`, `ClusterId` for the *storage* layer. |
| **Quorum / consensus** | Sketched in `RFC-0200` body section (Raft Log Replication Spec at line 1821-1997) and briefly in §A (Replication Model at line 2640) — "Recommendation: Start with Raft for strong consistency. Gossip for large-scale (future)." | `RFC-0852` (gossip anti-entropy), `RFC-0854` (proof substrate), `RFC-0740` (sharded consensus, `CrossShardMessage::StateSync` ~16-line struct at `RFC-0740:138-153` with `CrossShardMsgType` enum at line 155-162 having 3 variants: `StateSync`, `FraudProof`, `Transfer`). | No protocol is adopted end-to-end. |

**The single sentence summary:** the wire-format layer (DOT/OCrypt) and the storage-change-log layer (WAL) are both present and well-formed, but there is **no protocol that defines what bytes to put on the wire, in what order, with what identity, with what conflict resolution**. That protocol is what the next RFC must define.

---

## 3. Sync Approaches Analyzed

### 3.1 Approach A — Event-driven (DatabaseEvent over DOT)

**Idea.** Subclass `pubsub::EventPublisher` with a `RemoteEventPublisher` that, on every `DatabaseEvent::TransactionCommited { txn_id, affected_tables }`, serializes the event into a DOT envelope and ships it. Receiver obtains the event and replays by calling `db.execute(sql)`. New envelope subtype `DOT/1/SYNC_COMMIT { txn_id, table_set, origin_node }`.

**Pros.**

- Smallest change to fork. Only the executor (to actually publish `TransactionCommited` — currently it does not) and a new publisher implementation.
- Fits cleanly inside the existing `EventPublisher` extension point.
- Replay-safe via DOT's `ReplayCache` (per-peer) and OCrypt's per-mission replay cache.
- Each peer only needs the publisher trait; no new types in the public API.

**Cons.**

- **Payload is missing.** `TransactionCommited { txn_id, affected_tables }` does not contain the row data. The receiver would have to query its local store for what happened, but it has no way to know what changed because it never received the data. This is the show-stopper: a "commit happened" event is not a "send me the data" message.
- The executor would have to be extended to also emit per-table `TableModified` events with row payloads, which is essentially re-implementing WAL streaming at the event layer.
- Latency is poor — N round-trips per write, no batching.
- **Verdict: inadequate.** Rejected.

### 3.2 Approach B — WAL-tail streaming (WALEntry bytes over DOT)

**Idea.** Subclass `TransactionEngineOperations` (or wrap `PersistenceManager`) to capture the LSN range on each `record_commit`. Serialize WAL entries (`WALEntry::encode() -> Vec<u8>` — already in V2 binary format with CRC32) into a stream of DOT envelopes with subtype `DOT/1/SYNC_WAL_TAIL { from_lsn, to_lsn, table_filter?, entry_bytes }`. Receiver runs `WALManager::replay_two_phase(from_lsn, callback)` against its own `PersistenceManager::replay_two_phase` (at `persistence.rs:549`). Catch-up is "send me everything since LSN X". Identity is per-node `NodeId`. A new envelope subtype `DOT/1/SYNC_LSN_QUERY { from_lsn, max_bytes }` / `DOT/1/SYNC_LSN_RESP { from_lsn, to_lsn, entries }` for the request/response handshake.

**Pros.**

- The WAL is already the **source of truth** of the database. The format is self-describing (V2 magic + CRC32). No need to invent a new wire format for operations; just stream existing bytes.
- `WALManager::replay_two_phase` is the **built-in recovery path**. Receiver-side application is well-tested (`PersistenceManager::replay_two_phase` at `persistence.rs:549` is what `PersistenceManager::recover` callers actually use today — there is no `PersistenceManager::recover` method, just `replay_two_phase`).
- BLAKE3-256 hashing of entry bytes is straightforward; the OCrypt AAD binds `envelope_id || sender_ephemeral_public || mission_id || logical_timestamp || sequence` to the data.
- Replay-safe: each entry has a unique LSN; receiver dedupes by LSN.
- Idempotent at apply time (LSN-ordered, two-phase with commit markers).
- Compression-friendly: LZ4 already in `Cargo.toml` (`lz4_flex = "0.12"`).
- **The WALK-over-DOT approach is the most natural extension** of existing Stoolap primitives and the closest analog to PostgreSQL logical replication, MySQL binlog replication, and SQLite session extension. All three work this way for the same reason: the binary log is the source of truth.
- Cost to fork: one new module `src/sync/publisher.rs` + `src/sync/subscriber.rs` (or one new crate `stoolap-sync`). No changes to existing WAL format, no changes to public API other than an opt-in `Database::open_with_sync(...)`.

**Cons.**

- Single-leader only (only one writer at a time, by LSN ordering). Multi-leader is out of scope (see §3.6).
- WAL entries are versioned against the local schema, so backward compatibility requires careful format versioning (`RFC-0900` already requires this — the WAL already has a version byte).
- Need to handle `ConsensusFragment`-style snapshot shipping for first-time sync (full DB > 1 GB). DGP `SnapshotFragment` object type can be reused.
- **Verdict: best foundation for v1.**

### 3.3 Approach C — Operation-log sync (consensus::Operation over DOT/DGP)

**Idea.** Convert each WAL entry to `consensus::Operation` (note: missing variants: views, truncate, alter; column-level Update). Batch into `consensus::Block`. Send `Block` as a DGP `GossipObject` with `object_type = MissionState` (0x0004) or a new subtype. Receiver applies the block by replaying the operations.

**Pros.**

- Uses the existing high-level `Operation` enum and `Block` container.
- Gossip-friendly: any node can hold a `Block` and gossip it on demand.
- Format-version independent: `Operation::encode` is big-endian fixed-width.

**Cons.**

- **Coverage gap.** `consensus::Operation` is missing `CreateView/DropView/TruncateTable/AlterTable/VectorInsert/VectorUpdate/VectorDelete/SegmentCreate/SegmentMerge/IndexBuild/CompactionStart/CompactionFinish/SnapshotCommit`. Adapting all of these to the `Operation` enum is a non-trivial extension; many don't translate naturally (e.g., `AlterTable` is a column-level schema migration that needs DDL-aware replay, not row-level).
- The `Operation` layer is currently **not wired into the live `MVCCEngine`** — `executor/mod.rs` never constructs `consensus::Operation` from a real write. Building that wiring is a significant piece of work separate from the sync protocol itself.
- Block production implies block-time semantics (gas limits, batch intervals from `rollup::` — `BATCH_INTERVAL=10`, `MAX_BATCH_SIZE=10000`). These are the L2 rollup's concerns, not the Sync protocol's. Forcing this layer on every transaction would add unnecessary overhead.
- The `Operation::hash()` function is currently a placeholder XOR (file comment: "should be replaced with SHA-256"). It needs to be fixed before the wire format can claim Class A determinism.
- **Verdict: valuable for Phase 2+ when the L2 rollup story is real, but not the right v1 base.**

### 3.4 Approach D — Anti-entropy Merkle summary (catch-up handshake)

**Idea.** Reuse `RFC-0852 §7`'s `GossipStateSummary { domain_id, state_root, object_count, watermark }` for **per-table segment Merkle summaries**. New envelope subtype `DOT/1/SYNC_SUMMARY_REQ { table_filter, after_lsn }` / `DOT/1/SYNC_SUMMARY_RESP { table_id, segment_root, segment_count, watermark, hmac }`. Receiver diffs against its own summary, descends the Merkle tree to find missing segments, then requests them via `DOT/1/SYNC_SEGMENT_REQ { table_id, segment_index, expected_root }` / `DOT/1/SYNC_SEGMENT_RESP { ... }`. Segments are the same `snapshot-<ts>.bin` files already produced by `MVCCEngine::create_snapshot()` (`engine.rs:2642`).

**Pros.**

- Directly leverages the **DGP anti-entropy pattern** (RFC-0852 §7) — already the canonical mechanism in the overlay.
- Reuses **existing snapshot files** as the segment payload (no new format).
- Bitmap-summary compression (RFC-0852 §11) is a natural fit for "which segments does the peer have?" exchanges.
- Works for both first-time sync (full snapshot) and incremental sync (only missing segments).
- O(log N) descent to find missing segments.

**Cons.**

- Requires building a **per-table segment Merkle tree** in Stoolap that does not exist today. The `HexaryProof` (~120 bytes minimum for empty `levels` and `path` vectors, per `stoolap/src/trie/proof.rs:71-87`; larger when `levels` and `path` are populated) in `trie/proof.rs` is for rows in a hexary trie, not for snapshot segments.
- Cross-references between tables (foreign keys, indexes) are not segment-local. Snapshot shipping is whole-DB; per-table sync is for incremental catch-up only.
- The `SnapshotFragment` DGP object type (0x0008) is reserved but the fragment format is not specified — would need to be specified in the new RFC.
- **Verdict: essential for catch-up; should be combined with Approach B (WAL streaming) for incremental sync.**

### 3.5 Approach E — Native P2P (libp2p / Kademlia / gossipsub)

**Idea.** Use libp2p directly inside the Stoolap fork — Kademlia DHT for peer discovery, gossipsub for sync stream, request/response for direct fetches. Bypass DOT entirely.

**Pros.**

- Well-trodden path (Filecoin, IPFS, Ethereum devp2p all use libp2p).
- gossipsub is battle-tested.

**Cons.**

- **Bypasses the entire CipherOcto network stack** — the user's stated goal is for the Stoolap fork to use the CipherOcto network as the overlay, not replace it.
- Forces `tokio` as a dependency on the fork (it is currently a synchronous crate; the sync transport should be opt-in).
- Re-implements the multi-carrier abstraction that DOT already provides.
- **Verdict: explicitly rejected by the user's request.**

### 3.6 Approach Comparison Matrix

| Criterion | A (Event) | B (WAL streaming) | C (Operation) | D (Anti-entropy) | E (Native P2P) |
| --- | --- | --- | --- | --- | --- |
| Payload completeness | ❌ Missing row data | ✅ Full WAL | ⚠️ Missing op variants | ✅ Full segments | ✅ Full |
| Replay safety | ✅ via DOT cache | ✅ via LSN+CRC32 | ⚠️ needs hash fix | ✅ via Merkle | ✅ via libp2p |
| Catch-up cost (worst case) | N/A | `O(unapplied LSNs)` | `O(unapplied ops)` | `O(log N + missing segments)` | `O(log N)` |
| Reuses existing fork primitives | ✅ EventPublisher | ✅ WAL + record_commit | ⚠️ consensus::Operation partial | ✅ PersistenceManager | ❌ none |
| Reuses CipherOcto stack | ✅ DOT + OCrypt | ✅ DOT + OCrypt | ✅ DOT + DGP + OCrypt | ✅ DOT + DGP + OCrypt | ❌ none |
| Schema-migration aware | ⚠️ event-only | ✅ WAL format-versioned | ⚠️ op-level | ✅ snapshot-level | ⚠️ |
| Multi-leader capable | ❌ | ❌ | ❌ | ❌ | ✅ |
| Schema-evolution cost | low | low | medium (extend Operation) | medium (Merkle segment tree) | low |
| Implementation cost (est. LOC) | ~300 | ~1,500 | ~3,000 | ~2,500 | ~5,000+ |
| Fits user's "use cipherocto network" requirement | ✅ | ✅ | ✅ | ✅ | ❌ |

**Recommendation: a layered combination of B + D for v1.** WAL-tail streaming (B) for live replication; anti-entropy Merkle summaries (D) for first-time sync, partition healing, and catching up after long disconnects. Approach C (Operation) is held in reserve for a Phase 2 that wires the consensus/rollup layer into the live engine. Approach A is too payload-poor. Approach E is out of scope.

---

## 4. Recommendations

### 4.1 Recommended architecture (high level)

```
┌────────────────────────────────────────────────────────────────────┐
│                  CipherOcto Sync Sub-Protocol (NEW)                │
│  ┌────────────────────────────────────────────────────────────┐    │
│  │  SyncRequest / SyncResponse / SyncSegment / SyncSummary    │    │
│  │  (DCS-encoded, OCrypt-encrypted, RFC-0850 envelope subtypes │    │
│  │   DOT/1/SYNC_*)                                            │    │
│  └────────────────────┬───────────────────────────────────────┘    │
│                       │                                            │
│  ┌────────────────────┴───────────────────────────────────────┐    │
│  │  Sync Engine:                                              │    │
│  │   - LSN tracker (per-peer, per-table)                      │    │
│  │   - Merkle segment summary builder (per-table)            │    │
│  │   - Snapshot segment indexer (uses PersistenceManager)     │    │
│  │   - Dedup cache (BLAKE3-256 of (peer,lsn) → bool)         │    │
│  │   - Replay protection (RFC-0850 ReplayCache integration)  │    │
│  │   - Rate limiter (per-peer token bucket)                   │    │
│  └────────────────────┬───────────────────────────────────────┘    │
│                       │                                            │
│  ┌────────────────────┴───────────────────────────────────────┐    │
│  │  Transport adapters:                                       │    │
│  │   - NativeP2P (libp2p gossipsub, RFC-0850 §3.1 0x000A)    │    │
│  │   - QUIC (RFC-0850 §8.7) — alternative primary            │    │
│  │   - Webhook (HTTP) — fallback for air-gapped bridges      │    │
│  │   - Multi-carrier (Telegram/Discord/Matrix) — best-effort │    │
│  └────────────────────┬───────────────────────────────────────┘    │
│                       │                                            │
│  ┌────────────────────┴───────────────────────────────────────┐    │
│  │  OCrypt (RFC-0853)                                         │    │
│  │  MissionKeyHierarchy, HKDF-BLAKE3, ChaCha20-Poly1305,      │    │
│  │  MissionId-derived AAD, 24h revocation grace                │    │
│  └────────────────────┬───────────────────────────────────────┘    │
│                       │                                            │
│  ┌────────────────────┴───────────────────────────────────────┐    │
│  │  DOT (RFC-0850)                                            │    │
│  │  DeterministicEnvelope, 21 platform adapters,             │    │
│  │  fragmentation, replay cache, logical timestamps          │    │
│  └────────────────────────────────────────────────────────────┘    │
│                                                                    │
│  Above the new layer, the Sync protocol surfaces as:               │
│    crates/octo-sync/src/{summary,stream,segment,keyring,state}/   │
│    stoolap fork: src/sync/{publisher,subscriber,rpc}.rs            │
└────────────────────────────────────────────────────────────────────┘
```

**v1 design decisions (explicit, for the Use Case → RFC pipeline):**

- **G1 — Determinism.** All wire bytes are BLAKE3-256 hashed; HMAC-BLAKE3 for authenticators; LZ4 for compression; DCS for encoding. v1 single-leader: total order is LSN. (Phase 3 gossip requires causal/vector ordering — see F1.)
- **G2 — Replay safety.** Per-peer LSN watermark + RFC-0850 `ReplayCache` (`BTreeMap<envelope_id, first_seen>`) + OCrypt replay cache (1h or 10K entries per mission).
- **G3 — Idempotency.** All operations are LSN-keyed. `WalTailChunk` carries `from_lsn`/`to_lsn`; receiver dedupes by LSN. `SyncSegment` is keyed by `(table_id, segment_index)`; duplicate delivery is a no-op.
- **G4 — Catch-up cost (worst case).** `O(unapplied LSNs + missing segments)`. Bounded by `O(log N)` Merkle descent for segments.
- **G5 — LSN model.** **Each node has its own LSN counter.** Two nodes can have LSN 1000 referring to completely different entries; LSN is per-writer in v1. Readers track per-peer LSN.
- **G6 — Schema coordination.** Writer and reader must agree on schema at sync time. DDL entries (CreateTable/AlterTable/DropTable) are replicated in WAL order; reader applies them in LSN order, aborting if a referenced DDL is missing.

**Operational requirements (must be satisfied by the design but are not design goals per se):**

- **G7 — Read-while-syncing.** Readers always read against their own committed view (`LSN ≤ reader's current_lsn`). WAL entries are applied atomically per entry in LSN order. Readers see a monotonic, consistent view at all times.
- **G8 — Mission-binding precondition.** A node MUST be bound to a mission with sync-capable role (`Replicator` or `Observer`) before any sync attempts. Unbound missions fail at the AuthChallenge step.

> **Note on G7 and G8:** these are operational behaviors rather than design goals in the strict sense. The G1–G6 are *design* goals (determinism, replay-safety, idempotency, catch-up cost, LSN model, schema coordination); G7 and G8 are *operational requirements* that the design must satisfy. The v1 RFC-0862 should split these into a "Design Goals" section (G1–G6) and an "Operational Requirements" section (G7–G8). The 7-state per-peer lifecycle (`Init → Connecting → Authenticating → Streaming → Suspect → Reconnecting → Terminated`; will be specified in the future RFC-0862 per §11.2) has 7 states (not 8 like RFC-0855's `CoordinatorLifecycle`) because the Sync state machine does not transition through `Handover` (a coordinator-only state).

### 4.2 Key new types (sketch — full spec in the RFC)

```rust
// In cipherocto (proposed RFC-0862)
#[repr(u8)]
enum SyncEnvelopeType {
    SummaryRequest    = 0xA0,  // "give me your per-table Merkle summaries"
    SummaryResponse   = 0xA1,  // here are (table_id, segment_root, count, watermark)
    SegmentRequest    = 0xA2,  // "send me table T, segment S, expected root R"
    SegmentResponse   = 0xA3,  // here is the segment (snapshot file bytes)
    SegmentNotFound   = 0xA4,  // I don't have it
    WalTailRequest    = 0xB0,  // "send me WAL entries from LSN X"
    WalTailResponse   = 0xB1,  // here are N entries
    WalTailEnd        = 0xB2,  // I've sent you everything; closed by LsnAck
    LsnAck            = 0xB3,  // "I have applied up to LSN X"
    Heartbeat         = 0xC0,  // liveness probe
    AuthChallenge     = 0xC1,  // RFC-0853 mission-key derivation
    AuthResponse      = 0xC2,  // Ed25519-signed (peer_short_id || ts || pubkey || mission_id)
}
// Note on allocation: 0xA0-0xC2 are unallocated sub-types below
// the RFC-0850 envelope-type space. RFC-0850 reserves 0x0001-0x0015 for
// platform types; RFC-0852 reserves 0x0001-0x0008 for GossipObjectType;
// the Sync sub-types are envelope payload discriminators, not envelope
// types. The 8-bit envelope payload discriminator space has 256 values;
// 0xA0-0xC2 (35 values) is well below the limit and is reserved for
// Sync in the proposed RFC-0862. Reserved for future: 0xC3-0xFF.

struct SyncSummary {
    table_id: u32,                  // BLAKE3(table_name)
    segment_count: u32,
    segment_root: [u8; 32],         // Merkle root over segment_id hashes
    lsn_watermark: u64,             // highest LSN applied to this table
    hmac: [u8; 32],                 // HMAC-BLAKE3(transport_key, summary_body)
    // NodeStatus is sent as a separate envelope (0xA5) to avoid
    // forcing receivers to scan all tables for the node-level LSN.
}

struct SyncSegment {
    table_id: u32,
    segment_index: u32,
    segment_root: [u8; 32],         // matches the root in SyncSummary
    payload: Vec<u8>,               // a single snapshot-<ts>.bin file
    compression: u8,                // 0=raw, 1=lz4 (matches Cargo.toml dep)
    crc32: u32,                     // matches WAL V2 trailer convention
    lsn_watermark: u64,             // LSN at segment generation time
}

struct WalTailChunk {
    from_lsn: u64,
    to_lsn: u64,                    // entries in [from_lsn, to_lsn] inclusive
    entries: Vec<Vec<u8>>,          // raw WALEntry::encode() output
    is_last: bool,                  // true if to_lsn == writer.current_lsn
                                    // (defensive: if WalTailEnd is lost,
                                    //  the receiver can use this to know
                                    //  the stream is done)
}

struct NodeStatus {
    node_id: NodeId,
    current_lsn: u64,               // node-level LSN (max across tables)
    mission_id: [u8; 32],
    identity_epoch: u64,            // RFC-0853 §12 key rotation counter (NOT to be confused with MissionId.version)
}

// In stoolap fork (proposed new module)
pub trait SyncTransport: Send + Sync {
    fn open_node(local_dsn: &str, node_id: NodeId, writer_node_id: Option<NodeId>) -> Result<Self> where Self: Sized;
    fn publish_wal_tail(&self, peer: PeerId, from_lsn: u64) -> Result<u64>;
    fn request_summary(&self, peer: PeerId) -> Result<Vec<SyncSummary>>;
    fn request_segment(&self, peer: PeerId, table_id: u32, segment_index: u32) -> Result<SyncSegment>;
    fn current_lsn(&self) -> u64;
    fn apply_wal_entry(&self, entry: &[u8]) -> Result<u64>;  // canonical apply fn
}
// Note: trait is sync (blocking I/O) to match the fork's synchronous
// design (§2 Constraints, lines 55-62). Async I/O is provided by the
// `stoolap-sync` companion crate using `tokio` behind the `sync` feature.

pub struct NodeId(pub [u8; 32]);
pub struct PeerId(pub [u8; 32]);
// NodeId = BLAKE3(OverlayIdentity.public_key || mission_id) per
// RFC-0853:163. `OverlayIdentity.public_key` is Ed25519 public (32 bytes).
```

### 4.3 Identity, key hierarchy, and trust

- **Node identity** = `OverlayIdentity` per `RFC-0853 §4` (Ed25519 keypair: `public_key: [u8; 32]` per `RFC-0853:163`, with the corresponding private key held by the node and never advertised; the `signature: [u8; 64]` field is the Ed25519 signature that authenticates the identity). At Sync handshake time, the node advertises its `OverlayIdentity.public_key`. **This research does not invent a new `node_signing_pubkey` concept** — it reuses the existing `OverlayIdentity` type.
- **NodeId = BLAKE3(public_key || mission_id)** (32 bytes). First 16 bytes are the "short" id used in logs; full 32 bytes in envelopes.
- **PeerId = BLAKE3(peer_public_key || mission_id)** — same construction for remote nodes, where `peer_public_key` is the remote node's `OverlayIdentity.public_key`.
- **Encryption key** derived from `MissionKeyHierarchy.execution_keys_root` (RFC-0853) via `HKDF-BLAKE3(mission_root_key, "sync:v1", mission_id)`. Per-mission, not per-message.
- **AEAD AAD** for OCrypt: `(envelope_id || sender_ephemeral_public || mission_id || logical_timestamp || sequence)` per RFC-0853 §5. (Note: this is the AAD for the symmetric encryption, NOT the same as the Ed25519 signature payload below.)
- **AuthChallenge/AuthResponse signature payload** (the Ed25519 signature, separate from AAD): `(peer_short_id || timestamp || public_key || mission_id)`. The receiver validates the signature against the mission's public key set distributed via `GatewayAdvertisement.trust_root` (RFC-0851 §M-GDP-2, line 435).
- **Trust anchor**: a `DOT/1/SYNC_AUTH_RESPONSE` carries a signature over the tuple above. The peer validates with the mission's public key set. This reuses the RFC-0851p-a Mode A trust-anchored bootstrap pattern.
- **Rejection of CA / X.509**: keys are self-sovereign per mission; no external PKI.
- **Rate limit**: per-peer token bucket (100 envelopes/s sustained, 500 burst; configurable per mission). Enforced at the Sync engine, not at the platform adapter.

### 4.4 Canonical ordering and determinism

- **WAL application order** is the writer's local LSN order. The Sync protocol never re-orders; it ships entries in the order the writer committed them.
- **Table application order** is canonical: `(table_id, lsn, row_id, op_type)`. A node receiving entries from multiple peers at once (future Phase 3 gossip) sorts by this key and applies in order.
- **Hashing**: BLAKE3-256 for all sync wire hashes (envelope_id, segment_root, summary HMAC, node_id). Matches RFC-0850 `envelope_id`, RFC-0852 `object_hash`, RFC-0853 primitives, and the Stoolap `octo_determin` dependency already linked from this repo.
- **Merkle segment tree**: 16-way (matches `HexaryProof` convention in `trie/proof.rs`). Root = BLAKE3-256 of the 16 child hashes (or itself if leaf). Tree depth ≤ 4 for ≤ 65 536 segments per table.
- **Uncommitted transactions** are NOT shipped. Sync streams only entries with a `Commit` marker; `Rollback` markers trigger entry discard on the reader (matches `WALManager::replay_two_phase` semantics at `wal_manager.rs`).
- **v1 single-leader → total order via LSN.** Phase 3 multi-peer will need per-row HLC or vector clocks; deferred to F1.
- **RFC-0008 mapping**:

| Operation | Class | Rationale |
| --- | --- | --- |
| SyncSummary encoding | **A** | DCS-encoded, BLAKE3-256 hashed, HMAC-BLAKE3 — all deterministic |
| SyncSegment encoding | **A** | DCS-encoded, BLAKE3-256 hashed, CRC32 trailer, LZ4 (LZ4 is byte-deterministic) |
| WalTailChunk encoding | **A** | Raw `WALEntry::encode()` output (stoolap V2 binary is already canonical across implementations per RFC-0104) |
| NodeStatus encoding | **A** | Same as SyncSummary |
| AuthChallenge nonce | **A** | Must be unique per session; HKDF-BLAKE3-derived |
| Replay cache eviction | **A** | RFC-0850 already specifies BTreeMap with deterministic tie-break |
| LSN monotonicity enforcement on receiver | **A** | Per-entry `entry.lsn == previous_lsn + 1` check |
| Merkle segment tree root | **A** | BLAKE3-256 over 16 child hashes |
| Compression selection (LZ4 vs raw) | **A** | LZ4 is byte-deterministic; selection is encoded in the segment |
| Snapshot segment generation (atomic-rename) | **A** | The atomic-rename semantics of `MVCCEngine::create_snapshot` (`engine.rs:2642`, rename at `engine.rs:2828`) are part of the protocol contract; a reader that observes a half-written segment is a bug |
| Dedup cache eviction (per-peer LSN) | **A** | BTreeMap by LSN |
| Mission key derivation | **A** | RFC-0853 already Class A |
| Logical timestamp assignment | **A** | Counter, no wall clock |
| Transport selection (NativeP2P vs Webhook vs Telegram) | **B** | Affects message arrival order and reliability, hence convergence; deterministic when configured with a fixed transport |
| Retry/backoff | **B** | Affects convergence order; deterministic when retry interval is configured |
| Diagnostic logging | **C** | Does not affect state |
| Path selection in the DRS sense | **C** | Per RFC-0856 itself |

### 4.5 Trust assumptions and adversarial analysis (RFC-0008 §Adversary Analysis 5-Question Test)

The 5-Question Adversary Test asks for each threat: (1) Who benefits? — by capability, (2) What does it cost them? — quantified, (3) What do they gain if successful?, (4) What's our defense?, (5) What's the residual risk? — and is it acceptable?

| # | Threat | Q1 Who benefits? | Q2 Cost to attacker | Q3 Gain if successful | Q4 Defense | Q5 Residual risk |
| --- | --- | --- | --- | --- | --- | --- |
| 1 | **Malicious peer injects fake WAL entries** | A misbehaving peer wanting to corrupt the replica | Mission stake (≥ 1,000 OCTO global + role-specific per RFC-0855p-b); entry fabrication requires forging the per-mission `transport_keys_root` HMAC | Replica accepts bogus rows/tables; downstream ZK proofs reference false data | OCrypt signature per envelope + HMAC-BLAKE3 per SyncSummary + LSN monotonicity check on receiver + segment_root cross-check + duplicate-segment detection | Operator must run a Sybil-resistant peer set (RFC-0851 diversity constraints: ≥2 Regional, ≥3 Global). If all peers collude, residual = total corruption. **Acceptable** under standard Sybil-resistance assumptions. |
| 2 | **Malicious peer withholds WAL entries** | A misbehaving peer wanting to starve the replica or create a fork | Mission stake; sustained withholding drops trust score (PoRelay RFC-0860) | Replica falls behind, then forks if another peer advances | Heartbeat (5s interval) + LSN-watermark probe + `Suspect` after `2 × heartbeat_interval` (10s) + auto-mark peer unhealthy + reroute via DRS (RFC-0856) | If all peers withhold simultaneously, the replica stalls. Operator must configure ≥2 sync-capable peers. **Acceptable.** |
| 3 | **Eclipse attack on a new node** | An attacker controlling many fake identities | Many fake identities (cheap in many Sybil scenarios) + sustained mission stake if stake-gated | Surround the new node with attacker peers; control its view of the world | RFC-0851p-a Mode A (5 foundation nodes, 3-of-5 intersection, ≥80% peer-list overlap) + cross-platform diversity ≥2 Regional, ≥3 Global + invite-link Mode C for human anchor | A motivated attacker could still eclipse a node that joins from a single platform. **Operator policy** required: do not join from a single transport. |
| 4 | **Replay of an old WAL entry** | A passive eavesdropper or a peer that captured a stale envelope | Network cost to replay (bandwidth + latency); no new key material needed | Cause the replica to apply a stale entry (idempotency prevents incorrect state, but wastes compute) | Replay cache (RFC-0850 BTreeMap<envelope_id, first_seen>) + per-peer LSN watermark + HMAC binding `(envelope_id, lsn, sender_ephemeral_public)` via OCrypt AAD | None for state correctness (idempotency); bandwidth waste only. **Acceptable.** |
| 5 | **MITM during AuthChallenge** | A network-positioned attacker | Mission stake to register a public_key + network position | Impersonate a peer and receive Sync streams | Ed25519 signature in AuthResponse; peer_short_id derived from public_key; double-verify via `GatewayAdvertisement.trust_root` (RFC-0851) | Conditional: if `trust_root` is correctly bootstrapped (RFC-0851p-a Mode A or C), residual is **none**; if bootstrapped from a single untrusted source, residual is full impersonation. **Operator must verify trust_root at mission start.** |
| 6 | **Compromise of writer node (key exfiltration)** | An attacker with physical/logical access to the writer's host | Engineering effort (root/credential access) | Read all data; write any data; impersonate the writer to all readers | OS-level hardening (out of scope); F2 trust-anchored storage checkpoints (deferred); operational key rotation | High impact: writer key compromise equals full read/write on the entire fleet. **Operational**: rotate writer `identity_epoch` per RFC-0853 §12 (24h grace); consider HSM (Hardware Security Module) for writer key storage in a future Sync protocol release. |
| 7 | **DoS via flood of `WalTailRequest` or `SegmentRequest`** | A misbehaving peer or external attacker | Bandwidth only | Saturate writer's bandwidth or compute | Per-peer token bucket (100 req/s sustained, 500 burst; configurable) at Sync engine; platform-adapter-level rate limit at DOT | Adaptive rate-limiting adds 5-10% CPU. **Acceptable.** |
| 8 | **Long-tail replay after mission key rotation** | A peer that captured envelopes under the old `identity_epoch` | Storage of old envelopes; no new capability | Apply a stale envelope whose session keys happen to validate | RFC-0853 §12 (rotation) does NOT reset the RFC-0853 §7 (replay protection) window; AAD binds to `mission_id` (not `identity_epoch`), so old envelopes validate against the new key only if `mission_id` is unchanged | Residual: small replay window between rotation and observed rotation by all peers (24h grace). **Mitigation**: rotate `mission_id` (not just `identity_epoch`) on key compromise. |
| 9 | **Sybil attack creating a fake "primary" peer** | An attacker registering multiple "writer" identities | Mission stake per identity | Trick readers into syncing from a malicious "writer" | Reader is configured with a static `writer_node_id`; if a peer claims to be the writer but its `NodeId` doesn't match, the reader rejects. Requires operator-supplied `writer_node_id` at mission start (see §4.6 Assumption 19) | Residual: zero if `writer_node_id` is correctly configured. **Operator MUST supply the writer's `NodeId` at mission init**, not rely on election. |
| 10 | **Snapshot corruption in transit** | Bit-flip on the wire (natural or adversarial) | Bandwidth to inject | Force receiver to install a corrupted segment, breaking the database | CRC32 trailer (existing WAL convention) + segment_root hash cross-check (BLAKE3-256); on mismatch, receiver re-requests the segment and the writer re-sends | Residual: requires two consecutive bit-flips to defeat both CRC32 and BLAKE3-256. **Acceptable** (collision probability 2⁻²⁵⁶). |
| 11 | **Compromise of OCrypt primitives** | An attacker with breakthrough cryptanalysis | Years of research + compute | Break ChaCha20-Poly1305 or BLAKE3 | Use only standardized, well-reviewed primitives (RFC-0853 §3); BLAKE3-256 is finalist-equivalent | If a primitive breaks, all Sync traffic is exposed. **Accepted risk**: monitor NIST guidance; have a primitive-rotation RFC ready (F4 in the deferred list). |
| 12 | **Replay of old envelope against new mission key (key reuse bug)** | An attacker exploiting a derivation bug | Discovery of a derivation bug | Validate an old envelope under a new key | OCrypt's HKDF-BLAKE3 includes `mission_id` in AAD; if the implementation correctly includes `mission_id`, an old envelope will not validate. Code review + property tests (mission `0862h`) | Residual: zero if OCrypt is correctly implemented; high if a derivation bug exists. **Mitigation**: add a property test that any two `mission_id` values produce different AADs. |
| 13 | **Memory exhaustion via ReplayCache growth** | A peer that sends many unique envelopes | Bandwidth | Force the receiver to fill memory | Replay cache has a configurable max size (default 10K, evictable); `BTreeMap` eviction is deterministic and bounded | Residual: per-peer OOM requires 10K unique envelopes, ~5MB. **Acceptable.** |
| 14 | **Bandwidth exhaustion via `SnapshotFragment` flood** | A peer requesting many large segments | Bandwidth | Saturate writer's outbound | Per-peer rate limit + size cap per `SegmentResponse` (e.g., 100 MB) + total bandwidth cap per peer per minute | Residual: small overhead. **Acceptable.** |
| 15 | **Reader accepts a malicious "official" snapshot** | A peer providing a snapshot that claims a higher `state_root` than the writer's | Engineering effort to craft a plausible fake | Reader installs a corrupted state | Receiver verifies `segment_root` against the writer's published `SyncSummary`; `SyncSummary.hmac` binds the root to the writer's `transport_keys_root` | Residual: zero if the receiver cross-checks the summary. **Test required**: phase 2 sub-mission `0862c`. |
| 16 | **Merkle tree collision in `segment_root`** | Attacker who finds a BLAKE3 collision | 2¹²⁸ compute (infeasible) | Substitute a segment | BLAKE3-256 has 128-bit security against collision | Infeasible. **Acceptable.** |
| 17 | **Monotonic counter rollback attack on LSN** | An attacker with kernel/VM access to the writer | Root access to the writer host | Reset `current_lsn` to reuse a lower LSN, breaking monotonicity | LSN counter is per-process; if the writer restarts, the WAL manager's `find_safe_truncation_lsn` ensures the counter only advances. Receivers track per-peer watermarks and reject LSN regression | Residual: requires host compromise. **Operational**: deploy the writer on an immutable infrastructure (e.g., containers with read-only root FS). |
| 18 | **Slashing-misbehavior false positive** | The protocol itself (not an attacker) | N/A | Reader wrongly marks writer `Suspect` due to legitimate latency | Heartbeat tolerance (`2 × heartbeat_interval` = 10s) + configurable jitter (0-2s) + retry before escalation | Residual: false positive rate <1% under realistic network conditions. **Acceptable.** |
| 19 | **Natural partition (NOT an adversary attack)** | N/A — natural failure | N/A | Replica falls behind, then must reconcile on heal | Heartbeat detects partition; LSN-watermark probe; on heal, anti-entropy Merkle descent re-syncs missing segments | Listed in §5 Risks, not §4.5 Adversary (out of threat model scope). |

**Trust model summary:**

- v1 single-leader: writer is **trusted by configuration**, not by election. Operator supplies `writer_node_id` at mission init.
- Readers are **untrusted by default** (they can lie about their LSN). The writer keeps no state about readers.
- Peers are **authenticated by mission key** (RFC-0853). They are not trusted to behave correctly — the protocol assumes Byzantine peers and detects misbehavior via heartbeat + LSN-watermark probes.
- The trust anchor is `GatewayAdvertisement.trust_root` from RFC-0851, bootstrapped via RFC-0851p-a Mode A or C.

The 5 rows above that were in the v1.0 draft (Threats 1-5) have all been rewritten with quantified costs and separated Q1/Q3. The 13 additional rows (Threats 6-18) cover the gaps identified in Round 1 of the adversarial review. Threat 19 (natural partition) is added for context but is out of the adversary threat model — it is listed in §5 Risks.

### 4.6 Implicit Assumptions Audit (per RFC template v1.3, BLUEPRINT §"Categories to Audit")

| # | Category | Assumption | Where relied upon | Blast radius if false | Mitigation / Status |
| --- | --- | --- | --- | --- | --- |
| 1 | Data integrity | Receiver computes **BLAKE3-256** over each applied segment and aborts on mismatch | §4.2 (SyncSegment.segment_root) | If false, a corrupted segment is installed silently; downstream ZK proofs reference false data | Test: mission `0862c` property test "any segment whose BLAKE3-256 root ≠ claimed segment_root is rejected." |
| 2 | Transport framing | DOT platform adapters honor byte-exact framing | §4.2 (wire format) | Telegram/IRC adapters may lose bytes at the fragmentation boundary | Use the RFC-0850 fragmentation `DOT/F/...` envelope subtype for segments > adapter MTU. Test: round-trip 256B / 512B / 4KB / 1MB through every adapter. |
| 3 | Network behavior | Network has bounded partition duration | §4.4 (LSN monotonicity), §6 Phase 1 test | A long partition could force a snapshot re-ship on every reconnect; if partition > writer's WAL retention, the reader must resync from scratch | DGP anti-entropy Merkle summary limits the reship to *missing* segments; `SnapshotRequest` is the recovery path. **ACCEPTED RISK**: reader can lose data if partition > writer's WAL retention window. |
| 4 | Configuration | Sync config is correctly set up (peer IDs, mission ID, transport adapter selection, `writer_node_id`) | §4.3 (identity), §6 Phase 1 | A misconfigured reader may sync from the wrong peer or refuse to sync entirely | Operator runbook + `stoolap sync doctor` CLI that validates config before opening a `Database`. **Test**: mission `0862h` "config-error injection" test. |
| 5 | Identity stability | Node identity (`OverlayIdentity`) is stable for the duration of a sync session; the cipher suite (ChaCha20-Poly1305 + Ed25519 + HKDF-BLAKE3) is fixed for the session and does not downgrade mid-sync | §4.3 (identity) | A key rotation mid-sync would invalidate the per-peer LSN watermark and the HMAC; a cipher-suite downgrade would weaken confidentiality. | OCrypt mission key rotation triggers a fresh `AuthChallenge`; in-flight envelopes from the old key are dropped. Cipher suite is fixed in the Sync envelope header and cannot be changed mid-session. **Test**: mission `0862d` "rotation during sync" test. |
| 6 | Resource availability | Writer's commit rate is bounded below `5,000 commits/s` | §8 (performance target) | If writer exceeds this, reader cannot keep up; WAL buffer grows unbounded | Reader's per-peer backpressure: reader sends `PAUSE` if its apply queue > 10K entries. **ACCEPTED RISK**: above 5K commits/s sustained, reader falls behind. |
| 7 | Resource availability | Reader has enough disk space for incoming WAL + segments | §3.4 (snapshot shipping) | Reader crashes if `/` fills up | Disk-space check before applying each segment; reject segment if free space < 2× segment size. **Operational**: monitor `df` on reader. |
| 8 | Resource availability | System has enough memory for Sync engine + replay cache + dedup cache (≤ 50 MB total) | §4.3 (rate limit + replay cache) | OOM if peer sends many unique envelopes at high rate | Bounded caches with deterministic eviction; per-peer rate limit caps inbound rate. |
| 9 | Time source | OS provides monotonic time for `get_fast_timestamp()` (the writer's LSN counter) | §1.1, §4.4 | Counter rollback (kernel bug, VM migration) could break LSN monotonicity | Counter is per-process, persisted in WAL; `find_safe_truncation_lsn` ensures counter only advances. **ACCEPTED RISK**: host-level clock attack. |
| 10 | Network partition | The OS, network stack, and platform adapters all support ordered, reliable byte streams for the chosen transport (NativeP2P / QUIC) | §4.1 (transport selection) | Loss / reordering at the transport layer is handled by DOT's `ReplayCache` and fragmentation, but not by Sync itself | Documented in §5 Risks. |
| 11 | Upgrade safety | Writer and reader are on the same software version (no mixed-version operation) | §4.7 (compatibility) | A reader on v0.4 cannot read a writer on v0.5 if the wire format changes | WAL has format-version byte since V2; Sync envelope header has version byte `0x01` (v1) and `0x02` (v2). Reader rejects envelopes with unknown version. **ACCEPTED RISK**: rolling upgrades require coordination. |
| 12 | Configuration | Mission is bound and authenticated before any sync attempts | §4.3 (trust anchor) | Sync attempts fail at the AuthChallenge step; no state divergence | Reader checks mission state at startup; refuses to open if mission is not `Active` per RFC-0855 lifecycle. |
| 13 | Resource availability | The cipherocto node has sufficient stake to participate in the mission (per RFC-0855 dual-stake: ≥ 1,000 OCTO global + role-specific) | §1.2 (RFC-0855 mission) | Sync rejected by mission governance | Mission admission check before opening a `Database` with sync. **Test**: mission `0862h` "insufficient stake" test. |
| 14 | Configuration | At least one sync-capable peer is online and reachable when sync is attempted | §4.1 (transport) | Sync hangs; reader times out after `2 × heartbeat_interval` (10s) | Heartbeat + `Suspect` transition + `WriterUnreachable` event emitted locally. **ACCEPTED RISK**: zero-peer dead-end requires operator intervention. |
| 15 | Schema coordination | Writer and reader agree on schema (table definitions, column types) at sync time | §4.1 (G6), §5 Risks | DDL applied out of order; reader rejects | DDL entries applied in LSN order; missing dependency aborts the apply with a clear error. **Operational**: schema migrations must be coordinated. |
| 16 | Mission-binding precondition | A node MUST be bound to a mission with sync-capable role (`Replicator` or `Observer`) before any sync attempts. If the mission is bound but the role is not sync-capable, the AuthChallenge fails with `RoleNotSyncCapable` (no fallback, no downgrade). | §4.1 (G8) | Unbound mission during sync; AuthChallenge fails | Reader refuses to open with `sync=on` unless mission is bound and the local role is sync-capable. The error code is stable across implementations (DCS-encoded enum). |
| 17 | Snapshot atomicity | The writer never serves a half-written snapshot segment; segments are written to a temp file and atomic-rename'd when complete | §3.4 (snapshot shipping) | Reader sees a partial segment; CRC32 + segment_root detect and reject | Stoolap's `MVCCEngine::create_snapshot` already uses atomic-rename. **Verified**: `engine.rs:2642` (function definition), `engine.rs:2828` (the actual `std::fs::rename` call with rollback on partial failure). |
| 18 | Recovery semantics | After a dual-node crash, on restart, the reader sends a fresh `SummaryRequest` to re-establish its LSN watermark | §4.1 (catch-up), §6 Phase 1 | Reader skips ahead or falls behind on restart | Reader's persistent state (last applied LSN, replay cache snapshot) is on disk in `state/sync-watermarks.bin`. **Test**: mission `0862c` "dual-crash recovery" test. |
| 19 | Operator trust | Operator correctly designates the `writer_node_id` at mission start (no election in v1) | §4.1 (G6), §6 Phase 1 | Reader syncs from a wrong peer; data is exposed to an unauthorized party | CLI requires explicit `--writer-node-id` flag; refuses to start without it. **Operational**: this is a hard requirement, not a configuration option. |
| 20 | Platform trust | DOT platform adapters (Telegram, Discord, Matrix, etc.) honor byte-exact framing across their respective SDK upgrades | §4.2 (wire format) | Upstream SDK changes could lose bytes at the boundary | DOT's per-adapter MTU handling + `DOT/F/...` fragmentation makes this recoverable. **ACCEPTED RISK**: monitor upstream SDK changelogs. |
| 21 | Configuration | TLS / Noise / DTLS is correctly configured for the chosen transport (NativeP2P uses libp2p Noise; QUIC uses TLS 1.3) | §4.1 (transport) | MITM if transport-layer security is misconfigured | Documented in the operator runbook; not a Sync-protocol concern. **Operational**. |

### 4.7 Compatibility

- **Backward compat with single-process `Database::open(dsn)`**: zero change. Sync is opt-in via a new constructor `Database::open_with_sync(dsn, SyncConfig)`.
- **Backward compat with the WAL format**: V2 is unchanged. New envelope subtypes use unallocated envelope-payload discriminator codes (`0xA0–0xC2` — see §4.2 note) that do not conflict with `RFC-0850`'s platform-type table (`0x0001`–`0x0015`) or `RFC-0852`'s GossipObjectType table (`0x0001`–`0x0008`).
- **Forward compat**: envelope version byte in the Sync header. v1 fixes `0x01`. A v1 reader rejects envelopes with version ≠ `0x01` (forward-incompatible); a v2 reader accepts v1 envelopes (backward-compatible). The version byte is part of the OCrypt AAD, so a v1 reader cannot be tricked into accepting a v2 envelope as v1.
- **Cross-implementation**: every operation maps to either a Stoolap WAL entry (already versioned) or a CipherOcto RFC-0850 envelope subtype (already versioned). Two independent implementations should produce the same wire bytes.
- **Build profile**: All Sync code MUST inherit Stoolap's release profile per `stoolap/Cargo.toml:215-228` (`codegen-units = 1`, `lto = true`, `overflow-checks = false`, `panic = "abort"`, `-C target-feature=-fma` via RUSTFLAGS). The DFP comment block at `Cargo.toml:165-186` documents this requirement (RFC-0104 §"Determinism Hazards").

---

## 5. Risks and Mitigations

| # | Risk | Likelihood | Impact | Mitigation |
| --- | --- | --- | --- | --- |
| R-1 | Cryptographic mistakes in OCrypt reuse | Low (RFC-0853 is solid) | High (peer impersonation) | Adopt OCrypt as-is; reuse primitives unchanged. Replay-test vectors from RFC-0853 §Test Vectors. Mission `0862d` "OCrypt test-vector replay" test. |
| R-2 | Schema drift between peers (DDL during sync) | Medium | High (apply failure) | Apply DDL entries in LSN order; if a later DDL is missing, abort the apply with a clear error and surface `SchemaDrift` event. DDL is replicated atomically per entry. **Open gap**: schema migration across major version mismatches is F9 future work (see §11.8). |
| R-3 | Clock skew | None | None | Protocol is LSN-based; no wall clock on the wire |
| R-4 | Snapshot file corruption during transit | Low | Medium (apply failure) | CRC32 trailer (existing WAL convention) + segment_root hash cross-check (BLAKE3-256); on mismatch, re-request segment. See §4.5 Threat 10. |
| R-5 | Long-tail memory growth of ReplayCache | Medium | Medium (process OOM) | BTreeMap eviction (RFC-0850); configurable max size (default 10K); snapshot to disk on shutdown. |
| R-6 | DOT platform adapter MTU smaller than segment | High (IRC/LoRa) | Low (just fragmentation) | RFC-0850 `DOT/F/...` already supports fragmentation; segment may be split across many envelopes with deterministic reassembly |
| R-7 | Concurrent schema migrations across peers | Low (single-leader) | High | Reject DDL from non-leader; leader liveness is monitored via Heartbeat + PoRelay. See R-2. |
| R-8 | Network partition during bulk snapshot | Medium | Medium (wasted transfer) | Receiver sends `WalTailEnd` after each segment; sender re-sends from last `LsnAck` on reconnect. See §4.5 Threat 19. |
| R-9 | Backwards-incompatible WAL format change | Low (V2 is stable) | High | WAL has format-version byte since V2; receiver checks on first entry. See §4.6 Assumption 11. |
| R-10 | Adding `tokio` as hard dep | Low (would be opt-in) | Medium (forces async on users) | **Keep sync transport in a separate crate** `stoolap-sync` with its own `tokio` dep; the core `stoolap` crate stays sync. Trait `SyncTransport` in the core crate is sync; the `stoolap-sync` crate provides an async facade. |
| R-11 | Mission key rotation in flight | Low | Medium | Re-handshake on Heartbeat if `identity_epoch` differs; RFC-0853 §12 already supports rotation (24h grace). See §4.5 Threat 8. |
| R-12 | Anti-entropy overhead at scale | Medium | Medium (gossip spam) | DGP `GossipDomainId::MISSION` scope limits propagation; Bloom filter compression for large summary sets. |
| R-13 | Writer election / leader handoff | Low (v1 single-leader) | High (no failover) | **v1 has no failover** — operator must designate `writer_node_id` at mission start. F1 (multi-leader) and F8 (auto-failover via DomainCoordinator handover) are future work. See §4.6 Assumption 19. |
| R-14 | file:// DSN cross-process lock conflict | Low | Low | Sync transport is in a separate process/thread; the cross-process `flock` is held by the Stoolap process only. The Sync engine does NOT open a `Database` on the same DSN concurrently with another process. |
| R-15 | In-flight transaction at sync start | Medium | Low | Sync streams only entries with a `Commit` marker; uncommitted entries are buffered on the writer and sent only after commit. `Rollback` markers trigger entry discard. See §4.4. |
| R-16 | Read-while-syncing consistency | Low (always correct) | None | Reader reads against its own committed view (`LSN ≤ reader's current_lsn`). See §4.1 G7. |
| R-17 | Vector clock / causal ordering | N/A (v1 single-leader) | N/A | v1 uses LSN total order. F1 multi-leader requires per-row HLC. |
| R-18 | Discovery of new writer (failover) | Low (v1 has no auto-failover) | High (manual intervention) | Operator must reconfigure the reader with the new `writer_node_id`. F8 (auto-failover) is future work. |
| R-19 | Uncommitted transaction on writer when reader requests | Medium | Low | Sync uses `replay_two_phase` semantics: only entries after the writer's `Commit` marker are shipped. Matches existing WAL recovery path. |
| R-20 | Replicator role designation | Low | High (no role, no sync) | RFC-0855 must be amended to add `Replicator` (see §10.3). The writer holds the `Replicator` role; readers are `Observer`s. |
| R-21 | Caught-up reader receives a peer re-broadcasting the same segment | Low | Low | `SyncSegment` keyed by `(table_id, segment_index)`, CRC32 + BLAKE3 root check. Duplicate is a no-op. |
| R-22 | Reader holds a partial segment across a crash | Medium | Low | Atomic-rename: writer never serves a half-written segment (see §4.6 Assumption 17). On restart, reader re-requests the segment; writer re-sends. |
| R-23 | Schema-major-version mismatch (writer v2, reader v1) | Low | High | Sync header carries schema-major-version; reader rejects if mismatch. **Operational**: schema migrations require coordinated upgrade. |
| R-24 | Stoolap public API changes (e.g., new `Database::open_*` variant) | Low | Medium | Sync API is additive; new public methods only. CI gate: `cargo doc` for `stoolap` with the `sync` feature enabled must compile. |

---

## 6. Implementation Roadmap (proposed — actual phasing lives in Missions)

### Phase 1 — Minimum Viable Two-Node Sync (MVE)

- Single-leader, N read-replicas.
- WAL-tail streaming (Approach B).
- LSN-based catch-up; no snapshot shipping yet.
- **Transport**: NativeP2P (libp2p gossipsub, 0x000A) primary; QUIC (0x0015) alternative primary; Webhook fallback.
- Mission scope only.
- **Writer designation**: v1 has NO election. Operator configures `writer_node_id` at mission start on the reader. The writer is the node whose `OverlayIdentity.public_key` matches this `NodeId`. This is a hard requirement (see §4.6 Assumption 19, §4.5 Threat 9).
- **Failover**: NONE in v1. If writer is unreachable for `> 2 × heartbeat_interval` (10s) sustained, reader emits `WriterUnreachable` event locally and stops syncing. Auto-failover is F8 future work.
- Heartbeat: 5s interval. `Suspect` after `2 × heartbeat_interval` = 10s (matches RFC-0850 / RFC-0855p-c `2 × HEARTBEAT_INTERVAL` convention).
- Rate limit: 100 req/s sustained, 500 burst per peer.
- Test: two nodes, writer + reader-replica, run for 1h, no data drift. Verification: every table's `BLAKE3-256(SELECT * FROM table)` matches across both nodes. Test also covers: dual restart, single restart, 30s partition, 5min partition, 1hr partition (no data loss), schema add column, schema drop column, schema add table.

### Phase 2 — Catch-up via snapshot segments

- Anti-entropy Merkle summary exchange (Approach D).
- Snapshot segment request/response.
- `SnapshotFragment` DGP object type format specified and implemented.
- Bitmap-summary compression for large state sets.
- Heartbeat carries `(lsn_watermark, segment_root)` for divergence detection.
- Test: 1M-row DB, sync in <60s; kill writer mid-sync, restart, auto-resume from `LsnAck`.

### Phase 3 — Multi-node gossip

- DGP `GossipObject` with `object_type = MempoolIntent` (RFC-0857) or new `SyncBatch` (0x0009) carries batches of WAL entries.
- Any node can serve or receive sync.
- DRS-based peer selection (RFC-0856).
- PoRelay trust scoring (RFC-0860) ranks peers by sync reliability.
- Test: 5-node network, 1 writer, 4 readers, kill any node, verify convergence within 60s.

### Phase 4 — Cross-carrier, N-node, mission-aware

- Multi-carrier propagation: same sync stream across NativeP2P + Webhook + one social adapter.
- Per-mission key isolation (PRIVATE missions get encryption; PUBLIC missions send in clear).
- Slashing for misbehaving sync peers (PoRelay `0x0010` slash code).
- Interop test: two implementations (Rust + the eventual Cairo / Move ports) reach identical state.

---

## 7. Test Strategy

### Unit

- Envelope subtype codec round-trip.
- BLAKE3-256 / HMAC-BLAKE3 / LZ4 round-trip.
- Merkle segment tree root computation deterministic across builds.
- LSN monotonicity enforcement on the receiver (per-entry `entry.lsn == previous_lsn + 1`).
- `NodeId` / `PeerId` derivation deterministic across builds.
- `WALHeader` encode/decode byte-exact across Rust versions.
- Rate-limiter token-bucket math (no off-by-one).

### Integration

- Two `Database` instances in the same process, sharing a Sync transport over a Unix pipe (loopback NativeP2P). Writer commits 10K rows; reader replica converges.
- 3+ nodes, leader failover, follower promotion (**manual** in v1; **automated** is F8 future work).
- Restart scenarios: kill writer mid-commit, restart, resume sync from last `LsnAck`. Kill reader mid-apply, restart, re-handshake + re-summary.
- Network partition: simulate via iptables drop for 10s, 60s, 5min, 1hr; verify anti-entropy on heal (1hr requires snapshot re-ship — covered in Phase 2).
- Schema migration: writer adds a column, reader applies; writer adds a table, reader applies; writer drops a column, reader applies. All LSN-ordered.
- 1M-row DB, sync in <60s; kill writer mid-sync, restart, auto-resume from `LsnAck`.

### Adversarial

- Replay attack: re-inject an old envelope; expect rejection from ReplayCache.
- Bogus WAL entry: inject an entry that does not match expected format-version; expect rejection.
- Malicious peer omits segments: detect via Merkle descent, mark peer `Suspect`.
- MITM during AuthChallenge: tamper a byte; expect signature verification failure.
- Sybil claim: peer claims to be the writer with a different `public_key`; expect rejection.
- Snapshot corruption: flip a byte in transit; expect BLAKE3-256 root mismatch and re-request.
- Heartbeat flood: peer sends 10K heartbeats/s; expect rate-limit trigger.
- Replay across mission key rotation: capture envelope under old key, replay after rotation; expect rejection (AAD binds to `mission_id`).

### Property-based

- For any two WAL entries A and B with `A.lsn < B.lsn`, the resulting `Database` state is identical regardless of arrival order (LSN-monotonicity property; v1 single-leader).
- For any peer set of N ≥ 1, after `2 × heartbeat_interval` of no contact, the local sync state machine transitions to `Suspect` (heartbeat-loss property).
- For any mission key set, two distinct `mission_id` values produce different AADs (key-isolation property).
- For any rate-limit configuration, the inbound envelope rate is bounded by `100 req/s` sustained (rate-limit property).
- (Phase 3, not v1:) For any two sequences of valid sync messages from multiple peers, the resulting `Database` state is identical (commutativity + associativity of apply). This is the **F1 multi-leader** property test; v1 single-leader does not need it.

### Determinism

- **CI gate**: build the same code on Linux x86_64 and macOS arm64 with `RUSTFLAGS="-C target-feature=-fma"`, produce the same wire bytes for the same input. (Not a test, but a CI requirement.)
- Compile with `-C target-feature=-fma` per `stoolap/Cargo.toml:182, 212-213` (RFC-0104 §"Determinism Hazards"); verify identical hashes.
- Cross-check against a **second implementation** as part of the RFC acceptance process. A reference Python implementation of the wire format (decode-only) is produced as part of mission `0862h`; no pre-existing harness.

---

## 8. Performance Targets (proposed)

All targets assume: NativeP2P transport (libp2p gossipsub, `0x000A`), LZ4 compression on segments, `SyncMode::Normal` WAL fsync (every commit), single writer, single reader.

| Metric | Target | Notes |
| --- | --- | --- |
| **End-to-end replication latency (one-way)** | < 50 ms p50, < 200 ms p99 | LAN (≤ 10 ms RTT), 1 KB write, single envelope |
| End-to-end replication latency (one-way, WAN) | < 500 ms p99 | WAN (≤ 100 ms RTT), 1 KB write |
| **Throughput (single writer)** | > 5,000 commits/s | WAL streaming, batched; assumes 200-byte avg entry |
| Throughput (10 writers via DOM Phase 3) | > 50,000 commits/s | Aggregated via DOM (RFC-0857) — Phase 3 |
| **First-time snapshot sync (1 GB)** | < 60 s | LZ4, single parallel stream, ≥ 17 MB/s available bandwidth (typical residential broadband) |
| First-time snapshot sync (10 GB) | < 10 min | 4 parallel streams, ≥ 17 MB/s |
| Catch-up after 1 min partition | < 5 s | Anti-entropy Merkle descent, no snapshot re-ship |
| Catch-up after 1 hr partition | < 10 min | Snapshot re-ship from oldest LSN on disk |
| Heartbeat payload | ~ 64 bytes per heartbeat | Envelope overhead (~ 256 bytes) + Heartbeat (64 bytes) + LSN-watermark sample (8 bytes) = ~ 328 bytes per 5s |
| Control plane budget | < 1% of bandwidth | At 5K commits/s, 200-byte avg entry, the data plane is 1 MB/s. Heartbeats at 5s/heartbeat = 328 B / 5s = 66 B/s ≈ 0.007% of data plane. (328 B = 256-byte envelope overhead from §8 wire overhead row + 64-byte heartbeat payload + 8-byte LSN-watermark sample.) |
| **Memory overhead (Sync engine total)** | < 50 MB per peer | ReplayCache (10K envelopes × ~ 5 KB per envelope = 50 MB max, default 10K) + dedup cache (10K LSNs × 16 bytes = 160 KB) + in-flight segment buffers (default 0, lazy) + Sync engine state (negligible) |
| **Wire overhead per envelope** | 250–300 bytes | DOT envelope header (~ 100 bytes) + OCrypt overhead (12-byte nonce + 16-byte Poly1305 tag + 32-byte sender_ephemeral = 60 bytes) + Ed25519 signature (64 bytes) + Sync header (~ 32 bytes) = ~ 256 bytes. The "200 bytes" in v1.0 of this research was an underestimate; realistic is 250–300 bytes per envelope. |

---

## 9. Cross-References and Reuse Map

| Need | Reuse | Notes |
| --- | --- | --- |
| Envelope wire format | `RFC-0850` `DeterministicEnvelope` | Unchanged. New envelope payload discriminators `0xA0–0xC2` (see §4.2). |
| Fragmentation | `RFC-0850` `EnvelopeFragment`, `DOT/F/...` | Reuse for large segments. |
| Replay protection | `RFC-0850` `ReplayCache` (`BTreeMap<envelope_id, first_seen>`) | Reuse; snapshot to disk for restart survival. |
| Logical timestamps | `RFC-0850` `(epoch, monotonic_counter, gateway_id)` | Reuse. |
| Platform types | `RFC-0850 §3.1` (Broadcast Domain) 21 types | Primary: NativeP2P (libp2p gossipsub, `0x000A`). Alternative primary: QUIC (`0x0015`, profile in §8.7). Fallback: Webhook. Best-effort: Telegram/Discord/Matrix. |
| Discovery | `RFC-0851` `GatewayAdvertisement` | Reuse. Sync-capable peers advertise in `capabilities_root` (new bit `SyncCapable = 0x0020`; see §10.3). |
| Bootstrap | `RFC-0851p-a` 3 modes | Reuse for first-time connection. |
| Anti-entropy | `RFC-0852 §7` `GossipStateSummary` | Adapt to per-table segment Merkle tree. |
| Gossip | `RFC-0852` `GossipObject` `object_type = 0x0008 SnapshotFragment` | Specify the fragment format (currently reserved but not defined). |
| Bloom-filter compression | `RFC-0852 §11` | Implement. |
| Encryption | `RFC-0853` `OCrypt`, `MissionKeyHierarchy` | Reuse. New context: `"sync:v1"`. |
| Identity | `RFC-0853` `OverlayIdentity` | Reuse. `NodeId = BLAKE3(OverlayIdentity.public_key || mission_id)`. |
| Mission lifecycle | `RFC-0855` | Sync uses new `Role::Replicator` (v1, immediate; see §10.3). |
| Roles and Authorities | `RFC-0855 §4.2` | New `Replicator` role added to the 8-role list. |
| Coordinator | `RFC-0855p-b/0855p-c` | The writer is a `DomainCoordinator` (per `RFC-0855p-c`); readers are `Observer`s. |
| Route selection | `RFC-0856` DRS | Choose peers for sync by `(trust, bandwidth, latency)` — same as gossip. |
| Mempool | `RFC-0857` DOM | Future: ship batches via DOM intents (Phase 3). |
| Onion routing | `RFC-0858` ORR | Optional, for PRIVATE missions. |
| Proof carrying | `RFC-0859` PCE | Optional, for proof-of-sync (F3 future work). |
| Proof of relay | `RFC-0860` PoRelay | Use to score peers by sync reliability (composite = `(forwarding * WF + availability * WA + bandwidth * WB + uptime * WU + diversity * WD) * stake_multiplier / 1000` with `WF=300, WA=250, WB=200, WU=150, WD=100`). |
| Stoolap WAL | `stoolap/src/storage/mvcc/wal_manager.rs` | Source of truth on the writer; replay on the reader (V2 binary, 3,773 lines). |
| Stoolap per-table snapshots | `stoolap/src/storage/mvcc/snapshot.rs` `create_snapshot()` | Per-DSN-path: `<dsn-path>/snapshots/<table>/snapshot-<ts>.bin`; magic `"STSVSHD"`. |
| Stoolap snapshot metadata | `stoolap/src/storage/mvcc/engine.rs` | Magic `"SNAP"` (0x50414E53). |
| Stoolap commit hook | `stoolap/src/storage/mvcc/transaction.rs` `TransactionEngineOperations::record_commit(txn_id)` | Single chokepoint to capture LSN range. |
| Stoolap deterministic types | `stoolap/src/determ/` `DetermValue`, `DetermRow` | Wire format for row payloads when Class A required. |
| Stoolap pub-sub | `stoolap/src/pubsub/{event_bus,wal_pubsub,traits}.rs` | Use `EventPublisher` to surface sync events locally. **Namespace separation**: `WalPubSub` uses `pubsub-wal-*.log` files (event namespace) and is the cross-process local-FS cache-invalidation channel. Sync uses `state/sync-watermarks.bin` and `state/sync-replay-cache.bin` (sync namespace) and is the cross-node network-based data-transfer channel. The two are disjoint on disk and in memory. Cross-process `WalPubSub` events (e.g., `KeyInvalidated`) are NOT shipped over Sync; Sync only ships the `Database` state itself, not the pub-sub event stream. |
| RFC-0104 (DFP) | `stoolap/Cargo.toml:182, 212-213, 215-228` build profile settings | All sync code MUST inherit these (`inherits = "release"`, `codegen-units = 1`, `lto = true`, `overflow-checks = false`, `panic = "abort"`, `-C target-feature=-fma` via RUSTFLAGS). |
| RFC-0126 (DCS) | Canonical serialization | Use for all wire structs. |
| RFC-0008 (Determinism Boundary) | Class A for wire, Class B for transport/retry, Class C for diagnostics | See §4.4 mapping table. |

---

## 10. Proposed RFC Numbering and Mission Decomposition

The networking category is 0800–0899; storage is 0200–0299. This protocol sits at the boundary. Two reasonable numberings:

| Option | RFC | Path | Justification |
| --- | --- | --- | --- |
| **A** | `RFC-0210` | `rfcs/draft/storage/0210-stoolap-data-sync.md` | It is primarily a *storage* protocol; the network is just a transport. Aligns with `RFC-0200` (vector-sql storage) and `RFC-0201` (BLOB) which already reference replication in passing. |
| **B** | `RFC-0862` | `rfcs/accepted/networking/0862-stoolap-data-sync.md` | It is primarily a *networking* sub-protocol that uses Stoolap as one possible storage backend. Aligns with `RFC-0861` (the most recent networking RFC, all 17 findings closed). **Status: Accepted 2026-06-20** (post 12-round adversarial review; 60 findings resolved). |

**Recommendation: Option B (`RFC-0862`)** because (a) the network team owns this part of the stack and they have an active RFC reviewer for `0861`, (b) the protocol is reusable for any compatible storage backend, not Stoolap-specific, and (c) it preserves the existing storage RFCs (0200–0204) for the storage-engine concerns they already address. The cross-reference is added to `RFC-0200` "Raft Log Replication Spec" (body section, line 1821-1997) and the brief §A "Replication Model" table (line 2640) to point at the new RFC.

### 10.1 Base mission

`missions/open/0862-stoolap-data-sync-base.md`

Acceptance criteria:

- [ ] Two `stoolap::Database` instances can be opened and a `SyncTransport` initialized between them.
- [ ] A write on Node A is observable on Node B within 1s (LAN).
- [ ] `Database::query("SELECT COUNT(*) FROM ...", ())` returns the same result on both nodes.
- [ ] Kill Node A mid-write; on restart, sync resumes from last `LsnAck`.
- [ ] All wire bytes are byte-equal across two independent builds (CI gate).
- [ ] 100% of `WALEntry` variants round-trip through sync.

Type coverage:

| RFC type | Implemented by |
|----------|---------------|
| `SyncEnvelopeType` enum | This mission |
| `SyncSummary` struct | This mission |
| `SyncSegment` struct | This mission |
| `WalTailChunk` struct | This mission |
| `NodeStatus` struct | This mission |
| `NodeId`, `PeerId` | This mission |
| `SyncTransport` trait (stoolap) | This mission |
| `SyncEngine` struct (cipherocto) | This mission |

### 10.2 Sub-missions

| Mission | Purpose | Dependencies |
|---------|---------|--------------|
| `0862a-stoolap-data-sync-wal-tail.md` | WAL-tail streaming (Approach B) | base |
| `0862b-stoolap-data-sync-merkle-summary.md` | Per-table segment Merkle summary + anti-entropy handshake (Approach D) | base |
| `0862c-stoolap-data-sync-snapshot-segment.md` | Snapshot segment request/response, LZ4, CRC32, parallel download | 0862b |
| `0862d-stoolap-data-sync-ocrypt-bind.md` | OCrypt integration, mission key derivation, AAD binding, AuthChallenge/AuthResponse | base |
| `0862e-stoolap-data-sync-replay-cache-persistence.md` | Persist `ReplayCache` to disk for restart survival | base |
| `0862f-stoolap-data-sync-multi-peer.md` | N readers via DGP `GossipObject` `object_type=0x0008 SnapshotFragment` | 0862a, 0862b, 0862c |
| `0862g-stoolap-data-sync-cross-carrier.md` | Multi-carrier propagation (NativeP2P + Webhook + one social adapter) | 0862f |
| `0862h-stoolap-data-sync-property-tests.md` | Property tests (LSN monotonicity, heartbeat-loss, key-isolation, rate-limit) | 0862a, 0862b, 0862c |
| `0862i-stoolap-data-sync-raft-overlay.md` | (Phase 4 future) Raft/Paxos overlay for quorum replication | 0862f |

Total: 1 base + 9 sub-missions = **10 missions**.

### 10.3 Cross-RFC impact

The changes below are **proposed amendments**, not 1-line additions. Each is described in the size of the change it actually represents:

- `RFC-0850`: no protocol change. New envelope payload discriminators `0xA0–0xC2` (see §4.2). **No wire-format change.**
- `RFC-0851`: amend `GatewayAdvertisement.capabilities_root` to include a new bit `SyncCapable = 0x0020`. This is **one bit in a Merkle tree leaf**, not a one-line addition. Requires regenerating the capabilities root for every existing gateway.
- `RFC-0852`: specify the `SnapshotFragment` (0x0008) format. Replace the placeholder "Chunk of state snapshot" with the `SyncSummary` + `SyncSegment` spec. New section (§11.5 or similar) of about 200-300 lines.
- `RFC-0853`: no change. Reuse `MissionKeyHierarchy.execution_keys_root` with a new HKDF context `"sync:v1"`. To be documented in §6 (Mission Cryptography) of RFC-0853, alongside the existing `ocrypt:mission:execution:v1` and related mission contexts (the current §10 "Onion Relay Extension" of RFC-0853 is not the right location).
- `RFC-0855`: add a new membership role `Replicator` to the 8-role list in **§4.2 (Roles and Authorities)** at `RFC-0855:397-406`. Requires updating the role constraints table, the dual-stake requirements table, and the role-flag bitmask. Multi-line addition.
- `RFC-0860`: add a new forwarding proof variant `SyncForwardingProof` that scores a peer by `(delivered_segments, lsn_freshness, retry_rate)`. This becomes the basis for slash code `0x0010` (`sync_peer_misbehavior`). New struct + new verification logic. ~100 lines.
- `RFC-0200`: add a forward reference in §A "Replication Model" (line 2640) pointing at the new RFC. Remove the "Recommendation: Start with Raft" sentence (replaced by a pointer to RFC-0862 for protocol details). **Two-line edit.**
- `RFC-0126`: no change. Sync wire structs use DCS.

### 10.4 What about Raft?

`RFC-0200` "Raft Log Replication Spec" body section (line 1821-1997) sketches Raft at length, and the brief §A "Replication Model" table (line 2640) recommends "Start with Raft for strong consistency". The user's request is for *data sync between two nodes* — which does not require Raft. Raft is for **quorum** (3+ nodes with a single leader and majority-acknowledged commits). Two-node sync can be:

- **Active-passive** (one writer, one or more readers, no quorum needed) — what this research recommends for v1.
- **Active-active with conflict resolution** (multi-leader, LWW or CRDT) — explicitly out of scope (DGP rejects CRDTs per `RFC-0852 §Alternatives Considered`).
- **Raft (or Paxos)** — orthogonal; can be layered on top of the Sync protocol as a future mission. RFC-0740 (`CrossShardMessage::StateSync`) sketches the cross-shard analog.

Alternatives that should also be considered (deferred to F1):

- Chain replication.
- Primary-backup with log shipping (essentially what v1 is, but formalized).
- Quorum-based replication (3+ nodes, Raft/Paxos).
- Byzantine Fault Tolerant replication (e.g., HotStuff).

**Recommendation: do not require Raft for the two-node feature.** Add `0862i-stoolap-data-sync-raft-overlay.md` (Phase 4, listed in §10.2) for the quorum case.

---

## 11. Next Steps

### 11.1 Create a Use Case

1. **Create a Use Case** at `docs/use-cases/stoolap-data-sync-via-cipherocto-network.md` (the "WHY" layer). The Use Case should:
   - Reference this research and the existing `dot-network-bootstrap.md` and `stoolap-integration-research.md` use cases.
   - **Follow the BLUEPRINT Use Case template** (`BLUEPRINT.md` lines 311-358) which lists 8 sections: Problem, Stakeholders, Motivation, Success Metrics, Constraints, Non-Goals, Impact, Related RFCs.
   - **Also follow the repo's de facto pattern** (per `dot-network-bootstrap.md:99-126` and `stoolap-only-persistence.md`) by adding "Pipeline Position" and "Related Missions" sections (these are not in the BLUEPRINT template but are the established pattern in this repo).
   - Define the stakeholders, success metrics (latency, throughput, determinism), constraints, and non-goals.
   - List the related RFCs (`RFC-0850`, `RFC-0851`, `RFC-0851p-a`, `RFC-0852`, `RFC-0853`, `RFC-0855`, `RFC-0860`, plus the new `RFC-0862`).
   - Pipeline position: `Use Case (this) → RFC-0862 (DESIGN) → 0862 base mission (0862-base) + 9 sub-missions 0862a through 0862i (EXECUTION)`.

### 11.2 Draft the RFC

2. **Wait for Use Case acceptance**, then draft `RFC-0862` in `rfcs/draft/networking/0862-stoolap-data-sync.md` per the BLUEPRINT RFC template v1.3 (`BLUEPRINT.md` lines 472-744 — the RFC template ends before line 745 where the RFC process section begins). Required sections: Summary, Dependencies (RFC-0850, 0851, 0852, 0853, 0126, 0104), **Design Goals (G1–G6) and Operational Requirements (G7–G8) — see §4.1**, Motivation, Roles and Authorities table, Specification (envelope types, SyncSummary, SyncSegment, WalTailChunk, NodeStatus, identity, key hierarchy), Lifecycle Requirements (for the 7-state per-peer state machine — see §4.1 G7-G8 note: 7 states because the Sync state machine does not transition through `Handover`), Determinism Requirements, RFC-0008 Execution Class Mapping (see §4.4), Error Handling, Performance Targets (see §8), Implicit Assumptions Audit (see §4.6), Security Considerations, Adversary Analysis (5-Question Test, see §4.5), Compatibility (see §4.7), Test Vectors, Alternatives Considered (5 approaches, see §3), Implementation Phases (4 phases, see §6), Key Files to Modify, Future Work (F1–F10 — see §11.8), Rationale, Version History, Related RFCs, Related Use Cases, Appendices.

### 11.3 Open missions

3. **After RFC acceptance**, open the base mission `missions/open/0862-stoolap-data-sync-base.md` and the 9 sub-missions from §10.2.

### 11.4 Adversarial review

4. **Adversarial review** (BLUEPRINT §"Adversarial Review Process"): after the RFC is Draft, run a 2-round review (Round 1: initial; Round 2: post-fix verification) and grade all findings by the CRITICAL/HIGH/MEDIUM/LOW table in BLUEPRINT. The 5-Question Adversary Test (§4.5 above) is the rubric.

### 11.5 Cross-RFC consistency check

5. **Cross-RFC consistency check** (BLUEPRINT §"Cross-RFC Consistency"): verify the dependency graph is a DAG, the mission prerequisites match the RFC `Requires` section, and the §4.7 Compatibility claims are true.

### 11.6 Implementation in the Stoolap fork

6. **Implementation in `stoolap` fork**:
   - Add `tokio` to `Cargo.toml` as an **optional** dep behind a new feature `sync` (see §R-10 mitigation).
   - `blake3` and `lz4_flex` are already in the existing `Cargo.toml:74, 111`.
   - Add `stoolap-sync` as a new crate under `crates/stoolap-sync/` (preferred, isolates the optional async dep) **or** as a new module `src/sync/` in the main crate (acceptable but increases compile times for non-sync users).
   - Add `octo-sync` as a new crate under `crates/octo-sync/` in cipherocto, depending on `octo-network` and `octo-determin`.
   - Re-export `SyncTransport` and `SyncConfig` from the top-level `stoolap` crate when the `sync` feature is enabled.

### 11.7 Documentation

7. **Documentation** (split between repos):
   - **cipherocto repo** (this repo): `docs/use-cases/stoolap-data-sync-via-cipherocto-network.md` (the Use Case) and `rfcs/draft/networking/0862-stoolap-data-sync.md` (the RFC) — per BLUEPRINT workflow.
   - **stoolap fork** (`/home/mmacedoeu/_w/databases/stoolap`): `docs/sync.md` (user guide) covering enabling the feature, configuration, operator runbook; `examples/sync_two_nodes.rs` (the "hello world" two-node demo); update `ROADMAP.md` to mark Phase 3 (Network Protocol & Gossip) as **IN PROGRESS** with a link to the cipherocto research.

### 11.8 Open follow-up research items

8. **Open follow-up research** items (track in `docs/research/followups.md`):
   - **F1 — Multi-leader / active-active.** Investigate how to extend Sync with conflict resolution. Candidates: (a) per-row HLC + LWW, (b) move to a Raft/Paxos overlay (per `RFC-0200` body section, line 1821-1997), (c) restricted to specific table groups. **Note**: the §5 R-20 entry (originally labeled "F2 future work" for the `Replicator` role in an earlier draft) is incorrect — `Replicator` is a v1 role (immediate change to RFC-0855 §4.2).
   - **F2 — Trust-anchored storage checkpoint.** Mirror the RFC-0851p-a §6 Sybil-Eclipse Defense (line 365) "genesis checkpoint from CipherOcto website" pattern (referenced in the §5 Mode C Invite Link / §6 Sybil-Eclipse Defense table) for *storage* checkpoints. Without this, a brand-new node must trust the first peer it meets.
   - **F3 — Proof-of-sync.** Use RFC-0859 (PCE) to attach a ZK proof of state equivalence to a `SnapshotResponse`. Useful for "I just received a snapshot, here is the proof it matches the published state root." Requires STWO integration.
   - **F4 — ZK proof of state equivalence.** A zero-knowledge proof that two Stoolap states are equivalent. Composes with the existing `HexaryProof` and the L2 rollup module.
   - **F5 — Cairo/Move port of the Sync protocol.** The Cairo programs in the **stoolap fork's `cairo/`** directory (`hexary_verify.cairo`, `merkle_batch.cairo`, `state_transition.cairo`) already exist; port the Sync protocol to a Cairo implementation and test interop. (Note: `cipherocto/cairo/` does **not** exist; F5 was originally misreferenced.)
   - **F6 — Sync on a public network.** Investigate bandwidth, cost, and Sybil-resistance implications of running Sync over a high-cost public carrier (e.g., SMS, voice).
   - **F7 — Cross-`Database` flavor sync.** Investigate whether Sync can be extended to other forks of Stoolap (e.g., a future PostgreSQL-compat mode).
   - **F8 — Writer election / auto-failover.** v1 has no failover (operator must reconfigure `writer_node_id` on reader). F8 adds automatic failover via the `DomainCoordinator` handover protocol (RFC-0855p-c).
   - **F9 — Schema migration protocol.** v1 aborts on schema-version mismatch. F9 specifies a coordinated migration protocol (e.g., reader rejects write that introduces a new column not in reader's schema; operator must run a separate migration tool first).
   - **F10 — Reed-Solomon erasure coding for first-time sync.** RFC-0742 already specifies Reed-Solomon for data availability. F10 investigates whether RS chunks across multiple peers can speed up first-time snapshot sync (e.g., 10 peers each hold 1/10 of the encoded data, reader fetches 6-of-10 to reconstruct). **v1 uses per-segment download only.**

---

## 12. References

### CipherOcto documents

- [BLUEPRINT.md](../BLUEPRINT.md) — process architecture
- [Use Case: DOT Network Bootstrap](../use-cases/dot-network-bootstrap.md) — the closest existing "first network operation" use case
- [Use Case: Stoolap-only Persistence](../use-cases/stoolap-only-persistence.md) — single-node Stoolap commitment
- [Use Case: Verifiable Agent Memory Layer](../use-cases/verifiable-agent-memory-layer.md) — memory layer
- [Use Case: Data Marketplace](../use-cases/data-marketplace.md) — data trading
- [Research: Stoolap Research](stoolap-research.md) — original Stoolap catalogue
- [Research: Stoolap Integration](stoolap-integration-research.md) — Stoolap × AI Quota Marketplace
- [Research: Stoolap Determinism](stoolap-determinism-analysis.md) — RFC-0104 compliance
- [Research: Deterministic Overlay Transport](deterministic-overlay-transport.md) — 6,273-line design source
- [Research: Networking RFC Cross-Reference Analysis](networking-rfc-cross-reference-analysis.md) — RFC audit
- [Research: Social Platform Transport Patterns](social-platform-transport-patterns.md) — adapter pattern analysis
- [Research: Group Coordination Transport Adapters](group-coordination-transport-adapters.md) — adapter trait

### CipherOcto RFCs (existing, relied upon)

- [RFC-0126 (Numeric): Deterministic Serialization](../../rfcs/accepted/numeric/0126-deterministic-serialization.md) (canonical encoding)
- [RFC-0850 (Networking): Deterministic Overlay Transport](../../rfcs/accepted/networking/0850-deterministic-overlay-transport.md) (transport)
- [RFC-0851 (Networking): Gateway Discovery Protocol](../../rfcs/accepted/networking/0851-gateway-discovery-protocol.md) (discovery; 5-state `DiscoveryLifecycle`)
- [RFC-0851p-a (Networking): Network Bootstrap Protocol](../../rfcs/accepted/networking/0851p-a-network-bootstrap.md) (bootstrap; 7-state `BootstrapClientLifecycle`)
- [RFC-0852 (Networking): Deterministic Gossip Protocol](../../rfcs/draft/networking/0852-deterministic-gossip-protocol.md) (gossip, anti-entropy; `SnapshotFragment = 0x0008`)
- [RFC-0853 (Networking): Overlay Cryptography](../../rfcs/draft/networking/0853-overlay-cryptography.md) (crypto; `OverlayIdentity`, `MissionKeyHierarchy`)
- [RFC-0855 (Networking): Mission Overlay Networks](../../rfcs/accepted/networking/0855-mission-overlay-networks.md) (missions; 6 topology models, 5 governance, 8 roles)
- [RFC-0855p-b (Networking): Mission Coordinator Lifecycle](../../rfcs/accepted/networking/0855p-b-coordinator-lifecycle.md) (lifecycle; 8-state `CoordinatorLifecycle`)
- [RFC-0855p-c (Networking): Domain Coordinator Role](../../rfcs/accepted/networking/0855p-c-domain-coordinator-role.md) (DC; 90-epoch platform-loss window)
- [RFC-0856 (Networking): Deterministic Route Selection](../../rfcs/draft/networking/0856-deterministic-route-selection.md) (routing)
- [RFC-0857 (Networking): Deterministic Overlay Mempool](../../rfcs/draft/networking/0857-deterministic-overlay-mempool.md) (mempool)
- [RFC-0858 (Networking): Onion Relay Routing](../../rfcs/draft/networking/0858-onion-relay-routing.md) (onion)
- [RFC-0859 (Networking): Proof-Carrying Envelopes](../../rfcs/draft/networking/0859-proof-carrying-envelopes.md) (proofs)
- [RFC-0860 (Networking): Proof-of-Relay](../../rfcs/draft/networking/0860-proof-of-relay.md) (trust; composite scoring `composite = (forwarding * WF + availability * WA + bandwidth * WB + uptime * WU + diversity * WD) * stake_multiplier / 1000` with `WF=300, WA=250, WB=200, WU=150, WD=100`)
- [RFC-0861 (Networking): CoordinatorAdmin Trait Refinements](../../rfcs/accepted/networking/0861-coordinator-admin-trait-refinements.md) (most recent, 17 findings closed, 1,373 tests passing)
- [RFC-0200 (Storage): Production Vector-SQL Storage Engine v2](../../rfcs/draft/storage/0200-production-vector-sql-storage-v2.md) (the body-section Raft sketch at line 1821-1997 and brief §A Replication Model at line 2640 that we are superseding)
- [RFC-0201 (Storage): Binary BLOB Type Support](../../rfcs/accepted/storage/0201-binary-blob-type-support.md)
- [RFC-0740 (Consensus): Sharded Consensus Protocol](../../rfcs/draft/consensus/0740-sharded-consensus-protocol.md) (cross-shard `StateSync`)
- [RFC-0742 (Consensus): Data Availability & Sampling](../../rfcs/draft/consensus/0742-data-availability-sampling.md) (DAS)

### Stoolap fork files

- `stoolap/Cargo.toml` — crate manifest (zero network deps today); `octo-determin` at line 55
- `stoolap/src/api/database.rs` — `Database::open(dsn)`, `execute`, `query`, `transaction`, `create_snapshot`
- `stoolap/src/storage/mvcc/wal_manager.rs` — V2 binary WAL with LSN + CRC32 (3,773 lines); `WAL_HEADER_SIZE: u16 = 32` at line 72
- `stoolap/src/storage/mvcc/snapshot.rs` — per-table snapshot files with `"STSVSHD"` magic (line 37, 98)
- `stoolap/src/storage/mvcc/engine.rs` — `MVCCEngine::create_snapshot` (line 2642), atomic-rename at line 2828, `find_safe_truncation_lsn` (line 291); `SNAPSHOT_META_MAGIC: u32 = 0x50414E53` at line 153 (metadata only, NOT per-table)
- `stoolap/src/storage/mvcc/persistence.rs` — `PersistenceManager::replay_two_phase` (at line 549), snapshot metadata, replay
- `stoolap/src/storage/mvcc/transaction.rs` — `TransactionEngineOperations::record_commit(txn_id)` commit hook
- `stoolap/src/storage/mvcc/timestamp.rs` — monotonic `get_fast_timestamp`
- `stoolap/src/storage/traits/{engine,transaction,table,index_trait,scanner}.rs` — extension traits
- `stoolap/src/pubsub/{event_bus,wal_pubsub,traits}.rs` — pub-sub / cross-process primitives (file-based, NOT network)
- `stoolap/src/executor/{mod,context}.rs` — `ExecutionContext::event_publisher` plug point
- `stoolap/src/consensus/{operation,block}.rs` — Operation/Block encoding (data-only, not wired)
- `stoolap/src/determ/{value,row,collections}.rs` — deterministic wire types
- `stoolap/src/rollup/{types,execution,submission,fraud,withdrawal}.rs` — L2 rollup data types
- `stoolap/cairo/` — 3 Cairo programs (`hexary_verify.cairo`, `merkle_batch.cairo`, `state_transition.cairo`); used by F5 follow-up

---

## 13. Status & Decision Request

**Status:** Draft v2.0 (post Round 10 adversarial review — 1 pre-existing LOW from R10 resolved; see `docs/reviews/stoolap-data-sync-research-adversarial-review-r10.md`; awaiting Round 11 verification). Awaiting Use Case creation per BLUEPRINT §Canonical Workflow.

**Decision requested from CipherOcto maintainers:**

1. **Approve the research** (this document) as the basis for the next step.
2. **Choose the RFC number**: `RFC-0210` (Storage) or `RFC-0862` (Networking) — recommendation is `RFC-0862` per §10 rationale.
3. **Confirm the next-step action**: create a Use Case at `docs/use-cases/stoolap-data-sync-via-cipherocto-network.md` referencing this research.
4. **Adversarial review**: nominate at least 2 maintainers for the eventual 2-round review of the RFC and the 5-Question Adversary Test of the §4.5 table.
5. **Cross-RFC impact review**: confirm the §10.3 changes to existing RFCs are acceptable (these are **proposed amendments, not 1-line additions** — see §10.3 for the actual size of each change).
6. **Resolve the slash-code contradiction** (§1.2): `RFC-0851p-a` claims `0x000D = bootstrap_node_misbehavior`; `RFC-0850p-c` claims `0x000C-0x000D` are reserved for sub-DC delegation. The two RFCs disagree; this research uses the `0851p-a` interpretation but flags the contradiction for maintainer resolution.

---

**Version:** 2.0 (post Round 10 review)
**Date:** 2026-06-20
**Next review:** Round 11 of the adversarial review (see `docs/reviews/stoolap-data-sync-research-adversarial-review-r{1,2,3,4,5,6,7,8,9,10}.md` for prior findings).
