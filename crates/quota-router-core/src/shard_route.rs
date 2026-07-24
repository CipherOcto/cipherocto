//! Shard routing (PR-Q6, W7).
//!
//! Wraps `quota_router_sm_engine::shard::shard_for_segment` for the proxy.
//! Multi-shard variant of the 11-step exercise routes queries through
//! `route_to_shard` to direct writes to the appropriate shard.

use quota_router_sm_engine::shard::{num_shards_for, shard_for_segment, ShardId};

/// Cluster shard config for the proxy.
#[derive(Debug, Clone, Copy)]
pub struct ClusterShardConfig {
    pub num_nodes: usize,
    pub created_at_unix: u64,
}

impl ClusterShardConfig {
    /// Build a config from a cluster of `n` nodes.
    /// `num_shards = clamp(ceil(sqrt(n)), 4, 256)`.
    #[must_use]
    pub fn for_cluster(n: usize, created_at_unix: u64) -> Self {
        Self {
            num_nodes: n,
            created_at_unix,
        }
    }

    /// Number of shards this config produces.
    #[must_use]
    pub fn num_shards(&self) -> u32 {
        num_shards_for(self.num_nodes)
    }
}

/// Route a WAL segment id to the shard responsible for it.
pub fn route_to_shard(config: &ClusterShardConfig, wal_segment_id: &[u8; 32]) -> Option<ShardId> {
    let n = config.num_shards();
    shard_for_segment(wal_segment_id, n).ok().map(ShardId)
}

/// Route an ask_id to the shard responsible for it (PR-Q6).
///
/// Asks and reservations live on the shard that owns their content
/// hash. The proxy uses this to dispatch settlement calls to the right
/// shard connection.
pub fn route_ask(config: &ClusterShardConfig, ask_id: &[u8; 32]) -> Option<ShardId> {
    route_to_shard(config, ask_id)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cluster_shard_config_num_shards_clamp() {
        let s = ClusterShardConfig::for_cluster(0, 0);
        assert_eq!(s.num_shards(), 4);
        let s = ClusterShardConfig::for_cluster(100, 0);
        assert_eq!(s.num_shards(), 10);
        let s = ClusterShardConfig::for_cluster(100_000, 0);
        assert_eq!(s.num_shards(), 256);
    }

    #[test]
    fn route_to_shard_deterministic() {
        let config = ClusterShardConfig::for_cluster(100, 0);
        let seg = [0x42; 32];
        let a = route_to_shard(&config, &seg).unwrap();
        let b = route_to_shard(&config, &seg).unwrap();
        assert_eq!(a, b);
    }

    #[test]
    fn route_to_shard_in_range() {
        let config = ClusterShardConfig::for_cluster(100, 0);
        let seg = [0x99; 32];
        let s = route_to_shard(&config, &seg).unwrap();
        assert!(s.0 < config.num_shards());
    }

    #[test]
    fn route_ask_matches_route_to_shard() {
        let config = ClusterShardConfig::for_cluster(100, 0);
        let ask_id = [0xab; 32];
        let a = route_ask(&config, &ask_id).unwrap();
        let b = route_to_shard(&config, &ask_id).unwrap();
        assert_eq!(a, b);
    }
}
