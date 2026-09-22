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
//!
//! **LRU ordering:** delegated to the `lru` crate (shared with
//! `quota-router-core`) so the eviction order is true LRU by
//! access (touch-on-read in `get_fresh`), independent of
//! `HashMap` iteration order. The previous hand-rolled
//! `HashMap`-based LRU relied on insertion-order iteration,
//! which Rust's std `HashMap` does NOT guarantee (the
//! `RandomState` default seeds vary per build), so the
//! `capacity_triggers_lru_eviction` test was flaky.

use std::num::NonZeroUsize;

use lru::LruCache;

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
/// stale entries are evicted on read. A successful fresh read
/// also touches the LRU position so the entry moves to MRU.
#[derive(Debug)]
pub struct OctoGovernanceSnapshotCache {
    /// LRU-ordered index. The `lru` crate maintains true LRU
    /// order via an internal doubly-linked list, so eviction
    /// in `insert()` drops the least-recently-used entry
    /// regardless of the underlying hash iteration order.
    /// Capacity is held inside the `LruCache` itself.
    index: LruCache<SnapshotCacheKey, CachedEntry>,
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
        let capacity = capacity.max(1);
        let nz = NonZeroUsize::new(capacity).expect("capacity clamped to ≥ 1");
        Self {
            index: LruCache::new(nz),
        }
    }

    /// Fetch a fresh (non-stale) snapshot for the given key.
    /// Returns `None` if the key is absent OR if the cached entry
    /// has expired (`now_unix >= expires_at_unix`). A successful
    /// fresh read also touches the LRU position.
    pub fn get_fresh(&mut self, key: &SnapshotCacheKey, now_unix: u64) -> Option<SnapshotRef> {
        // Peek first to read the TTL without mutating LRU order —
        // if the entry is stale we evict, which would otherwise be
        // a redundant touch before the pop.
        let entry = self.index.peek(key).cloned();
        match entry {
            None => None,
            Some(e) if now_unix >= e.expires_at_unix => {
                // Stale — evict on read.
                self.index.pop(key);
                None
            }
            Some(e) => {
                // Fresh — touch the LRU position by issuing a
                // `get`. The returned `&CachedEntry` is discarded;
                // we use the cloned payload from the peek above.
                let _ = self.index.get(key);
                Some(e.snapshot)
            }
        }
    }

    /// Insert a snapshot under the given key. Returns
    /// `Ok(())` on success; `Err(reason)` if the cache fails to
    /// accommodate the entry (capacity exhausted and eviction
    /// failed — currently a defensive guard). The underlying
    /// `LruCache::put` returns `()` and evicts the LRU entry
    /// when at capacity, so the `Err` arm is unreachable in
    /// practice and retained only for API compatibility with
    /// the previous hand-rolled implementation.
    pub fn insert(
        &mut self,
        key: SnapshotCacheKey,
        snapshot: SnapshotRef,
        expires_at_unix: u64,
    ) -> Result<(), String> {
        self.index.put(
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
        // Regression guard: prior hand-rolled HashMap-based LRU
        // relied on `HashMap::keys().next()` for "oldest" key,
        // but std HashMap iteration order is non-deterministic
        // (RandomState default). This test ran ~50% of the time
        // pre-fix because k1 was not always first. The `lru`
        // crate uses a doubly-linked list so eviction is true
        // LRU by access, independent of hash iteration order.
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
