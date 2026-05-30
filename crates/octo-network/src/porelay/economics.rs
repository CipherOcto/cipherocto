//! Economic Integration (RFC-0860 §8)

use serde::{Deserialize, Serialize};

/// Reward distribution per proof type
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct RewardDistribution {
    /// OCTO-B per envelope forwarded
    pub octo_b_per_envelope: u64,
    /// OCTO-N per hour of uptime
    pub octo_n_per_hour: u64,
    /// OCTO-B per byte relayed
    pub octo_b_per_byte: u64,
    /// OCTO-N per compliant window
    pub octo_n_per_window: u64,
}

/// Slashing conditions (RFC-0860 §8.2)
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[repr(u16)]
pub enum SlashingCondition {
    /// Invalid proof submitted — 10% slashing
    InvalidProof = 0x0001,
    /// Proof replay attempt — 25% slashing
    ProofReplay = 0x0002,
    /// Consensus violation — 50% slashing + gateway ban
    ConsensusViolation = 0x0003,
    /// Sustained low availability (<50%) — reward reduction
    LowAvailability = 0x0004,
}

/// Slashing penalty rates (basis points of stake)
pub const SLASH_INVALID_PROOF: u64 = 1000; // 10%
pub const SLASH_PROOF_REPLAY: u64 = 2500; // 25%
pub const SLASH_CONSENSUS_VIOLATION: u64 = 5000; // 50%

impl RewardDistribution {
    /// Compute OCTO-B reward for forwarding
    pub fn forwarding_reward(&self, envelope_count: u64) -> u64 {
        envelope_count.saturating_mul(self.octo_b_per_envelope)
    }

    /// Compute OCTO-N reward for availability
    pub fn availability_reward(&self, uptime_hours: u64, availability_score: u16) -> u64 {
        uptime_hours
            .saturating_mul(self.octo_n_per_hour)
            .saturating_mul(availability_score as u64)
            .saturating_div(1000)
    }

    /// Compute OCTO-B reward for bandwidth
    pub fn bandwidth_reward(&self, bytes_relayed: u64) -> u64 {
        bytes_relayed.saturating_mul(self.octo_b_per_byte)
    }

    /// Compute OCTO-N reward for uptime
    pub fn uptime_reward(&self, compliant_windows: u64) -> u64 {
        compliant_windows.saturating_mul(self.octo_n_per_window)
    }

    /// Compute slashing amount
    pub fn slashing_amount(stake: u64, condition: SlashingCondition) -> u64 {
        let basis_points = match condition {
            SlashingCondition::InvalidProof => SLASH_INVALID_PROOF,
            SlashingCondition::ProofReplay => SLASH_PROOF_REPLAY,
            SlashingCondition::ConsensusViolation => SLASH_CONSENSUS_VIOLATION,
            SlashingCondition::LowAvailability => return 0, // reward reduction, not slashing
        };
        stake.saturating_mul(basis_points).saturating_div(10000)
    }

    /// Reward reduction for low availability.
    /// When availability_score < 500, reward is reduced proportionally:
    ///   reduced = base_reward * availability_score / 500
    /// When availability_score >= 500, full reward is returned.
    pub fn reward_reduction(base_reward: u64, availability_score: u16) -> u64 {
        if availability_score >= 500 {
            return base_reward;
        }
        base_reward
            .saturating_mul(availability_score as u64)
            .saturating_div(500)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn default_rewards() -> RewardDistribution {
        RewardDistribution {
            octo_b_per_envelope: 10,
            octo_n_per_hour: 100,
            octo_b_per_byte: 1,
            octo_n_per_window: 50,
        }
    }

    #[test]
    fn test_forwarding_reward() {
        assert_eq!(default_rewards().forwarding_reward(1000), 10_000);
    }

    #[test]
    fn test_availability_reward_full() {
        let reward = default_rewards().availability_reward(24, 1000);
        assert_eq!(reward, 2400);
    }

    #[test]
    fn test_bandwidth_reward() {
        assert_eq!(default_rewards().bandwidth_reward(102400), 102400);
    }

    #[test]
    fn test_slashing_invalid_proof() {
        assert_eq!(
            RewardDistribution::slashing_amount(10000, SlashingCondition::InvalidProof),
            1000
        );
    }

    #[test]
    fn test_slashing_consensus_violation() {
        assert_eq!(
            RewardDistribution::slashing_amount(10000, SlashingCondition::ConsensusViolation),
            5000
        );
    }

    #[test]
    fn test_slashing_low_availability_zero() {
        assert_eq!(
            RewardDistribution::slashing_amount(10000, SlashingCondition::LowAvailability),
            0
        );
    }
}
