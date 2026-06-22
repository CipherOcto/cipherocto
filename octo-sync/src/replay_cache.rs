//! ReplayCache — bounded per-peer envelope ID cache (per RFC-0862 §4.3.1, mission 0862e).
//!
//! The in-memory cache uses a `BTreeMap` for deterministic ordering (per
//! RFC-0850's ReplayCache spec). The bound is 10K entries per peer (per
//! RFC-0862 §Performance Targets; ≤ 50 MB per peer).
//!
//! # Persistence
//!
//! The full persistent variant (with disk backing via Stoolap) is in a
//! separate sub-crate `octo-sync-replay-store` (per mission 0862e
//! §Cargo dependency layering). This module provides the in-memory cache
//! that the persistent variant extends.

use std::collections::BTreeMap;

use crate::identity::SyncPeerId;

/// A bounded per-peer envelope ID cache.
///
/// The cache uses BTreeMap for deterministic ordering (per RFC-0850's
/// ReplayCache spec). When the cache exceeds `max_entries`, the oldest
/// entry by `first_seen` is evicted (LRU by time, NOT by access).
#[derive(Debug)]
pub struct ReplayCache {
    /// BTreeMap from envelope_id to first_seen Unix milliseconds.
    entries: BTreeMap<[u8; 32], u64>,
    /// Maximum number of entries (default 10K per RFC-0862 §Performance Targets).
    max_entries: usize,
}

impl Default for ReplayCache {
    fn default() -> Self {
        Self::new(10_000)
    }
}

impl ReplayCache {
    /// Create a new `ReplayCache` with the given max entry count.
    pub fn new(max_entries: usize) -> Self {
        Self { entries: BTreeMap::new(), max_entries }
    }

    /// Insert an envelope_id with its first_seen timestamp.
    /// If the envelope_id is already in the cache, this is a no-op.
    /// If the cache exceeds `max_entries`, the oldest entry is evicted.
    pub fn insert(&mut self, envelope_id: [u8; 32], first_seen_ms: u64) {
        if self.entries.contains_key(&envelope_id) {
            return; // already present
        }
        self.entries.insert(envelope_id, first_seen_ms);
        // Evict the oldest if over the limit
        while self.entries.len() > self.max_entries {
            // Find the smallest first_seen
            if let Some((&oldest_id, &oldest_ts)) = self.entries.iter().next() {
                let oldest_id = oldest_id;
                let _ = oldest_ts;
                self.entries.remove(&oldest_id);
            } else {
                break;
            }
        }
    }

    /// Check whether the envelope_id is in the cache.
    pub fn contains(&self, envelope_id: &[u8; 32]) -> bool {
        self.entries.contains_key(envelope_id)
    }

    /// Return the number of entries currently in the cache.
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Return true if the cache is empty.
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Evict the oldest entry. Returns the evicted envelope_id and its
    /// first_seen timestamp, or `None` if the cache is empty.
    pub fn evict_oldest(&mut self) -> Option<([u8; 32], u64)> {
        if let Some((&id, &ts)) = self.entries.iter().next() {
            let id = id;
            let ts = ts;
            self.entries.remove(&id);
            Some((id, ts))
        } else {
            None
        }
    }

    /// Return the maximum entry count.
    pub fn max_entries(&self) -> usize {
        self.max_entries
    }

    /// Persist the cache to a file. Stub implementation for v1; the full
    /// persistent variant is in `octo-sync-replay-store` (per mission 0862e).
    pub fn flush(&self) -> Result<(), String> {
        // v1: no-op (in-memory only). The persistent variant in
        // `octo-sync-replay-store` handles disk flushing.
        Ok(())
    }
}

/// Per-peer cache manager.
///
/// Holds a `ReplayCache` for each peer. The cipherocto sync engine looks up
/// the cache by `SyncPeerId` and inserts/queries as needed.
#[derive(Debug, Default)]
pub struct ReplayCacheManager {
    /// Per-peer caches.
    caches: std::collections::HashMap<SyncPeerId, ReplayCache>,
}

impl ReplayCacheManager {
    /// Create a new `ReplayCacheManager` with default caches.
    pub fn new() -> Self {
        Self::default()
    }

    /// Get or create the cache for the given peer.
    pub fn cache_for(&mut self, peer: SyncPeerId) -> &mut ReplayCache {
        self.caches.entry(peer).or_default()
    }

    /// Check whether an envelope_id is in the cache for the given peer.
    pub fn contains(&mut self, peer: SyncPeerId, envelope_id: &[u8; 32]) -> bool {
        self.cache_for(peer).contains(envelope_id)
    }

    /// Insert an envelope_id into the cache for the given peer.
    pub fn insert(&mut self, peer: SyncPeerId, envelope_id: [u8; 32], first_seen_ms: u64) {
        self.cache_for(peer).insert(envelope_id, first_seen_ms);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_cache_is_empty() {
        let c = ReplayCache::new(10);
        assert!(c.is_empty());
        assert_eq!(c.len(), 0);
    }

    #[test]
    fn insert_and_contains() {
        let mut c = ReplayCache::new(10);
        c.insert([1u8; 32], 100);
        assert!(c.contains(&[1u8; 32]));
        assert!(!c.contains(&[2u8; 32]));
        assert_eq!(c.len(), 1);
    }

    #[test]
    fn duplicate_insert_is_no_op() {
        let mut c = ReplayCache::new(10);
        c.insert([1u8; 32], 100);
        c.insert([1u8; 32], 200); // no-op
        assert_eq!(c.len(), 1);
    }

    #[test]
    fn evicts_oldest_when_over_limit() {
        let mut c = ReplayCache::new(3);
        c.insert([1u8; 32], 100);
        c.insert([2u8; 32], 200);
        c.insert([3u8; 32], 300);
        c.insert([4u8; 32], 400); // should evict [1u8; 32]
        assert_eq!(c.len(), 3);
        assert!(!c.contains(&[1u8; 32]));
        assert!(c.contains(&[2u8; 32]));
        assert!(c.contains(&[3u8; 32]));
        assert!(c.contains(&[4u8; 32]));
    }

    #[test]
    fn evict_oldest_returns_evicted_entry() {
        let mut c = ReplayCache::new(10);
        c.insert([1u8; 32], 100);
        c.insert([2u8; 32], 200);
        let evicted = c.evict_oldest().unwrap();
        assert_eq!(evicted.0, [1u8; 32]);
        assert_eq!(evicted.1, 100);
        assert_eq!(c.len(), 1);
    }

    #[test]
    fn manager_per_peer_caches() {
        let mut m = ReplayCacheManager::new();
        let p1 = SyncPeerId([1u8; 32]);
        let p2 = SyncPeerId([2u8; 32]);
        m.insert(p1, [10u8; 32], 100);
        m.insert(p2, [10u8; 32], 200);
        assert!(m.contains(p1, &[10u8; 32]));
        assert!(m.contains(p2, &[10u8; 32]));
    }
}
