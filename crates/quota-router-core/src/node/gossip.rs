use std::collections::BTreeMap;

use super::provider::{ProviderCapacity, RouterNodeId};

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct CapacityGossipPayload {
    pub sender_id: RouterNodeId,
    pub timestamp: u64,
    pub capacities: Vec<ProviderCapacity>,
    pub known_peers: Vec<RouterNodeId>,
    pub hmac: [u8; 32],
}

pub struct GossipCache {
    inner: std::sync::RwLock<GossipCacheInner>,
}

struct GossipCacheInner {
    entries: BTreeMap<RouterNodeId, Vec<ProviderCapacity>>,
    last_updated: BTreeMap<RouterNodeId, u64>,
}

/// Staleness threshold in seconds. Entries older than this are evicted
/// from the gossip cache. Default: 30s (3 × default gossip_interval).
const STALENESS_THRESHOLD: u64 = 30;

impl Default for GossipCache {
    fn default() -> Self {
        Self::new()
    }
}

impl GossipCache {
    pub fn new() -> Self {
        Self {
            inner: std::sync::RwLock::new(GossipCacheInner {
                entries: BTreeMap::new(),
                last_updated: BTreeMap::new(),
            }),
        }
    }

    pub fn merge(&self, sender_id: RouterNodeId, capacities: Vec<ProviderCapacity>) {
        let now = monotonic_now();
        let mut inner = self.inner.write().unwrap();
        inner.entries.insert(sender_id, capacities);
        inner.last_updated.insert(sender_id, now);
    }

    pub fn snapshot(&self) -> Vec<(RouterNodeId, Vec<ProviderCapacity>)> {
        let now = monotonic_now();
        let inner = self.inner.read().unwrap();
        inner
            .entries
            .iter()
            .filter(|(id, _)| {
                inner
                    .last_updated
                    .get(id)
                    .map(|t| now.saturating_sub(*t) <= STALENESS_THRESHOLD)
                    .unwrap_or(false)
            })
            .map(|(id, caps)| (*id, caps.clone()))
            .collect()
    }
}

use std::sync::OnceLock;
use std::time::Instant;

static START: OnceLock<Instant> = OnceLock::new();

/// Returns seconds elapsed since the first call in this process.
/// This is used for staleness comparisons and is wall-clock based
/// (not a monotonic counter), so `STALENESS_THRESHOLD` of 30 means
/// 30 real seconds.
pub fn monotonic_now() -> u64 {
    START.get_or_init(Instant::now).elapsed().as_secs()
}

#[cfg(test)]
mod tests {
    use super::*;
    use super::super::provider::{ModelPricing, ProviderHealth, ProviderId};

    fn test_capacity(name: &str, remaining: u64) -> ProviderCapacity {
        ProviderCapacity {
            provider_id: ProviderId([1u8; 32]),
            provider_name: name.into(),
            router_node_id: RouterNodeId([0u8; 32]),
            models: vec!["gpt-4o".into()],
            requests_remaining: remaining,
            pricing: vec![ModelPricing {
                model: "gpt-4o".into(),
                price_per_1k_tokens: 3,
            }],
            status: ProviderHealth::Healthy,
            latency_ms: 200,
            success_rate_bps: 9500,
            last_updated: 0,
        }
    }

    #[test]
    fn gossip_payload_roundtrip() {
        let payload = CapacityGossipPayload {
            sender_id: RouterNodeId([1u8; 32]),
            timestamp: 100,
            capacities: vec![test_capacity("openai", 50)],
            known_peers: vec![RouterNodeId([2u8; 32]), RouterNodeId([3u8; 32])],
            hmac: [42u8; 32],
        };
        let encoded = bincode::serialize(&payload).unwrap();
        let decoded: CapacityGossipPayload = bincode::deserialize(&encoded).unwrap();
        assert_eq!(decoded.sender_id, RouterNodeId([1u8; 32]));
        assert_eq!(decoded.timestamp, 100);
        assert_eq!(decoded.capacities.len(), 1);
        assert_eq!(decoded.known_peers.len(), 2);
        assert_eq!(decoded.hmac, [42u8; 32]);
    }

    #[test]
    fn gossip_cache_merge_and_snapshot() {
        let cache = GossipCache::new();
        let sender = RouterNodeId([1u8; 32]);
        let caps = vec![test_capacity("openai", 50)];
        cache.merge(sender, caps.clone());
        let snap = cache.snapshot();
        assert_eq!(snap.len(), 1);
        assert_eq!(snap[0].0, sender);
        assert_eq!(snap[0].1[0].requests_remaining, 50);
    }

    #[test]
    fn gossip_cache_snapshot_returns_fresh_entries() {
        let cache = GossipCache::new();
        let sender = RouterNodeId([1u8; 32]);
        cache.merge(sender, vec![]);
        // Freshly merged entries should always appear in snapshot
        let snap = cache.snapshot();
        assert_eq!(snap.len(), 1);
        assert_eq!(snap[0].0, sender);
    }

    #[test]
    fn gossip_cache_snapshot_empty_when_no_merges() {
        let cache = GossipCache::new();
        let snap = cache.snapshot();
        assert!(snap.is_empty());
    }

    #[test]
    fn gossip_cache_merge_overwrite() {
        let cache = GossipCache::new();
        let sender = RouterNodeId([1u8; 32]);
        cache.merge(sender, vec![test_capacity("openai", 100)]);
        cache.merge(sender, vec![test_capacity("openai", 10)]);
        let snap = cache.snapshot();
        assert_eq!(snap.len(), 1);
        assert_eq!(snap[0].1[0].requests_remaining, 10);
    }

    #[test]
    fn gossip_cache_multi_sender() {
        let cache = GossipCache::new();
        let a = RouterNodeId([1u8; 32]);
        let b = RouterNodeId([2u8; 32]);
        let c = RouterNodeId([3u8; 32]);
        cache.merge(a, vec![test_capacity("openai", 50)]);
        cache.merge(b, vec![test_capacity("anthropic", 30)]);
        cache.merge(c, vec![test_capacity("google", 20)]);
        let snap = cache.snapshot();
        assert_eq!(snap.len(), 3);
        let names: Vec<_> = snap
            .iter()
            .map(|(_, caps)| caps[0].provider_name.as_str())
            .collect();
        assert!(names.contains(&"openai"));
        assert!(names.contains(&"anthropic"));
        assert!(names.contains(&"google"));
    }
}
