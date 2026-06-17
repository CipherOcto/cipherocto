//! DomainCoordinator reputation (mission 0855p-c-reputation).
//!
//! Per-DC reputation across the domains the DC has managed.
//! Similar to RFC-0855p-b F(cross-mission coordinator reputation)
//! but DC-scoped.
//!
//! ## Quorum
//!
//! - `priority = stake / (1 + cross_domain_slash_count)` (soft)
//! - `cross_domain_slash_count >= 5` → excluded
//!
//! ## Gossip
//!
//! `/dot/reputation/dc/{dc_pubkey}`

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

/// Hard threshold: DCs with >= this many cross-domain slashes
/// are excluded from elections.
pub const DC_REPUTATION_HARD_THRESHOLD: u32 = 5;

/// A slash event reference for a specific domain.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct DcSlashEventRef {
    pub domain_id: String,
    pub event_hash: [u8; 32],
    pub slash_reason: u16,
    pub epoch: u64,
}

/// Per-DC reputation entry.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct DcReputationEntry {
    pub slashes: Vec<DcSlashEventRef>,
}

impl DcReputationEntry {
    pub fn cross_domain_slash_count(&self) -> u32 {
        self.slashes.len() as u32
    }

    pub fn add_slash(&mut self, ev: DcSlashEventRef) {
        if !self.slashes.contains(&ev) {
            self.slashes.push(ev);
        }
    }
}

/// The DC reputation store: maps `dc_pubkey` to a list of slash
/// events across all domains the DC has managed.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct DcRootedSlashReputationStore {
    by_dc: HashMap<String, DcReputationEntry>,
}

impl DcRootedSlashReputationStore {
    pub fn new() -> Self {
        Self::default()
    }

    /// Record a slash event for a DC.
    pub fn record_slash(&mut self, dc_pubkey: impl Into<String>, ev: DcSlashEventRef) {
        let dc = dc_pubkey.into();
        self.by_dc.entry(dc).or_default().add_slash(ev);
    }

    /// Returns the cross-domain slash count for a DC.
    pub fn cross_domain_slash_count(&self, dc: &str) -> u32 {
        self.by_dc
            .get(dc)
            .map(|e| e.cross_domain_slash_count())
            .unwrap_or(0)
    }

    /// Returns true if the DC is excluded (>= HARD_THRESHOLD).
    pub fn is_excluded(&self, dc: &str) -> bool {
        self.cross_domain_slash_count(dc) >= DC_REPUTATION_HARD_THRESHOLD
    }

    /// Returns the priority for a candidate: `stake / (1 + cross_domain_slash_count)`.
    /// Returns None if excluded.
    pub fn priority(&self, dc: &str, stake: u64) -> Option<u64> {
        let count = self.cross_domain_slash_count(dc);
        if count >= DC_REPUTATION_HARD_THRESHOLD {
            return None;
        }
        Some(stake / (1 + count as u64))
    }

    /// Build the libp2p gossip topic.
    pub fn gossip_topic(dc_pubkey: &str) -> String {
        assert!(!dc_pubkey.is_empty(), "dc_pubkey must not be empty");
        format!("/dot/reputation/dc/{dc_pubkey}")
    }

    /// Returns the number of tracked DCs.
    pub fn dc_count(&self) -> usize {
        self.by_dc.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ev(domain: &str, reason: u16) -> DcSlashEventRef {
        DcSlashEventRef {
            domain_id: domain.into(),
            event_hash: [0; 32],
            slash_reason: reason,
            epoch: 100,
        }
    }

    #[test]
    fn unknown_dc_zero_count() {
        let s = DcRootedSlashReputationStore::new();
        assert_eq!(s.cross_domain_slash_count("nope"), 0);
    }

    #[test]
    fn record_and_query() {
        let mut s = DcRootedSlashReputationStore::new();
        s.record_slash("dc-1", ev("domain-a", 0x000F));
        s.record_slash("dc-1", ev("domain-b", 0x000F));
        s.record_slash("dc-1", ev("domain-c", 0x000F));
        assert_eq!(s.cross_domain_slash_count("dc-1"), 3);
    }

    #[test]
    fn record_dedupes() {
        let mut s = DcRootedSlashReputationStore::new();
        s.record_slash("dc-1", ev("d", 0x000F));
        s.record_slash("dc-1", ev("d", 0x000F));
        assert_eq!(s.cross_domain_slash_count("dc-1"), 1);
    }

    #[test]
    fn hard_threshold_excludes() {
        let mut s = DcRootedSlashReputationStore::new();
        for i in 0..5 {
            s.record_slash("dc-1", ev(&format!("d-{i}"), 0x000F));
        }
        assert!(s.is_excluded("dc-1"));
        assert!(s.priority("dc-1", 1000).is_none());
    }

    #[test]
    fn priority_soft_penalty() {
        let mut s = DcRootedSlashReputationStore::new();
        s.record_slash("dc-1", ev("d-1", 0x000F));
        s.record_slash("dc-1", ev("d-2", 0x000F));
        // priority = 1000 / (1 + 2) = 333
        assert_eq!(s.priority("dc-1", 1000), Some(333));
    }

    #[test]
    fn topic_format() {
        assert_eq!(
            DcRootedSlashReputationStore::gossip_topic("dc-1"),
            "/dot/reputation/dc/dc-1"
        );
    }

    #[test]
    #[should_panic(expected = "dc_pubkey must not be empty")]
    fn gossip_topic_rejects_empty() {
        let _ = DcRootedSlashReputationStore::gossip_topic("");
    }

    #[test]
    fn dc_count() {
        let mut s = DcRootedSlashReputationStore::new();
        s.record_slash("dc-1", ev("d", 0x000F));
        s.record_slash("dc-2", ev("d", 0x000F));
        assert_eq!(s.dc_count(), 2);
    }

    #[test]
    fn threshold_constant() {
        assert_eq!(DC_REPUTATION_HARD_THRESHOLD, 5);
    }
}
