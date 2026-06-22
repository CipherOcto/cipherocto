//! LSN monotonicity enforcement (per RFC-0862 §4.3.2).
//!
//! LSNs (Logical Sequence Numbers) are append-only per the WAL V2 binary format at
//! `stoolap/src/storage/mvcc/wal_manager.rs:69`. The cipherocto sync engine uses
//! per-peer LSN watermarks to detect regressions (per G3 "Idempotency" and G5
//! "LSN model" in RFC-0862 §Design Goals).
//!
//! The `LsnTracker` is a per-peer monotonic counter. It is used by:
//! - `WalTailStreamer::on_commit` (mission 0862a) — to validate the LSN range
//! - `WalTailStreamer::on_lsn_ack` (mission 0862a) — to advance the per-peer watermark
//! - `apply_wal_entry` (mission 0862-base) — to detect out-of-order entries

use crate::error::SyncError;
use crate::types::Lsn;

/// Per-peer LSN watermark tracker.
///
/// Holds the highest LSN that has been applied for a given peer. Used to detect
/// regressions (any incoming LSN that is less than the watermark is rejected
/// with [`SyncError::LsnRegression`]). Gaps are allowed at this level; the
/// cipherocto sync engine uses a separate mechanism (in mission 0862a) to
/// detect missing LSNs at the `WalTailChunk` level.
///
/// # Example
///
/// ```
/// use octo_sync::lsn::LsnTracker;
/// use octo_sync::error::SyncError;
///
/// let mut tracker = LsnTracker::new();
/// assert_eq!(tracker.watermark(), 0);
///
/// // First entry at LSN 1
/// tracker.advance(1).unwrap();
/// assert_eq!(tracker.watermark(), 1);
///
/// // Same LSN again — idempotent
/// tracker.advance(1).unwrap();
/// assert_eq!(tracker.watermark(), 1);
///
/// // Gap is allowed (e.g., LSN 5: engine skipped 2-4)
/// tracker.advance(5).unwrap();
/// assert_eq!(tracker.watermark(), 5);
///
/// // Regression — should error
/// let err = tracker.advance(3).unwrap_err();
/// assert_eq!(err, SyncError::LsnRegression { expected: 5, actual: 3 });
/// ```
#[derive(Debug, Clone, Default)]
pub struct LsnTracker {
    /// The highest LSN that has been applied.
    watermark: Lsn,
}

impl LsnTracker {
    /// Create a new LSN tracker with watermark = 0.
    pub fn new() -> Self {
        Self { watermark: 0 }
    }

    /// Return the current LSN watermark.
    pub fn watermark(&self) -> Lsn {
        self.watermark
    }

    /// Advance the watermark to `lsn`.
    ///
    /// # Rules
    /// - If `lsn == watermark`, this is a no-op (idempotent).
    /// - If `lsn > watermark`, advance to `lsn` (a gap is allowed; the
    ///   cipherocto sync engine tracks missing LSNs at the chunk level via
    ///   `WalTailChunk.from_lsn != previous_chunk.to_lsn + 1`, not here).
    /// - If `lsn < watermark`, return [`SyncError::LsnRegression`] with
    ///   `expected = watermark` and `actual = lsn`.
    ///
    /// # Note
    ///
    /// A separate gap-detection mechanism (per
    /// `WalTailChunk.from_lsn != previous_chunk.to_lsn + 1`) is in mission 0862a.
    /// The per-peer `LsnTracker` is intentionally lenient about gaps because
    /// individual LSN updates are the unit of advance, not chunk ranges.
    pub fn advance(&mut self, lsn: Lsn) -> Result<(), SyncError> {
        if lsn <= self.watermark {
            if lsn == self.watermark {
                return Ok(()); // idempotent
            }
            return Err(SyncError::LsnRegression {
                expected: self.watermark,
                actual: lsn,
            });
        }
        // lsn > watermark: advance
        self.watermark = lsn;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_tracker_has_zero_watermark() {
        let t = LsnTracker::new();
        assert_eq!(t.watermark(), 0);
    }

    #[test]
    fn first_entry_at_lsn_1_advances() {
        let mut t = LsnTracker::new();
        t.advance(1).unwrap();
        assert_eq!(t.watermark(), 1);
    }

    #[test]
    fn same_lsn_is_idempotent() {
        let mut t = LsnTracker::new();
        t.advance(1).unwrap();
        t.advance(1).unwrap();
        assert_eq!(t.watermark(), 1);
    }

    #[test]
    fn regression_returns_error() {
        let mut t = LsnTracker::new();
        t.advance(100).unwrap();
        let err = t.advance(50).unwrap_err();
        assert_eq!(
            err,
            SyncError::LsnRegression {
                expected: 100,
                actual: 50
            }
        );
    }

    #[test]
    fn gap_is_allowed() {
        let mut t = LsnTracker::new();
        // Gaps are allowed at the per-peer watermark level; a separate
        // mechanism (in mission 0862a) detects missing LSNs at the chunk level.
        t.advance(10).unwrap();
        assert_eq!(t.watermark(), 10);
        t.advance(20).unwrap();
        assert_eq!(t.watermark(), 20);
    }

    #[test]
    fn consecutive_advances() {
        let mut t = LsnTracker::new();
        for i in 1..=1000 {
            t.advance(i).unwrap();
        }
        assert_eq!(t.watermark(), 1000);
    }
}
