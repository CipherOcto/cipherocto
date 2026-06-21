# Mission: 0862f — Multi-Peer via DGP (Deterministic Gossip Protocol)

## Status

Draft (awaiting adversarial review)

## RFC

RFC-0862 (Networking): Stoolap Data Sync Protocol — §Implementation Phases Phase 3; RFC-0852 §3 (DGP `GossipObjectType::SnapshotFragment = 0x0008`)

## Summary

Extend the single-leader WAL-tail streaming to N peers via DGP anti-entropy gossip. Each peer holds a copy of the database; gossip happens via the DGP `SnapshotFragment = 0x0008` object type. DRS-based peer selection (RFC-0856) chooses the best peers for sync; PoRelay trust scoring (RFC-0860) ranks peers by reliability.

This is the **N-node extension** to v1. Phase 3 of RFC-0862. Per RFC-0862 §Implementation Phases Phase 3, "DGP `GossipObject` with `object_type = 0x0008 SnapshotFragment`. N readers via gossip; any node can serve or receive." In Phase 3, a reader that has fallen behind can fetch missing segments from any other peer (not just the writer). The writer-fanout star topology from Phase 1/2 is a degenerate special case of Phase 3.

## Design

### New module: `crates/octo-sync/src/dgp_bridge.rs`

```rust
use octo_network::dgp::{GossipObject, GossipStateSummary};

pub struct DgpSyncBridge {
    /// Local Sync engine (the SyncStateMachine from 0862-base).
    sync: Arc<SyncEngine>,

    /// DGP gossip state.
    gossip_state: GossipStateSummary,

    /// Per-peer state machines.
    peers: HashMap<SyncPeerId, SyncLifecycle>,
}

impl DgpSyncBridge {
    /// Called when DGP delivers a SnapshotFragment (object_type = 0x0008).
    pub async fn on_snapshot_fragment(&self, fragment: GossipObject) -> Result<()> {
        // 1. Verify the fragment is for this mission
        if fragment.mission_id != self.sync.mission_id() {
            return Ok(());  // ignore other missions
        }
        // 2. Decode the fragment as a SyncSummary, SyncSegment, or WalTailChunk
        match fragment.subtype {
            0xA1 => {  // SummaryResponse
                let summary: SyncSummary = fragment.decode()?;
                self.on_summary_response(fragment.peer_id, summary).await?;
            }
            0xA3 => {  // SegmentResponse
                let segment: SyncSegment = fragment.decode()?;
                self.on_segment_response(fragment.peer_id, segment).await?;
            }
            0xB1 => {  // WalTailResponse
                let chunk: WalTailChunk = fragment.decode()?;
                self.on_wal_tail_chunk(fragment.peer_id, chunk).await?;
            }
            _ => return Err(SyncError::UnknownEnvelopeSubtype(fragment.subtype)),
        }
    }

    /// Periodic tick: check peer health, gossip summaries to neighbors.
    pub async fn tick(&self) -> Result<()> {
        // 1. For each peer in `Suspect` or `Reconnecting` state, try to reconnect
        // 2. For each peer in `Streaming` state, send a `SummaryRequest` if no recent activity
        // 3. Gossip our own SummaryResponse to DRS-selected neighbors
    }
}
```

### DGP integration

Per RFC-0852, the DGP uses a `GossipStateSummary` to detect divergence. The Sync protocol adapts this to per-table segments (per RFC-0862 §4.3.4). The DGP `object_type = 0x0008 SnapshotFragment` is used to carry SyncSummary, SyncSegment, and WalTailChunk.

### Peer selection (DRS)

Per RFC-0856, the Deterministic Route Selection (DRS) chooses the best peers for sync. The criteria are:
1. **Forwarding proof:** RFC-0860 composite score (forwarding/availability/bandwidth/uptime/diversity)
2. **Diversity:** at least 2 Regional and 3 Global peers (per RFC-0851 §Operational Rules)
3. **Liveness:** no missed heartbeats in the last 30s

### N-reader topology (Phase 3)

In Phase 3, multiple readers can subscribe to a single writer (the writer-fanout star topology from Phase 1/2). Additionally, in Phase 3, a reader that has fallen behind can fetch missing segments from any other peer (not just the writer). This is the full DGP anti-entropy gossip model: any node can serve or receive `SnapshotFragment` envelopes.

In v1 (single-leader), only the writer produces new WAL entries; readers only apply them. The "any node can serve" property of Phase 3 refers to historical segments (snapshots), not to new WAL entries. New WAL entries always come from the writer.

## Acceptance Criteria

- [ ] `crates/octo-sync/src/dgp_bridge.rs` exists with `DgpSyncBridge` struct
- [ ] `on_snapshot_fragment` decodes and dispatches based on `fragment.subtype`
- [ ] `tick()` runs every 5s: handles reconnection, sends `SummaryRequest` for stale peers, gossips summaries to DRS-selected neighbors
- [ ] DRS-based peer selection respects the 2 Regional + 3 Global diversity rule
- [ ] PoRelay trust scoring ranks peers by reliability
- [ ] Per-peer state machines are isolated (one peer's `Suspect` does not affect another's `Streaming`)
- [ ] Multiple readers can subscribe to a single writer
- [ ] Writer fans out `WalTailChunk` to all subscribers
- [ ] `object_type = 0x0008 SnapshotFragment` is reserved in the DGP namespace (per RFC-0852 §GossipObjectType)
- [ ] Unit tests for: on_snapshot_fragment dispatch, tick scheduling, DRS selection, peer ranking
- [ ] Integration test: 1 writer + 4 readers; writer commits 1000 rows; all 4 readers receive and apply; state matches

## Tests

- **Unit:**
  - `on_snapshot_fragment` with `subtype = 0xA1` calls `on_summary_response`
  - `on_snapshot_fragment` with `subtype = 0xA3` calls `on_segment_response`
  - `on_snapshot_fragment` with `subtype = 0xB1` calls `on_wal_tail_chunk`
  - `on_snapshot_fragment` with unknown subtype returns `SyncError::UnknownEnvelopeSubtype`
  - `on_snapshot_fragment` for a different mission_id is a no-op
  - `tick()` runs every 5s (configurable)
  - DRS selection picks 2 Regional + 3 Global peers when available
  - DRS selection falls back to all-available when diversity constraint can't be met
  - PoRelay ranking sorts peers by composite score

- **Integration:**
  - 1 writer + 4 readers; writer commits 1000 rows; all 4 readers apply
  - 1 writer + 4 readers; one reader is offline for 1 min; on reconnect, catches up via Merkle descent
  - 1 writer + 4 readers; one reader is misbehaving (forging LSNs); other readers are unaffected
  - 1 writer + 4 readers; writer is restarted; all readers reconnect and catch up

## Dependencies

- **Requires:**
  - `0862-base` — for the per-peer state machine and Sync engine
  - `0862a` — WAL-tail streamer (writer-side)
  - `0862b` — Merkle summary (for divergence detection)
  - `0862c` — snapshot segment indexer
  - `0862d` — OCrypt key ring
  - `0862e` — ReplayCache persistence
  - RFC-0852 (Deterministic Gossip Protocol)
  - RFC-0856 (Deterministic Route Selection)
  - RFC-0860 (Proof-of-Relay)

- **Required by:**
  - `0862g` (cross-carrier)
  - `0862h` (property tests for N-peer scenarios)

## Blockers / Dependencies

- **Blocked by:** `0862-base`, `0862a`, `0862b`, `0862c`, `0862d`, `0862e`
- **Blocks:** `0862g`, `0862h`

## Description

Phase 3 of RFC-0862 extends the single-leader v1 to N peers via DGP gossip. The writer is still a single designated node (no election in v1), but multiple readers can subscribe. The DGP `SnapshotFragment` object type carries SyncSummary, SyncSegment, and WalTailChunk envelopes. DRS chooses the best peers, and PoRelay trust scoring ranks them.

## Technical Details

### Performance

- **Throughput:** > 50,000 commits/s aggregated (5K per writer × 10 readers via DOM)
- **Latency:** < 100 ms p50, < 500 ms p99 (WAN, 1 KB write, 5 hops)
- **Memory:** ≤ 50 MB per peer × N peers (the in-memory ReplayCache is per-peer)

### Why DGP (not custom protocol)?

The Stoolap fork has zero networking code. Building a custom gossip protocol is out of scope. DGP is the cipherocto-standard gossip protocol with anti-entropy Merkle summary (RFC-0852 §7); the Sync protocol adapts it for per-table segments.

### Why DRS for peer selection?

DRS provides a deterministic, stake-weighted selection of peers based on:
- Forwarding proof (RFC-0860)
- Diversity (2 Regional + 3 Global minimum)
- Liveness (no missed heartbeats)

This is the same selection criterion used for mission membership; reusing it for Sync peer selection ensures consistency.

### Pitfalls

- **Don't gossip to all peers.** DRS-based selection limits the gossip fanout to a small set; broadcasting is wasteful.
- **Don't merge WalTailChunk from multiple peers.** v1 is single-leader, so all `WalTailChunk` envelopes come from the same writer. The reader rejects chunks from non-writer peers with `E_SYNC_AUTH_FAIL`.
- **Don't share ReplayCache across peers.** Each peer has its own ReplayCache (per (mission_id, peer_id) pair).
- **Don't allow the writer to be a reader.** In v1, the writer is a `Replicator` (writes only); a reader is an `Observer` (reads only). A node that is both violates the 7-state machine.

---

**Mission Type:** Implementation
**Priority:** High
**Phase:** 3 (Multi-node gossip)
**RFC Section Coverage:** §Implementation Phases Phase 3, §Envelope Payload Discriminators (DGP `0x0008`)

## Type Coverage

This mission implements the following RFC-0862 types:

| Type | Role in this mission |
|------|---------------------|
| `DgpSyncBridge` | The bridge between DGP gossip and the Sync engine; routes `SnapshotFragment` (DGP `object_type = 0x0008`) envelopes to `SyncSummary` / `SyncSegment` / `WalTailChunk` handlers |
| `DRS-Selected-Peers` (per RFC-0856) | The set of peers chosen for sync by the Deterministic Route Selection (2 Regional + 3 Global minimum) |
| `PoRelay-Trust-Score` (per RFC-0860) | The per-peer trust score used to rank gossip candidates |

The mission does NOT implement `SyncSummary`, `SyncSegment`, or `WalTailChunk` themselves — those are in missions 0862b, 0862c, 0862a respectively. This mission only routes them. See the Type Coverage table in 0862-base for the full mapping.
