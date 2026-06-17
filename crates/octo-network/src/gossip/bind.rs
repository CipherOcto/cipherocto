//! BIND envelope gossip over libp2p (mission 0850p-c-libp2p-propagation).
//!
//! The BIND envelope is gossiped on the libp2p mesh using the
//! standard DOT gossip protocol (RFC-0852 "Deterministic Gossip
//! Protocol"). The gossip topic is derived from the `domain_id`:
//!
//! ```text
//! /dot/bind/{domain_id_base58}
//! ```
//!
//! ## Why off by default?
//!
//! Pre-admission nodes may be in a privacy-sensitive location
//! (e.g., a journalist's laptop). Defaulting to off means the
//! operator opts into receiving BIND gossip for specific
//! `domain_id`s.
//!
//! ## Why informational?
//!
//! The libp2p-delivered BIND is not authoritative. The
//! authoritative BIND is the one delivered via the platform
//! group (where the DomainCoordinator has actual admin status).
//! The libp2p delivery is for pre-fetching only.

use std::collections::HashSet;
use std::sync::Mutex;

use crate::mon::bind_envelope::BindEnvelope;

/// Configuration for BIND gossip subscription.
#[derive(Clone, Debug, Default)]
pub struct BindGossipConfig {
    /// The `domain_id`s the operator wants to receive BIND gossip
    /// for. If empty, no gossip is received.
    pub subscribed_domains: HashSet<String>,
    /// Whether to forward received BINDs to other subscribers
    /// (default: false for privacy).
    pub forward_received: bool,
}

impl BindGossipConfig {
    /// Returns true if the config subscribes to `domain_id`.
    pub fn is_subscribed(&self, domain_id: &str) -> bool {
        self.subscribed_domains.contains(domain_id)
    }
}

/// State for the BIND gossip handler. Tracks received BINDs and
/// their delivery status.
#[derive(Debug, Default)]
pub struct BindGossipState {
    /// BIND envelopes received via libp2p, keyed by `domain_id`.
    received: Mutex<Vec<(String, BindEnvelope)>>,
    /// Total BIND envelopes received (statistic).
    received_count: Mutex<u64>,
    /// BIND envelopes delivered to the local mission handler.
    delivered_count: Mutex<u64>,
}

impl BindGossipState {
    pub fn new() -> Self {
        Self::default()
    }

    /// Returns the BIND envelopes received for `domain_id`.
    pub fn received_for(&self, domain_id: &str) -> Vec<BindEnvelope> {
        self.received
            .lock()
            .unwrap()
            .iter()
            .filter(|(d, _)| d == domain_id)
            .map(|(_, b)| b.clone())
            .collect()
    }

    /// Record a BIND envelope received via libp2p. Returns true
    /// if the envelope was newly received (not a duplicate).
    pub fn record_received(&self, envelope: BindEnvelope) -> bool {
        let domain_id = envelope.domain_id.clone();
        let mut received = self.received.lock().unwrap();
        if received.iter().any(|(d, b)| d == &domain_id && b == &envelope) {
            return false;
        }
        received.push((domain_id, envelope));
        *self.received_count.lock().unwrap() += 1;
        true
    }

    /// Mark a BIND envelope as delivered to the local mission
    /// handler. Used for statistics.
    pub fn mark_delivered(&self) {
        *self.delivered_count.lock().unwrap() += 1;
    }

    /// Returns the total number of BIND envelopes received.
    pub fn received_count(&self) -> u64 {
        *self.received_count.lock().unwrap()
    }

    /// Returns the total number of BIND envelopes delivered to
    /// the local mission handler.
    pub fn delivered_count(&self) -> u64 {
        *self.delivered_count.lock().unwrap()
    }
}

/// Derive the libp2p gossip topic for a `domain_id`.
///
/// Per the mission spec: `/dot/bind/{domain_id}`.
pub fn bind_gossip_topic(domain_id: &str) -> String {
    format!("/dot/bind/{domain_id}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gossip_topic_format() {
        assert_eq!(bind_gossip_topic("d1"), "/dot/bind/d1");
    }

    #[test]
    fn empty_config_subscribes_to_nothing() {
        let cfg = BindGossipConfig::default();
        assert!(!cfg.is_subscribed("d1"));
    }

    #[test]
    fn explicit_subscription_works() {
        let mut cfg = BindGossipConfig::default();
        cfg.subscribed_domains.insert("d1".into());
        assert!(cfg.is_subscribed("d1"));
        assert!(!cfg.is_subscribed("d2"));
    }

    #[test]
    fn record_received_dedupes() {
        let state = BindGossipState::new();
        let env = BindEnvelope::new("d1", "whatsapp", "g1");
        assert!(state.record_received(env.clone()));
        assert!(!state.record_received(env));
        assert_eq!(state.received_count(), 1);
    }

    #[test]
    fn different_envelopes_dedup_by_equality() {
        let state = BindGossipState::new();
        let env1 = BindEnvelope::new("d1", "whatsapp", "g1");
        let env2 = BindEnvelope::new("d2", "matrix", "g2");
        assert!(state.record_received(env1));
        assert!(state.record_received(env2));
        assert_eq!(state.received_count(), 2);
    }

    #[test]
    fn received_for_filters_by_domain() {
        let state = BindGossipState::new();
        state.record_received(BindEnvelope::new("d1", "whatsapp", "g1"));
        state.record_received(BindEnvelope::new("d1", "matrix", "g2"));
        state.record_received(BindEnvelope::new("d2", "telegram", "g3"));
        assert_eq!(state.received_for("d1").len(), 2);
        assert_eq!(state.received_for("d2").len(), 1);
        assert_eq!(state.received_for("d3").len(), 0);
    }

    #[test]
    fn mark_delivered_increments_counter() {
        let state = BindGossipState::new();
        state.mark_delivered();
        state.mark_delivered();
        state.mark_delivered();
        assert_eq!(state.delivered_count(), 3);
    }
}
