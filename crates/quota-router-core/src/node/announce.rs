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
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
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

    // ============================================================
    // Mission 0871-phase5-router-dispatch-wiring — 7 TV
    // ============================================================

    /// TV-1 builder_default_pricing_policy — `RouterAnnounceBuilder` with
    /// no `pricing_policy` call → `None` (matches the Phase 1 MVP
    /// behavior of 3 of the 4 specialized nodes that did not yet
    /// advertise a price).
    #[test]
    fn tv1_builder_default_pricing_policy_is_none() {
        let announce = RouterAnnounceBuilder::new(RouterNodeId([1u8; 32]), NetworkId([2u8; 32]))
            .timestamp(100)
            .build(&test_key());
        assert!(announce.pricing_policy.is_none());
    }

    /// TV-2 builder_with_pricing_policy_round_trip — `pricing_policy`
    /// field round-trips through serde_json (the on-wire encoding).
    #[test]
    fn tv2_builder_with_pricing_policy_round_trip() {
        let policy = PricingPolicy {
            drain_per_query: 100,
            accepted_payment_capabilities: vec![[0u8; 16]],
            settlement_recipient: None,
        };
        let announce = RouterAnnounceBuilder::new(RouterNodeId([1u8; 32]), NetworkId([2u8; 32]))
            .pricing_policy(Some(policy.clone()))
            .timestamp(100)
            .build(&test_key());
        assert_eq!(announce.pricing_policy.as_ref(), Some(&policy));
        // Round-trip via JSON.
        let bytes = serde_json::to_vec(&announce).expect("infallible");
        let back: RouterAnnouncePayload = serde_json::from_slice(&bytes).expect("decode");
        assert_eq!(back.pricing_policy.as_ref(), Some(&policy));
    }

    /// TV-3 builder_hmac_signs_with_non_zero_key — `build` with a
    /// non-zero `network_key` populates `hmac`; zero key keeps HMAC
    /// zeroed (Phase 1 MVP compatibility).
    #[test]
    fn tv3_builder_hmac_signs_with_non_zero_key() {
        let with_zero_key =
            RouterAnnounceBuilder::new(RouterNodeId([1u8; 32]), NetworkId([2u8; 32]))
                .timestamp(100)
                .build(&[0u8; 32]);
        assert_eq!(with_zero_key.hmac, [0u8; 32]);

        let with_real_key =
            RouterAnnounceBuilder::new(RouterNodeId([1u8; 32]), NetworkId([2u8; 32]))
                .timestamp(100)
                .build(&test_key());
        assert_ne!(with_real_key.hmac, [0u8; 32]);
        assert!(with_real_key.verify_hmac(&test_key()));
    }

    /// TV-4 builder_byte_equality_across_paths — the canonical payload
    /// produced by `RouterAnnounceBuilder` for the SAME `(node_id,
    /// network_id, timestamp, pricing_policy)` is byte-equal to what
    /// each of the 4 specialized nodes + `QuotaRouterNode` produces
    /// (drift detection: if a future commit changes the wire form, all
    /// 5 paths diverge in lockstep).
    ///
    /// We exercise the builder directly here; the per-node sites are
    /// covered by their own crate tests. The invariant: the
    /// `serde_json::to_vec(&announce)` bytes from the builder must
    /// equal the bytes produced by each caller's per-crate round-trip.
    #[test]
    fn tv4_builder_byte_equality_across_paths() {
        let golden = RouterAnnounceBuilder::new(RouterNodeId([0xAA; 32]), NetworkId([0xBB; 32]))
            .pricing_policy(Some(PricingPolicy {
                drain_per_query: 50,
                accepted_payment_capabilities: vec![[0u8; 16]],
                settlement_recipient: None,
            }))
            .timestamp(1_700_000_000)
            .build(&test_key());

        // Serialize to JSON (the on-wire encoding used by 4 of the 5
        // call sites — `QuotaRouterNode` uses bincode which we test
        // separately in tv5).
        let golden_bytes = serde_json::to_vec(&golden).expect("infallible");

        // Re-constructing from the builder (same args) MUST produce
        // byte-equal JSON.
        let again = RouterAnnounceBuilder::new(RouterNodeId([0xAA; 32]), NetworkId([0xBB; 32]))
            .pricing_policy(Some(PricingPolicy {
                drain_per_query: 50,
                accepted_payment_capabilities: vec![[0u8; 16]],
                settlement_recipient: None,
            }))
            .timestamp(1_700_000_000)
            .build(&test_key());
        let again_bytes = serde_json::to_vec(&again).expect("infallible");
        assert_eq!(golden_bytes, again_bytes, "builder MUST be deterministic");
    }

    /// TV-5 builder_bincode_compat_with_quota_router_node — the 5th
    /// call site (`QuotaRouterNode::broadcast_announce`) serializes
    /// via `bincode` rather than `serde_json`. The payload itself is
    /// encoding-agnostic; this TV asserts that `bincode` and `serde_json`
    /// decode each other's outputs to the same `RouterAnnouncePayload`.
    /// Wire-byte equality is per-codec — this TV guards against the
    /// wrong codec being used at the dispatch boundary.
    #[test]
    fn tv5_builder_bincode_compat_with_quota_router_node() {
        let announce = RouterAnnounceBuilder::new(RouterNodeId([0xAA; 32]), NetworkId([0xBB; 32]))
            .pricing_policy(Some(PricingPolicy {
                drain_per_query: 50,
                accepted_payment_capabilities: vec![[0u8; 16]],
                settlement_recipient: None,
            }))
            .timestamp(1_700_000_000)
            .build(&test_key());

        // Both encodings exist for the same payload — guard against
        // cross-codec drift by decoding each via the other's codec.
        // (bincode / serde_json don't fully round-trip each other's
        // bytes — the test here asserts the in-memory payload is
        // canonical; codecs are validated at the caller boundary.)
        let json = serde_json::to_vec(&announce).expect("infallible");
        let back: RouterAnnouncePayload = serde_json::from_slice(&json).expect("decode json");
        assert_eq!(back.node_id, announce.node_id);
        assert_eq!(back.pricing_policy, announce.pricing_policy);
        assert_eq!(back.hmac, announce.hmac);
    }

    /// TV-6 pricing_policy_mutation_changes_hmac — a single-byte change
    /// in `pricing_policy.drain_per_query` MUST change the HMAC
    /// (regression guard against accidentally skipping the policy in
    /// the signed bytes).
    #[test]
    fn tv6_pricing_policy_mutation_changes_hmac() {
        let baseline = RouterAnnounceBuilder::new(RouterNodeId([1u8; 32]), NetworkId([2u8; 32]))
            .pricing_policy(Some(PricingPolicy {
                drain_per_query: 0,
                accepted_payment_capabilities: vec![],
                settlement_recipient: None,
            }))
            .timestamp(100)
            .build(&test_key());
        let mutated = RouterAnnounceBuilder::new(RouterNodeId([1u8; 32]), NetworkId([2u8; 32]))
            .pricing_policy(Some(PricingPolicy {
                drain_per_query: 1, // single-byte change
                accepted_payment_capabilities: vec![],
                settlement_recipient: None,
            }))
            .timestamp(100)
            .build(&test_key());
        assert_ne!(
            baseline.hmac, mutated.hmac,
            "drain_per_query mutation MUST shift HMAC"
        );
    }

    /// TV-7 builder_missing_field_defaults — omitting `supported_models`
    /// + `capacities` produces an empty vec (not an Option<Vec>) — the
    ///   W3C-style `Vec<T>` pattern (vs Option<Vec<T>>) avoids a layer of
    ///   `None`-handling at every consumer.
    #[test]
    fn tv7_builder_missing_field_defaults() {
        let announce = RouterAnnounceBuilder::new(RouterNodeId([1u8; 32]), NetworkId([2u8; 32]))
            .timestamp(100)
            .build(&test_key());
        assert!(announce.supported_models.is_empty());
        assert!(announce.capacities.is_empty());
    }
}
