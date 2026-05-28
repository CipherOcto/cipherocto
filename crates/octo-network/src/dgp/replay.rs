//! Gossip Replay Cache (RFC-0852 §12)
//!
//! Uses BTreeMap for deterministic iteration order (Class A requirement).
//! Modeled after DOT's ReplayCache.

use std::collections::BTreeMap;

/// Gossip replay cache for deterministic deduplication.
///
/// BTreeMap ensures deterministic eviction order.
#[derive(Debug, Clone)]
pub struct GossipReplayCache {
    /// Object hash → first seen logical timestamp
    seen: BTreeMap<[u8; 32], u64>,
    /// Maximum entries before eviction
    max_entries: u32,
    /// Replay window in logical time units
    window_duration: u64,
}

impl GossipReplayCache {
    pub fn new(max_entries: u32, window_duration: u64) -> Self {
        Self {
            seen: BTreeMap::new(),
            max_entries,
            window_duration,
        }
    }

    /// Check if an object is a replay. Returns true if REPLAY DETECTED.
    /// Also inserts the object if it's new.
    pub fn check_and_insert(&mut self, object_hash: [u8; 32], logical_timestamp: u64) -> bool {
        if let Some(&first_seen) = self.seen.get(&object_hash) {
            // Object seen before — check if within replay window
            if logical_timestamp.saturating_sub(first_seen) <= self.window_duration {
                return true; // Replay detected
            }
        }

        // New object or outside replay window — insert
        if self.seen.len() >= self.max_entries as usize {
            self.evict();
        }
        self.seen.insert(object_hash, logical_timestamp);
        false
    }

    /// Remove expired entries outside the replay window.
    pub fn purge_expired(&mut self, current_timestamp: u64) {
        let cutoff = current_timestamp.saturating_sub(self.window_duration);
        self.seen.retain(|_, ts| *ts >= cutoff);
    }

    /// Deterministic eviction: remove entry with smallest (first_seen, object_hash).
    fn evict(&mut self) {
        if let Some(key) = self.seen.keys().next().cloned() {
            self.seen.remove(&key);
        }
    }

    pub fn len(&self) -> usize {
        self.seen.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_replay_cache_new_object() {
        let mut cache = GossipReplayCache::new(100, 1000);
        assert!(!cache.check_and_insert([1u8; 32], 100));
    }

    #[test]
    fn test_replay_cache_replay_detected() {
        let mut cache = GossipReplayCache::new(100, 1000);
        cache.check_and_insert([1u8; 32], 100);
        assert!(cache.check_and_insert([1u8; 32], 200));
    }

    #[test]
    fn test_replay_cache_outside_window() {
        let mut cache = GossipReplayCache::new(100, 1000);
        cache.check_and_insert([1u8; 32], 100);
        // Outside window — not a replay
        assert!(!cache.check_and_insert([1u8; 32], 2000));
    }

    #[test]
    fn test_replay_cache_deterministic_eviction() {
        let mut cache = GossipReplayCache::new(3, 1000);
        cache.check_and_insert([3u8; 32], 300);
        cache.check_and_insert([1u8; 32], 100);
        cache.check_and_insert([2u8; 32], 200);
        // Cache is full. Insert 4th — evicts smallest key
        cache.check_and_insert([4u8; 32], 400);
        assert_eq!(cache.len(), 3);
    }

    #[test]
    fn test_replay_cache_purge() {
        let mut cache = GossipReplayCache::new(100, 1000);
        cache.check_and_insert([1u8; 32], 100);
        cache.check_and_insert([2u8; 32], 5000);
        cache.purge_expired(6000);
        assert_eq!(cache.len(), 1); // only entry at ts=5000 survives (6000-1000=5000 cutoff)
    }
}
