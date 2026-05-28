//! Route cache with deterministic eviction (RFC-0856 §14)

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

/// Deterministic route cache using BTreeMap (RFC-0856 §14.1).
///
/// Eviction key: (score ASC, cached_at ASC, route_id ASC)
/// min_by_key evicts the worst route first.
#[derive(Debug, Clone)]
pub struct RouteCache {
    /// route_id → CachedRoute
    by_id: BTreeMap<[u8; 32], CachedRoute>,
    /// Eviction key → route_id
    by_score: BTreeMap<(u64, u64, [u8; 32]), [u8; 32]>,
    max_entries: u32,
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
    pub fn insert(
        &mut self,
        route: DeterministicRoute,
        score: u64,
        current_epoch: u64,
    ) -> Result<bool, DrsError> {
        let route_id = route.route_id;

        // Remove old entry if updating
        if let Some(old) = self.by_id.get(&route_id) {
            let old_key = (old.score, old.cached_at, route_id);
            self.by_score.remove(&old_key);
        } else if self.by_id.len() >= self.max_entries as usize {
            self.evict_worst();
        }

        let entry = CachedRoute {
            route,
            score,
            cached_at: current_epoch,
            last_accessed: current_epoch,
        };

        let key = (score, current_epoch, route_id);
        self.by_score.insert(key, route_id);
        let is_new = self.by_id.insert(route_id, entry).is_none();
        Ok(is_new)
    }

    /// Look up a route by ID.
    pub fn get(&self, route_id: &[u8; 32]) -> Option<&CachedRoute> {
        self.by_id.get(route_id)
    }

    /// Remove a route. Returns true if it existed.
    pub fn remove(&mut self, route_id: &[u8; 32]) -> bool {
        if let Some(entry) = self.by_id.remove(route_id) {
            let key = (entry.score, entry.cached_at, *route_id);
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
                let key = (entry.score, entry.cached_at, id);
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
            ttl_hops: 10,
        }
    }

    #[test]
    fn test_cache_insert_and_get() {
        let mut cache = RouteCache::new(100);
        cache.insert(make_route(1, 100), 5000, 100).unwrap();
        assert!(cache.get(&[1; 32]).is_some());
        assert_eq!(cache.len(), 1);
    }

    #[test]
    fn test_cache_update() {
        let mut cache = RouteCache::new(100);
        assert!(cache.insert(make_route(1, 100), 5000, 100).unwrap()); // new
        assert!(!cache.insert(make_route(1, 200), 8000, 200).unwrap()); // update
        assert_eq!(cache.len(), 1);
        assert_eq!(cache.get(&[1; 32]).unwrap().score, 8000);
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
        // Cache full (2). Insert route 3 with lower score — should evict route 1
        cache.insert(make_route(3, 100), 3000, 100).unwrap();
        assert_eq!(cache.len(), 2);
        assert!(cache.get(&[1; 32]).is_none()); // evicted (lowest score)
        assert!(cache.get(&[2; 32]).is_some());
        assert!(cache.get(&[3; 32]).is_some());
    }

    #[test]
    fn test_cache_evict_expired() {
        let mut cache = RouteCache::new(100);
        cache.insert(make_route(1, 100), 5000, 100).unwrap(); // age=400
        cache.insert(make_route(2, 100), 6000, 400).unwrap(); // age=100
        let evicted = cache.evict_expired(500, 150); // max_age=150
        assert_eq!(evicted, 1); // route 1: 500-100=400 > 150, route 2: 500-400=100 < 150
        assert!(cache.get(&[1; 32]).is_none());
        assert!(cache.get(&[2; 32]).is_some());
    }

    #[test]
    fn test_cache_deterministic_ordering() {
        let mut cache = RouteCache::new(100);
        cache.insert(make_route(3, 100), 5000, 100).unwrap();
        cache.insert(make_route(1, 100), 5000, 100).unwrap();
        cache.insert(make_route(2, 100), 5000, 100).unwrap();
        // BTreeMap iterates by key order — (score, cached_at, route_id)
        let keys: Vec<[u8; 32]> = cache.by_score.values().copied().collect();
        assert_eq!(keys, vec![[1; 32], [2; 32], [3; 32]]);
    }
}
