//! `QuotaRouterNode` — RFC-0870 Distributed Quota Router Network
//! substrate projection (RFC-0011-p Phase 8 G10).
//!
//! Layer B substrate addition per RFC-0011-h
//! §Substrate-Additions Companion Missions row G10.
//! Per-extension transport impl crates (Layer D) are
//! OUT OF SCOPE per per-extension crate pattern; this
//! module exposes only the Layer B substrate projection
//! consumed by `octo network router status` (read) +
//! `octo network router peers <peer_node_id_hex>` (read)
//! per RFC-0011-p Phase 8 §Subcommand Taxonomy.
//!
//! `BTreeMap` chosen over `HashMap` for deterministic
//! iteration order per RFC-0011-h §Output Envelope
//! order determinism.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// `QuotaRouterNode` — substrate-faithful quota router
/// node state for RFC-0870 Distributed Quota Router
/// Network. Per RFC-0011-p Phase 8 §Substrate Mapping
/// Table, this struct is the Layer B substrate projection
/// consumed by `octo network router status` + `octo
/// network router peers`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct QuotaRouterNode {
    /// Node-local self identifier (32-byte canonical).
    pub node_id: [u8; 32],
    /// Current operational status (Healthy + Degraded +
    /// Offline enum).
    pub status: RouterStatus,
    /// Per-peer remaining capacity in quota units.
    /// `BTreeMap` for deterministic iteration order per
    /// RFC-0011-h §Output Envelope order determinism.
    pub peer_capacities: BTreeMap<[u8; 32], u64>,
    /// Last capacity sync epoch (operator-side
    /// observability).
    pub last_sync_epoch: u64,
}

/// `RouterStatus` — operational status enum for the
/// quota router node per RFC-0870 §Operational States.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum RouterStatus {
    /// Node is operational and routing quota traffic
    /// normally.
    Healthy,
    /// Node is operational but experiencing degraded
    /// performance (high latency, partial peer
    /// reachability, or quota pressure). Operator
    /// should investigate.
    Degraded,
    /// Node is not routing quota traffic. No read or
    /// write path is available.
    Offline,
}

/// `QuotaRouterNodeAccess` — trait abstraction over
/// quota router node state per RFC-0011-p Phase 8 G10
/// per-extension crate pattern. Trait in Layer B;
/// concrete impl crates (substrate-ext-quota-router-*)
/// in Layer D, OUT OF SCOPE.
pub trait QuotaRouterNodeAccess: Send + Sync {
    /// Return the local node_id (32-byte canonical).
    fn node_id(&self) -> [u8; 32];
    /// Return the current operational status.
    fn status(&self) -> RouterStatus;
    /// Return the remaining quota capacity for the given
    /// peer node id, or `None` if the peer is not in the
    /// local routing table.
    fn peer_capacity(&self, peer_node_id: &[u8; 32]) -> Option<u64>;
    /// Return the number of peers with non-zero capacity.
    fn reachable_peer_count(&self) -> usize;
    /// Return the total peer count including zero-capacity
    /// peers.
    fn total_peer_count(&self) -> usize;
    /// Return the last capacity sync epoch.
    fn last_sync_epoch(&self) -> u64;
}

impl QuotaRouterNodeAccess for QuotaRouterNode {
    fn node_id(&self) -> [u8; 32] {
        self.node_id
    }
    fn status(&self) -> RouterStatus {
        self.status
    }
    fn peer_capacity(&self, peer_node_id: &[u8; 32]) -> Option<u64> {
        self.peer_capacities.get(peer_node_id).copied()
    }
    fn reachable_peer_count(&self) -> usize {
        self.peer_capacities.values().filter(|&&c| c > 0).count()
    }
    fn total_peer_count(&self) -> usize {
        self.peer_capacities.len()
    }
    fn last_sync_epoch(&self) -> u64 {
        self.last_sync_epoch
    }
}

impl Default for QuotaRouterNode {
    /// Default node state is Offline with empty peer
    /// capacities (substrate-faithful projection of a
    /// node that has not yet joined the router network).
    fn default() -> Self {
        Self {
            node_id: [0u8; 32],
            status: RouterStatus::Offline,
            peer_capacities: BTreeMap::new(),
            last_sync_epoch: 0,
        }
    }
}

impl QuotaRouterNode {
    /// Substrate-faithful status accessor for `octo
    /// network router status` (RFC-0011-p Phase 8).
    /// Returns the current operational status.
    #[must_use]
    pub fn status(&self) -> RouterStatus {
        self.status
    }

    /// Substrate-faithful peer-capacity accessor for
    /// `octo network router peers <peer_node_id_hex>`
    /// (RFC-0011-p Phase 8). Returns the remaining
    /// quota capacity for the given peer node id, or
    /// `None` if the peer is not in the local routing
    /// table.
    #[must_use]
    pub fn peer_capacity(&self, peer_node_id: &[u8; 32]) -> Option<u64> {
        self.peer_capacities.get(peer_node_id).copied()
    }

    /// Returns the number of peers with non-zero capacity
    /// (operator-side observability helper).
    #[must_use]
    pub fn reachable_peer_count(&self) -> usize {
        self.peer_capacities.values().filter(|&&c| c > 0).count()
    }

    /// Returns the total peer count including zero-capacity
    /// peers (operator-side observability helper).
    #[must_use]
    pub fn total_peer_count(&self) -> usize {
        self.peer_capacities.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_router_node_default_status_is_offline() {
        let node = QuotaRouterNode::default();
        assert_eq!(node.status(), RouterStatus::Offline);
        assert_eq!(node.node_id, [0u8; 32]);
        assert_eq!(node.last_sync_epoch, 0);
        assert!(node.peer_capacities.is_empty());
    }

    #[test]
    fn test_peer_capacity_miss_returns_none() {
        let node = QuotaRouterNode::default();
        let unknown_peer = [0xAB; 32];
        assert_eq!(node.peer_capacity(&unknown_peer), None);
    }

    #[test]
    fn test_peer_capacity_hit_returns_value() {
        let peer_id = [0x42u8; 32];
        let mut peer_capacities = BTreeMap::new();
        peer_capacities.insert(peer_id, 1024_u64);

        let node = QuotaRouterNode {
            node_id: [0x01u8; 32],
            status: RouterStatus::Healthy,
            peer_capacities,
            last_sync_epoch: 100,
        };

        assert_eq!(node.peer_capacity(&peer_id), Some(1024));
        assert_eq!(node.status(), RouterStatus::Healthy);
        assert_eq!(node.last_sync_epoch, 100);
    }

    #[test]
    fn test_reachable_peer_count_excludes_zero_capacity() {
        let peer_a = [0x01u8; 32];
        let peer_b = [0x02u8; 32];
        let peer_c = [0x03u8; 32];

        let mut peer_capacities = BTreeMap::new();
        peer_capacities.insert(peer_a, 100_u64);
        peer_capacities.insert(peer_b, 0_u64);
        peer_capacities.insert(peer_c, 50_u64);

        let node = QuotaRouterNode {
            status: RouterStatus::Degraded,
            peer_capacities,
            ..Default::default()
        };

        assert_eq!(node.reachable_peer_count(), 2);
        assert_eq!(node.total_peer_count(), 3);
    }

    #[test]
    fn test_btreemap_deterministic_iteration_order() {
        let peer_a = [0x01u8; 32];
        let peer_b = [0x02u8; 32];
        let peer_c = [0x03u8; 32];

        let mut peer_capacities = BTreeMap::new();
        peer_capacities.insert(peer_c, 30_u64);
        peer_capacities.insert(peer_a, 10_u64);
        peer_capacities.insert(peer_b, 20_u64);

        let keys: Vec<[u8; 32]> = peer_capacities.keys().copied().collect();
        assert_eq!(keys, vec![peer_a, peer_b, peer_c]);
    }

    #[test]
    fn test_router_status_serde_lowercase() {
        let healthy = serde_json::to_string(&RouterStatus::Healthy).unwrap();
        let degraded = serde_json::to_string(&RouterStatus::Degraded).unwrap();
        let offline = serde_json::to_string(&RouterStatus::Offline).unwrap();
        assert_eq!(healthy, "\"healthy\"");
        assert_eq!(degraded, "\"degraded\"");
        assert_eq!(offline, "\"offline\"");
    }
}
