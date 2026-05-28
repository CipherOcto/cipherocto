//! Trust Registry (RFC-0860 §6)

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use super::score::RelayScore;

/// Trust Registry — maintains composite scores for all known gateways.
///
/// Uses BTreeMap for deterministic iteration (Class A).
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TrustRegistry {
    /// Map of gateway_id → current RelayScore
    pub scores: BTreeMap<[u8; 32], RelayScore>,
    /// Map of gateway_id → stake amount (OCTO-B)
    pub stakes: BTreeMap<[u8; 32], u64>,
    /// Current epoch
    pub current_epoch: u64,
    /// Score history depth (number of epochs retained)
    pub history_depth: u32,
}

impl TrustRegistry {
    /// Create a new empty trust registry
    pub fn new(history_depth: u32) -> Self {
        Self {
            scores: BTreeMap::new(),
            stakes: BTreeMap::new(),
            current_epoch: 0,
            history_depth,
        }
    }

    /// Update or insert a relay score
    pub fn update_score(&mut self, score: RelayScore) {
        self.scores.insert(score.gateway_id, score);
    }

    /// Get relay score for a gateway
    pub fn get_score(&self, gateway_id: &[u8; 32]) -> Option<&RelayScore> {
        self.scores.get(gateway_id)
    }

    /// Set stake amount for a gateway
    pub fn set_stake(&mut self, gateway_id: [u8; 32], amount: u64) {
        self.stakes.insert(gateway_id, amount);
    }

    /// Get stake amount for a gateway
    pub fn get_stake(&self, gateway_id: &[u8; 32]) -> u64 {
        self.stakes.get(gateway_id).copied().unwrap_or(0)
    }

    /// Apply score decay to all gateways that haven't submitted proofs recently
    pub fn apply_decay(&mut self, epochs_inactive_threshold: u32) {
        for score in self.scores.values_mut() {
            let inactive_epochs = self.current_epoch.saturating_sub(score.epoch) as u32;
            if inactive_epochs > epochs_inactive_threshold {
                let excess = inactive_epochs - epochs_inactive_threshold;
                score.composite = RelayScore::decay_score(score.composite, excess);
            }
        }
    }

    /// Get top-N gateways by composite score (deterministic ordering)
    pub fn top_gateways(&self, n: usize) -> Vec<&RelayScore> {
        let mut entries: Vec<&RelayScore> = self.scores.values().collect();
        entries.sort_by(|a, b| {
            b.composite
                .cmp(&a.composite)
                .then_with(|| a.gateway_id.cmp(&b.gateway_id))
        });
        entries.into_iter().take(n).collect()
    }

    /// Number of registered gateways
    pub fn len(&self) -> usize {
        self.scores.len()
    }

    /// Check if registry is empty
    pub fn is_empty(&self) -> bool {
        self.scores.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_score(id: u8, composite: u64) -> RelayScore {
        RelayScore {
            gateway_id: [id; 32],
            epoch: 1,
            forwarding_score: 0,
            availability_score: 0,
            bandwidth_score: 0,
            uptime_score: 0,
            diversity_bonus: 0,
            stake_multiplier: 1000,
            composite,
        }
    }

    #[test]
    fn test_registry_insert_and_get() {
        let mut reg = TrustRegistry::new(100);
        reg.update_score(make_score(1, 500));
        assert_eq!(reg.get_score(&[1u8; 32]).unwrap().composite, 500);
    }

    #[test]
    fn test_registry_stake() {
        let mut reg = TrustRegistry::new(100);
        reg.set_stake([1u8; 32], 1000);
        assert_eq!(reg.get_stake(&[1u8; 32]), 1000);
        assert_eq!(reg.get_stake(&[2u8; 32]), 0);
    }

    #[test]
    fn test_registry_top_gateways() {
        let mut reg = TrustRegistry::new(100);
        reg.update_score(make_score(1, 500));
        reg.update_score(make_score(2, 900));
        reg.update_score(make_score(3, 700));

        let top = reg.top_gateways(2);
        assert_eq!(top.len(), 2);
        assert_eq!(top[0].gateway_id, [2u8; 32]);
        assert_eq!(top[1].gateway_id, [3u8; 32]);
    }

    #[test]
    fn test_registry_deterministic_ordering() {
        let mut reg = TrustRegistry::new(100);
        reg.update_score(make_score(1, 500));
        reg.update_score(make_score(2, 500));

        let top = reg.top_gateways(10);
        assert_eq!(top[0].gateway_id, [1u8; 32]); // lower ID wins tiebreak
        assert_eq!(top[1].gateway_id, [2u8; 32]);
    }

    #[test]
    fn test_registry_len() {
        let mut reg = TrustRegistry::new(100);
        assert!(reg.is_empty());
        reg.update_score(make_score(1, 500));
        assert_eq!(reg.len(), 1);
    }
}
