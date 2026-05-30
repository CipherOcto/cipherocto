//! Gateway Cache with deterministic eviction (RFC-0851 §10)

use crate::dot::gateway::GatewayIdentity;
use crate::gdp::overlay_endpoint::OverlayEndpoint;
use crate::gdp::types::GatewayCapability;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// Cache entry for a discovered gateway
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GatewayCacheEntry {
    /// Hash of the advertisement
    pub advertisement_hash: [u8; 32],
    /// Epoch when first seen
    pub first_seen: u64,
    /// Epoch when last seen
    pub last_seen: u64,
    /// Trust score (higher = more trusted, 0-1000 per RFC Section 10)
    pub trust_score: u32,
    /// Gateway identity
    pub identity: GatewayIdentity,
    /// Capabilities sorted by enum value
    pub capabilities: Vec<GatewayCapability>,
    /// Endpoints sorted by (transport_type, endpoint_hash)
    pub endpoints: Vec<OverlayEndpoint>,
}

/// Deterministic gateway cache (RFC-0851 §10)
///
/// Uses BTreeMap for deterministic iteration order.
/// Eviction: lower eviction_score evicted first.
/// Ties broken by lexicographic gateway_id.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GatewayCache {
    entries: BTreeMap<[u8; 32], GatewayCacheEntry>,
    max_entries: u32,
}

impl GatewayCache {
    pub fn new(max_entries: u32) -> Self {
        Self {
            entries: BTreeMap::new(),
            max_entries,
        }
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    pub fn contains(&self, gateway_id: &[u8; 32]) -> bool {
        self.entries.contains_key(gateway_id)
    }

    pub fn get(&self, gateway_id: &[u8; 32]) -> Option<&GatewayCacheEntry> {
        self.entries.get(gateway_id)
    }

    /// Insert a gateway entry. If cache is full, evict the lowest-scoring entry.
    pub fn insert(&mut self, entry: GatewayCacheEntry, current_epoch: u64) {
        let key = entry.identity.gateway_id;

        // If already present, update
        if let std::collections::btree_map::Entry::Occupied(mut e) = self.entries.entry(key) {
            e.insert(entry);
            return;
        }

        // If cache is full, evict lowest-scoring entry
        if self.entries.len() >= self.max_entries as usize {
            self.evict(current_epoch);
        }

        self.entries.insert(key, entry);
    }

    /// Deterministic eviction: evict entry with lowest eviction_score.
    /// eviction_score = trust_score * 10 + utility_score * 5 + recency_score * 2
    /// Ties broken by lexicographic gateway_id.
    fn evict(&mut self, current_epoch: u64) {
        if self.entries.is_empty() {
            return;
        }

        let mut best_evict_key = None;
        let mut best_evict_score = u64::MAX;

        for (key, entry) in self.entries.iter() {
            let trust_component = (entry.trust_score as u64).saturating_mul(10);
            let recency = current_epoch.saturating_sub(entry.last_seen).min(1000);
            let recency_score = 1000u64.saturating_sub(recency);
            // TODO: track route count per gateway (RFC Section 13)
            let utility_score = 0u64;
            let eviction_score = trust_component
                .saturating_add(utility_score.saturating_mul(5))
                .saturating_add(recency_score.saturating_mul(2));

            if eviction_score < best_evict_score
                || (eviction_score == best_evict_score
                    && best_evict_key.as_ref().is_none_or(|k: &[u8; 32]| key < k))
            {
                best_evict_score = eviction_score;
                best_evict_key = Some(*key);
            }
        }

        if let Some(key) = best_evict_key {
            self.entries.remove(&key);
        }
    }

    /// Remove all entries that have not been seen since the given epoch.
    pub fn remove_expired(&mut self, current_epoch: u64, max_age: u64) {
        let cutoff = current_epoch.saturating_sub(max_age);
        self.entries.retain(|_, entry| entry.last_seen >= cutoff);
    }

    pub fn iter(&self) -> impl Iterator<Item = (&[u8; 32], &GatewayCacheEntry)> {
        self.entries.iter()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dot::gateway::GatewayClass;

    fn make_entry(id: u8, trust: u32, endpoints: usize) -> GatewayCacheEntry {
        GatewayCacheEntry {
            advertisement_hash: [id; 32],
            first_seen: 100,
            last_seen: 200,
            trust_score: trust,
            identity: GatewayIdentity::new([id; 32], 1, GatewayClass::Edge, 100),
            capabilities: vec![],
            endpoints: (0..endpoints)
                .map(|i| OverlayEndpoint::new(i as u16, [i as u8; 32]))
                .collect(),
        }
    }

    #[test]
    fn test_cache_insert_and_get() {
        let mut cache = GatewayCache::new(10);
        let entry = make_entry(1, 100, 2);
        let key = entry.identity.gateway_id;
        cache.insert(entry, 300);
        assert!(cache.contains(&key));
        assert_eq!(cache.len(), 1);
    }

    #[test]
    fn test_cache_eviction_deterministic() {
        let mut cache = GatewayCache::new(3);
        cache.insert(make_entry(1, 100, 2), 300);
        cache.insert(make_entry(2, 50, 1), 300);
        cache.insert(make_entry(3, 200, 3), 300);
        // Cache is full. Insert a 4th — should evict lowest-scoring (id=2)
        cache.insert(make_entry(4, 150, 2), 300);
        assert_eq!(cache.len(), 3);
        // id=2 should be evicted (lowest trust_score)
        assert!(!cache.contains(&[2u8; 32]));
    }

    #[test]
    fn test_cache_deterministic_order() {
        let mut cache = GatewayCache::new(100);
        for i in 0..50u8 {
            cache.insert(make_entry(i, i as u32 * 10, 1), 1000);
        }
        // BTreeMap guarantees deterministic iteration
        let ids: Vec<[u8; 32]> = cache.iter().map(|(_, e)| e.identity.gateway_id).collect();
        let mut sorted = ids.clone();
        sorted.sort();
        assert_eq!(ids, sorted);
    }

    #[test]
    fn test_cache_remove_expired() {
        let mut cache = GatewayCache::new(10);
        cache.insert(make_entry(1, 100, 1), 300); // last_seen=200
        let mut old_entry = make_entry(2, 100, 1);
        old_entry.last_seen = 50;
        cache.insert(old_entry, 300);
        cache.remove_expired(300, 100); // cutoff = 200, removes entry with last_seen=50
        assert_eq!(cache.len(), 1); // entry with last_seen=200 survives
    }
}
