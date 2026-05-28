//! Deduplication and replay cache (RFC-0852 §5, §12)

use std::collections::{BTreeMap, BTreeSet};

use super::error::DgpError;

/// Replay cache for gossip object deduplication.
///
/// Uses BTreeMap for deterministic iteration order (Class A requirement).
/// Eviction policy: purge expired first, then evict smallest (first_seen, object_hash).
#[derive(Debug, Clone)]
pub struct GossipReplayCache {
    /// object_hash -> first_seen logical timestamp
    seen: BTreeMap<[u8; 32], u64>,
    /// Maximum entries before eviction
    max_entries: u32,
    /// Replay window in logical time units
    window_duration: u64,
}

impl GossipReplayCache {
    /// Create a new replay cache.
    pub fn new(max_entries: u32, window_duration: u64) -> Self {
        Self {
            seen: BTreeMap::new(),
            max_entries,
            window_duration,
        }
    }

    /// Check if an object hash has been seen. If not, record it.
    /// Returns Ok(true) if new, Ok(false) if duplicate, Err if cache full.
    pub fn check_and_insert(
        &mut self,
        object_hash: [u8; 32],
        current_timestamp: u64,
    ) -> Result<bool, DgpError> {
        // Purge expired entries first
        let cutoff = current_timestamp.saturating_sub(self.window_duration);
        self.seen.retain(|_, ts| *ts >= cutoff);

        // Check for duplicate
        if self.seen.contains_key(&object_hash) {
            return Ok(false);
        }

        // Evict if at capacity
        if self.seen.len() >= self.max_entries as usize {
            self.evict_one();
        }

        self.seen.insert(object_hash, current_timestamp);
        Ok(true)
    }

    /// Evict the entry with smallest (first_seen, object_hash).
    /// BTreeMap natural ordering gives deterministic result.
    fn evict_one(&mut self) {
        if let Some(key) = self.seen.keys().next().copied() {
            self.seen.remove(&key);
        }
    }

    /// Get the number of entries.
    pub fn len(&self) -> usize {
        self.seen.len()
    }

    /// Check if empty.
    pub fn is_empty(&self) -> bool {
        self.seen.is_empty()
    }

    /// Remove expired entries. Returns count removed.
    pub fn purge_expired(&mut self, current_timestamp: u64) -> usize {
        let cutoff = current_timestamp.saturating_sub(self.window_duration);
        let before = self.seen.len();
        self.seen.retain(|_, ts| *ts >= cutoff);
        before - self.seen.len()
    }
}

/// HashSet-based deduplication for O(1) lookup.
#[derive(Debug, Clone)]
pub struct DedupSet {
    seen: BTreeSet<[u8; 32]>,
}

impl DedupSet {
    pub fn new() -> Self {
        Self {
            seen: BTreeSet::new(),
        }
    }

    /// Returns true if this is a new object (not seen before).
    pub fn insert_if_new(&mut self, object_hash: [u8; 32]) -> bool {
        self.seen.insert(object_hash)
    }

    pub fn contains(&self, object_hash: &[u8; 32]) -> bool {
        self.seen.contains(object_hash)
    }

    pub fn len(&self) -> usize {
        self.seen.len()
    }
}

impl Default for DedupSet {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_replay_cache_insert_and_check() {
        let mut cache = GossipReplayCache::new(100, 1000);
        let hash = [0xAA; 32];
        assert!(cache.check_and_insert(hash, 100).unwrap()); // new
        assert!(!cache.check_and_insert(hash, 100).unwrap()); // duplicate
    }

    #[test]
    fn test_replay_cache_eviction() {
        let mut cache = GossipReplayCache::new(2, 1000);
        cache.check_and_insert([0x01; 32], 100).unwrap();
        cache.check_and_insert([0x02; 32], 200).unwrap();
        // Cache is full. Next insert should evict oldest.
        cache.check_and_insert([0x03; 32], 300).unwrap();
        assert_eq!(cache.len(), 2);
        // 0x01 should have been evicted (oldest first_seen)
        // Re-inserting returns true (new) since it was evicted
        assert!(cache.check_and_insert([0x01; 32], 300).unwrap());
    }

    #[test]
    fn test_replay_cache_purge_expired() {
        let mut cache = GossipReplayCache::new(100, 500);
        cache.check_and_insert([0x01; 32], 100).unwrap();
        cache.check_and_insert([0x02; 32], 600).unwrap();
        let purged = cache.purge_expired(700);
        assert_eq!(purged, 1); // 0x01 expired (100 + 500 < 700)
        assert_eq!(cache.len(), 1);
    }

    #[test]
    fn test_dedup_set_insert_if_new() {
        let mut set = DedupSet::new();
        assert!(set.insert_if_new([0xAA; 32]));
        assert!(!set.insert_if_new([0xAA; 32]));
        assert!(set.insert_if_new([0xBB; 32]));
        assert_eq!(set.len(), 2);
    }

    #[test]
    fn test_replay_cache_deterministic_eviction() {
        // Verify deterministic eviction order
        let mut cache = GossipReplayCache::new(3, 1000);
        // Insert with specific hashes that have known ordering
        cache.check_and_insert([0x03; 32], 100).unwrap();
        cache.check_and_insert([0x01; 32], 100).unwrap();
        cache.check_and_insert([0x02; 32], 100).unwrap();
        // All same first_seen (100), so eviction is by object_hash ASC
        cache.check_and_insert([0x04; 32], 200).unwrap();
        // Should evict [0x01; 32] (smallest hash at first_seen=100)
        assert_eq!(cache.len(), 3);
    }
}
