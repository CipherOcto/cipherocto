//! Overlay routing — RFC-0850 §7
//!
//! Deterministic route computation for DOT overlay network.
//! Routes MUST NOT depend on latency, local heuristics, wall-clock, or CPU load.

use serde::{Deserialize, Serialize};

/// Route commitment for replay verification.
///
/// commitment = BLAKE3-256(gateway_sequence_hash || weights_hash || epoch)
#[derive(Debug, Clone, Serialize, Deserialize)]
#[repr(C)]
pub struct RouteCommitment {
    /// BLAKE3-256 of the gateway sequence
    pub gateway_sequence_hash: [u8; 32],
    /// BLAKE3-256 of the route weights
    pub weights_hash: [u8; 32],
    /// Network epoch at commitment time
    pub epoch: u64,
    /// The commitment hash (computed)
    pub commitment: [u8; 32],
}

impl RouteCommitment {
    /// Compute the commitment deterministically.
    pub fn compute(gateway_sequence_hash: [u8; 32], weights_hash: [u8; 32], epoch: u64) -> Self {
        let mut hasher = blake3::Hasher::new();
        hasher.update(&gateway_sequence_hash);
        hasher.update(&weights_hash);
        hasher.update(&epoch.to_be_bytes());
        let commitment = *hasher.finalize().as_bytes();

        Self {
            gateway_sequence_hash,
            weights_hash,
            epoch,
            commitment,
        }
    }

    /// Verify this commitment matches the expected computation.
    pub fn verify(&self) -> bool {
        let expected = Self::compute(self.gateway_sequence_hash, self.weights_hash, self.epoch);
        self.commitment == expected.commitment
    }
}

/// Route weights for deterministic scoring.
///
/// All weights are u64 to avoid floating-point non-determinism.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[repr(C)]
pub struct RouteWeights {
    /// Trust score weight (0-1000 basis points)
    pub trust_weight: u64,
    /// Bandwidth class weight (0-1000)
    pub bandwidth_weight: u64,
    /// Censorship resistance weight (0-1000)
    pub censorship_weight: u64,
    /// Cost weight (0-1000)
    pub cost_weight: u64,
}

impl Default for RouteWeights {
    fn default() -> Self {
        Self {
            trust_weight: 400,
            bandwidth_weight: 300,
            censorship_weight: 200,
            cost_weight: 100,
        }
    }
}

/// Gateway route entry in the overlay graph.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[repr(C)]
pub struct GatewayRoute {
    /// Target gateway ID
    pub gateway_id: [u8; 32],
    /// Connected broadcast domain hashes
    pub domain_hashes: Vec<[u8; 32]>,
    /// Route weights for scoring
    pub weights: RouteWeights,
    /// Current score (computed deterministically)
    pub score: u64,
    /// Route commitment
    pub commitment: RouteCommitment,
    /// Whether this route is active
    pub active: bool,
}

/// Partition event for platform failover.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[repr(C)]
pub struct PartitionEvent {
    /// Affected domain hash
    pub domain_hash: [u8; 32],
    /// Epoch when partition detected
    pub detected_epoch: u64,
    /// Remaining available carriers
    pub remaining_carriers: Vec<[u8; 32]>,
}

/// Compute a deterministic route score.
///
/// score = trust_weight * trust + bandwidth_weight * bandwidth +
///         censorship_weight * censorship - cost_weight * cost
///
/// All arithmetic is saturating to avoid overflow.
pub fn compute_route_score(
    weights: &RouteWeights,
    trust: u64,
    bandwidth: u64,
    censorship: u64,
    cost: u64,
) -> u64 {
    let trust_component = weights.trust_weight.saturating_mul(trust);
    let bandwidth_component = weights.bandwidth_weight.saturating_mul(bandwidth);
    let censorship_component = weights.censorship_weight.saturating_mul(censorship);
    let cost_component = weights.cost_weight.saturating_mul(cost);

    trust_component
        .saturating_add(bandwidth_component)
        .saturating_add(censorship_component)
        .saturating_sub(cost_component)
}

/// Select the best route deterministically.
///
/// Routes are sorted by (score DESC, gateway_id ASC) to ensure
/// deterministic selection regardless of insertion order.
pub fn select_best_route(routes: &[GatewayRoute]) -> Option<&GatewayRoute> {
    routes.iter().filter(|r| r.active).max_by(|a, b| {
        a.score
            .cmp(&b.score)
            .reverse()
            .then_with(|| a.gateway_id.cmp(&b.gateway_id))
    })
}

/// Handle a partition event by filtering affected routes.
///
/// Returns remaining routes that are not on the affected domain.
pub fn handle_partition<'a>(
    routes: &'a [GatewayRoute],
    event: &PartitionEvent,
) -> Vec<&'a GatewayRoute> {
    routes
        .iter()
        .filter(|r| r.active && !r.domain_hashes.contains(&event.domain_hash))
        .collect()
}

/// Compute a gateway sequence hash for route commitment.
pub fn compute_gateway_sequence_hash(gateway_ids: &[[u8; 32]]) -> [u8; 32] {
    let mut hasher = blake3::Hasher::new();
    for id in gateway_ids {
        hasher.update(id);
    }
    *hasher.finalize().as_bytes()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_route_commitment_deterministic() {
        let seq_hash = [1u8; 32];
        let weights_hash = [2u8; 32];
        let epoch = 100u64;

        let c1 = RouteCommitment::compute(seq_hash, weights_hash, epoch);
        let c2 = RouteCommitment::compute(seq_hash, weights_hash, epoch);
        assert_eq!(c1.commitment, c2.commitment);
    }

    #[test]
    fn test_route_commitment_verify() {
        let c = RouteCommitment::compute([1u8; 32], [2u8; 32], 100);
        assert!(c.verify());
    }

    #[test]
    fn test_route_commitment_different_epochs() {
        let c1 = RouteCommitment::compute([1u8; 32], [2u8; 32], 100);
        let c2 = RouteCommitment::compute([1u8; 32], [2u8; 32], 101);
        assert_ne!(c1.commitment, c2.commitment);
    }

    #[test]
    fn test_route_score_deterministic() {
        let weights = RouteWeights::default();
        let s1 = compute_route_score(&weights, 100, 50, 30, 10);
        let s2 = compute_route_score(&weights, 100, 50, 30, 10);
        assert_eq!(s1, s2);
    }

    #[test]
    fn test_route_score_saturating() {
        let weights = RouteWeights {
            trust_weight: u64::MAX,
            bandwidth_weight: 0,
            censorship_weight: 0,
            cost_weight: 0,
        };
        let score = compute_route_score(&weights, u64::MAX, 0, 0, 0);
        assert_eq!(score, u64::MAX); // saturates, doesn't overflow
    }

    #[test]
    fn test_select_best_route() {
        let routes = vec![
            GatewayRoute {
                gateway_id: [1u8; 32],
                domain_hashes: vec![],
                weights: RouteWeights::default(),
                score: 100,
                commitment: RouteCommitment::compute([0u8; 32], [0u8; 32], 0),
                active: true,
            },
            GatewayRoute {
                gateway_id: [2u8; 32],
                domain_hashes: vec![],
                weights: RouteWeights::default(),
                score: 200,
                commitment: RouteCommitment::compute([0u8; 32], [0u8; 32], 0),
                active: true,
            },
        ];
        let best = select_best_route(&routes).unwrap();
        assert_eq!(best.gateway_id, [2u8; 32]); // higher score wins
    }

    #[test]
    fn test_select_best_route_deterministic_tiebreak() {
        let routes = vec![
            GatewayRoute {
                gateway_id: [2u8; 32],
                domain_hashes: vec![],
                weights: RouteWeights::default(),
                score: 100,
                commitment: RouteCommitment::compute([0u8; 32], [0u8; 32], 0),
                active: true,
            },
            GatewayRoute {
                gateway_id: [1u8; 32],
                domain_hashes: vec![],
                weights: RouteWeights::default(),
                score: 100,
                commitment: RouteCommitment::compute([0u8; 32], [0u8; 32], 0),
                active: true,
            },
        ];
        let best = select_best_route(&routes).unwrap();
        assert_eq!(best.gateway_id, [1u8; 32]); // lower ID wins tiebreak
    }

    #[test]
    fn test_select_best_route_skips_inactive() {
        let routes = vec![
            GatewayRoute {
                gateway_id: [1u8; 32],
                domain_hashes: vec![],
                weights: RouteWeights::default(),
                score: 999,
                commitment: RouteCommitment::compute([0u8; 32], [0u8; 32], 0),
                active: false,
            },
            GatewayRoute {
                gateway_id: [2u8; 32],
                domain_hashes: vec![],
                weights: RouteWeights::default(),
                score: 100,
                commitment: RouteCommitment::compute([0u8; 32], [0u8; 32], 0),
                active: true,
            },
        ];
        let best = select_best_route(&routes).unwrap();
        assert_eq!(best.gateway_id, [2u8; 32]);
    }

    #[test]
    fn test_handle_partition() {
        let domain_affected = [0xAAu8; 32];
        let routes = vec![
            GatewayRoute {
                gateway_id: [1u8; 32],
                domain_hashes: vec![domain_affected, [0xBBu8; 32]],
                weights: RouteWeights::default(),
                score: 100,
                commitment: RouteCommitment::compute([0u8; 32], [0u8; 32], 0),
                active: true,
            },
            GatewayRoute {
                gateway_id: [2u8; 32],
                domain_hashes: vec![[0xCCu8; 32]],
                weights: RouteWeights::default(),
                score: 100,
                commitment: RouteCommitment::compute([0u8; 32], [0u8; 32], 0),
                active: true,
            },
        ];
        let event = PartitionEvent {
            domain_hash: domain_affected,
            detected_epoch: 500,
            remaining_carriers: vec![[2u8; 32]],
        };
        let remaining = handle_partition(&routes, &event);
        assert_eq!(remaining.len(), 1);
        assert_eq!(remaining[0].gateway_id, [2u8; 32]);
    }

    #[test]
    fn test_gateway_sequence_hash_deterministic() {
        let ids = vec![[1u8; 32], [2u8; 32], [3u8; 32]];
        let h1 = compute_gateway_sequence_hash(&ids);
        let h2 = compute_gateway_sequence_hash(&ids);
        assert_eq!(h1, h2);
    }
}
