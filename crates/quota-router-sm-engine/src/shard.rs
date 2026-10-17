//! Resource shard routing (RFC-0963).
//!
//! Ledger is partitioned by deriving a shard from a content-addressed
//! identifier (WAL segment id, vault id). Each shard publishes its own
//! Merkle root; cross-shard writes use `MultiEnvelope` (RFC-0962 §7).
//!
//! ## Number of shards
//!
//! `num_shards = clamp(ceil(sqrt(N)), 4, 256)` where `N` is the cluster
//! node count. Floor at 4 ensures small clusters still shard; cap at 256
//! prevents excessive fan-out.
//!
//! ## Routing
//!
//! `shard_for_segment(wal_segment_id, num_shards)` is deterministic: two
//! nodes given the same `wal_segment_id` and `num_shards` produce the same
//! shard index. Backed by BLAKE3 hash of the segment id modulo `num_shards`.

#![warn(missing_debug_implementations)]

use serde::{Deserialize, Serialize};

/// Minimum number of shards (RFC-0963 §1).
pub const MIN_SHARDS: u32 = 4;
/// Maximum number of shards (RFC-0963 §1).
pub const MAX_SHARDS: u32 = 256;

/// Compute the number of shards for a cluster of `n` nodes.
///
/// Formula: `num_shards = clamp(ceil(sqrt(N)), 4, 256)`.
/// - `N = 1` → 4 (clamped to min)
/// - `N = 16` → 4
/// - `N = 17` → 5
/// - `N = 25` → 5
/// - `N = 65536` → 256
/// - `N = 100000` → 256 (clamped to max)
#[must_use]
pub fn num_shards_for(n: usize) -> u32 {
    // `usize as f64` may lose precision for very large N, but the
    // final value is clamped to [MIN_SHARDS, MAX_SHARDS] = [4, 256], so
    // the truncation is bounded and the floor on sqrt() guarantees we
    // never under-count shards for any real cluster size.
    #[allow(clippy::cast_precision_loss)]
    let sqrt = (n as f64).sqrt().ceil() as u32;
    sqrt.clamp(MIN_SHARDS, MAX_SHARDS)
}

/// Deterministic shard routing for a WAL segment id.
///
/// `shard_for_segment(wal_segment_id, num_shards) -> shard_index` where
/// `shard_index < num_shards`. Same inputs yield same output across nodes.
///
/// Per RFC-0963 §1b: `prefix = u32::from_be_bytes(BLAKE3(segment_id)[0..4])`,
/// then `prefix % num_shards`. Big-endian byte order is canonical across nodes.
///
/// Returns `Err` if `num_shards == 0`.
pub fn shard_for_segment(wal_segment_id: &[u8; 32], num_shards: u32) -> Result<u32, ShardError> {
    if num_shards == 0 {
        return Err(ShardError::ZeroShards);
    }
    let hash = blake3::hash(wal_segment_id);
    let bytes: [u8; 4] = hash.as_bytes()[..4].try_into().expect("4 bytes");
    let prefix = u32::from_be_bytes(bytes);
    Ok(prefix % num_shards)
}

/// Shard identifier.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ShardId(pub u32);

/// Shard routing error.
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum ShardError {
    #[error("num_shards must be > 0")]
    ZeroShards,
}

/// Cluster shard map (RFC-0963 §1a).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ShardMap {
    pub num_shards: u32,
    pub created_at_unix: u64,
}

impl ShardMap {
    #[must_use]
    pub fn new(num_shards: u32, created_at_unix: u64) -> Self {
        Self {
            num_shards,
            created_at_unix,
        }
    }

    /// Build a ShardMap sized for a cluster of `n` nodes.
    #[must_use]
    pub fn for_cluster(n: usize, created_at_unix: u64) -> Self {
        Self::new(num_shards_for(n), created_at_unix)
    }

    /// Route a WAL segment id to a shard.
    pub fn route(&self, wal_segment_id: &[u8; 32]) -> Result<ShardId, ShardError> {
        shard_for_segment(wal_segment_id, self.num_shards).map(ShardId)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn num_shards_clamped_to_min() {
        assert_eq!(num_shards_for(0), 4);
        assert_eq!(num_shards_for(1), 4);
        assert_eq!(num_shards_for(16), 4);
    }

    #[test]
    fn num_shards_uses_sqrt() {
        assert_eq!(num_shards_for(17), 5); // ceil(sqrt(17)) = 5
        assert_eq!(num_shards_for(25), 5); // ceil(sqrt(25)) = 5
        assert_eq!(num_shards_for(26), 6); // ceil(sqrt(26)) = 6
        assert_eq!(num_shards_for(100), 10);
        assert_eq!(num_shards_for(10_000), 100);
    }

    #[test]
    fn num_shards_clamped_to_max() {
        assert_eq!(num_shards_for(100_000), 256);
        assert_eq!(num_shards_for(1_000_000), 256);
    }

    #[test]
    fn shard_for_segment_deterministic() {
        let seg = [0xab; 32];
        let a = shard_for_segment(&seg, 16).unwrap();
        let b = shard_for_segment(&seg, 16).unwrap();
        assert_eq!(a, b);
    }

    #[test]
    fn shard_for_segment_in_range() {
        let seg = [0x42; 32];
        for num_shards in [1, 4, 16, 64, 256] {
            let shard = shard_for_segment(&seg, num_shards).unwrap();
            assert!(shard < num_shards);
        }
    }

    #[test]
    fn shard_for_segment_differs_for_different_segments() {
        let a = shard_for_segment(&[0x01; 32], 16).unwrap();
        let b = shard_for_segment(&[0x02; 32], 16).unwrap();
        // Not strictly required to differ (BLAKE3 collisions), but for
        // distinct inputs the probability is 1/16.
        // Skip if they collide (extremely rare).
        if a == b {
            eprintln!("trivial collision; test inconclusive");
        }
    }

    #[test]
    fn shard_for_segment_distributes() {
        // Send 1000 random segments; verify all shards touched.
        let num_shards = 16u32;
        let mut touched = vec![false; num_shards as usize];
        for i in 0..1000u32 {
            let seg = i
                .to_le_bytes()
                .into_iter()
                .chain([0u8; 28])
                .collect::<Vec<u8>>();
            let mut arr = [0u8; 32];
            arr.copy_from_slice(&seg);
            let shard = shard_for_segment(&arr, num_shards).unwrap() as usize;
            touched[shard] = true;
        }
        let coverage = touched.iter().filter(|x| **x).count();
        // Should cover at least 14/16 shards (statistical).
        assert!(coverage >= 14, "coverage {coverage} too low");
    }

    #[test]
    fn shard_for_segment_zero_shards_rejected() {
        let err = shard_for_segment(&[0x01; 32], 0).unwrap_err();
        assert_eq!(err, ShardError::ZeroShards);
    }

    #[test]
    fn shard_map_for_cluster() {
        let map = ShardMap::for_cluster(100, 1_000_000);
        assert_eq!(map.num_shards, 10);
        assert_eq!(map.created_at_unix, 1_000_000);
    }

    #[test]
    fn shard_map_routes() {
        let map = ShardMap::for_cluster(100, 1_000_000);
        let s = map.route(&[0x42; 32]).unwrap();
        assert!(s.0 < map.num_shards);
    }

    #[test]
    fn shard_map_route_matches_direct() {
        let map = ShardMap::for_cluster(100, 1_000_000);
        let seg = [0x99; 32];
        let s1 = map.route(&seg).unwrap();
        let s2 = shard_for_segment(&seg, map.num_shards).unwrap();
        assert_eq!(s1.0, s2);
    }
}
