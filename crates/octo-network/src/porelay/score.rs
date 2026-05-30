//! Relay Score Model (RFC-0860 §4)

use serde::{Deserialize, Serialize};

/// Score weights in basis points (total = 1000)
pub const WEIGHT_FORWARDING: u64 = 300;
pub const WEIGHT_AVAILABILITY: u64 = 250;
pub const WEIGHT_BANDWIDTH: u64 = 200;
pub const WEIGHT_UPTIME: u64 = 150;
pub const WEIGHT_DIVERSITY: u64 = 100;

/// RelayScore — combines all proof types into a single trust metric.
///
/// 9 fields per RFC-0860 §4.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[repr(C)]
pub struct RelayScore {
    /// Gateway being scored
    pub gateway_id: [u8; 32],
    /// Computation epoch
    pub epoch: u64,
    /// Forwarding score (0-1000)
    pub forwarding_score: u16,
    /// Availability score (0-1000)
    pub availability_score: u16,
    /// Bandwidth score (0-1000)
    pub bandwidth_score: u16,
    /// Uptime score (0-1000)
    pub uptime_score: u16,
    /// Diversity bonus (0-500)
    pub diversity_bonus: u16,
    /// Stake multiplier (1000 = 1.0x, 2000 = 2.0x)
    pub stake_multiplier: u32,
    /// Composite score (computed)
    /// composite = (forwarding * 300 + availability * 250 + bandwidth * 200 + uptime * 150 + diversity * 100) * stake_multiplier / 1000
    pub composite: u64,
}

/// Default stake multiplier (1.0x)
pub const DEFAULT_STAKE_MULTIPLIER: u32 = 1000;

/// Maximum stake multiplier (10.0x)
pub const MAX_STAKE_MULTIPLIER: u32 = 10000;

impl RelayScore {
    /// Compute composite score using integer basis point arithmetic (Class A).
    ///
    /// Formula: raw_score = forwarding * 300 + availability * 250 + bandwidth * 200
    ///          + uptime * 150 + diversity * 100
    ///          composite = raw_score * stake_multiplier / 1000
    ///
    /// All component scores must be 0-1000. Out-of-range values are clamped.
    pub fn compute_composite(&mut self) {
        // Clamp all component scores to 0-1000
        self.forwarding_score = self.forwarding_score.min(1000);
        self.availability_score = self.availability_score.min(1000);
        self.bandwidth_score = self.bandwidth_score.min(1000);
        self.uptime_score = self.uptime_score.min(1000);
        self.diversity_bonus = self.diversity_bonus.min(500); // max 500 per RFC

        // Cap stake multiplier to MAX_STAKE_MULTIPLIER
        let stake_mult = self.stake_multiplier.min(MAX_STAKE_MULTIPLIER);

        let raw = (self.forwarding_score as u64)
            .saturating_mul(WEIGHT_FORWARDING)
            .saturating_add((self.availability_score as u64).saturating_mul(WEIGHT_AVAILABILITY))
            .saturating_add((self.bandwidth_score as u64).saturating_mul(WEIGHT_BANDWIDTH))
            .saturating_add((self.uptime_score as u64).saturating_mul(WEIGHT_UPTIME))
            .saturating_add((self.diversity_bonus as u64).saturating_mul(WEIGHT_DIVERSITY));

        self.composite = raw.saturating_mul(stake_mult as u64).saturating_div(1000);
    }

    /// Compute score decay for inactive gateways.
    /// effective_score = current_score * 950^epochs / 1000^epochs
    pub fn decay_score(current_score: u64, epochs_inactive: u32) -> u64 {
        if epochs_inactive == 0 {
            return current_score;
        }
        let mut score = current_score;
        for _ in 0..epochs_inactive {
            score = score.saturating_mul(950).saturating_div(1000);
        }
        score
    }

    /// Compute stake multiplier from OCTO-B stake amount.
    /// stake_multiplier = 1000 + min(staked / STAKE_UNIT, max_boost)
    /// where max_boost is capped so the result never exceeds MAX_STAKE_MULTIPLIER.
    pub fn compute_stake_multiplier(staked: u64, stake_unit: u64, max_boost: u32) -> u32 {
        if stake_unit == 0 {
            return DEFAULT_STAKE_MULTIPLIER;
        }
        let max_boost = max_boost.min(MAX_STAKE_MULTIPLIER - DEFAULT_STAKE_MULTIPLIER);
        let boost = (staked / stake_unit).min(max_boost as u64) as u32;
        DEFAULT_STAKE_MULTIPLIER.saturating_add(boost)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_composite_score_basic() {
        let mut score = RelayScore {
            gateway_id: [0u8; 32],
            epoch: 1,
            forwarding_score: 1000,
            availability_score: 1000,
            bandwidth_score: 1000,
            uptime_score: 1000,
            diversity_bonus: 0,
            stake_multiplier: 1000,
            composite: 0,
        };
        score.compute_composite();
        // raw = 1000*300 + 1000*250 + 1000*200 + 1000*150 + 0*100 = 900000
        // composite = 900000 * 1000 / 1000 = 900000
        assert_eq!(score.composite, 900_000);
    }

    #[test]
    fn test_composite_score_with_stake() {
        let mut score = RelayScore {
            gateway_id: [0u8; 32],
            epoch: 1,
            forwarding_score: 500,
            availability_score: 500,
            bandwidth_score: 500,
            uptime_score: 500,
            diversity_bonus: 500,
            stake_multiplier: 2000, // 2.0x
            composite: 0,
        };
        score.compute_composite();
        // raw = 500*300 + 500*250 + 500*200 + 500*150 + 500*100 = 500000
        // composite = 500000 * 2000 / 1000 = 1000000
        assert_eq!(score.composite, 1_000_000);
    }

    #[test]
    fn test_decay_score() {
        let score = RelayScore::decay_score(1000, 10);
        // 1000 * 0.95^10 ≈ 598
        assert!(score < 600);
        assert!(score > 590);
    }

    #[test]
    fn test_decay_score_zero_epochs() {
        assert_eq!(RelayScore::decay_score(1000, 0), 1000);
    }

    #[test]
    fn test_stake_multiplier_basic() {
        assert_eq!(RelayScore::compute_stake_multiplier(0, 100, 5000), 1000);
        assert_eq!(RelayScore::compute_stake_multiplier(500, 100, 5000), 1005);
        assert_eq!(
            RelayScore::compute_stake_multiplier(500000, 100, 5000),
            6000
        );
    }

    #[test]
    fn test_stake_multiplier_capped() {
        assert_eq!(
            RelayScore::compute_stake_multiplier(u64::MAX, 1, 5000),
            6000
        );
    }
}
