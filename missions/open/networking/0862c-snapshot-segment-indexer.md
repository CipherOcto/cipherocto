# Mission: 0862c — Snapshot Segment Indexer

## Status

Draft (awaiting adversarial review)

## RFC

RFC-0862 (Networking): Stoolap Data Sync Protocol — §4.3.4 step 4 (snapshot segment shipping), §Implementation Phases Phase 2, §Envelope Payload Discriminators (`0xA3`/`0xA4`)

## Summary

Implement the snapshot segment indexer and shipper: when the writer receives a `SegmentRequest { table_id, segment_index, expected_root }` from a reader, locate the corresponding snapshot file, package it as a `SyncSegment`, and ship it via the `SegmentLookupResult::Segment` internal variant. The wire envelope `SegmentResponse` (code 0xA3) is produced at the transport boundary by the DOT platform adapter layer. Handle `SegmentNotFound` by regenerating the snapshot per-table via `MVCCEngine::create_snapshot_for_table` (the new fork API method added to the Stoolap fork) and signalling the reader to re-fetch the summary (the `SegmentLookupResult::Regenerated` variant, mapped to wire envelope `0xA4 SegmentNotFound` at the transport boundary).

## Design

### New module: `crates/octo-sync/src/segment.rs`

The Sync protocol uses the existing Stoolap snapshot file format (the RFC-0862 §4.3 `SyncSegment.payload` is `<dsn-path>/snapshots/<table>/snapshot-<ts>.bin` — the same format produced by `MVCCEngine::create_snapshot` at `stoolap/src/storage/mvcc/engine.rs:2642`). The mission does NOT introduce a new `segment-NNNNNNNN.bin` filename; it reads the existing `snapshot-<ts>.bin` files and ships them as SyncSegments.

`segment_index` is the ordinal position of the snapshot file within its table directory, sorted lexicographically by filename. The mapping `segment_index → snapshot-<ts>.bin` is computed on demand from `std::fs::read_dir`.

```rust
use std::collections::VecDeque;

// (Note: VecDeque import is preserved for the SyncEngine's error queue, which is
// defined in 0862-base. The SegmentIndexer module does not use VecDeque directly.)

/// Internal writer-side return type for `SegmentIndexer::handle_segment_request`.
/// Distinct from the wire-level `SegmentResponse` envelope (RFC-0862 §Envelope Payload Discriminators 0xA3/0xA4).
/// The wire envelope is bytes-on-the-wire; this is a Rust enum used inside the
/// writer to distinguish the two success cases:
///
/// 1. Found a segment at the requested ordinal position with the requested root.
/// 2. Regenerated, but the new file is at a different ordinal position; reader must
///    re-fetch the summary.
///
/// The "not found at all" case is NOT a `SegmentLookupResult` variant — it propagates
/// as `Err(SyncError::SegmentNotFound { regenerated: false, .. })` and is converted
/// to wire envelope `0xA4 SegmentNotFound` at the wire boundary.
pub enum SegmentLookupResult {
    /// Found a segment file at the requested ordinal position with the requested root.
    Segment(SyncSegment),
    /// Regeneration succeeded but the new file is at a different ordinal position.
    /// The reader should re-fetch the summary and descend the new Merkle tree.
    Regenerated { table_id: u32, new_segment_count: u32 },
}

/// (No `to_envelope` function: `SegmentLookupResult` is internal-only. The wire envelope
/// mapping is done inline in `handle_segment_request` by matching the two variants and
/// the error case directly. Keeping the mapping inline avoids a layer of indirection
/// that doesn't earn its keep.)

pub struct SegmentIndexer {
    engine: Arc<MVCCEngine>,
    snapshot_dir: PathBuf,
    lz4_enabled: bool,
}

impl SegmentIndexer {
    /// Handle a SegmentRequest from a reader.
    ///
    /// Return-path semantics:
    /// - `Err(SyncError::SegmentNotFound { regenerated: false, .. })`: the requested
    ///   ordinal position is empty, or the file at that position has a different root.
    /// - `Ok(SegmentLookupResult::Regenerated { .. })`: regeneration succeeded, but the new
    ///   file is at a different ordinal position. The reader should re-fetch the summary
    ///   and descend the new Merkle tree. (At the wire boundary, this maps to envelope
    ///   `0xA4 SegmentNotFound`, which the reader treats as a hint to re-fetch.)
    /// - `Ok(SegmentLookupResult::Segment(..))`: found and ready to ship.
    /// - `Err(other)`: any other error (e.g., I/O error after retries).
    /// `SegmentNotFound` (error) and `Regenerated` (success) are distinct conceptually
    /// but share the same wire envelope (`0xA4`). The wire-level mapping is done at the
    /// transport boundary (not in this mission) by the DOT platform adapter layer.
    pub async fn handle_segment_request(
        &self,
        request: SegmentRequest,
    ) -> Result<SegmentLookupResult> {
        let table_dir = self.snapshot_dir.join(table_id_to_dir(request.table_id));
        let segment_file = match self.find_segment_file(&table_dir, request.segment_index) {
            Some(p) => p,
            None => return Err(SyncError::SegmentNotFound {
                table_id: request.table_id,
                segment_index: request.segment_index,
                regenerated: false,
            }),
        };

        let payload = match tokio::fs::read(&segment_file).await {
            Ok(p) => p,
            Err(_) => {
                // File deleted between read_dir and read; regenerate and signal the
                // reader to re-fetch the summary. The new file will have a new
                // timestamped name (newer than existing ones), so it sorts LAST
                // in the directory, NOT at the requested segment_index position.
                // The regenerated=true flag tells the reader to re-run Merkle descent.
                self.regenerate_snapshot(request.table_id, request.segment_index).await?;
                return Ok(SegmentLookupResult::Regenerated {
                    table_id: request.table_id,
                    new_segment_count: self.count_segments(&table_dir)?,
                });
            }
        };

        // Verify the segment matches the expected root
        let actual_root = blake3::hash(&payload).into();
        if actual_root != request.expected_root {
            return Err(SyncError::SegmentNotFound {
                table_id: request.table_id,
                segment_index: request.segment_index,
                regenerated: false,
            });
        }

        // LZ4-compress the entire file (including the 8-byte STSVSHD magic header).
        let (payload_for_ship, compression_flag) = if self.lz4_enabled && payload.len() > 1024 {
            (lz4_flex::compress(&payload), 1u8)
        } else {
            (payload.clone(), 0u8)
        };

        // CRC32 over the RAW (uncompressed) payload, matching the WAL V2 convention.
        let crc = crc32fast::hash(&payload);

        Ok(SegmentLookupResult::Segment(SyncSegment {
            table_id: request.table_id,
            segment_index: request.segment_index,
            segment_root: actual_root,
            payload: payload_for_ship,
            compression: compression_flag,
            crc32: crc,
            lsn_watermark: self.engine.wal_manager().current_lsn(),
        }))
    }

    /// Find the path of the segment at the given ordinal position, or None.
    /// Wraps `std::fs::read_dir` in `block_in_place` because this is called from
    /// an async function (`handle_segment_request`) and blocking the async runtime
    /// thread on filesystem metadata reads is unacceptable.
    ///
    /// Returns `Option<PathBuf>` (not `Option<Result<PathBuf, _>>`): DirEntry::path()
    /// returns `Result<PathBuf, io::Error>`, but we use `e.path().ok()` to drop the
    /// error and return None if the path is unreadable. This matches the function's
    /// `Option<PathBuf>` return type.
    fn find_segment_file(&self, table_dir: &Path, segment_index: u32) -> Option<PathBuf> {
        let table_dir = table_dir.to_path_buf();
        tokio::task::block_in_place(|| {
            let mut entries: Vec<_> = std::fs::read_dir(&table_dir).ok()?
                .filter_map(Result::ok)
                .filter(|e| {
                    let name = e.file_name().to_string_lossy().to_string();
                    name.starts_with("snapshot-") && name.ends_with(".bin")
                })
                .collect();
            entries.sort_by_key(|e| e.file_name());
            entries.get(segment_index as usize)
                .and_then(|e| e.path().ok())
        })
    }

    /// Count the number of snapshot files in a table directory.
    /// Uses `spawn_blocking` to avoid blocking the async runtime.
    /// The `?` operator requires `From<io::Error> for SyncError`; this is established
    /// in the `error.rs` module of 0862-base via `#[from] io::Error` derive.
    fn count_segments(&self, table_dir: &Path) -> Result<u32> {
        let table_dir = table_dir.to_path_buf();
        tokio::task::block_in_place(|| {
            Ok(std::fs::read_dir(&table_dir)?
                .filter_map(Result::ok)
                .filter(|e| {
                    let name = e.file_name().to_string_lossy().to_string();
                    name.starts_with("snapshot-") && name.ends_with(".bin")
                })
                .count() as u32)
        })
    }

    /// Regenerate the snapshot for a single table.
    /// Returns `Result<()>`; the caller (`handle_segment_request`) handles the
    /// regeneration result and returns `Ok(SegmentLookupResult::Regenerated{..})` to
    /// the reader.
    ///
    /// `_segment_index: u32` is currently unused (prefixed with `_`); the function
    /// regenerates the entire table, not a specific segment. This matches the
    /// RFC amendment: `MVCCEngine::create_snapshot_for_table(table_id, snapshot_dir)`
    /// regenerates ALL segments for the given table.
    async fn regenerate_snapshot(
        &self,
        table_id: u32,
        _segment_index: u32,
    ) -> Result<()> {
        // The new snapshot file will sort LAST (highest timestamp), not at
        // segment_index. The caller (handle_segment_request) returns
        // SegmentLookupResult::Regenerated{...}, which is converted to
        // wire-envelope 0xA4 SegmentNotFound at the wire boundary, signalling the
        // reader to re-fetch the summary.
        //
        // `&self.snapshot_dir` (a `PathBuf`) is automatically deref-coerced to `&Path`
        // via the `Deref<Target=Path>` impl on `PathBuf`. No explicit `.as_path()` needed.
        self.engine.create_snapshot_for_table(table_id, &self.snapshot_dir)?;
        Ok(())
    }
}

/// Map a `SyncSummary.table_id` (u32, BLAKE3-256 of table_name per RFC-0862 §4.3)
/// to a directory name under the DSN's `snapshots/` path. Defined here for
/// 0862c; will be moved to a shared `table_id` helper in 0862-base if other
/// missions need it.
fn table_id_to_dir(table_id: u32) -> std::path::PathBuf {
    std::path::PathBuf::from(format!("table-{:016x}", table_id))
}
```


### Fork API additions (resolves L-R3-2)

Mission 0862c adds one new method to the Stoolap fork:

```rust
// stoolap/src/storage/mvcc/engine.rs (Mission 0862c — fork API addition)
impl MVCCEngine {
    /// New method: create a snapshot for a specific table only.
    /// (Existing create_snapshot is for the entire DB.)
    pub fn create_snapshot_for_table(
        &self,
        table_id: u32,
        snapshot_dir: &Path,
    ) -> Result<()> {
        // existing create_snapshot logic, but filtered to one table
        // atomic-rename: write to snapshot-<ts>.tmp, rename to snapshot-<ts>.bin
    }
}
```

This is documented in the RFC's §Key Files to Modify; the RFC should be amended to add this method. (Amending the RFC is tracked as a follow-up action; the mission proceeds with the fork API addition pending the amendment.)

### Compression

LZ4 (`lz4_flex` crate, already in `stoolap/Cargo.toml:74`) is byte-deterministic. The writer compresses segments > 1 KB; the reader decompresses after verifying the CRC32 and segment_root.

## Acceptance Criteria

- [ ] `crates/octo-sync/src/segment.rs` exists with `SegmentIndexer` struct
- [ ] `handle_segment_request` reads the snapshot file by **ordinal position** (`segment_index`) and returns `SyncSegment`
- [ ] `SyncSegment` has all 7 fields: `table_id`, `segment_index`, `segment_root`, `payload`, `compression`, `crc32`, `lsn_watermark`
- [ ] `SyncSegment.segment_root == BLAKE3-256(raw_payload)` (verified AFTER decompression on the reader side)
- [ ] `SyncSegment.crc32 == crc32fast::hash(raw_payload)` (CRC32 is over the **raw** payload, matching WAL V2 convention; verified AFTER decompression)
- [ ] LZ4 compression: `compression = 1` when raw payload > 1 KB; `compression = 0` otherwise
- [ ] LZ4 wraps the **entire** file (including the 8-byte `STSVSHD` magic header); the reader decompresses first, then verifies both the magic and the segment_root
- [ ] `SegmentNotFound` (envelope `0xA4`) returned when expected_root doesn't match OR when the file at the requested ordinal position doesn't exist
- [ ] Snapshot regeneration: writer calls `MVCCEngine::create_snapshot_for_table` when the file is missing, then retries the read
- [ ] `MVCCEngine::create_snapshot_for_table` added to `stoolap/src/storage/mvcc/engine.rs` with atomic-rename semantics
- [ ] `SegmentRequest` and `SegmentResponse` envelopes (codes `0xA3` and `0xA4`) implemented in `envelope.rs`
- [ ] Unit tests for handle_segment_request with all 4 outcomes (file present + root matches, file present + root mismatches, file missing + regeneration succeeds, file missing + regeneration fails)
- [ ] Integration test: writer has 10 tables × 10 segments = 100 segments; reader requests all 100; reader applies; state matches
- [ ] The `snapshot-<ts>.bin` filename format from the existing Stoolap snapshot machinery is used; no new `segment-NNNNNNNN.bin` format is introduced

## Tests

- **Unit:**
  - File present + root matches → returns `SyncSegment`
  - File present + root mismatches → returns `SegmentNotFound`
  - File missing + regeneration succeeds → returns `Ok(SegmentLookupResult::Regenerated { table_id, new_segment_count })` (regeneration succeeded, but the new file is at a different ordinal position; reader must re-fetch the summary and re-descend the Merkle tree)
  - File missing + regeneration fails → returns error
  - LZ4 compression round-trip: `decompress(lz4(payload)) == payload` (LZ4 includes the STSVSHD magic header)
  - CRC32 verification: `crc32fast::hash(raw_payload) == segment.crc32` (CRC32 is over the raw, uncompressed payload)
  - `segment_root` verification: `BLAKE3-256(raw_payload) == segment.segment_root`
  - Atomic-rename: snapshot file never exists in a half-written state
  - `segment_index` resolution: 10 files in table dir, request `segment_index = 5` returns the 6th file sorted lexicographically
  - Out-of-range `segment_index` (≥ file count) returns `SegmentNotFound`

- **Integration:**
  - Writer with 1 table, 1M rows, 100 segments → reader requests 100 segments by ordinal index → reader applies → state matches
  - Writer deletes a segment file → reader requests it by the same ordinal index → writer regenerates (creating a new snapshot, possibly shifting ordinal positions) → reader retries → applies → state matches
  - Reader sends `SegmentRequest` for a non-existent table → writer returns `SegmentNotFound`
  - Reader sends `SegmentRequest` with wrong expected_root → writer returns `SegmentNotFound` (even if file exists at that ordinal position)

## Dependencies

- **Requires:**
  - `0862-base` — envelope types, identity, state machine
  - `0862a` — WAL-tail streamer (for LSN watermarks)
  - `0862b` — Merkle segment summary (for the divergent segment list)
  - `stoolap/src/storage/mvcc/snapshot.rs` — `MVCCEngine::create_snapshot` (for regeneration; the new `MVCCEngine::create_snapshot_for_table` method is an addition to the fork API, see "Fork API additions" below)
  - `stoolap/src/storage/mvcc/engine.rs:2642` (existing snapshot creation)
  - `stoolap/src/storage/mvcc/engine.rs:2828` (atomic-rename for the new `create_snapshot_for_table`)

- **Required by:**
  - `0862f` (multi-peer — multiple readers can request the same segments)
  - `0862h` (property tests for segment integrity)

## Blockers / Dependencies

- **Blocked by:** `0862-base`, `0862a`, `0862b`
- **Blocks:** `0862f`

## Description

The snapshot segment indexer is the second half of the catch-up flow. After the reader uses the Merkle summary to find divergent segments, it requests them one by one via `SegmentRequest`. The writer locates the snapshot file (or regenerates it if missing), packages it as a `SyncSegment`, and ships it. The reader verifies the segment_root and CRC32, applies the segment, and moves on to the next divergent segment.

## Technical Details

### Performance

- **Segment request throughput:** bounded by per-peer rate limit (100 envelopes/s sustained, 500 burst)
- **Segment size:** default 16 MB (matches `MVCCEngine::create_snapshot` block size)
- **LZ4 compression ratio:** typically 2-3x for table data; effective bandwidth is ~2x the raw WAL bandwidth
- **Regeneration cost:** O(table size) on first request after deletion; subsequent requests are O(1)

### Cargo dependencies (resolves N9)

The 0862c pseudocode references three external crates:

- `blake3` (already in `stoolap/Cargo.toml:111`)
- `lz4_flex` (already in `stoolap/Cargo.toml:74`)
- `crc32fast` — must be added to `crates/octo-sync/Cargo.toml` as a new direct dependency.

Acceptance criterion: "`crc32fast` ≥ 1.3 added to `crates/octo-sync/Cargo.toml` dependencies."

### Atomic-rename guarantee

Per RFC-0862 §Implicit Assumptions Audit row 17, the writer MUST never serve a half-written segment. `MVCCEngine::create_snapshot_for_table` MUST use the atomic-rename pattern:
1. Write to `snapshot-<ts>.tmp`
2. `fsync()` the file
3. `std::fs::rename` to `snapshot-<ts>.bin`

This is verified by the `snapshot.rs:37, 98` "STSVSHD" magic and the `engine.rs:2642`/`engine.rs:2828` atomic-rename pattern.

### Filename convention (resolves H3)

The Sync protocol uses the existing Stoolap snapshot file format (`<dsn-path>/snapshots/<table>/snapshot-<ts>.bin` — see RFC-0862 §4.3 `SyncSegment.payload` doc-comment and `stoolap/src/storage/mvcc/snapshot.rs:1533` for the `create_snapshot` return type). No new `segment-NNNNNNNN.bin` format is introduced. `segment_index` is the ordinal position of the snapshot file in its table directory (sorted by timestamp), not a filename.

### LZ4 vs STSVSHD magic (resolves L4)

LZ4 compression wraps the **entire** file including the 8-byte `STSVSHD` magic header at the start. The reader's apply path is:
1. LZ4-decompress the entire payload (if `compression == 1`).
2. Verify the first 8 bytes match `"STSVSHD"` (the magic from `snapshot.rs:38, 98`).
3. Verify `BLAKE3-256(raw_payload) == segment.segment_root`.
4. Verify `crc32fast::hash(raw_payload) == segment.crc32`.
5. Apply the segment via `MVCCEngine::replay_snapshot`.

The LZ4 wrapping includes the magic for byte-determinism: the LZ4 stream is byte-deterministic, so two implementations produce the same compressed bytes for the same input, and the magic is preserved through the round-trip.

### Pitfalls

- **Don't read the segment file without verifying the expected_root first.** A reader's `SegmentRequest` carries the root the reader expects; if the writer's actual root doesn't match, the writer must return `SegmentNotFound` (the reader may have a stale summary).
- **Don't compress segments < 1 KB.** LZ4 overhead makes small payloads larger than the raw form.
- **Don't hold the snapshot file lock during the entire shipment.** The reader's apply queue may be large; the writer should hand off the segment bytes to the transport and release the lock.
- **Don't use `tokio::fs` for the read inside a `spawn_blocking` task.** `tokio::fs` is async; the segment bytes may be > 16 MB.
- **Don't introduce a new `segment-NNNNNNNN.bin` filename format.** Use the existing `snapshot-<ts>.bin` format from `MVCCEngine::create_snapshot`. The `segment_index` is the ordinal position, not a filename.
- **Don't compute CRC32 over the compressed payload.** CRC32 is over the **raw** (uncompressed) payload, matching the WAL V2 trailer convention. The reader decompresses first, then verifies both CRC32 and segment_root on the raw bytes.

---

**Mission Type:** Implementation
**Priority:** High
**Phase:** 2 (Catch-up via snapshot segments)
**RFC Section Coverage:** §4.3.4 step 4 (segment shipping)

## Type Coverage

This mission implements the following RFC-0862 types:

| Type | Role in this mission |
|------|---------------------|
| `SyncSegment` | The per-table snapshot segment envelope (codes 0xA3/0xA4); the payload is a single `<dsn-path>/snapshots/<table>/snapshot-<ts>.bin` file |
| `SegmentIndexer` | The writer-side struct that handles `SegmentRequest` and produces `SyncSegment` (or `SegmentNotFound` for stale roots) |
| `MVCCEngine::create_snapshot_for_table` (new Stoolap fork method) | Generates a fresh snapshot for a single table when the requested segment is missing |

The mission does NOT implement the per-table Merkle summary (`SyncSummary`) — that is handled by mission 0862b. See the Type Coverage table in 0862-base for the full mapping.
