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
            entries: BTreeMap::new(),
            last_updated: BTreeMap::new(),
        }
    }

    pub fn merge(&mut self, sender_id: RouterNodeId, capacities: Vec<ProviderCapacity>) {
        let now = monotonic_now();
        self.entries.insert(sender_id, capacities);
        self.last_updated.insert(sender_id, now);
    }

    pub fn snapshot(&self) -> Vec<(RouterNodeId, Vec<ProviderCapacity>)> {
        let now = monotonic_now();
        self.entries
            .iter()
            .filter(|(id, _)| {
                self.last_updated
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
