//! The [`DatabaseSyncAdapter`] trait — the integration boundary between the cipherocto
//! sync engine and the underlying database.
//!
//! Per RFC-0862 v1.1.0 §DatabaseSyncAdapter Trait. The cipherocto sync engine does NOT
//! call Stoolap DB functions directly; it consumes this trait. The Stoolap fork
//! provides a `StoolapAdapter` impl (mission 0862-base §Stoolap fork changes); the
//! cipherocto sync engine is generic over `A: DatabaseSyncAdapter`.
//!
//! # Sync vs async
//!
//! This trait is **sync** (not `#[async_trait]`) per the cipherocto convention for
//! compute/state traits ([`Witness`](https://docs.rs/octo-network), `DeterministicProofSystem`,
//! `BINDHook` are also sync; `PlatformAdapter`, `CoordinatorAdmin` are async because they
//! do network I/O). Database operations are local disk I/O; the cipherocto async runtime
//! (`tokio`) wraps every trait call at the boundary via `tokio::task::spawn_blocking`.
//!
//! # Send + Sync + 'static
//!
//! The trait requires `Send + Sync + 'static`:
//! - `Send + Sync` — the cipherocto convention (see e.g. `PlatformAdapter: Send + Sync`).
//! - `'static` — needed to store the trait object in `Box<dyn DatabaseSyncAdapter + 'static>`
//!   and to satisfy the `'static` requirements of the cipherocto async runtime. None of the
//!   5 existing cipherocto adapter traits have this bound; it is a new addition justified
//!   by the trait-object storage pattern.
//!
//! # Error model
//!
//! Every method returns `Result<T, SyncError>`. The cipherocto sync engine maps
//! `SyncError` to the wire-level error codes (RFC-0862 §Error Handling) via
//! `From<SyncError> for WireError` in [`crate::error`].

use crate::error::SyncError;
use crate::types::{Lsn, MissionId, NodeId, SegmentIndex, TableId};

/// Snapshot segment payload returned by [`DatabaseSyncAdapter::read_snapshot_segment`].
///
/// The cipherocto sync engine applies its own LZ4 compression (per RFC-0862 §4.3.4) at
/// the transport boundary; the adapter returns the raw, uncompressed segment bytes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SnapshotSegment {
    /// The table this segment belongs to.
    pub table_id: TableId,
    /// The ordinal position of this segment in the table's snapshot directory.
    pub segment_index: SegmentIndex,
    /// The raw, uncompressed segment payload (typically a full
    /// `snapshot-<ts>.bin` file from the underlying DB).
    pub payload: Vec<u8>,
    /// The LSN watermark at the time the segment was generated.
    pub lsn_watermark: Lsn,
}

/// The integration boundary between the cipherocto sync engine and the underlying database.
///
/// # Method overview
///
/// | Method | Direction | Purpose |
/// |---|---|---|
/// | [`read_wal_range`](Self::read_wal_range) | writer | Ship raw WAL entries |
/// | [`current_lsn`](Self::current_lsn) | both | Monotonic LSN counter |
/// | [`apply_wal_entry`](Self::apply_wal_entry) | reader | Idempotent WAL apply |
/// | [`read_snapshot_segment`](Self::read_snapshot_segment) | reader | Merkle tree descent |
/// | [`write_snapshot_segment`](Self::write_snapshot_segment) | writer | Atomic-rename segment write |
/// | [`set_paused`](Self::set_paused) | reader → writer | Backpressure (default no-op) |
/// | [`mission_id`](Self::mission_id) | both | Per-mission identity |
/// | [`node_id`](Self::node_id) | both | `BLAKE3(public_key ‖ mission_id)` |
///
/// 8 methods total: 5 RFC-0862 ops + 1 backpressure + 2 auxiliary. The default
/// no-op `set_paused` allows databases that don't support writer-side pause to
/// opt out; the cipherocto sync engine falls back to per-peer rate-limiting.
pub trait DatabaseSyncAdapter: Send + Sync + 'static {
    // ── A. WAL-tail streaming (RFC-0862 §4.3.3) ──────────────────────

    /// Read WAL entries in the range `[from_lsn, to_lsn]` (inclusive on both ends).
    ///
    /// Returns the raw `WALEntry::encode()` bytes (not parsed) so the cipherocto sync
    /// engine can ship them verbatim per RFC-0862 §4.2.
    ///
    /// # Monotonicity
    ///
    /// MUST be monotonic: if `from_lsn < current_lsn()`, the call returns only the
    /// entries with LSN ≥ `from_lsn`; entries with LSN < `from_lsn` are silently
    /// dropped (they've already been shipped). The cipherocto sync engine relies on
    /// this to handle restart-after-crash correctly.
    ///
    /// # Errors
    ///
    /// - [`SyncError::InvalidLsnRange`] if `from_lsn > to_lsn`.
    /// - [`SyncError::LsnRegression`] if `from_lsn` is below the adapter's
    ///   current high-water mark (i.e., the entry range has already been shipped).
    /// - [`SyncError::BackendNotReady`] if the DB is shutting down or the apply
    ///   queue is full (the cipherocto sync engine retries with backoff).
    fn read_wal_range(&self, from_lsn: Lsn, to_lsn: Lsn) -> Result<Vec<Vec<u8>>, SyncError>;

    /// Return the current LSN of the database (highest LSN that has been committed).
    ///
    /// MUST be monotonic across calls (LSN counters are append-only per the WAL V2
    /// binary format at `stoolap/src/storage/mvcc/wal_manager.rs:69`).
    fn current_lsn(&self) -> Result<Lsn, SyncError>;

    /// Apply a single WAL entry to the database.
    ///
    /// The entry is the raw `WALEntry::encode()` output (not parsed). The cipherocto
    /// sync engine calls this on the reader side after a successful `WalTailChunk`
    /// reception and a verified `LsnAck`.
    ///
    /// # Durability
    ///
    /// MUST persist the entry to the write-ahead log. After this call returns,
    /// the entry MUST be readable via `read_wal_range(entry.lsn, entry.lsn)`.
    /// This is required for chain relay topologies where intermediate nodes
    /// forward entries to downstream peers (see RFC-0862 §Chain Relay).
    ///
    /// # LSN Advancement
    ///
    /// MUST advance `current_lsn()` if the applied entry's LSN exceeds the
    /// current value. The LSN counter must remain monotonic (never decrease).
    /// This ensures downstream peers can detect new entries via `current_lsn()`.
    ///
    /// # Idempotency
    ///
    /// MUST be idempotent: replaying the same entry twice is a no-op (the WAL V2
    /// binary format is designed for this). The adapter MUST NOT advance the LSN
    /// counter on replay of an already-applied entry.
    ///
    /// # Chain Relay Semantics
    ///
    /// In a chain relay topology (A → B → C), node B receives entries from A
    /// via `apply_wal_entry`. Node C then connects to B and calls
    /// `read_wal_range` to fetch those entries. For this to work:
    /// 1. B's `apply_wal_entry` MUST persist to WAL (not just in-memory state)
    /// 2. B's `current_lsn()` MUST reflect the applied entries
    /// 3. B's `read_wal_range` MUST return the persisted entries
    ///
    /// If the adapter only applies to in-memory state without WAL persistence,
    /// chain relay will fail silently (downstream peers receive no entries).
    fn apply_wal_entry(&self, entry: &[u8]) -> Result<(), SyncError>;

    // ── B. Anti-entropy Merkle summary (RFC-0862 §4.3.4) ─────────────

    /// Read the snapshot segment at ordinal position `segment_index` in the snapshot
    /// directory for `table_id`.
    ///
    /// Returns `Ok(Some(segment))` if the file exists, `Ok(None)` if no file at that
    /// position (the cipherocto sync engine interprets `None` as a signal to descend
    /// the Merkle tree or request a different ordinal).
    ///
    /// The payload is the **uncompressed** segment bytes (the cipherocto sync engine
    /// applies its own LZ4 compression per RFC-0862 §4.3.4). The `STSVSHD` magic and
    /// atomic-rename semantics (per `stoolap/src/storage/mvcc/snapshot.rs:37,98`) are
    /// the underlying database's responsibility.
    ///
    /// # Errors
    ///
    /// - [`SyncError::SegmentNotFound`] if the file is missing or the root doesn't
    ///   match the expected value. The `regenerated` flag is set by the adapter if
    ///   it has already triggered a regeneration (in which case the reader should
    ///   re-fetch the summary).
    fn read_snapshot_segment(
        &self,
        table_id: TableId,
        segment_index: SegmentIndex,
    ) -> Result<Option<SnapshotSegment>, SyncError>;

    /// Write a snapshot segment at ordinal position `segment_index` in the snapshot
    /// directory for `table_id`.
    ///
    /// The `payload` is the uncompressed segment bytes (typically the full
    /// `snapshot-<ts>.bin` file). Returns once the segment is durably written
    /// (atomic-rename completed).
    ///
    /// # Atomicity
    ///
    /// MUST be atomic: either the segment is fully visible to subsequent
    /// `read_snapshot_segment` calls, or it is not visible at all. The atomic-rename
    /// pattern at `stoolap/src/storage/mvcc/engine.rs:2642` / `:2828` is the
    /// canonical implementation.
    fn write_snapshot_segment(
        &self,
        table_id: TableId,
        segment_index: SegmentIndex,
        payload: &[u8],
    ) -> Result<(), SyncError>;

    /// Regenerate ALL snapshot segments for a table.
    ///
    /// This is called by the cipherocto sync engine when a `SegmentRequest`
    /// fails (the requested segment is missing or has a stale root). The
    /// adapter impl is responsible for invoking the underlying database's
    /// per-table snapshot API (e.g., `MVCCEngine::create_snapshot_for_table`
    /// in the Stoolap fork).
    ///
    /// Returns the new segment count after regeneration. The cipherocto
    /// sync engine uses this to decide whether to re-send the summary or
    /// re-attempt the segment request.
    fn regenerate_snapshot(&self, table_id: TableId) -> Result<u32, SyncError>;

    // ── C. LSN model and backpressure (RFC-0862 §4.3.2) ──────────────

    /// Set or clear the writer's pause flag.
    ///
    /// The cipherocto sync engine calls this when the reader's apply queue exceeds
    /// 10K entries (per RFC-0862 §4.3.2). When `paused = true`, the writer skips
    /// fan-out in `WalTailStreamer::on_commit`; the LSN counter still advances.
    /// When `paused = false`, normal fan-out resumes.
    ///
    /// # Default implementation
    ///
    /// The default no-op allows databases that don't support writer-side pause to
    /// ignore the call; the cipherocto sync engine falls back to per-peer
    /// rate-limiting in that case.
    fn set_paused(&self, _paused: bool) -> Result<(), SyncError> {
        Ok(())
    }

    // ── D. Identity, key hierarchy, and trust (RFC-0862 §4.3.1) ──────

    /// Return the mission ID that this database instance is bound to.
    ///
    /// The cipherocto sync engine uses this to derive the per-mission `transport_key`
    /// and `execution_key` via `HKDF-BLAKE3(mission_root_key, "sync:v1", mission_id)`
    /// (per RFC-0862 §4.3.1 and mission 0862d).
    fn mission_id(&self) -> Result<MissionId, SyncError>;

    /// Return the local node's `SyncNodeId = BLAKE3(public_key || mission_id)`.
    ///
    /// MUST be stable for the lifetime of the sync session (per RFC-0862
    /// §Implicit Assumptions Audit row 5: "Node identity is stable for the
    /// duration of a sync session"). The cipherocto sync engine caches this
    /// value at session start.
    fn node_id(&self) -> Result<NodeId, SyncError>;
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Compile-time check: the trait can be used as a trait object.
    #[test]
    fn trait_object_compiles() {
        fn _accepts_trait_object(_a: Box<dyn DatabaseSyncAdapter>) {}
    }
}
