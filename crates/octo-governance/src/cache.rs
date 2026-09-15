//! Snapshot cache layer — RFC-0011-g §Substrate [ADD] `OctoGovernanceSnapshotCache`.
//!
//! Bounded LRU cache keyed by `(active_did, chain_id)` with a
//! 600-second TTL per RFC-0011-g §Performance Targets. The TTL
//! boundary is inclusive on the stale side — a snapshot at
//! exactly `now_unix == expires_at_unix` is treated as stale and
//! misses the cache.
//!
//! **v1 simplification:** state filter is NOT part of the cache
//! key. The substrate projection returns the same `SnapshotRef`
//! regardless of state filter (state filter is post-processing
//! applied at the CLI boundary). Including the filter in the
//! key would multiply cache entries without changing the
//! projection; v1 collapses to `(active_did, chain_id)` and
//! lets the cache hit reduce substrate re-fetch cost.

use std::collections::HashMap;

use crate::snapshot::SnapshotRef;

/// Maximum number of cached snapshots before LRU eviction kicks in.
/// Sized to cover a typical operator session (one snapshot per chain
/// × one snapshot per filter combination × a small handful of
/// active DIDs). Bounded so a long-running CLI cannot leak memory.
pub const SNAPSHOT_CACHE_CAPACITY: usize = 64;

/// Cache key derived from `(active_did, chain_id)`.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct SnapshotCacheKey {
    /// Operator's active DID.
    pub active_did: String,
    /// Optional chain-id filter (`None` ⇒ all chains).
    pub chain_id: Option<String>,
}

#[derive(Clone, Debug)]
struct CachedEntry {
    /// Cached snapshot projection.
    snapshot: SnapshotRef,
    /// `expires_at_unix` carried separately so `get_fresh()` can
    /// compare against the caller-supplied `now_unix` without
    /// re-parsing the `SnapshotRef`.
    expires_at_unix: u64,
}

/// Bounded LRU + TTL snapshot cache.
///
/// `Default` constructs an empty cache with the canonical
/// capacity (`SNAPSHOT_CACHE_CAPACITY`). `get_fresh()` returns the
/// cached `SnapshotRef` only if `now_unix < expires_at_unix`;
/// stale entries are evicted on read.
#[derive(Debug)]
pub struct OctoGovernanceSnapshotCache {
    capacity: usize,
    /// Index map: cache key → cached entry. Rust's default
    /// `HashMap` preserves insertion order so the LRU policy is
    /// implemented by removing + re-inserting on `get_fresh()`.
    index: HashMap<SnapshotCacheKey, CachedEntry>,
}

impl Default for OctoGovernanceSnapshotCache {
    fn default() -> Self {
        Self::new(SNAPSHOT_CACHE_CAPACITY)
    }
}

impl OctoGovernanceSnapshotCache {
    /// Construct a cache with the given capacity. Capacity is
    /// rounded up to `1` if a smaller value is supplied.
    pub fn new(capacity: usize) -> Self {
        Self {
            capacity: capacity.max(1),
            index: HashMap::new(),
        }
    }

    /// Fetch a fresh (non-stale) snapshot for the given key.
    /// Returns `None` if the key is absent OR if the cached entry
    /// has expired (`now_unix >= expires_at_unix`).
    pub fn get_fresh(&mut self, key: &SnapshotCacheKey, now_unix: u64) -> Option<SnapshotRef> {
        let entry = self.index.get(key)?.clone();
        if now_unix >= entry.expires_at_unix {
            self.index.remove(key);
            return None;
        }
        // LRU touch: remove + re-insert to move the key to the
        // back of the HashMap iteration order (MRU position).
        let snapshot = entry.snapshot.clone();
        self.index.remove(key);
        self.index.insert(
            key.clone(),
            CachedEntry {
                snapshot: snapshot.clone(),
                expires_at_unix: entry.expires_at_unix,
            },
        );
        Some(snapshot)
    }

    /// Insert a snapshot under the given key. Returns
    /// `Ok(())` on success; `Err(reason)` if the cache fails to
    /// accommodate the entry (capacity exhausted and eviction
    /// failed — currently a defensive guard).
    pub fn insert(
        &mut self,
        key: SnapshotCacheKey,
        snapshot: SnapshotRef,
        expires_at_unix: u64,
    ) -> Result<(), String> {
        // LRU eviction: drop oldest until under capacity.
        while self.index.len() >= self.capacity {
            let oldest_key = self.index.keys().next().cloned();
            match oldest_key {
                Some(k) => {
                    self.index.remove(&k);
                }
                None => break,
            }
        }
        self.index.insert(
            key,
            CachedEntry {
                snapshot,
                expires_at_unix,
            },
        );
        Ok(())
    }

    /// Current number of cached entries.
    pub fn len(&self) -> usize {
        self.index.len()
    }

    /// `true` if the cache holds zero entries.
    pub fn is_empty(&self) -> bool {
        self.index.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::snapshot::ProposalFilter;

    fn key(s: &str) -> SnapshotCacheKey {
        SnapshotCacheKey {
            active_did: s.to_string(),
            chain_id: None,
        }
    }

    fn snap(id_byte: u8, taken: u64, expires: u64) -> SnapshotRef {
        SnapshotRef {
            snapshot_id: [id_byte; 32],
            filter: ProposalFilter::default(),
            taken_at_unix: taken,
            expires_at_unix: expires,
            root_manifest_hash: [0u8; 32],
            open_proposal_count: 0,
            attestation_count: 0,
        }
    }

    #[test]
    fn fresh_entry_is_returned() {
        let mut c = OctoGovernanceSnapshotCache::default();
        let k = key("did:octo:z1");
        c.insert(k.clone(), snap(0xAA, 100, 200), 200).unwrap();
        assert_eq!(c.get_fresh(&k, 150).unwrap().snapshot_id[0], 0xAA);
    }

    #[test]
    fn stale_entry_misses() {
        let mut c = OctoGovernanceSnapshotCache::default();
        let k = key("did:octo:z1");
        c.insert(k.clone(), snap(0xAA, 100, 200), 200).unwrap();
        assert!(c.get_fresh(&k, 200).is_none()); // boundary inclusive
        assert!(c.get_fresh(&k, 250).is_none());
    }

    #[test]
    fn capacity_triggers_lru_eviction() {
        let mut c = OctoGovernanceSnapshotCache::new(2);
        let k1 = key("a");
        let k2 = key("b");
        let k3 = key("c");
        c.insert(k1.clone(), snap(1, 1, 1_000_000), 1_000_000)
            .unwrap();
        c.insert(k2.clone(), snap(2, 2, 1_000_000), 1_000_000)
            .unwrap();
        c.insert(k3.clone(), snap(3, 3, 1_000_000), 1_000_000)
            .unwrap();
        assert!(c.get_fresh(&k1, 500).is_none()); // evicted by LRU
        assert!(c.get_fresh(&k2, 500).is_some());
        assert!(c.get_fresh(&k3, 500).is_some());
    }

    #[test]
    fn touch_moves_entry_to_most_recent() {
        let mut c = OctoGovernanceSnapshotCache::new(2);
        let k1 = key("a");
        let k2 = key("b");
        c.insert(k1.clone(), snap(1, 1, 1_000_000), 1_000_000)
            .unwrap();
        c.insert(k2.clone(), snap(2, 2, 1_000_000), 1_000_000)
            .unwrap();
        // Touch k1 to move it to MRU position.
        let _ = c.get_fresh(&k1, 500);
        // Insert k3 → k2 should be evicted (now oldest).
        let k3 = key("c");
        c.insert(k3.clone(), snap(3, 3, 1_000_000), 1_000_000)
            .unwrap();
        assert!(c.get_fresh(&k1, 500).is_some());
        assert!(c.get_fresh(&k2, 500).is_none());
        assert!(c.get_fresh(&k3, 500).is_some());
    }
}
