//! Proof-of-Relay (PoRelay) — RFC-0860
//!
//! Cryptographic proof of relay participation for economic validation,
//! trust scoring, and Sybil resistance.

pub mod aggregation;
pub mod anti_sybil;
pub mod availability;
pub mod bandwidth;
pub mod economics;
pub mod error;
pub mod forwarding;
pub mod heartbeat;
pub mod registry;
pub mod score;
pub mod uptime;

pub use aggregation::AggregatedRelayProof;
pub use anti_sybil::{DiversityConstraint, SybilAnalysis};
pub use availability::AvailabilityProof;
pub use bandwidth::BandwidthProof;
pub use economics::{RewardDistribution, SlashingCondition};
pub use error::PoRelayError;
pub use forwarding::ForwardingProof;
pub use heartbeat::{GatewayHeartbeat, LoadClass, UptimeClass};
pub use registry::TrustRegistry;
pub use score::{apply_por_boost, relay_score_to_trust_factor, RelayScore};
pub use uptime::UptimeProof;

/// Extends GatewayAdvertisement with relay proofs (RFC-0860 §10)
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct GatewayAdvertisementWithPoR {
    /// Current relay score
    pub relay_score: u64,
    /// Proof commitment (Merkle root of recent proofs)
    pub proof_commitment: [u8; 32],
    /// Stake amount (OCTO-B)
    pub staked_octo_b: u64,
}

/// RFC-0008 Execution Class Mapping (RFC-0860 §2)
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[repr(u16)]
pub enum PoRelayOperation {
    /// Forwarding proof generation — Class C
    ForwardingProofGeneration = 0x0001,
    /// Availability proof generation — Class C
    AvailabilityProofGeneration = 0x0002,
    /// Bandwidth proof generation — Class C
    BandwidthProofGeneration = 0x0003,
    /// Uptime proof generation — Class C
    UptimeProofGeneration = 0x0004,
    /// Forwarding proof verification — Class A
    ForwardingProofVerification = 0x0005,
    /// Availability proof verification — Class A
    AvailabilityProofVerification = 0x0006,
    /// Bandwidth proof verification — Class A
    BandwidthProofVerification = 0x0007,
    /// Uptime proof verification — Class A
    UptimeProofVerification = 0x0008,
    /// Trust score computation — Class A
    TrustScoreComputation = 0x0009,
    /// Gateway heartbeat verification — Class A
    GatewayHeartbeatVerification = 0x000A,
    /// Score decay computation — Class A
    ScoreDecayComputation = 0x000B,
    /// Stake multiplier computation — Class A
    StakeMultiplierComputation = 0x000C,
    /// Proof archival — Class B
    ProofArchival = 0x000D,
    /// Reward distribution — Class A
    RewardDistribution = 0x000E,
}

#[cfg(test)]
mod tests {
    use super::score::RelayScore;
    use super::*;

    #[test]
    fn test_gateway_advertisement_with_por() {
        let adv = GatewayAdvertisementWithPoR {
            relay_score: 900_000,
            proof_commitment: [0xAAu8; 32],
            staked_octo_b: 10_000,
        };
        assert_eq!(adv.relay_score, 900_000);
        assert_eq!(adv.staked_octo_b, 10_000);
    }

    #[test]
    fn test_porelay_operation_enum() {
        assert_eq!(PoRelayOperation::ForwardingProofGeneration as u16, 0x0001);
        assert_eq!(PoRelayOperation::RewardDistribution as u16, 0x000E);
    }

    #[test]
    fn test_end_to_end_score_computation() {
        let mut score = RelayScore {
            gateway_id: [0x42u8; 32],
            epoch: 1,
            forwarding_score: 800,
            availability_score: 950,
            bandwidth_score: 700,
            uptime_score: 900,
            diversity_bonus: 400,
            stake_multiplier: 1500,
            composite: 0,
        };
        score.compute_composite();
        // raw = 800*300 + 950*250 + 700*200 + 900*150 + 400*100
        //     = 240000 + 237500 + 140000 + 135000 + 40000 = 792500
        // composite = 792500 * 1500 / 1000 = 1188750
        assert_eq!(score.composite, 1_188_750);
    }
}
