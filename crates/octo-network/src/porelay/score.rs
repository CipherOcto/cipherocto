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

/// PoR boost multiplier for high-scoring relays (1000 = 1.0x, 1500 = 1.5x).
const POR_BOOST_BASELINE: u64 = 1000;
const POR_BOOST_MAX: u64 = 2000; // 2.0x max boost
const POR_BOOST_COMPOSITE_THRESHOLD: u64 = 500_000;

/// Apply a Proof-of-Relay boost multiplier to a base score.
///
/// Relays with composite scores above the threshold receive a boost
/// proportional to their relay score, capped at POR_BOOST_MAX.
///
/// boost_factor = baseline + (composite - threshold) / threshold * (max - baseline)
/// clamped to [baseline, max]
pub fn apply_por_boost(base_score: u64, relay_score: &RelayScore) -> u64 {
    let composite = relay_score.composite;
    if composite < POR_BOOST_COMPOSITE_THRESHOLD {
        return base_score;
    }

    let excess = composite.saturating_sub(POR_BOOST_COMPOSITE_THRESHOLD);
    let boost_range = POR_BOOST_MAX.saturating_sub(POR_BOOST_BASELINE);
    let boost_factor = POR_BOOST_BASELINE.saturating_add(
        excess
            .saturating_mul(boost_range)
            .saturating_div(POR_BOOST_COMPOSITE_THRESHOLD),
    );
    let clamped = boost_factor.min(POR_BOOST_MAX);
    base_score
        .saturating_mul(clamped)
        .saturating_div(POR_BOOST_BASELINE)
}

/// Convert a RelayScore to a trust factor in the 0-10000 range.
///
/// Maps the composite score to a normalized trust value where:
/// - 0 = untrusted (composite 0)
/// - 10000 = maximum trust (composite >= max_composite)
///
/// The mapping uses a logarithmic-ish scaling with a reference composite
/// of 1,000,000 (typical high-performing relay with 1.0x stake).
pub fn relay_score_to_trust_factor(relay_score: &RelayScore) -> u64 {
    const REFERENCE_COMPOSITE: u64 = 1_000_000;
    const MAX_TRUST: u64 = 10_000;

    if relay_score.composite == 0 {
        return 0;
    }

    let scaled = (relay_score.composite as u128)
        .saturating_mul(MAX_TRUST as u128)
        .saturating_div(REFERENCE_COMPOSITE as u128);
    (scaled.min(MAX_TRUST as u128)) as u64
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

    #[test]
    fn test_por_boost_below_threshold() {
        let score = make_score(500_000 - 1);
        assert_eq!(apply_por_boost(10_000, &score), 10_000);
    }

    #[test]
    fn test_por_boost_at_threshold() {
        let score = make_score(500_000);
        assert_eq!(apply_por_boost(10_000, &score), 10_000);
    }

    #[test]
    fn test_por_boost_above_threshold() {
        let score = make_score(750_000);
        let boosted = apply_por_boost(10_000, &score);
        assert!(boosted > 10_000);
        assert!(boosted <= 20_000); // max 2x
    }

    #[test]
    fn test_por_boost_max_cap() {
        let score = make_score(u64::MAX);
        let boosted = apply_por_boost(10_000, &score);
        assert_eq!(boosted, 20_000); // capped at 2x
    }

    #[test]
    fn test_por_boost_zero_score() {
        let score = make_score(0);
        assert_eq!(apply_por_boost(10_000, &score), 10_000);
    }

    #[test]
    fn test_trust_factor_zero_composite() {
        let score = make_score(0);
        assert_eq!(relay_score_to_trust_factor(&score), 0);
    }

    #[test]
    fn test_trust_factor_reference() {
        let score = make_score(1_000_000);
        assert_eq!(relay_score_to_trust_factor(&score), 10_000);
    }

    #[test]
    fn test_trust_factor_half() {
        let score = make_score(500_000);
        assert_eq!(relay_score_to_trust_factor(&score), 5_000);
    }

    #[test]
    fn test_trust_factor_capped() {
        let score = make_score(2_000_000);
        assert_eq!(relay_score_to_trust_factor(&score), 10_000);
    }

    fn make_score(composite: u64) -> RelayScore {
        RelayScore {
            gateway_id: [0u8; 32],
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
}
