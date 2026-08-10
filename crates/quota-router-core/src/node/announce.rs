use super::provider::{NetworkId, ProviderCapacity, RouterNodeId};

/// Paid-query pricing policy advertised by a router node (RFC-0871
/// Phase 5 + RFC-0957 §Algorithms).
///
/// Attached to `RouterAnnouncePayload::pricing_policy` so wallets
/// can construct `PaymentCaveat` chains matching the announced
/// drain rate. The drain rate is a u128 to match the spend-ledger
/// arithmetic type (RFC-0871 §Adversary A7 — overflow impossible
/// at worst-case scale).
///
/// `accepted_payment_capabilities` is a set of macaroon root-ids
/// the router will honor; empty set means "no capability gating,
/// rate-limit only". `settlement_recipient` identifies the
/// router's settlement address (placeholder `WireDid` until
/// RFC-0862 on-chain binding lands per mission phase J).
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct PricingPolicy {
    /// Micro-OCTO_W cost per single query at this router.
    pub drain_per_query: u128,
    /// Macaroon root-ids the router accepts as payment; empty =
    /// rate-limit only (no paid-query gating).
    pub accepted_payment_capabilities: Vec<[u8; 16]>,
    /// Optional settlement recipient (placeholder DID string; uses
    /// `String` not `WireDid` because `WireDid` is borsh-only and
    /// `RouterAnnouncePayload` is serde-JSON canonicalized for HMAC).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub settlement_recipient: Option<String>,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct RouterAnnouncePayload {
    pub node_id: RouterNodeId,
    pub network_id: NetworkId,
    pub supported_models: Vec<String>,
    pub capacities: Vec<ProviderCapacity>,
    pub timestamp: u64,
    pub hmac: [u8; 32],
    /// Optional pricing policy (mission 0871e-phase5c). `serde(default)`
    /// keeps backward compat with legacy announce payloads that
    /// predate the field — they decode to `None` and wallets
    /// treat the announce as rate-limit-only.
    #[serde(default)]
    pub pricing_policy: Option<PricingPolicy>,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct RouterWithdrawPayload {
    pub node_id: RouterNodeId,
    pub reason: WithdrawReason,
    pub timestamp: u64,
    pub hmac: [u8; 32],
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub enum WithdrawReason {
    Graceful,
    Maintenance,
    Decommissioned,
}

pub trait SignedPayload {
    fn compute_hmac(&self, network_key: &[u8; 32]) -> [u8; 32];
    fn verify_hmac(&self, network_key: &[u8; 32]) -> bool;
}

impl SignedPayload for RouterAnnouncePayload {
    fn compute_hmac(&self, network_key: &[u8; 32]) -> [u8; 32] {
        let mut clone = self.clone();
        clone.hmac = [0u8; 32];
        let bytes = serde_json::to_vec(&clone).expect("infallible");
        *blake3::keyed_hash(network_key, &bytes).as_bytes()
    }
    fn verify_hmac(&self, network_key: &[u8; 32]) -> bool {
        let expected = self.compute_hmac(network_key);
        constant_time_eq(&self.hmac, &expected)
    }
}

/// Single-source-of-truth builder for `RouterAnnouncePayload`
/// (mission 0871-phase5-router-dispatch-wiring).
///
/// Construct via [`RouterAnnounceBuilder::new`], then call
/// [`RouterAnnounceBuilder::build`] to produce the canonical payload
/// + signed HMAC (when `network_key` is non-zero). Replaces the
/// 4-specialized-node inline struct literals (each duplicated the
/// announce construction with drift risk).
#[allow(clippy::doc_lazy_continuation)]
#[derive(Clone, Debug)]
pub struct RouterAnnounceBuilder {
    node_id: RouterNodeId,
    network_id: NetworkId,
    supported_models: Vec<String>,
    capacities: Vec<ProviderCapacity>,
    pricing_policy: Option<PricingPolicy>,
    timestamp: u64,
}

impl RouterAnnounceBuilder {
    /// Construct a new builder with the current wall-clock timestamp.
    #[must_use]
    pub fn new(node_id: RouterNodeId, network_id: NetworkId) -> Self {
        Self {
            node_id,
            network_id,
            supported_models: Vec::new(),
            capacities: Vec::new(),
            pricing_policy: None,
            timestamp: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_secs())
                .unwrap_or(0),
        }
    }

    /// Set the supported-models list.
    #[must_use]
    pub fn supported_models(mut self, models: Vec<String>) -> Self {
        self.supported_models = models;
        self
    }

    /// Set the provider capacities list.
    #[must_use]
    pub fn capacities(mut self, capacities: Vec<ProviderCapacity>) -> Self {
        self.capacities = capacities;
        self
    }

    /// Set the pricing policy (mission 0871e-phase5c).
    #[must_use]
    pub fn pricing_policy(mut self, policy: Option<PricingPolicy>) -> Self {
        self.pricing_policy = policy;
        self
    }

    /// Override the timestamp (for test vectors + golden fixtures).
    #[must_use]
    pub fn timestamp(mut self, timestamp: u64) -> Self {
        self.timestamp = timestamp;
        self
    }

    /// Build the canonical payload + sign the HMAC.
    ///
    /// When `network_key` is the all-zero sentinel, no signing is
    /// performed (HMAC stays zero — Phase 1 MVP compatibility).
    /// Production deployments pass a non-zero key per RFC-0870
    /// §Announce HMAC.
    #[must_use]
    pub fn build(self, network_key: &[u8; 32]) -> RouterAnnouncePayload {
        let mut payload = RouterAnnouncePayload {
            node_id: self.node_id,
            network_id: self.network_id,
            supported_models: self.supported_models,
            capacities: self.capacities,
            timestamp: self.timestamp,
            hmac: [0u8; 32],
            pricing_policy: self.pricing_policy,
        };
        if *network_key != [0u8; 32] {
            payload.hmac = payload.compute_hmac(network_key);
        }
        payload
    }
}

impl SignedPayload for RouterWithdrawPayload {
    fn compute_hmac(&self, network_key: &[u8; 32]) -> [u8; 32] {
        let mut clone = self.clone();
        clone.hmac = [0u8; 32];
        let bytes = serde_json::to_vec(&clone).expect("infallible");
        *blake3::keyed_hash(network_key, &bytes).as_bytes()
    }
    fn verify_hmac(&self, network_key: &[u8; 32]) -> bool {
        let expected = self.compute_hmac(network_key);
        constant_time_eq(&self.hmac, &expected)
    }
}

impl SignedPayload for super::gossip::CapacityGossipPayload {
    fn compute_hmac(&self, network_key: &[u8; 32]) -> [u8; 32] {
        let mut clone = self.clone();
        clone.hmac = [0u8; 32];
        let bytes = serde_json::to_vec(&clone).expect("infallible");
        *blake3::keyed_hash(network_key, &bytes).as_bytes()
    }
    fn verify_hmac(&self, network_key: &[u8; 32]) -> bool {
        let expected = self.compute_hmac(network_key);
        constant_time_eq(&self.hmac, &expected)
    }
}

// ForwardRequestPayload lives in `forward.rs` to avoid a cyclic import
// (forward depends on provider, and announce also depends on provider).
// The impl is registered here so the `SignedPayload` trait surface stays
// in one module.
impl SignedPayload for super::forward::ForwardRequestPayload {
    fn compute_hmac(&self, network_key: &[u8; 32]) -> [u8; 32] {
        let mut clone = self.clone();
        clone.hmac = [0u8; 32];
        let bytes = serde_json::to_vec(&clone).expect("infallible");
        *blake3::keyed_hash(network_key, &bytes).as_bytes()
    }
    fn verify_hmac(&self, network_key: &[u8; 32]) -> bool {
        let expected = self.compute_hmac(network_key);
        constant_time_eq(&self.hmac, &expected)
    }
}

/// Constant-time comparison of two byte arrays.
/// XORs all bytes and accumulates — total time depends only on length,
/// not on the values being compared.
fn constant_time_eq(a: &[u8; 32], b: &[u8; 32]) -> bool {
    let mut diff = 0u8;
    for i in 0..32 {
        diff |= a[i] ^ b[i];
    }
    diff == 0
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_key() -> [u8; 32] {
        [42u8; 32]
    }

    #[test]
    fn announce_hmac_roundtrip() {
        let mut announce = RouterAnnouncePayload {
            node_id: RouterNodeId([1u8; 32]),
            network_id: NetworkId([2u8; 32]),
            supported_models: vec!["gpt-4o".into()],
            capacities: vec![],
            timestamp: 100,
            hmac: [0u8; 32],
            pricing_policy: None,
        };
        announce.hmac = announce.compute_hmac(&test_key());
        assert!(announce.verify_hmac(&test_key()));
    }

    #[test]
    fn announce_hmac_wrong_key() {
        let mut announce = RouterAnnouncePayload {
            node_id: RouterNodeId([1u8; 32]),
            network_id: NetworkId([2u8; 32]),
            supported_models: vec![],
            capacities: vec![],
            timestamp: 100,
            hmac: [0u8; 32],
            pricing_policy: None,
        };
        announce.hmac = announce.compute_hmac(&test_key());
        assert!(!announce.verify_hmac(&[99u8; 32]));
    }

    /// TV (mission 0871e-phase5c) — `pricing_policy` presence changes
    /// the HMAC. Two payloads differing only in `pricing_policy`
    /// produce different HMACs; a third-party attacker cannot mutate
    /// the pricing without invalidating the signature.
    #[test]
    fn pricing_policy_changes_hmac() {
        let key = test_key();
        let mut with_policy = RouterAnnouncePayload {
            node_id: RouterNodeId([1u8; 32]),
            network_id: NetworkId([2u8; 32]),
            supported_models: vec!["gpt-4o".into()],
            capacities: vec![],
            timestamp: 100,
            hmac: [0u8; 32],
            pricing_policy: Some(PricingPolicy {
                drain_per_query: 1_000,
                accepted_payment_capabilities: vec![],
                settlement_recipient: None,
            }),
        };
        with_policy.hmac = with_policy.compute_hmac(&key);
        let mut without_policy = with_policy.clone();
        without_policy.pricing_policy = None;
        without_policy.hmac = [0u8; 32];
        without_policy.hmac = without_policy.compute_hmac(&key);
        assert_ne!(
            with_policy.hmac, without_policy.hmac,
            "pricing_policy presence must change HMAC"
        );
        assert!(with_policy.verify_hmac(&key));
        assert!(without_policy.verify_hmac(&key));
    }

    /// TV (mission 0871-phase5-router-dispatch-wiring) — the
    /// `RouterAnnounceBuilder` produces byte-equal payloads across
    /// invocations with the same inputs + fixed timestamp. The
    /// 5 broadcast paths (QuotaRouterNode + 4 specialized nodes)
    /// delegate to this builder; the TV guarantees wire-form
    /// stability across all callers.
    #[test]
    fn router_announce_builder_byte_equality() {
        let key = [0x42u8; 32];
        let node_id = RouterNodeId([0x07; 32]);
        let network_id = NetworkId([0x08; 32]);
        let a = RouterAnnounceBuilder::new(node_id, network_id)
            .supported_models(vec!["gpt-4o".into()])
            .capacities(vec![])
            .pricing_policy(Some(PricingPolicy {
                drain_per_query: 1_000,
                accepted_payment_capabilities: vec![],
                settlement_recipient: None,
            }))
            .timestamp(1_700_000_000)
            .build(&key);
        let b = RouterAnnounceBuilder::new(node_id, network_id)
            .supported_models(vec!["gpt-4o".into()])
            .capacities(vec![])
            .pricing_policy(Some(PricingPolicy {
                drain_per_query: 1_000,
                accepted_payment_capabilities: vec![],
                settlement_recipient: None,
            }))
            .timestamp(1_700_000_000)
            .build(&key);
        assert_eq!(a.hmac, b.hmac);
        assert!(a.verify_hmac(&key));
        assert!(b.verify_hmac(&key));
        assert!(serde_json::to_vec(&a).unwrap() == serde_json::to_vec(&b).unwrap());
    }

    #[test]
    fn withdraw_hmac_roundtrip() {
        let mut withdraw = RouterWithdrawPayload {
            node_id: RouterNodeId([1u8; 32]),
            reason: WithdrawReason::Graceful,
            timestamp: 100,
            hmac: [0u8; 32],
        };
        withdraw.hmac = withdraw.compute_hmac(&test_key());
        assert!(withdraw.verify_hmac(&test_key()));
    }

    #[test]
    fn gossip_hmac_roundtrip() {
        let mut gossip = super::super::gossip::CapacityGossipPayload {
            sender_id: RouterNodeId([1u8; 32]),
            timestamp: 100,
            capacities: vec![],
            known_peers: vec![],
            hmac: [0u8; 32],
        };
        gossip.hmac = gossip.compute_hmac(&test_key());
        assert!(gossip.verify_hmac(&test_key()));
    }

    #[test]
    fn gossip_hmac_wrong_key() {
        let mut gossip = super::super::gossip::CapacityGossipPayload {
            sender_id: RouterNodeId([1u8; 32]),
            timestamp: 100,
            capacities: vec![],
            known_peers: vec![],
            hmac: [0u8; 32],
        };
        gossip.hmac = gossip.compute_hmac(&test_key());
        assert!(!gossip.verify_hmac(&[99u8; 32]));
    }

    #[test]
    fn gossip_hmac_differs_per_sender() {
        let make = |sender: u8| {
            let mut g = super::super::gossip::CapacityGossipPayload {
                sender_id: RouterNodeId([sender; 32]),
                timestamp: 100,
                capacities: vec![],
                known_peers: vec![],
                hmac: [0u8; 32],
            };
            g.hmac = g.compute_hmac(&test_key());
            g
        };
        let g1 = make(1);
        let g2 = make(2);
        assert_ne!(g1.hmac, g2.hmac);
    }
}
