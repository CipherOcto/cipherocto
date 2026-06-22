//! Test utilities for the octo-sync crate.
//!
//! Provides [`MockAdapter`], an in-memory implementation of
//! [`DatabaseSyncAdapter`](crate::DatabaseSyncAdapter) for unit tests.
//!
//! The MockAdapter is **always** built in test mode (i.e., the module is gated on
//! `#[cfg(any(test, feature = "test-util"))]`). The `test-util` feature flag is
//! intended for downstream crates (e.g., the cipherocto sync engine) that want to
//! depend on `octo-sync` with the MockAdapter enabled in their test builds.

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;

use parking_lot::Mutex;

use crate::adapter::{DatabaseSyncAdapter, SnapshotSegment};
use crate::error::SyncError;
use crate::types::{Lsn, MissionId, NodeId, SegmentIndex, TableId};

/// In-memory implementation of [`DatabaseSyncAdapter`] for unit tests.
///
/// Stores:
/// - WAL entries: `Vec<(Lsn, Vec<u8>)>`, in insertion order. `read_wal_range`
///   returns entries with LSN in `[from, to]` and `>= current_high_watermark - evicted_below`.
/// - Snapshots: `HashMap<(TableId, SegmentIndex), Vec<u8>>`.
/// - LSN counter: `AtomicU64`, incremented on every `apply_wal_entry` and read by
///   `current_lsn` and `read_wal_range`.
/// - Pause flag: `AtomicBool`, toggled by `set_paused`.
/// - Identity: `MissionId` and `NodeId` (set at construction).
///
/// Concurrency: all mutable state is behind `parking_lot::Mutex` or atomics.
/// The struct is `Send + Sync` (suitable for the trait object).
#[derive(Debug, Clone)]
pub struct MockAdapter {
    inner: Arc<MockAdapterInner>,
}

#[derive(Debug)]
struct MockAdapterInner {
    /// WAL entries in insertion order. Each entry is `(lsn, encoded_bytes)`.
    wal: Mutex<Vec<(Lsn, Vec<u8>)>>,
    /// The highest LSN that has been applied via `apply_wal_entry`. Used as the
    /// "current_lsn" value and as the upper bound for `read_wal_range` consistency.
    current_lsn: AtomicU64,
    /// Per-table, per-segment-index snapshot payloads.
    snapshots: Mutex<HashMap<(TableId, SegmentIndex), Vec<u8>>>,
    /// Pause flag.
    paused: AtomicBool,
    /// Identity.
    mission_id: MissionId,
    node_id: NodeId,
}

impl MockAdapter {
    /// Create a new `MockAdapter` with the given identity.
    ///
    /// The mission_id and node_id MUST be 32 bytes each.
    pub fn new(mission_id: MissionId, node_id: NodeId) -> Self {
        Self {
            inner: Arc::new(MockAdapterInner {
                wal: Mutex::new(Vec::new()),
                current_lsn: AtomicU64::new(0),
                snapshots: Mutex::new(HashMap::new()),
                paused: AtomicBool::new(false),
                mission_id,
                node_id,
            }),
        }
    }

    /// Test-only helper: append a WAL entry with the given LSN and bytes.
    ///
    /// This bypasses the trait's `apply_wal_entry` (which only sets the LSN and
    /// stores the entry) and lets tests pre-populate the WAL. Useful for testing
    /// the cipherocto sync engine's `read_wal_range` behavior.
    pub fn append_wal_entry(&self, lsn: Lsn, entry: Vec<u8>) {
        let mut wal = self.inner.wal.lock();
        wal.push((lsn, entry));
        // Advance the current_lsn counter
        let prev = self.inner.current_lsn.load(Ordering::SeqCst);
        if lsn > prev {
            self.inner.current_lsn.store(lsn, Ordering::SeqCst);
        }
    }

    /// Test-only helper: count WAL entries currently in the mock.
    pub fn wal_entry_count(&self) -> usize {
        self.inner.wal.lock().len()
    }

    /// Test-only helper: insert a snapshot segment.
    pub fn put_snapshot(&self, table_id: TableId, segment_index: SegmentIndex, payload: Vec<u8>) {
        self.inner.snapshots.lock().insert((table_id, segment_index), payload);
    }

    /// Test-only helper: read the pause flag.
    pub fn is_paused(&self) -> bool {
        self.inner.paused.load(Ordering::SeqCst)
    }
}

impl DatabaseSyncAdapter for MockAdapter {
    fn read_wal_range(&self, from_lsn: Lsn, to_lsn: Lsn) -> Result<Vec<Vec<u8>>, SyncError> {
        if from_lsn > to_lsn {
            return Err(SyncError::InvalidLsnRange { from: from_lsn, to: to_lsn });
        }
        let wal = self.inner.wal.lock();
        Ok(wal
            .iter()
            .filter(|(lsn, _)| *lsn >= from_lsn && *lsn <= to_lsn)
            .map(|(_, entry)| entry.clone())
            .collect())
    }

    fn current_lsn(&self) -> Result<Lsn, SyncError> {
        Ok(self.inner.current_lsn.load(Ordering::SeqCst))
    }

    fn apply_wal_entry(&self, entry: &[u8]) -> Result<(), SyncError> {
        // The MockAdapter doesn't parse the entry; it just records it. The next
        // LSN is current_lsn + 1.
        let lsn = self.inner.current_lsn.fetch_add(1, Ordering::SeqCst) + 1;
        self.inner.wal.lock().push((lsn, entry.to_vec()));
        Ok(())
    }

    fn read_snapshot_segment(
        &self,
        table_id: TableId,
        segment_index: SegmentIndex,
    ) -> Result<Option<SnapshotSegment>, SyncError> {
        let snapshots = self.inner.snapshots.lock();
        let payload = snapshots.get(&(table_id, segment_index)).cloned();
        Ok(payload.map(|p| SnapshotSegment {
            table_id,
            segment_index,
            payload: p,
            lsn_watermark: self.inner.current_lsn.load(Ordering::SeqCst),
        }))
    }

    fn write_snapshot_segment(
        &self,
        table_id: TableId,
        segment_index: SegmentIndex,
        payload: &[u8],
    ) -> Result<(), SyncError> {
        self.inner
            .snapshots
            .lock()
            .insert((table_id, segment_index), payload.to_vec());
        Ok(())
    }

    fn set_paused(&self, paused: bool) -> Result<(), SyncError> {
        self.inner.paused.store(paused, Ordering::SeqCst);
        Ok(())
    }

    fn mission_id(&self) -> Result<MissionId, SyncError> {
        Ok(self.inner.mission_id)
    }

    fn node_id(&self) -> Result<NodeId, SyncError> {
        Ok(self.inner.node_id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_identity() -> (MissionId, NodeId) {
        let mut mid = [0u8; 32];
        mid[0] = 0xAB;
        let mut nid = [0u8; 32];
        nid[0] = 0xCD;
        (mid, nid)
    }

    #[test]
    fn read_wal_range_filters_by_lsn() {
        let (mid, nid) = sample_identity();
        let a = MockAdapter::new(mid, nid);
        a.append_wal_entry(1, b"entry1".to_vec());
        a.append_wal_entry(2, b"entry2".to_vec());
        a.append_wal_entry(3, b"entry3".to_vec());

        let r = a.read_wal_range(1, 2).unwrap();
        assert_eq!(r.len(), 2);
        assert_eq!(r[0], b"entry1");
        assert_eq!(r[1], b"entry2");
    }

    #[test]
    fn read_wal_range_invalid_returns_err() {
        let (mid, nid) = sample_identity();
        let a = MockAdapter::new(mid, nid);
        let err = a.read_wal_range(5, 2).unwrap_err();
        assert_eq!(
            err,
            SyncError::InvalidLsnRange { from: 5, to: 2 }
        );
    }

    #[test]
    fn current_lsn_tracks_apply() {
        let (mid, nid) = sample_identity();
        let a = MockAdapter::new(mid, nid);
        assert_eq!(a.current_lsn().unwrap(), 0);
        a.apply_wal_entry(b"x").unwrap();
        assert_eq!(a.current_lsn().unwrap(), 1);
        a.apply_wal_entry(b"y").unwrap();
        assert_eq!(a.current_lsn().unwrap(), 2);
    }

    #[test]
    fn apply_then_read_wal_round_trip() {
        let (mid, nid) = sample_identity();
        let a = MockAdapter::new(mid, nid);
        a.apply_wal_entry(b"hello").unwrap();
        a.apply_wal_entry(b"world").unwrap();
        let entries = a.read_wal_range(1, 2).unwrap();
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0], b"hello");
        assert_eq!(entries[1], b"world");
    }

    #[test]
    fn read_snapshot_segment_returns_none_for_missing() {
        let (mid, nid) = sample_identity();
        let a = MockAdapter::new(mid, nid);
        let r = a.read_snapshot_segment(42, 7).unwrap();
        assert!(r.is_none());
    }

    #[test]
    fn write_then_read_snapshot_segment() {
        let (mid, nid) = sample_identity();
        let a = MockAdapter::new(mid, nid);
        a.put_snapshot(42, 7, b"segment-payload".to_vec());
        let r = a.read_snapshot_segment(42, 7).unwrap().unwrap();
        assert_eq!(r.table_id, 42);
        assert_eq!(r.segment_index, 7);
        assert_eq!(r.payload, b"segment-payload");
    }

    #[test]
    fn set_paused_toggles_flag() {
        let (mid, nid) = sample_identity();
        let a = MockAdapter::new(mid, nid);
        assert!(!a.is_paused());
        a.set_paused(true).unwrap();
        assert!(a.is_paused());
        a.set_paused(false).unwrap();
        assert!(!a.is_paused());
    }

    #[test]
    fn identity_methods_return_constructor_values() {
        let (mid, nid) = sample_identity();
        let a = MockAdapter::new(mid, nid);
        assert_eq!(a.mission_id().unwrap(), mid);
        assert_eq!(a.node_id().unwrap(), nid);
    }

    #[test]
    fn mock_is_send_and_sync() {
        // Compile-time check: MockAdapter can be shared across threads.
        fn assert_send_sync<T: Send + Sync>() {}
        assert_send_sync::<MockAdapter>();
        let (mid, nid) = sample_identity();
        let a = MockAdapter::new(mid, nid);
        // Use it as a trait object to exercise the 'static bound.
        let _boxed: Box<dyn DatabaseSyncAdapter> = Box::new(a);
    }
}
