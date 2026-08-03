# Use Case: Two-Node Data Synchronization for the Stoolap Fork via the CipherOcto Network

**Date:** 2026-06-20
**Status:** Draft v1.8 (post Round 8 adversarial review — 1 LOW from R8 resolved; see `docs/reviews/stoolap-data-sync-use-case-adversarial-review-r8.md`; awaiting Round 9 verification)

---

## Problem

The Stoolap fork (at `/home/mmacedoeu/_w/databases/stoolap`) is a complete embedded SQL database with MVCC transactions, HNSW vector search, AS OF time-travel, a binary WAL with LSN, snapshot persistence, and an event publisher trait. However, the fork has **zero networking code** — no TCP, no UDP, no libp2p, no async runtime (`stoolap/Cargo.toml:36-131`; the only network-adjacent code is `libc::flock` for cross-process file locking at `src/storage/mvcc/file_lock.rs:129`). The fork's own `ROADMAP.md` lists Phase 3 "Network Protocol & Gossip" as DRAFT and unimplemented (the corresponding network-protocol RFC is not yet written in `stoolap/rfcs/`).

Without a Sync protocol, operators of the fork can only synchronize data between two `Database` instances by copying files out-of-band. The CipherOcto network (in this repo) already provides a complete overlay transport stack — `RFC-0850` (Deterministic Overlay Transport), `RFC-0852` (Deterministic Gossip Protocol), `RFC-0853` (Overlay Cryptography), `RFC-0855` (Mission Overlay Networks) — but **no RFC specifies the wire-level protocol for synchronizing application-level database state between two nodes**. The closest sketches are: (a) a `catch_up` pseudocode fragment in `RFC-0200` body section (line 1821-1997) with no wire format, no RFC number, and no mission; and (b) the DGP anti-entropy Merkle-descent sketch in `RFC-0852 §7` (scoped to *overlay* state, not application storage).

The research `docs/research/stoolap-data-sync-via-cipherocto-network.md` (v2.0, 968 lines, post Round 10 adversarial review; Round 11 verification pending at time of writing) investigates whether — and how — these two pieces can be combined to deliver a *first-class* two-node data synchronization feature for the fork.

## Stakeholders

- **Primary:** Stoolap fork operators (data engineers, AI/agent backend developers, decentralized-app developers, embedded-system integrators) who need to replicate state between two running `Database` instances.
- **Secondary:** CipherOcto network operators (gateway runners, mission maintainers) and DOT platform adapter maintainers; Stoolap fork contributors who integrate the Sync API.
- **Affected:** End users of any downstream service that depends on a synchronized Stoolap view (e.g., the AI Quota Marketplace, agent memory layers, verifiable data marketplace).

## Motivation

### Why Two-Node Sync Matters

A two-node data synchronization feature turns the fork from a single-process embedded database into a **node in a CipherOcto network**. This unlocks:

1. **High availability.** Read replicas, failover, disaster recovery across geographies.
2. **Horizontal scale.** Distribute read traffic across mirrors; aggregate writes to a coordinator.
3. **Disconnected operation.** Nodes write locally, sync when reconnected (DGP's anti-entropy model).
4. **Cryptographic provenance.** Every replicated byte is OCrypt-signed and replay-protected (`RFC-0853`).
5. **Cross-carrier delivery.** The same sync stream can ride NativeP2P (libp2p gossipsub, `RFC-0850 §3.1` `0x000A`), QUIC (`0x0015` profile in `§8.7`), Webhook, or a social adapter (Telegram/Discord/Matrix) — the same multi-carrier abstraction DOT already provides.
6. **Deterministic convergence.** Under the DGP anti-entropy rule, any two nodes with the same operation set reach the same state regardless of arrival order, because the operation order is canonical (LSN, then table id, then row id, then op) and the values are DCS-encoded (`RFC-0126`).

### Why Now

The fork already ships every local primitive required for Sync (per the research §1.1 substrate map): V2 binary WAL with LSN + CRC32 (`wal_manager.rs:72` `WAL_HEADER_SIZE: u16 = 32`), per-table snapshot files with atomic-rename (`engine.rs:2642` `MVCCEngine::create_snapshot`), the `TransactionEngineOperations::record_commit(txn_id)` commit hook, the `determ::DetermValue`/`DetermRow` deterministic types, and the `octo-determin` dependency already linked from this repo (`stoolap/Cargo.toml:55`). The CipherOcto network already has DOT envelopes with 21 platform types, OCrypt with per-mission key hierarchies, DGP with anti-entropy Merkle summaries, and PoRelay for trust scoring. The missing piece is **one RFC** that defines the Sync sub-protocol's envelope types, identity derivation, key hierarchy, and state-transition function.

### Why This Use Case Over a "Roll Our Own" Replication

The Stoolap fork could implement replication using off-the-shelf libraries (e.g., `raft-rs`, `rqlite`). The research's §3 approach analysis rejected this because: (a) the user explicitly requested the CipherOcto network as the transport; (b) the fork is currently synchronous with no `tokio` dependency, and pulling in a full Raft crate forces async I/O on the entire user base; (c) the CipherOcto stack already provides multi-carrier propagation, mission-scoped key isolation, and proof-of-relay trust scoring that no off-the-shelf library provides.

## Success Metrics

### Runtime metrics (measured in production-like deployments)

| Metric | Target | Measurement |
| ------ | ------ | ----------- |
| End-to-end replication latency (one-way, LAN) | < 50 ms p50, < 200 ms p99 | Bench harness: writer commits 1 KB, reader observes via `BLAKE3-256(SELECT * FROM table)` |
| End-to-end replication latency (one-way, WAN) | < 500 ms p99 | Same harness, WAN emulator with 100 ms RTT |
| Throughput (single writer) | > 5,000 commits/s | WAL streaming, batched, 200-byte avg entry, `SyncMode::Normal` |
| First-time snapshot sync (1 GB) | < 60 s | LZ4, single parallel stream, ≥ 17 MB/s available bandwidth |
| Catch-up after 1 min partition | < 5 s | Anti-entropy Merkle descent, no snapshot re-ship |
| Catch-up after 1 hr partition | < 10 min | Snapshot re-ship from oldest LSN on disk |
| Wire overhead per envelope | 250–300 bytes | DOT header (100) + OCrypt (60) + Ed25519 signature (64) + Sync header (32) |
| Memory overhead (Sync engine per peer) | ≤ 50 MB | ReplayCache (10K × ~5 KB = 50 MB max) + dedup cache (160 KB) + in-flight buffers (lazy); the `< 50 MB` target uses **decimal** MB (1 MB = 1,000,000 bytes); the 50 MB ReplayCache + 160 KB dedup cache sums to ~50.16 MB in steady state, which is **at the target limit, not below** — operators may need to reduce ReplayCache to 9,000 envelopes in tight-memory deployments. |
| Cross-implementation determinism | 100% byte-exact | CI gate: Linux x86_64 and macOS arm64 builds produce identical wire bytes |

### Process goals (per BLUEPRINT workflow)

| Goal | Target |
| ---- | ------ |
| `RFC-0862` adversarial review | Round 2 of BLUEPRINT adversarial review process (R1 = initial; R2 = post-fix verification; 0 CRITICAL/HIGH after R2) |
| `RFC-0862` acceptance | 2 maintainer approvals, no blocking objections, per BLUEPRINT §"RFC Acceptance Process" |

## Constraints

- **Must not** break the existing single-process `Database::open(dsn)` API or change WAL file format compatibility (V2 is stable per `wal_manager.rs:72`).
- **Must not** require `tokio` as a hard dependency on the fork's main crate. The Sync transport lives in a separate `stoolap-sync` crate with its own `tokio` dep; the core `stoolap` crate remains synchronous. The `SyncTransport` trait in the core crate is sync (blocking I/O); the `stoolap-sync` crate provides an async facade.
- **Must** preserve the fork's determinism invariants (RFC-0104): DFP arithmetic via `octo-determin`, software-emulated ordering, no FMA (`-C target-feature=-fma` per `stoolap/Cargo.toml:182, 212-213`), fixed encoding.
- **Must** ride the existing DOT envelope wire format (`DOT/1/{base64}` / `DOT/2/{msg_id}` / `DOT/F/{base64_frag}` / `RAW/{binary}`) without modifying `RFC-0850`.
- **Must** be replay-safe across all RFC-0008 Class A boundaries (the wire protocol and the resulting state) — `ReplayCache` (RFC-0850) + per-peer LSN watermark + OCrypt replay cache (1h or 10K entries per `RFC-0853 §7`).
- **Must** be wire-compatible with OCrypt encryption when the mission is configured PRIVATE (no plaintext leak).
- **Must** round-trip 100% of the 22 `WALOperationType` variants (`wal_manager.rs:163-187`) through the Sync layer.
- **Limited to:** single-leader topology in v1. v1 is single-writer (one writer node, N read-replicas). Multi-leader is `F1` future work. Raft/Paxos overlay is a sub-mission (`0862i`) deferred beyond Phase 4 (F1 future work, not part of the v1–v4 phased rollout).
- **Limited to:** mission scope only. Sync runs within a single mission. PRIVATE missions are encrypted via OCrypt (RFC-0853); PUBLIC missions are in clear. Cross-mission sync is out of scope.
- **Limited to:** NativeP2P (libp2p gossipsub, `0x000A`) primary + QUIC (`0x0015`) alternative + Webhook fallback. Multi-carrier is `0862g` (sub-mission per §Related Missions).
- **Note on F-items vs missions:** the Constraints/Non-Goals sections reference F1–F10 as future-work items. These are NOT separate missions; they describe future directions for Sync evolution tracked in the research doc's §11.8 follow-ups list. The base mission and 0862a–0862i (listed in §Related Missions) are the active execution chain once `RFC-0862` is accepted. `0862i` (Raft overlay) is a Phase 4 future mission tied to F1.

## Non-Goals

- **Not in scope for v1:** Multi-leader / active-active conflict resolution. CRDTs are explicitly rejected by `RFC-0852 §Alternatives Considered` for consensus-relevant state; this use case does not introduce them either. Multi-leader is `F1` future work.
- **Not in scope for v1:** Native browser/browser-node sync (WebRTC data channel). v1 uses DOT platform adapters; WebRTC can be a Phase 3 adapter per the existing `RFC-0850 §3.1` allocation.
- **Not in scope for v1:** Trust-anchored storage checkpoints (the analog of `RFC-0851p-a` §6 "Sybil / Eclipse Defense" "genesis checkpoint" for peer lists, but for storage). A brand-new reader must trust the first peer it meets; this is `F2` future work.
- **Not in scope for v1:** Sharding across multiple Stoolap instances (different schemas per shard). v1 is whole-DB replication; sharded replication is a follow-up.
- **Not in scope for v1:** Automatic writer election / failover. v1 has no failover (operator must reconfigure `writer_node_id` on the reader when the writer is unreachable). Auto-failover is `F8` future work via `DomainCoordinator` handover (`RFC-0855p-c`).
- **Not in scope for v1:** Schema migration across major version mismatches. v1 aborts on schema-version mismatch. Coordinated migration protocol is `F9` future work.
- **Not in scope for v1:** Reed-Solomon erasure coding for first-time sync. v1 uses per-segment download only. RS integration is `F10` future work.

## Impact

If this use case is implemented:

1. **The fork becomes a CipherOcto network node.** Stoolap data can be replicated, with cryptographic provenance, across any combination of NativeP2P, QUIC, Webhook, and social-platform carriers.
2. **High availability is achievable.** Read-replica failover (manual in v1, automatic in `F8`); disaster recovery across geographies via DGP anti-entropy.
3. **Disconnected operation is enabled.** Local writes during partitions, full reconciliation on heal via the anti-entropy Merkle-descent handshake.
4. **The AI Quota Marketplace gains a verifiable state backend.** The existing `stoolap-integration-research.md` integration story becomes a true multi-node deployment, not a single-process file.
5. **The cipherocto crates gain a new `octo-sync` crate** alongside `octo-network` and `octo-determin`.
6. **The Stoolap fork's `pub use` surface gains a `SyncTransport` trait and `SyncConfig` struct** (gated behind the `sync` feature), enabling application code to opt into two-node replication without changing the existing single-process API.

## Implementation Phases

### Phase 1 — Minimum Viable Two-Node Sync (MVE)

- Single-leader, N read-replicas.
- WAL-tail streaming (research §3.2 Approach B).
- LSN-based catch-up; no snapshot shipping yet.
- NativeP2P transport primary; QUIC alternative; Webhook fallback.
- Mission scope only.
- **Writer designation:** v1 has NO election. Operator configures `writer_node_id` at mission start. This is a hard requirement, not a configuration option.
- **Failover:** NONE in v1. Reader emits `WriterUnreachable` event locally if writer is unreachable for `> 2 × heartbeat_interval` (10s) sustained; operator must reconfigure.
- Heartbeat: 5s interval, `Suspect` after 10s, per-peer token bucket rate limit (100 req/s sustained, 500 burst).
- Test: two nodes, writer + reader-replica, run for 1h, every table's `BLAKE3-256(SELECT * FROM table)` matches across both nodes. Tests also cover: dual restart, single restart, 30s/5min/1hr partition, schema add column, schema drop column, schema add table.

### Phase 2 — Catch-up via Snapshot Segments

- Anti-entropy Merkle summary exchange (research §3.4 Approach D).
- Snapshot segment request/response, LZ4, CRC32, parallel download.
- 1M-row DB, sync in <60s; kill writer mid-sync, restart, auto-resume from `LsnAck`.
- 1 GB snapshot sync < 60s; 10 GB < 10 min.

### Phase 3 — Multi-node Gossip

- DGP `GossipObject` with `object_type = 0x0008 SnapshotFragment` carries batches of WAL entries.
- Any node can serve or receive sync.
- DRS-based peer selection (RFC-0856).
- PoRelay trust scoring (RFC-0860) ranks peers by sync reliability.
- Test: 5-node network, 1 writer, 4 readers, kill any node, verify convergence within 60s.

### Phase 4 — Cross-carrier, Mission-aware

- Multi-carrier propagation: same sync stream across NativeP2P + Webhook + one social adapter.
- Per-mission key isolation (PRIVATE missions get encryption; PUBLIC missions send in clear).
- Slashing for misbehaving sync peers (slash code TBD by maintainer decision — see the RFC-0860 entry in §Related RFCs for the conflict-flagging note).
- Interop test: two implementations (Rust + the eventual Cairo / Move ports) reach identical state.

## Technical Approach (from research)

The research recommends a layered combination of:

1. **Approach B (WAL-tail streaming)** for live replication — reuses the existing V2 binary WAL + `record_commit` hook + `replay_two_phase` recovery path. This is the same pattern as PostgreSQL logical replication, MySQL binlog replication, and SQLite session extension; the binary log is the source of truth.
2. **Approach D (anti-entropy Merkle summary)** for first-time sync and partition healing — adapts the DGP `GossipStateSummary` pattern in `RFC-0852 §7` to per-table segment Merkle trees. The 16-way Merkle convention matches `HexaryProof` in `stoolap/src/trie/proof.rs`.

Five sync approaches were analyzed in the research (event-driven, WAL-tail, operation-log, anti-entropy, native P2P). Approach B+D was recommended; Approach A was rejected for missing payload; Approach C was held for Phase 2+ when the consensus/rollup layer is wired into the live engine; Approach E was rejected by the user's "use the cipherocto network" requirement.

## Risks

| Risk | Likelihood | Impact | Mitigation |
| --- | --- | --- | --- |
| Cryptographic mistakes in OCrypt reuse | Low | High (peer impersonation) | Adopt OCrypt as-is; mission `0862d` "OCrypt test-vector replay" test |
| Schema drift between peers (DDL during sync) | Medium | High (apply failure) | Apply DDL in LSN order; if a later DDL is missing, abort with `SchemaDrift` event. Cross-major-version migration is `F9` future work. |
| Writer election / leader handoff | Low (v1 single-leader) | High (no failover) | v1 has no failover; operator reconfigures `writer_node_id`. `F8` future work via `DomainCoordinator` handover. |
| Replicator role designation | Low | High (no role, no sync) | `RFC-0855 §4.2` adds `Replicator` (immediate §10.3 change). Writer holds `Replicator`; readers are `Observer`. |
| `tokio` as hard dep | Low | Medium | `SyncTransport` trait in core is sync; `stoolap-sync` companion crate provides async. |
| Mission key rotation in flight | Low | Medium | Re-handshake on Heartbeat if `identity_epoch` differs; `RFC-0853 §12` already supports rotation (24h grace). |

## Related RFCs

- **RFC-0126: Deterministic Serialization** — canonical encoding (DCS) for all Sync wire structs.
- **RFC-0850: Deterministic Overlay Transport** — transport. 21 platform types in `§3.1`; wire formats `DOT/1/{base64}`, `DOT/2/{msg_id}`, `DOT/F/{base64_frag}`, `RAW/{binary}`; `BTreeMap<envelope_id, first_seen>` replay cache; QUIC profile in `§8.7`. **No protocol change** — new envelope payload discriminators `0xA0–0xC2` reserved.
- **RFC-0851: Gateway Discovery Protocol** — discovery. 5-state `DiscoveryLifecycle`. `GatewayAdvertisement` with Merkle-committed `capabilities_root` etc. **A new `SyncCapable` bit is added to `capabilities_root` — bit position TBD by maintainer decision. The base 6 capability bits (Edge=0x0001, Relay=0x0002, Consensus=0x0004, Archive=0x0008, Stealth=0x0010, Translation=0x0020) per `RFC-0850:284-287` and `RFC-0851:210-213,558` are already allocated; the new bit must be at a higher position (e.g., 0x0040+ per the GDP extension pattern). Maintainer decision required.**
- **RFC-0851p-a (Networking): Network Bootstrap Protocol** — bootstrap. 7-state `BootstrapClientLifecycle`. **Note:** slash code `0x000D` claim in `0851p-a:420,431,726` contradicts `0850p-c:460` claim of `0x000C-0x000D` for sub-DC delegation. **Maintainer decision required.**
- **RFC-0852: Deterministic Gossip Protocol** — gossip, anti-entropy. `GossipObjectType = 0x0008 SnapshotFragment` format to be specified in `RFC-0862` (immediate §10.3 change).
- **RFC-0853: Overlay Cryptography** — crypto. `OverlayIdentity { public_key, identity_epoch }`; `MissionKeyHierarchy`; new HKDF context `"sync:v1"` in §6 Mission Cryptography (immediate §10.3 change).
- **RFC-0855: Mission Overlay Networks** — missions. New `Replicator` role in `§4.2` (immediate §10.3 change).
- **RFC-0855p-b (Networking): Mission Coordinator Lifecycle** — 8-state `CoordinatorLifecycle`.
- **RFC-0855p-c (Networking): Domain Coordinator Role** — 90-epoch platform-loss window; basis for `F8` auto-failover.
- **RFC-0856: Deterministic Route Selection** — DRS scoring for peer selection.
- **RFC-0857: Deterministic Overlay Mempool** — Phase 3 batch shipping.
- **RFC-0858: Onion Relay Routing** — optional for PRIVATE missions.
- **RFC-0859: Proof-Carrying Envelopes** — `F3` proof-of-sync.
- **RFC-0860: Proof-of-Relay** — composite scoring (forwarding/availability/bandwidth/uptime/diversity with `WF=300, WA=250, WB=200, WU=150, WD=100`); `SyncForwardingProof` variant added. **Note: slash code allocation for `sync_peer_misbehavior` is TBD by maintainer decision. The code `0x0010` is already allocated to `FalseWitness` per `RFC-0850p-c:463`, `RFC-0850p-d:392`, `RFC-0850p-e:305`, and `RFC-0855p-b:963`. The code `0x0012` is allocated to `CrossPlatformWitnessCollusion` per `RFC-0855p-c §9b:507`. A new slash code (e.g., `0x0013` or higher per the `RFC-0850p-c §6` reserved range `0x0013-0xFFFF`) must be chosen. Maintainer decision required.**
- **RFC-0861: CoordinatorAdmin Trait Refinements** — 17 findings closed, 1,373 tests; basis for `0862d` OCrypt-binding mission.
- **RFC-0200: Production Vector-SQL Storage Engine v2** — supersedes the body-section Raft sketch (line 1821-1997) and the brief §A "Replication Model" (line 2640); add forward reference to `RFC-0862` in §A (immediate §10.3 change).
- **RFC-0740: Sharded Consensus Protocol** — `CrossShardMessage::StateSync` is the cross-shard analog; basis for `F9` schema migration.

**New RFC required:** `RFC-0862: Stoolap Data Sync Protocol` (recommended; alternative `RFC-0210` in storage rejected per research §10 rationale).

## Related Use Cases

- [DOT Network Bootstrap](dot-network-bootstrap.md) — the closest existing "first network operation" use case. Bootstrap must complete before Sync can run.
- [Stoolap-Only Persistence for Quota Router](stoolap-only-persistence.md) — single-node Stoolap usage. This use case extends that to two-node replication.
- [Stoolap Integration with AI Quota Marketplace](../research/stoolap-integration-research.md) (research, not use case) — the immediate downstream consumer of multi-node Stoolap.
- [Verifiable Agent Memory Layer](verifiable-agent-memory-layer.md) — memory layer; would benefit from Sync for cross-node memory consistency.
- [Data Marketplace](data-marketplace.md) — data trading; Sync is the substrate for cross-node data replication.
- [Stoolap MVCC Transaction Aggregate Support](stoolap-mvcc-transaction-aggregate-support.md) — single-node MVCC; Sync is the multi-node extension.

## Pipeline Position

```
Research (docs/research/stoolap-data-sync-via-cipherocto-network.md, v2.0 — post Round 10 adversarial review; Round 11 verification pending at time of writing)
   │
   ▼
Use Case (this document)
   │
   ▼
RFC-0862: Stoolap Data Sync Protocol (ACCEPTED, at `rfcs/accepted/networking/0862-stoolap-data-sync.md`)
   │
   ▼
0862 base mission (0862-stoolap-data-sync-base.md) (EXECUTION)
   │
   ▼
0862a–0862i sub-missions (EXECUTION)
   │
   ▼
Implementation in stoolap fork:
   - New crate: crates/stoolap-sync/ (sync feature, optional tokio dep)
   - New trait: SyncTransport in src/sync/ (sync I/O, no tokio)
   - New method: Database::open_with_sync(dsn, SyncConfig)
   - Re-export SyncTransport and SyncConfig when sync feature enabled
```

## Related Missions

**Note on F-items vs missions:** F1–F10 are **Future Work items** tracked in the research doc's §11.8 follow-ups list. They are NOT separate missions — they describe future directions for Sync evolution. The base mission and 0862a–0862i (listed below) are the active execution chain once `RFC-0862` is accepted.

Under the new `RFC-0862`:

- `missions/open/0862-stoolap-data-sync-base.md` — base mission; types `SyncEnvelopeType`, `SyncSummary`, `SyncSegment`, `WalTailChunk`, `NodeStatus`, `NodeId`, `PeerId`, `SyncTransport` trait, `SyncEngine` struct.
- `missions/open/0862a-stoolap-data-sync-wal-tail.md` — WAL-tail streaming (Approach B).
- `missions/open/0862b-stoolap-data-sync-merkle-summary.md` — Per-table segment Merkle summary + anti-entropy handshake (Approach D).
- `missions/open/0862c-stoolap-data-sync-snapshot-segment.md` — Snapshot segment request/response, LZ4, CRC32, parallel download.
- `missions/open/0862d-stoolap-data-sync-ocrypt-bind.md` — OCrypt integration, mission key derivation, AAD binding, AuthChallenge/AuthResponse.
- `missions/open/0862e-stoolap-data-sync-replay-cache-persistence.md` — Persist `ReplayCache` to disk for restart survival.
- `missions/open/0862f-stoolap-data-sync-multi-peer.md` — N readers via DGP `GossipObject` `object_type=0x0008 SnapshotFragment`.
- `missions/open/0862g-stoolap-data-sync-cross-carrier.md` — Multi-carrier propagation (NativeP2P + Webhook + one social adapter).
- `missions/open/0862h-stoolap-data-sync-property-tests.md` — Property tests (LSN monotonicity, heartbeat-loss, key-isolation, rate-limit).
- `missions/open/0862i-stoolap-data-sync-raft-overlay.md` — (Phase 4 future) Raft/Paxos overlay for quorum replication.

---

**Category:** Networking (with Storage/Retrieval overlap)
**Priority:** High (unlocks HA, scale, and the AI Quota Marketplace multi-node deployment)
**RFCs:** RFC-0862 (proposed), plus amendments to RFC-0851 §4, RFC-0852 §10, RFC-0853 §6, RFC-0855 §4.2, RFC-0860 (new forwarding proof variant), RFC-0200 §A
**Status:** Defined → Mission phase (after RFC-0862 acceptance per BLUEPRINT §Canonical Workflow)
