//! Replay cache for DOT envelopes
//!
//! RFC-0850 §11.2: Canonical Replay Protection
//!
//! Eviction strategy: when at capacity, evicts the entry with the smallest
//! (first_seen, envelope_id) pair — matching the RFC's deterministic rule:
//! "smallest first_seen timestamp; if equal, lexicographically smallest envelope_id."

use std::collections::BTreeMap;

use super::error::DotError;

/// Replay cache for envelope deduplication.
///
/// Two maps maintain the same data in different orderings:
/// - `by_id`: envelope_id → first_seen (O(1) duplicate lookup)
/// - `by_time`: (first_seen, envelope_id) → () (deterministic eviction by time, then ID)
pub struct ReplayCache {
    by_id: BTreeMap<[u8; 32], u64>,
    by_time: BTreeMap<(u64, [u8; 32]), ()>,
    window_duration: u64,
    max_entries: u32,
}

impl ReplayCache {
    /// Create a new replay cache.
    ///
    /// # Panics
    /// Panics if `max_entries` is 0.
    pub fn new(window_duration: u64, max_entries: u32) -> Self {
        assert!(max_entries > 0, "max_entries must be > 0");
        Self {
            by_id: BTreeMap::new(),
            by_time: BTreeMap::new(),
            window_duration,
            max_entries,
        }
    }

    /// Check if envelope_id is a replay. If not, insert it.
    ///
    /// Determinism: eviction removes the entry with smallest (first_seen, envelope_id).
    pub fn check_and_insert(
        &mut self,
        envelope_id: [u8; 32],
        current_epoch: u64,
    ) -> Result<(), DotError> {
        // Always purge expired entries first
        self.purge_expired(current_epoch);

        if let Some(&first_seen) = self.by_id.get(&envelope_id) {
            return Err(DotError::ReplayDetected {
                envelope_id,
                first_seen,
            });
        }

        // If at capacity, evict oldest entry by (first_seen, envelope_id)
        if self.by_id.len() >= self.max_entries as usize {
            self.evict_oldest();
        }

        self.by_id.insert(envelope_id, current_epoch);
        self.by_time.insert((current_epoch, envelope_id), ());
        Ok(())
    }

    /// Remove entries outside the replay window.
    fn purge_expired(&mut self, current_epoch: u64) {
        let cutoff = current_epoch.saturating_sub(self.window_duration);
        // Collect expired keys from by_time (sorted by timestamp)
        let expired: Vec<(u64, [u8; 32])> = self
            .by_time
            .range(..(cutoff, [0xff; 32]))
            .map(|(k, _)| *k)
            .collect();
        for (ts, id) in expired {
            self.by_time.remove(&(ts, id));
            self.by_id.remove(&id);
        }
    }

    /// Evict the entry with smallest (first_seen, envelope_id).
    fn evict_oldest(&mut self) {
        if let Some((&(ts, id), _)) = self.by_time.iter().next() {
            self.by_time.remove(&(ts, id));
            self.by_id.remove(&id);
        }
    }

    /// Get the number of entries in the cache.
    pub fn len(&self) -> usize {
        self.by_id.len()
    }

    /// Check if the cache is empty.
    pub fn is_empty(&self) -> bool {
        self.by_id.is_empty()
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
    fn test_replay_cache_eviction_by_timestamp_then_id() {
        // RFC: evict smallest (first_seen, envelope_id) pair
        let mut cache = ReplayCache::new(10000, 2);
        let id_a = [0x01; 32]; // inserted at t=100
        let id_b = [0x02; 32]; // inserted at t=200 (later)

        assert!(cache.check_and_insert(id_a, 100).is_ok());
        assert!(cache.check_and_insert(id_b, 200).is_ok());
        assert_eq!(cache.len(), 2);

        // At capacity. Insert id_c at t=300.
        // Eviction should remove id_a because (100, [0x01;32]) < (200, [0x02;32])
        let id_c = [0x03; 32];
        assert!(cache.check_and_insert(id_c, 300).is_ok());
        assert_eq!(cache.len(), 2); // still at max capacity

        // id_a was evicted — can be reinserted (proves it's gone)
        assert!(cache.check_and_insert(id_a, 300).is_ok());
        assert_eq!(cache.len(), 2); // evicts id_b this time (t=200 < t=300)
    }

    #[test]
    fn test_replay_cache_eviction_same_timestamp() {
        // RFC: when timestamps are equal, evict lexicographically smallest envelope_id
        let mut cache = ReplayCache::new(10000, 2);
        let id_high = [0x80; 32];
        let id_low = [0x01; 32];

        assert!(cache.check_and_insert(id_high, 100).is_ok());
        assert!(cache.check_and_insert(id_low, 100).is_ok());

        // At capacity, same timestamp. Eviction should remove id_low (0x01 < 0x80)
        let id_new = [0xFF; 32];
        assert!(cache.check_and_insert(id_new, 100).is_ok());
        assert_eq!(cache.len(), 2);

        // id_low was evicted — can be reinserted
        assert!(cache.check_and_insert(id_low, 100).is_ok());
    }

    #[test]
    fn test_replay_cache_window_expiry() {
        let mut cache = ReplayCache::new(50, 1000);
        let id1 = [0x01; 32];
        let id2 = [0x02; 32];

        assert!(cache.check_and_insert(id1, 100).is_ok());
        // At epoch 200: cutoff = 200 - 50 = 150. id1 (ts=100) is expired.
        assert!(cache.check_and_insert(id2, 200).is_ok());
        assert_eq!(cache.len(), 1);

        // id1 can now be reinserted (was evicted by window)
        assert!(cache.check_and_insert(id1, 200).is_ok());
        assert_eq!(cache.len(), 2);
    }

    #[test]
    #[should_panic(expected = "max_entries must be > 0")]
    fn test_replay_cache_zero_capacity_panics() {
        let _cache = ReplayCache::new(3600, 0);
    }
}
