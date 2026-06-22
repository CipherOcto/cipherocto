//! Snapshot segment indexer (per RFC-0862 §4.3.4, mission 0862c).
//!
//! Handles `SegmentRequest` from a reader: locates the requested segment
//! via the `DatabaseSyncAdapter` trait, packages it as a `SyncSegment`,
//! and ships it back to the reader.
//!
//! Per RFC-0862 v1.1.0 §Migration path step v1.1.0.d, all segment reads and
//! writes go through the trait. The cipherocto sync engine never calls
//! `MVCCEngine::create_snapshot_for_table` directly; the underlying
//! `StoolapAdapter` impl handles that internally.

use std::sync::Arc;

use blake3::Hasher;

use crate::adapter::{DatabaseSyncAdapter, SnapshotSegment as AdapterSegment};
use crate::error::SyncError;
use crate::types::{Lsn, SegmentIndex, TableId};

/// Result of a `SegmentRequest` (writer-side return type).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SegmentLookupResult {
    /// Found a segment file at the requested ordinal position with the requested root.
    Segment(SyncSegment),
    /// Regeneration succeeded but the new file is at a different ordinal position.
    /// The reader should re-fetch the summary and descend the new Merkle tree.
    Regenerated {
        /// The table that was regenerated.
        table_id: u32,
        /// The new segment count for the table.
        new_segment_count: u32,
    },
}

/// A per-table snapshot segment envelope (RFC-0862 §4.3, code 0xA3).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SyncSegment {
    /// The table this segment belongs to.
    pub table_id: TableId,
    /// The ordinal position in the table's snapshot directory.
    pub segment_index: SegmentIndex,
    /// The BLAKE3-256 root of the segment payload.
    pub segment_root: [u8; 32],
    /// The LZ4-compressed segment payload (matches `lz4_flex::compress`).
    pub payload: Vec<u8>,
    /// The compression flag: 0 = raw, 1 = LZ4.
    pub compression: u8,
    /// CRC32 over the raw (uncompressed) payload, matching the WAL V2 trailer convention.
    pub crc32: u32,
    /// The LSN watermark at the time the segment was generated.
    pub lsn_watermark: Lsn,
}

/// The snapshot segment indexer.
///
/// Holds the `DatabaseSyncAdapter` trait object (per RFC-0862 v1.1.0).
pub struct SegmentIndexer {
    /// The database adapter (trait object).
    adapter: Arc<dyn DatabaseSyncAdapter>,
    /// Whether to LZ4-compress segments > 1 KB.
    lz4_enabled: bool,
}

impl SegmentIndexer {
    /// Create a new `SegmentIndexer`.
    pub fn new(adapter: Arc<dyn DatabaseSyncAdapter>) -> Self {
        Self { adapter, lz4_enabled: true }
    }

    /// Set whether to LZ4-compress segments.
    pub fn with_lz4(mut self, enabled: bool) -> Self {
        self.lz4_enabled = enabled;
        self
    }

    /// Handle a `SegmentRequest` from a reader.
    ///
    /// Returns the `SegmentLookupResult` (Segment or Regenerated) on success,
    /// or `Err(SyncError::SegmentNotFound)` if the file is missing or the
    /// root doesn't match.
    pub async fn handle_segment_request(
        &self,
        table_id: TableId,
        segment_index: SegmentIndex,
        expected_root: [u8; 32],
    ) -> Result<SegmentLookupResult, SyncError> {
        // Per RFC-0862 v1.1.0, the segment read goes through the trait.
        let segment: Option<AdapterSegment> = self
            .adapter
            .read_snapshot_segment(table_id, segment_index)?;
        let segment = match segment {
            Some(s) => s,
            None => {
                return Err(SyncError::SegmentNotFound {
                    table_id,
                    segment_index,
                    regenerated: false,
                });
            }
        };
        // Verify the segment matches the expected root.
        let actual_root = blake3_hash(&segment.payload);
        if actual_root != expected_root {
            return Err(SyncError::SegmentNotFound {
                table_id,
                segment_index,
                regenerated: false,
            });
        }
        // LZ4-compress the payload if enabled and the payload is > 1 KB.
        let (payload_for_ship, compression_flag) = if self.lz4_enabled && segment.payload.len() > 1024 {
            (lz4_flex::compress(&segment.payload), 1u8)
        } else {
            (segment.payload.clone(), 0u8)
        };
        // CRC32 over the raw (uncompressed) payload.
        let crc = crc32(&segment.payload);
        // LSN watermark comes from the adapter (NOT from self.engine.wal_manager()).
        let lsn_watermark = self.adapter.current_lsn()?;
        Ok(SegmentLookupResult::Segment(SyncSegment {
            table_id,
            segment_index,
            segment_root: actual_root,
            payload: payload_for_ship,
            compression: compression_flag,
            crc32: crc,
            lsn_watermark,
        }))
    }

    /// Request a snapshot regeneration via the adapter.
    /// The StoolapAdapter impl calls `MVCCEngine::create_snapshot_for_table`
    /// internally and returns the new segment count.
    pub async fn regenerate_snapshot(
        &self,
        table_id: TableId,
    ) -> Result<SegmentLookupResult, SyncError> {
        // Delegate to the trait; the adapter impl handles the actual
        // MVCCEngine call and returns the new segment count.
        let new_segment_count = self.adapter.regenerate_snapshot(table_id)?;
        Ok(SegmentLookupResult::Regenerated {
            table_id,
            new_segment_count,
        })
    }
}

/// BLAKE3-256 hash helper.
fn blake3_hash(data: &[u8]) -> [u8; 32] {
    let mut hasher = Hasher::new();
    hasher.update(data);
    *hasher.finalize().as_bytes()
}

/// CRC32 helper using the standard polynomial. The Stoolap fork uses
/// `crc32fast`; for the cipherocto sync engine, we use a simple table-based
/// implementation that matches the WAL V2 trailer convention.
fn crc32(data: &[u8]) -> u32 {
    let mut crc: u32 = 0xFFFFFFFF;
    for &byte in data {
        crc ^= byte as u32;
        for _ in 0..8 {
            crc = if crc & 1 != 0 { (crc >> 1) ^ 0xEDB88320 } else { crc >> 1 };
        }
    }
    !crc
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_util::MockAdapter;

    #[tokio::test]
    async fn missing_segment_returns_not_found() {
        let a: Arc<dyn DatabaseSyncAdapter> =
            Arc::new(MockAdapter::new([0u8; 32], [0u8; 32]));
        let idx = SegmentIndexer::new(a);
        let err = idx
            .handle_segment_request(42, 7, [1u8; 32])
            .await
            .unwrap_err();
        assert_eq!(
            err,
            SyncError::SegmentNotFound {
                table_id: 42,
                segment_index: 7,
                regenerated: false,
            }
        );
    }

    #[tokio::test]
    async fn present_segment_with_matching_root_succeeds() {
        let adapter: Arc<MockAdapter> = Arc::new(MockAdapter::new([0u8; 32], [0u8; 32]));
        let payload = b"test-segment-payload".to_vec();
        let expected_root = blake3_hash(&payload);
        adapter.put_snapshot(42, 7, payload.clone());
        let idx = SegmentIndexer::new(adapter as Arc<dyn DatabaseSyncAdapter>);
        let result = idx
            .handle_segment_request(42, 7, expected_root)
            .await
            .unwrap();
        match result {
            SegmentLookupResult::Segment(s) => {
                assert_eq!(s.table_id, 42);
                assert_eq!(s.segment_index, 7);
                assert_eq!(s.segment_root, expected_root);
                // Compression: 0 because payload is < 1 KB
                assert_eq!(s.compression, 0);
                // CRC32 over the raw payload
                assert_eq!(s.crc32, crc32(&payload));
            }
            _ => panic!("expected Segment variant"),
        }
    }

    #[tokio::test]
    async fn present_segment_with_mismatched_root_returns_not_found() {
        let adapter: Arc<MockAdapter> = Arc::new(MockAdapter::new([0u8; 32], [0u8; 32]));
        adapter.put_snapshot(42, 7, b"some-bytes".to_vec());
        let idx = SegmentIndexer::new(adapter as Arc<dyn DatabaseSyncAdapter>);
        let err = idx
            .handle_segment_request(42, 7, [1u8; 32]) // wrong expected root
            .await
            .unwrap_err();
        assert_eq!(
            err,
            SyncError::SegmentNotFound {
                table_id: 42,
                segment_index: 7,
                regenerated: false,
            }
        );
    }

    #[test]
    fn blake3_hash_is_deterministic() {
        let h1 = blake3_hash(b"hello");
        let h2 = blake3_hash(b"hello");
        assert_eq!(h1, h2);
    }

    #[test]
    fn blake3_hash_differs_for_different_inputs() {
        assert_ne!(blake3_hash(b"hello"), blake3_hash(b"world"));
    }

    #[test]
    fn crc32_known_value() {
        // Known CRC32 of "123456789" is 0xCBF43926
        assert_eq!(crc32(b"123456789"), 0xCBF43926);
    }
}
