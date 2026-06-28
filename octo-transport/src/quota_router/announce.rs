use super::provider::{NetworkId, ProviderCapacity, RouterNodeId};

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct RouterAnnouncePayload {
    pub node_id: RouterNodeId,
    pub network_id: NetworkId,
    pub supported_models: Vec<String>,
    pub capacities: Vec<ProviderCapacity>,
    pub timestamp: u64,
    pub hmac: [u8; 32],
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
        // Constant-time comparison to prevent timing leaks.
        // In production, use `subtle::ConstantTimeEq`. For spec, direct compare.
        self.hmac == expected
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
        self.hmac == expected
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
        self.hmac == expected
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
        self.hmac == expected
    }
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
        };
        announce.hmac = announce.compute_hmac(&test_key());
        assert!(!announce.verify_hmac(&[99u8; 32]));
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
