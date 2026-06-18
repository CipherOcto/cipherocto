//! Route cache with deterministic eviction (RFC-0856 Section 14)

use std::collections::BTreeMap;

use crate::drs::error::DrsError;
use crate::drs::route::DeterministicRoute;

/// Cached route entry.
#[derive(Debug, Clone)]
pub struct CachedRoute {
    pub route: DeterministicRoute,
    /// Score computed with current weights
    pub score: u64,
    /// When first cached
    pub cached_at: u64,
    /// Last time accessed/used
    pub last_accessed: u64,
}

/// Deterministic route cache using BTreeMap (RFC-0856 Section 14.1).
///
/// Eviction key: (score ASC, last_accessed ASC, inverted_cost ASC, route_id ASC)
/// min_by_key evicts the worst route first: lowest score, oldest access, highest cost.
#[derive(Debug, Clone)]
pub struct RouteCache {
    /// route_id -> CachedRoute
    by_id: BTreeMap<[u8; 32], CachedRoute>,
    /// Eviction key -> route_id
    by_score: BTreeMap<(u64, u64, u64, [u8; 32]), [u8; 32]>,
    max_entries: u32,
}

/// Create an eviction key from a cached route.
/// Cost is inverted so higher-cost routes sort first (evicted first).
/// Route ID is included as tiebreaker for uniqueness.
fn eviction_key(
    score: u64,
    last_accessed: u64,
    route_cost: u64,
    route_id: [u8; 32],
) -> (u64, u64, u64, [u8; 32]) {
    (score, last_accessed, u64::MAX - route_cost, route_id)
}

impl RouteCache {
    pub fn new(max_entries: u32) -> Self {
        Self {
            by_id: BTreeMap::new(),
            by_score: BTreeMap::new(),
            max_entries,
        }
    }

    /// Insert or update a route. Returns Ok(true) if new, Ok(false) if updated.
    /// When the cache is full, the new route is only inserted if its score beats
    /// the evicted worst route.
    pub fn insert(
        &mut self,
        route: DeterministicRoute,
        score: u64,
        current_epoch: u64,
    ) -> Result<bool, DrsError> {
        let route_id = route.route_id;

        // Remove old entry if updating
        if let Some(old) = self.by_id.get(&route_id) {
            let old_key =
                eviction_key(old.score, old.last_accessed, old.route.route_cost, route_id);
            self.by_score.remove(&old_key);
        } else if self.by_id.len() >= self.max_entries as usize {
            // Evict worst and compare scores
            if let Some((evicted_key, _)) = self.by_score.iter().next() {
                let evicted_score = evicted_key.0;
                if score < evicted_score {
                    // New route is worse than the worst -- reject insertion
                    return Err(DrsError::CacheFull {
                        max_entries: self.max_entries,
                    });
                }
            }
            self.evict_worst();
        }

        let entry = CachedRoute {
            route,
            score,
            cached_at: current_epoch,
            last_accessed: current_epoch,
        };

        let key = eviction_key(score, current_epoch, entry.route.route_cost, route_id);
        self.by_score.insert(key, route_id);
        let is_new = self.by_id.insert(route_id, entry).is_none();
        Ok(is_new)
    }

    /// Look up a route by ID and update its `last_accessed` timestamp.
    pub fn get(&mut self, route_id: &[u8; 32], current_epoch: u64) -> Option<&CachedRoute> {
        if let Some(entry) = self.by_id.get_mut(route_id) {
            // Re-key eviction index before updating
            let old_key = eviction_key(
                entry.score,
                entry.last_accessed,
                entry.route.route_cost,
                *route_id,
            );
            self.by_score.remove(&old_key);

            entry.last_accessed = current_epoch;
            let new_key = eviction_key(
                entry.score,
                current_epoch,
                entry.route.route_cost,
                *route_id,
            );
            self.by_score.insert(new_key, *route_id);

            // Return immutable ref
            Some(self.by_id.get(route_id).unwrap())
        } else {
            None
        }
    }

    /// Remove a route. Returns true if it existed.
    pub fn remove(&mut self, route_id: &[u8; 32]) -> bool {
        if let Some(entry) = self.by_id.remove(route_id) {
            let key = eviction_key(
                entry.score,
                entry.last_accessed,
                entry.route.route_cost,
                *route_id,
            );
            self.by_score.remove(&key);
            true
        } else {
            false
        }
    }

    /// Evict the worst-scoring route (deterministic).
    fn evict_worst(&mut self) {
        if let Some((key, route_id)) = self.by_score.iter().next() {
            let key = *key;
            let route_id = *route_id;
            self.by_score.remove(&key);
            self.by_id.remove(&route_id);
        }
    }

    /// Evict expired routes. Returns count removed.
    pub fn evict_expired(&mut self, current_epoch: u64, max_age: u64) -> usize {
        let before = self.by_id.len();
        let expired: Vec<[u8; 32]> = self
            .by_id
            .iter()
            .filter(|(_, entry)| current_epoch.saturating_sub(entry.cached_at) > max_age)
            .map(|(id, _)| *id)
            .collect();

        for id in expired {
            if let Some(entry) = self.by_id.remove(&id) {
                let key =
                    eviction_key(entry.score, entry.last_accessed, entry.route.route_cost, id);
                self.by_score.remove(&key);
            }
        }
        before - self.by_id.len()
    }

    /// Number of cached routes.
    pub fn len(&self) -> usize {
        self.by_id.len()
    }

    pub fn is_empty(&self) -> bool {
        self.by_id.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_route(id: u8, epoch: u64) -> DeterministicRoute {
        DeterministicRoute {
            route_id: [id; 32],
            source_gateway: [0x01; 32],
            destination_gateway: [0x02; 32],
            next_hop: [0x03; 32],
            transport_vector_root: [0u8; 32],
            trust_score: 500,
            bandwidth_class: 100,
            latency_class: 50,
            censorship_resistance_class: 200,
            route_cost: 1000,
            route_epoch: epoch,
            valid_until_epoch: 0,
            ttl_hops: 10,
            signature: [0u8; 64],
        }
    }

    #[test]
    fn test_cache_insert_and_get() {
        let mut cache = RouteCache::new(100);
        cache.insert(make_route(1, 100), 5000, 100).unwrap();
        assert!(cache.get(&[1; 32], 100).is_some());
        assert_eq!(cache.len(), 1);
    }

    #[test]
    fn test_cache_update() {
        let mut cache = RouteCache::new(100);
        assert!(cache.insert(make_route(1, 100), 5000, 100).unwrap()); // new
        assert!(!cache.insert(make_route(1, 200), 8000, 200).unwrap()); // update
        assert_eq!(cache.len(), 1);
        assert_eq!(cache.get(&[1; 32], 200).unwrap().score, 8000);
    }

    #[test]
    fn test_cache_remove() {
        let mut cache = RouteCache::new(100);
        cache.insert(make_route(1, 100), 5000, 100).unwrap();
        assert!(cache.remove(&[1; 32]));
        assert!(!cache.remove(&[1; 32]));
        assert!(cache.is_empty());
    }

    #[test]
    fn test_cache_evict_worst() {
        let mut cache = RouteCache::new(2);
        cache.insert(make_route(1, 100), 5000, 100).unwrap();
        cache.insert(make_route(2, 100), 8000, 100).unwrap();
        // Cache full (2). Insert route 3 with score >= 5000 (the worst).
        // Route 3 (6000) beats route 1 (5000), so route 1 is evicted.
        cache.insert(make_route(3, 100), 6000, 100).unwrap();
        assert_eq!(cache.len(), 2);
        assert!(cache.get(&[1; 32], 100).is_none()); // evicted (lowest score)
        assert!(cache.get(&[2; 32], 100).is_some());
        assert!(cache.get(&[3; 32], 100).is_some());
    }

    #[test]
    fn test_cache_evict_reject_worse() {
        let mut cache = RouteCache::new(2);
        cache.insert(make_route(1, 100), 5000, 100).unwrap();
        cache.insert(make_route(2, 100), 8000, 100).unwrap();
        // Cache full (2). Route 3 (3000) is worse than worst (5000) — rejected.
        let result = cache.insert(make_route(3, 100), 3000, 100);
        assert!(result.is_err());
        assert_eq!(cache.len(), 2);
        assert!(cache.get(&[1; 32], 100).is_some()); // still there
        assert!(cache.get(&[2; 32], 100).is_some());
    }

    #[test]
    fn test_cache_evict_expired() {
        let mut cache = RouteCache::new(100);
        cache.insert(make_route(1, 100), 5000, 100).unwrap(); // age=400
        cache.insert(make_route(2, 100), 6000, 400).unwrap(); // age=100
        let evicted = cache.evict_expired(500, 150); // max_age=150
        assert_eq!(evicted, 1); // route 1: 500-100=400 > 150, route 2: 500-400=100 < 150
        assert!(cache.get(&[1; 32], 500).is_none());
        assert!(cache.get(&[2; 32], 500).is_some());
    }

    #[test]
    fn test_cache_deterministic_ordering() {
        let mut cache = RouteCache::new(100);
        // Same score, same epoch, same cost — ordered by route_id
        cache.insert(make_route(3, 100), 5000, 100).unwrap();
        cache.insert(make_route(1, 100), 5000, 100).unwrap();
        cache.insert(make_route(2, 100), 5000, 100).unwrap();
        // BTreeMap iterates by key order: (score, last_accessed, inverted_cost, route_id)
        // All same except route_id: [1;32] < [2;32] < [3;32]
        let keys: Vec<[u8; 32]> = cache.by_score.values().copied().collect();
        assert_eq!(keys, vec![[1; 32], [2; 32], [3; 32]]);
    }

    #[test]
    fn test_cache_evict_higher_cost_first() {
        let mut cache = RouteCache::new(2);
        let mut route1 = make_route(1, 100);
        route1.route_cost = 500;
        cache.insert(route1, 5000, 100).unwrap();

        let mut route2 = make_route(2, 100);
        route2.route_cost = 2000;
        cache.insert(route2, 5000, 100).unwrap();

        // Cache full. Same score. Higher cost (route 2) should be evicted first.
        // Route 3 score (5000) >= evicted worst (5000) — accepted.
        cache.insert(make_route(3, 100), 5000, 100).unwrap();
        assert_eq!(cache.len(), 2);
        assert!(cache.get(&[1; 32], 100).is_some()); // kept (lower cost)
        assert!(cache.get(&[2; 32], 100).is_none()); // evicted (higher cost)
        assert!(cache.get(&[3; 32], 100).is_some());
    }
}
