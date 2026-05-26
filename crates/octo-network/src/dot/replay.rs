//! Replay cache for DOT envelopes
//!
//! RFC-0850 §11.2: Canonical Replay Protection

use std::collections::BTreeMap;

use super::error::DotError;

/// Replay cache for envelope deduplication
///
/// Uses BTreeMap for deterministic iteration order.
/// Eviction removes the entry with smallest first_seen timestamp.
pub struct ReplayCache {
    seen: BTreeMap<[u8; 32], u64>,
    window_duration: u64,
    max_entries: u32,
}

impl ReplayCache {
    /// Create a new replay cache
    pub fn new(window_duration: u64, max_entries: u32) -> Self {
        Self {
            seen: BTreeMap::new(),
            window_duration,
            max_entries,
        }
    }

    /// Check if envelope_id is a replay. If not, insert it.
    ///
    /// Determinism: BTreeMap provides sorted iteration order.
    /// Eviction removes the entry with smallest first_seen timestamp.
    pub fn check_and_insert(
        &mut self,
        envelope_id: [u8; 32],
        current_epoch: u64,
    ) -> Result<(), DotError> {
        // Always purge expired entries first
        self.purge_expired(current_epoch);

        if let Some(&first_seen) = self.seen.get(&envelope_id) {
            return Err(DotError::ReplayDetected {
                envelope_id,
                first_seen,
            });
        }

        // If at capacity, evict oldest entry (BTreeMap is sorted)
        if self.seen.len() >= self.max_entries as usize {
            if let Some(oldest_key) = self.seen.keys().next().copied() {
                self.seen.remove(&oldest_key);
            }
        }

        self.seen.insert(envelope_id, current_epoch);
        Ok(())
    }

    /// Remove entries outside the replay window.
    fn purge_expired(&mut self, current_epoch: u64) {
        let cutoff = current_epoch.saturating_sub(self.window_duration);
        self.seen.retain(|_, &mut ts| ts > cutoff);
    }

    /// Get the number of entries in the cache
    pub fn len(&self) -> usize {
        self.seen.len()
    }

    /// Check if the cache is empty
    pub fn is_empty(&self) -> bool {
        self.seen.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_replay_cache_insert() {
        let mut cache = ReplayCache::new(3600, 1000);
        let envelope_id = [0x01; 32];
        assert!(cache.check_and_insert(envelope_id, 100).is_ok());
        assert_eq!(cache.len(), 1);
    }

    #[test]
    fn test_replay_cache_detects_replay() {
        let mut cache = ReplayCache::new(3600, 1000);
        let envelope_id = [0x01; 32];
        assert!(cache.check_and_insert(envelope_id, 100).is_ok());
        assert!(cache.check_and_insert(envelope_id, 101).is_err());
    }

    #[test]
    fn test_replay_cache_different_ids() {
        let mut cache = ReplayCache::new(3600, 1000);
        let id1 = [0x01; 32];
        let id2 = [0x02; 32];
        assert!(cache.check_and_insert(id1, 100).is_ok());
        assert!(cache.check_and_insert(id2, 100).is_ok());
        assert_eq!(cache.len(), 2);
    }

    #[test]
    fn test_replay_cache_eviction() {
        // Large window so only capacity eviction triggers
        let mut cache = ReplayCache::new(10000, 3);
        let id1 = [0x01; 32];
        let id2 = [0x02; 32];
        let id3 = [0x03; 32];
        let id4 = [0x04; 32];

        assert!(cache.check_and_insert(id1, 100).is_ok());
        assert!(cache.check_and_insert(id2, 110).is_ok());
        assert!(cache.check_and_insert(id3, 120).is_ok());
        assert_eq!(cache.len(), 3);

        // At capacity, inserting a 4th triggers capacity eviction (removes oldest by BTreeMap key)
        assert!(cache.check_and_insert(id4, 130).is_ok());
        assert_eq!(cache.len(), 3); // Still at max capacity
    }

    #[test]
    fn test_replay_cache_window_expiry() {
        // Small window (50 epochs) and small capacity (3)
        let mut cache = ReplayCache::new(50, 3);
        let id1 = [0x01; 32];
        let id2 = [0x02; 32];
        let id3 = [0x03; 32];
        let id4 = [0x04; 32];

        assert!(cache.check_and_insert(id1, 100).is_ok());
        assert!(cache.check_and_insert(id2, 110).is_ok());
        assert!(cache.check_and_insert(id3, 120).is_ok());

        // Insert at epoch 200: cutoff = 200 - 50 = 150
        // id1 (ts=100), id2 (ts=110), id3 (ts=120) all have ts <= 150 → evicted
        assert!(cache.check_and_insert(id4, 200).is_ok());
        // All old entries evicted by window, only id4 remains
        assert_eq!(cache.len(), 1);

        // id1 can now be reinserted (was evicted)
        assert!(cache.check_and_insert(id1, 200).is_ok());
        assert_eq!(cache.len(), 2);
    }
}
