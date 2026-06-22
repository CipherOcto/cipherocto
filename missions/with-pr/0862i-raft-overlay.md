# Mission: 0862i — Raft Overlay (F1/F8 Future)

## Status

Draft (deferred)

## RFC

RFC-0862 v1.1.0 (Networking): Stoolap Data Sync Protocol — §Future Work F1 (Multi-leader / active-active), §Future Work F8 (Writer election / auto-failover), §DatabaseSyncAdapter Trait (v1.1.0)

## Summary

**This mission is DEFERRED to a future phase** (post-Phase 4). v1 of RFC-0862 is single-leader with no auto-failover; this mission implements the Raft/Paxos overlay that would make Sync multi-leader with automatic failover.

Per RFC-0862 §Future Work F1, the candidates are:
- (a) per-row HLC + LWW (Hybrid Logical Clock + Last-Write-Wins)
- (b) move to a Raft/Paxos overlay (per `RFC-0200` body section, line 1821-1999)
- (c) restricted to specific table groups

This mission implements option (b): the Raft overlay. It is **NOT in v1 or Phase 4**; it is a future mission tied to F1 and F8.

## Design

### High-level

The Raft overlay is a separate sub-protocol that:
1. Elects a writer via Raft consensus (one writer per mission)
2. Heartbeats writer liveness (3 missed heartbeats → election)
3. Auto-failover: when the writer fails, a new writer is elected from the readers

### Raft integration with Sync

The Raft overlay produces "Raft entries" (each entry is a `WALEntry` from the Sync protocol). The Sync engine wraps each `WALEntry` in a Raft entry and submits it to the Raft consensus. When the Raft entry is committed, the Sync engine applies it via `adapter.apply_wal_entry(entry)` — the underlying `StoolapAdapter` impl (per RFC-0862 v1.1.0) internally calls `MVCCEngine::replay_two_phase`. **The cipherocto sync engine never calls `MVCCEngine::replay_two_phase` directly; the trait is the integration boundary.**

### Domain Coordinator

Per RFC-0855p-c, the writer is a `DomainCoordinator`. The Raft overlay elects a new `DomainCoordinator` when the current one fails. The `DomainCoordinatorRecord` (RFC-0855p-c) tracks the current writer's identity.

## Status: DEFERRED

This mission is **deferred beyond Phase 4** of RFC-0862. It is documented here for completeness but is NOT in scope for v1 or any of the current implementation phases.

## Acceptance Criteria (placeholder — to be refined when mission is un-deferred)

- [ ] `octo-sync/src/raft_overlay.rs` (in the `octo-sync/` leaf workspace) exists with the Raft state machine
- [ ] Election: candidate sends `RequestVote` to peers; majority wins
- [ ] Heartbeat: leader sends `AppendEntries` every 100ms; 3 missed → election
- [ ] Auto-failover: when leader fails, a new leader is elected within 5s
- [ ] Raft entries are Sync `WALEntry`s
- [ ] When a Raft entry is committed, the Sync engine applies it via `adapter.apply_wal_entry(entry)` (per RFC-0862 v1.1.0; the underlying `StoolapAdapter` impl delegates to `MVCCEngine::replay_two_phase` internally)
- [ ] Integration with `DomainCoordinatorRecord` (RFC-0855p-c) for writer identity

## Dependencies

- **Requires:**
  - `0862-base` (single-leader core, **`DatabaseSyncAdapter` trait**)
  - `0862f` (multi-peer)
  - RFC-0855p-c (Domain Coordinator Role)
  - RFC-0200 (Production Vector-SQL Storage Engine v2) — body-section Raft sketch

- **Required by:** none

## Blockers / Dependencies

- **Blocked by:** F1 (Multi-leader / active-active) and F8 (Writer election / auto-failover) being unblocked
- **Blocks:** none

## Description

The Raft overlay is the natural extension of v1's single-leader model to multi-leader with auto-failover. It is a substantial design effort (probably 3-6 months of implementation) and is not in scope for v1. The mission is documented here so that future work has a clear starting point.

## Technical Details

### Why Raft (not Paxos or HLC+LWW)?

- **Raft** is well-understood, has a working `raft-rs` implementation, and is the most common consensus algorithm in production.
- **Paxos** is theoretically equivalent but harder to implement and verify.
- **HLC+LWW** is the simplest but has weaker consistency guarantees (last-write-wins can lose data if clocks are skewed).

For an application-level state sync (not consensus), HLC+LWW might be sufficient; for a mission-critical system, Raft is the safer choice.

### Why deferred?

v1 is single-leader; multi-leader is a substantial design effort that requires:
- Schema-level conflict resolution (e.g., what happens if two writers commit to the same row?)
- HLC or vector clock implementation
- Per-row conflict resolution policy
- Extensive testing of Byzantine scenarios

This is F1 (Multi-leader / active-active) in the Future Work section. It is a significant research and engineering effort.

### Pitfalls (for future implementation)

- **Don't use raw `raft-rs` directly.** It requires async; the Sync engine is currently sync. Wrap it in a `spawn_blocking` task.
- **Don't conflate "Raft leader" with "Sync writer".** The Raft leader is a coordinator; the Sync writer is the database. They may be the same node, but the abstractions are different.
- **Don't implement Raft from scratch.** Use `raft-rs` and add the Sync-specific extensions on top.
- **Don't call `MVCCEngine::replay_two_phase` from the cipherocto sync engine.** Per RFC-0862 v1.1.0, all DB writes go through `adapter.apply_wal_entry(entry)`. The Raft overlay (which IS part of the cipherocto sync engine) must follow the same rule; the underlying `StoolapAdapter` impl handles the engine-level call.

---

**Mission Type:** Implementation
**Priority:** Future (deferred)
**Phase:** Post-Phase 4
**RFC Section Coverage:** §Future Work F1, F8

## Type Coverage

This mission (when un-deferred) will implement the following RFC-0862 types:

| Type | Role in this mission |
|------|---------------------|
| `RaftOverlay` | The Raft consensus state machine that elects a writer and replicates WAL entries |
| `DomainCoordinatorRecord` integration (per RFC-0855p-c) | Tracks the current writer's identity for auto-failover |

**STATUS: DEFERRED.** This mission is not in scope for v1 or any of the current implementation phases (Phase 1–4). It is documented here for completeness. See RFC-0862 §Future Work F1 and F8.
