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

    /// Feed relay trust scores from this registry into a `SyncSessionManager`.
    ///
    /// For each gateway in the registry that is also a known sync peer,
    /// converts the composite `RelayScore` to a trust factor (0–10000)
    /// and calls `session.update_relay_score()`. This is the "last mile"
    /// wiring that connects PoRelay scoring to sync peer selection.
    ///
    /// Returns the number of peers whose scores were updated.
    pub fn feed_sync_session(
        &self,
        session: &octo_sync::session::SyncSessionManager,
    ) -> usize {
        let mut updated = 0;
        for (gw_id, relay_score) in &self.scores {
            let trust_factor = super::score::relay_score_to_trust_factor(relay_score);
            let peer_id = octo_sync::identity::SyncPeerId(*gw_id);
            // Only update if the peer is known to the session
            if session.peer_state(peer_id).is_some() {
                session.update_relay_score(peer_id, trust_factor);
                updated += 1;
            }
        }
        updated
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

    #[test]
    fn test_feed_sync_session_updates_known_peers() {
        use octo_sync::config::{SyncConfig, SyncRole};
        use octo_sync::session::SyncSessionManager;
        use octo_sync::test_util::MockAdapter;
        use std::sync::Arc;

        let mut mission_id = [0u8; 32];
        mission_id[0] = 0xAB;
        let node_id = [0x01u8; 32];

        let config = SyncConfig::new(mission_id, SyncRole::Replicator, vec![0x02; 32]);
        let adapter: Arc<dyn octo_sync::adapter::DatabaseSyncAdapter> =
            Arc::new(MockAdapter::new(mission_id, node_id));
        let session =
            SyncSessionManager::new(adapter, config, &[0x42u8; 32]).unwrap();

        // Subscribe two peers
        let peer_a = octo_sync::identity::SyncPeerId([0x10u8; 32]);
        let peer_b = octo_sync::identity::SyncPeerId([0x20u8; 32]);
        let peer_c = octo_sync::identity::SyncPeerId([0x30u8; 32]);
        session.subscribe_peer(peer_a).unwrap();
        session.subscribe_peer(peer_b).unwrap();
        // peer_c NOT subscribed

        // Registry has scores for all three
        let mut reg = TrustRegistry::new(100);
        reg.update_score(make_score(0x10, 800_000));
        reg.update_score(make_score(0x20, 200_000));
        reg.update_score(make_score(0x30, 500_000));

        let updated = reg.feed_sync_session(&session);
        // Only peer_a and peer_b should be updated (peer_c not subscribed)
        assert_eq!(updated, 2);

        // Verify trust factors were set
        let trust_a = session.peer_relay_score(peer_a).unwrap();
        let trust_b = session.peer_relay_score(peer_b).unwrap();
        assert!(trust_a > 0, "peer_a trust should be non-zero");
        assert!(trust_b > 0, "peer_b trust should be non-zero");
        assert!(trust_a > trust_b, "peer_a has higher composite → higher trust");

        // peer_c not subscribed, so no relay score
        assert!(session.peer_relay_score(peer_c).is_none());
    }

    #[test]
    fn test_feed_sync_session_empty_registry() {
        use octo_sync::config::{SyncConfig, SyncRole};
        use octo_sync::session::SyncSessionManager;
        use octo_sync::test_util::MockAdapter;
        use std::sync::Arc;

        let mut mission_id = [0u8; 32];
        mission_id[0] = 0xCD;
        let config = SyncConfig::new(mission_id, SyncRole::Replicator, vec![0x03; 32]);
        let adapter: Arc<dyn octo_sync::adapter::DatabaseSyncAdapter> =
            Arc::new(MockAdapter::new(mission_id, [0x11; 32]));
        let session =
            SyncSessionManager::new(adapter, config, &[0x42u8; 32]).unwrap();

        let reg = TrustRegistry::new(100);
        let updated = reg.feed_sync_session(&session);
        assert_eq!(updated, 0);
    }
}
