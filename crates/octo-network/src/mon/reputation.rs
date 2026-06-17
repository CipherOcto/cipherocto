//! Cross-mission coordinator reputation
//! (mission 0855p-b-cross-mission-reputation).
//!
//! Each `SlashEvent` per §"Slash Reason Codes" carries a
//! per-mission `slash_count`. For cross-mission reputation, the
//! local count is augmented with a global view fetched from a
//! `SlashReputationStore` (a map `coordinator_pubkey -> Vec<SlashEvent>`
//! from across all missions the coordinator has participated in).
//!
//! On election, candidates with a higher global slash count are
//! deprioritized.
//!
//! ## Priority formula
//!
//! `priority = stake / (1 + global_slash_count)` (soft penalty)
//!
//! ## Hard threshold
//!
//! `global_slash_count >= 5` → excluded from the election.
//!
//! ## Gossip
//!
//! The store is gossiped across the libp2p mesh under
//! `/dot/reputation/{coordinator_pubkey}` (referenced in
//! `mon::gossip::reputation_topic`).

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

/// Hard threshold: candidates with >= this many global slashes
/// are excluded from the election.
pub const HARD_THRESHOLD: u32 = 5;

/// A slash event reference stored in the reputation store.
/// The full event is fetched on demand (privacy: only the hash
/// is gossiped by default).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SlashEventRef {
    /// The mission this slash occurred in.
    pub mission_id: String,
    /// The slash event's hash.
    pub event_hash: [u8; 32],
    /// The slash reason code.
    pub slash_reason: u16,
    /// The epoch when the slash was finalized.
    pub epoch: u64,
}

/// Per-coordinator reputation.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct CoordinatorReputation {
    /// All slash events for this coordinator across missions.
    pub slashes: Vec<SlashEventRef>,
}

impl CoordinatorReputation {
    /// Returns the global slash count.
    pub fn global_slash_count(&self) -> u32 {
        self.slashes.len() as u32
    }

    /// Add a slash event.
    pub fn add_slash(&mut self, ev: SlashEventRef) {
        if !self.slashes.contains(&ev) {
            self.slashes.push(ev);
        }
    }
}

/// The reputation store: maps `coordinator_pubkey` to reputation.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct SlashReputationStore {
    by_coordinator: HashMap<String, CoordinatorReputation>,
}

impl SlashReputationStore {
    pub fn new() -> Self {
        Self::default()
    }

    /// Record a slash event for a coordinator.
    pub fn record_slash(&mut self, coordinator: impl Into<String>, ev: SlashEventRef) {
        let coordinator = coordinator.into();
        self.by_coordinator
            .entry(coordinator)
            .or_default()
            .add_slash(ev);
    }

    /// Returns the global slash count for `coordinator` (0 if
    /// not in the store).
    pub fn global_slash_count(&self, coordinator: &str) -> u32 {
        self.by_coordinator
            .get(coordinator)
            .map(|r| r.global_slash_count())
            .unwrap_or(0)
    }

    /// Returns true if the coordinator is excluded from
    /// elections (>= HARD_THRESHOLD slashes).
    pub fn is_excluded(&self, coordinator: &str) -> bool {
        self.global_slash_count(coordinator) >= HARD_THRESHOLD
    }

    /// Returns the priority for a candidate: `stake / (1 + global_slash_count)`.
    /// Returns None if the candidate is excluded.
    pub fn priority(&self, coordinator: &str, stake: u64) -> Option<u64> {
        let count = self.global_slash_count(coordinator);
        if count >= HARD_THRESHOLD {
            return None;
        }
        Some(stake / (1 + count as u64))
    }

    /// Returns the gossip topic for a coordinator's reputation.
    pub fn gossip_topic(coordinator: &str) -> String {
        format!("/dot/reputation/{coordinator}")
    }

    /// Returns the number of tracked coordinators.
    pub fn coordinator_count(&self) -> usize {
        self.by_coordinator.len()
    }

    /// Returns the number of tracked slash events.
    pub fn total_slash_events(&self) -> usize {
        self.by_coordinator
            .values()
            .map(|r| r.slashes.len())
            .sum()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ev(mission: &str, reason: u16) -> SlashEventRef {
        SlashEventRef {
            mission_id: mission.into(),
            event_hash: [0u8; 32],
            slash_reason: reason,
            epoch: 100,
        }
    }

    #[test]
    fn global_slash_count_zero_for_unknown() {
        let s = SlashReputationStore::new();
        assert_eq!(s.global_slash_count("nope"), 0);
    }

    #[test]
    fn record_and_query_slash() {
        let mut s = SlashReputationStore::new();
        s.record_slash("coord-1", ev("mission-a", 0x0001));
        s.record_slash("coord-1", ev("mission-b", 0x0003));
        assert_eq!(s.global_slash_count("coord-1"), 2);
    }

    #[test]
    fn record_dedupes_identical_events() {
        let mut s = SlashReputationStore::new();
        s.record_slash("coord-1", ev("mission-a", 0x0001));
        s.record_slash("coord-1", ev("mission-a", 0x0001));
        assert_eq!(s.global_slash_count("coord-1"), 1);
    }

    #[test]
    fn hard_threshold_excludes() {
        let mut s = SlashReputationStore::new();
        for i in 0..5 {
            s.record_slash("coord-1", ev(&format!("m-{i}"), 0x0001));
        }
        assert!(s.is_excluded("coord-1"));
        assert!(s.priority("coord-1", 1000).is_none());
    }

    #[test]
    fn priority_soft_penalty() {
        let mut s = SlashReputationStore::new();
        s.record_slash("coord-1", ev("m-1", 0x0001));
        s.record_slash("coord-1", ev("m-2", 0x0001));
        // priority = 1000 / (1 + 2) = 333
        assert_eq!(s.priority("coord-1", 1000), Some(333));
    }

    #[test]
    fn priority_zero_slashes() {
        let s = SlashReputationStore::new();
        assert_eq!(s.priority("coord-1", 1000), Some(1000));
    }

    #[test]
    fn gossip_topic_format() {
        assert_eq!(
            SlashReputationStore::gossip_topic("coord-1"),
            "/dot/reputation/coord-1"
        );
    }

    #[test]
    fn coordinator_count_and_total() {
        let mut s = SlashReputationStore::new();
        s.record_slash("a", ev("m-1", 0x0001));
        s.record_slash("a", ev("m-2", 0x0001));
        s.record_slash("b", ev("m-1", 0x0001));
        assert_eq!(s.coordinator_count(), 2);
        assert_eq!(s.total_slash_events(), 3);
    }

    #[test]
    fn hard_threshold_constant() {
        assert_eq!(HARD_THRESHOLD, 5);
    }
}
