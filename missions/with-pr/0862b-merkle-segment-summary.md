# Mission: 0862b — Per-Table Merkle Segment Summary Builder

## Status

Draft (awaiting adversarial review)

## RFC

RFC-0862 v1.1.0 (Networking): Stoolap Data Sync Protocol — §4.3.4 Anti-entropy Merkle summary, §Envelope Payload Discriminators (`0xA0`/`0xA1`), §Implementation Phases Phase 2, §DatabaseSyncAdapter Trait (v1.1.0)

## Summary

Implement the per-table Merkle segment summary builder: for each table in the local DB, build a 16-way Merkle tree over the snapshot segments, with leaf = `BLAKE3-256(payload)` and root = `BLAKE3-256(children)`. Ship the summaries in `SummaryResponse` envelopes on `SummaryRequest`. The reader compares its local summary to the writer's and descends the Merkle tree to find divergent segments.

## Design

### New module: `octo-sync/src/summary.rs` (leaf workspace at `cipherocto/octo-sync/src/summary.rs`)

The Merkle summary builder is **pure compute** — it does not call any DB functions. The cipherocto sync engine feeds it a list of `SegmentMetadata` (built from `adapter.read_snapshot_segment` results; see mission 0862c) and it returns a `MerkleSegmentTree`. The adapter boundary means this module is testable in isolation with hand-crafted `SegmentMetadata` (no DB needed).

```rust
pub struct SyncSummary {
    pub table_id: u32,                  // BLAKE3-256(table_name)
    pub segment_count: u32,
    pub segment_root: [u8; 32],         // BLAKE3-256 over 16-way Merkle tree
    pub lsn_watermark: u64,             // highest LSN applied to this table
    pub hmac: [u8; 32],                 // HMAC-BLAKE3(transport_key, summary_body)
}

pub struct MerkleSegmentTree {
    leaves: Vec<[u8; 32]>,              // 16^4 = 65536 max
}

impl MerkleSegmentTree {
    pub fn from_segments(segments: &[SegmentMetadata]) -> Self {
        // 1. Hash each segment: leaf[i] = BLAKE3-256(segment.payload)
        // 2. Pad to multiple of 16 with zero-hashes
        // 3. Build 16-way tree: root = BLAKE3-256([child0, child1, ..., child15])
        //    if level has < 16 children, pad with zero-hashes
        // 4. Tree depth ≤ 4 for ≤ 65536 segments
    }

    pub fn root(&self) -> [u8; 32] {
        // return top of tree
    }

    /// Return the list of (level, index) where this tree diverges from `other`.
    pub fn diff(&self, other: &Self) -> Vec<(usize, usize)> {
        // descend both trees, return divergent positions
    }

    /// Return the segments at the given (level, index) positions.
    pub fn segments_at(&self, positions: &[(usize, usize)]) -> Vec<SegmentMetadata> {
        // for each divergent position, return the corresponding segment metadata
    }
}
```

### Segment metadata

A "segment" is a single snapshot file (`<dsn-path>/snapshots/<table>/snapshot-<ts>.bin`). The metadata includes:
- `segment_index: u32` (sequential, 0-indexed; matches `SyncSegment.segment_index` per RFC-0862 §4.3)
- `payload_hash: [u8; 32]` (BLAKE3-256 of the file contents)
- `file_path: PathBuf` (relative to the DSN)
- `lsn_watermark: u64` (LSN at segment generation time)
- `byte_size: u64` (size of the file in bytes)

### Algorithm (per RFC-0862 §4.3.4)

1. **Reader → Writer (initial sync):** Send `SummaryRequest` (no payload).
2. **Writer → Reader:** Send `SummaryResponse { summaries: Vec<SyncSummary> }` for all tables.
3. **Reader:** For each table, compare local `SyncSummary` to writer's:
   - If `segment_root` matches AND `lsn_watermark` matches: no-op.
   - If `segment_root` matches BUT `lsn_watermark` is behind: request `WalTailRequest` for the missing LSN range.
   - If `segment_root` differs: descend the Merkle tree to find divergent segments, then send `SegmentRequest { table_id, segment_index, expected_root }` for each.
4. **Writer → Reader (per segment):** Send `SegmentResponse { segment }` or `SegmentNotFound` (forces writer to re-snapshot).
5. **Reader:** Verify `BLAKE3-256(payload) == segment.segment_root` and `crc32(payload) == segment.crc32`. On mismatch: retry with exponential backoff (max 3 attempts); on persistent mismatch: mark peer `Suspect`, then `Terminated`.

### HMAC binding

`summary.hmac = HMAC-BLAKE3(transport_key, summary_body || node_id)`. The transport_key is per-peer per-mission; recomputing on the writer and reader sides must produce the same bytes.

## Acceptance Criteria

- [ ] `octo-sync/src/summary.rs` (in the `octo-sync/` leaf workspace) exists with `SyncSummary`, `MerkleSegmentTree`, and segment metadata types
- [ ] `SyncSummary` has all 5 fields: `table_id`, `segment_count`, `segment_root`, `lsn_watermark`, `hmac`
- [ ] `MerkleSegmentTree::from_segments` builds a 16-way tree with depth ≤ 4
- [ ] Empty segments tree returns a single zero-hash root
- [ ] Tree with exactly 16 leaves returns `BLAKE3-256(sorted_leaves)` as root
- [ ] Tree with 17 leaves pads to 32 leaves (next multiple of 16); level 1 has 2 nodes; level 2 (root) pads to 16 children (2 nodes + 14 zero-hashes); root = BLAKE3-256(those 16 children)
- [ ] `MerkleSegmentTree::diff(other)` returns divergent positions correctly for all edge cases (identical, fully disjoint, partially overlapping, deep difference)
- [ ] `MerkleSegmentTree::segments_at(positions)` returns the correct segments for each position
- [ ] HMAC binding: `summary.hmac == HMAC-BLAKE3(transport_key, summary_body || node_id)`
- [ ] `SummaryRequest` and `SummaryResponse` envelopes (codes `0xA0` and `0xA1`) implemented in `envelope.rs`
- [ ] Writer-side `handle_summary_request` returns all per-table summaries
- [ ] Reader-side `on_summary_response` triggers Merkle descent and SegmentRequest issuance
- [ ] Unit tests for Merkle tree construction, diff, segments_at, and HMAC binding
- [ ] Integration test: writer with 1M rows across 10 tables, reader with empty DB → reader receives summaries, descends tree, requests segments, applies, state matches

## Tests

- **Unit:**
  - Empty tree returns zero-hash root
  - 1 leaf: root = BLAKE3-256(leaf)
  - 16 leaves: root = BLAKE3-256(sorted_leaves)
  - 17 leaves: leaves pad to 32 (next multiple of 16); level 1 has 2 nodes; level 2 (root) pads to 16 children (2 nodes + 14 zero-hashes); root = BLAKE3-256(those 16 children)
  - 65536 leaves: depth 4, root computed correctly
  - `diff` returns empty Vec for identical trees
  - `diff` returns all positions for completely disjoint trees
  - `diff` returns specific positions for partially overlapping trees
  - `diff` returns deep position for tree-difference at depth 3
  - `segments_at` returns correct segments for each position
  - HMAC binding: same transport_key produces same HMAC
  - HMAC binding: different transport_key produces different HMAC
  - HMAC binding: different node_id produces different HMAC

- **Integration:**
  - Writer with 10 tables, 100 rows each → reader receives 10 summaries
  - Writer with 1 table, 1M rows, 100 segments → reader descends tree, requests 100 segments, applies
  - Writer snapshot removed (SegmentNotFound) → **writer regenerates the snapshot via `MVCCEngine::create_snapshot_for_table` (per-table; see mission 0862c)**, ships, reader retries
  - Reader with stale summary → writer sends fresh summary, reader updates watermark

## Dependencies

- **Requires:**
  - `0862-base` — envelope types, identity, state machine, **`DatabaseSyncAdapter` trait**
  - `0862a` — WAL-tail streamer (for LSN watermarks)
  - `octo_sync::DatabaseSyncAdapter::read_snapshot_segment` (per RFC-0862 v1.1.0 §DatabaseSyncAdapter Trait) — used by mission 0862c to enumerate the writer's segments; this mission consumes the resulting `SegmentMetadata` list as input
  - RFC-0862 §4.3.4 (anti-entropy Merkle summary algorithm)
  - RFC-0852 §7 (DGP anti-entropy pattern, adapted for per-table segments)

- **Required by:**
  - `0862c` (snapshot segment indexer — uses the Merkle tree to decide which segments to ship)
  - `0862f` (multi-peer — multiple readers can verify against the same Merkle root)

- **No longer requires direct access to:**
  - `stoolap/src/storage/mvcc/snapshot.rs` — segment enumeration is now done by 0862c via `adapter.read_snapshot_segment`

## Blockers / Dependencies

- **Blocked by:** `0862-base`, `0862a`
- **Blocks:** `0862c`

## Description

The anti-entropy Merkle summary is the canonical mechanism for partition healing. When a reader falls behind the writer (due to network partition, crash, or operator pause), the reader can resync in `O(log N)` time by sending a `SummaryRequest` and descending the Merkle tree to find only the divergent segments. This avoids re-shipping the entire database on every reconnect.

## Technical Details

### Performance

- **First-time snapshot sync (1 GB):** < 60 s (per RFC-0862 G4)
- **First-time snapshot sync (10 GB):** < 10 min (per RFC-0862 G4)
- **Catch-up after 1 min partition:** < 5 s (no snapshot re-ship)
- **Catch-up after 1 hr partition:** < 10 min (snapshot re-ship from oldest LSN on disk)

### Why 16-way (not binary)?

The Stoolap fork's `stoolap/src/trie/proof.rs:71-87` defines `HexaryProof` with 16-way branching (`levels: Vec<Vec<Hash>>`, `path: Vec<u8>`). The 16-way choice matches the existing `HexaryProof` convention, reducing implementation surface area. Tree depth ≤ 4 for ≤ 65,536 segments per table.

### Why BLAKE3-256 (not SHA-256)?

BLAKE3-256 is RFC-0853's standardized hash for overlay state (`GossipStateSummary` uses BLAKE3). It is also what Stoolap's `octo_determin` dependency uses. Consistency with the rest of the cipherocto stack.

### Pitfalls

- **Don't include `node_id` in the Merkle tree itself.** The HMAC binds the root to the node; the tree is content-only.
- **Don't sort leaves by `table_id`.** Sort by `segment_index` (the file's position in the snapshot directory).
- **Don't compute the root differently on writer and reader.** Both sides must use the same `MerkleSegmentTree::from_segments` algorithm.
- **Don't reuse zero-hashes across different tables.** Each table has its own tree, and the zero-hash for an empty slot at depth 2 is different from the zero-hash at depth 3.
- **Don't ship the Merkle tree itself.** Ship only the `segment_root` in `SyncSummary`. The reader descends by requesting individual segments via `SegmentRequest`.
- **Don't call Stoolap DB functions from this module.** The Merkle summary builder is pure compute over `SegmentMetadata`; segment enumeration is delegated to mission 0862c via the `DatabaseSyncAdapter` trait.

---

**Mission Type:** Implementation
**Priority:** High
**Phase:** 2 (Catch-up via snapshot segments)
**RFC Section Coverage:** §4.3.4 Anti-entropy Merkle summary

## Type Coverage

This mission implements the following RFC-0862 types:

| Type | Role in this mission |
|------|---------------------|
| `SyncSummary` | The per-table Merkle summary envelope (codes 0xA0/0xA1); built from a table's segment Merkle root |
| `MerkleSegmentTree` | The 16-way Merkle tree builder over per-table snapshot segments |
| `SegmentMetadata` | Metadata for a single snapshot segment: `segment_index`, `payload_hash`, `file_path`, `lsn_watermark`, `byte_size` |

The mission does NOT implement the segment transport (`SegmentRequest` / `SegmentResponse` / `SegmentNotFound`) — those are handled by mission 0862c. See the Type Coverage table in 0862-base for the full mapping.
