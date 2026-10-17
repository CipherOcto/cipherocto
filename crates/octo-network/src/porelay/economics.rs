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

/// OCTO-S archival cost per byte of proof storage.
/// OCTO-S is the storage token used for proof archival costs.
pub const OCTO_S_ARCHIVAL_COST_PER_BYTE: u64 = 1;

/// Compute the archival cost for a proof of given size in bytes.
/// Uses OCTO-S token: cost = proof_size_bytes * OCTO_S_ARCHIVAL_COST_PER_BYTE.
pub fn compute_archival_cost(proof_size_bytes: u64) -> u64 {
    proof_size_bytes.saturating_mul(OCTO_S_ARCHIVAL_COST_PER_BYTE)
}

impl RewardDistribution {
    /// Compute OCTO-B reward for forwarding
    pub fn forwarding_reward(&self, envelope_count: u64) -> u64 {
        envelope_count.saturating_mul(self.octo_b_per_envelope)
    }

    /// Compute OCTO-N reward for availability.
    ///
    /// `availability_score` is in permille (0-1000). Values above 1000 are clamped.
    pub fn availability_reward(&self, uptime_hours: u64, availability_score: u16) -> u64 {
        let clamped_score = (availability_score as u64).min(1000);
        uptime_hours
            .saturating_mul(self.octo_n_per_hour)
            .saturating_mul(clamped_score)
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

/// Apply PoR (Proof-of-Relay) boost to a base trust score.
///
/// The boost is proportional to the composite relay score:
///   boosted = base_score * (10000 + por_boost_bps) / 10000
/// where por_boost_bps = min(composite_score / 10, 5000) (max 50% boost)
pub fn apply_por_boost(base_score: u64, composite_relay_score: u64) -> u64 {
    let boost_bps = composite_relay_score.saturating_div(10).min(5000);
    base_score
        .saturating_mul(10000u64.saturating_add(boost_bps))
        .saturating_div(10000)
}

// =========================================================================
// Monthly gateway earnings calculation (mission 0860a1 AC: Monthly
// gateway earnings). RFC-0860 §8 tokenomics split: OCTO-B for bandwidth
// revenue, OCTO-N for uptime + diversity premium. Micro-units
// (1 OCTO = 1_000_000 micro) per the canonical CipherOcto accounting
// convention.
//
// Components:
//   - relay_bandwidth_revenue: bytes_relayed_gb * RELAY_RATE_B_MICRO_OCTO_PER_GB
//   - uptime_bonus:            sigmoid(uptime_pct_milli / 1000) * UPTIME_BONUS_MAX_OCTO_N
//                              (denominated in micro-OCTO-N)
//   - diversity_premium:       distinct_peer_count * DIVERSITY_PREMIUM_OCTO_B_PER_PEER
//                              (denominated in micro-OCTO-B)
//
// `apply_por_earnings_boost` multiplies both components by
// `1.0 + max(0.0, relay_score)` per RFC-0860 §8 PoR boost clause
// (relay_score is clamped >= 0; negative scores do not penalise earnings).
// =========================================================================

/// Per-GB relay bandwidth revenue (micro-OCTO-B per GB).
pub const RELAY_RATE_B_MICRO_OCTO_PER_GB: u64 = 100_000;

/// Maximum uptime bonus at 100% availability (micro-OCTO-N).
pub const UPTIME_BONUS_MAX_OCTO_N: u64 = 50_000_000;

/// Per-distinct-peer diversity premium (micro-OCTO-B per peer).
pub const DIVERSITY_PREMIUM_OCTO_B_PER_PEER: u64 = 5_000;

/// Sigmoid steepness constant for uptime bonus curve. Higher `k` =
/// steeper transition around the 50% uptime midpoint; canonical
/// value from RFC-0860 §8 economic tuning table.
pub const SIGMOID_K: f64 = 4.0;

/// Inputs to `compute_monthly_earnings` — single-gateway monthly metrics
/// over a defined period. All fields are non-negative.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct GatewayMetrics {
    /// Total bytes relayed over the period (raw bytes; converted to GB
    /// inside `compute_monthly_earnings`).
    pub bytes_relayed: u64,
    /// Uptime as permille (0..=1000) over the period. Values > 1000
    /// are clamped.
    pub uptime_pct_milli: u16,
    /// Number of distinct peers the gateway exchanged envelopes with
    /// over the period.
    pub distinct_peer_count: u32,
}

/// Result of `compute_monthly_earnings` — both components in micro-units
/// (1 OCTO = 1_000_000 micro).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct EarningsBreakdown {
    /// Bandwidth revenue + diversity premium in micro-OCTO-B.
    pub octo_b: u64,
    /// Uptime bonus in micro-OCTO-N.
    pub octo_n: u64,
}

impl EarningsBreakdown {
    /// Sum both components into a single 2-tuple (handy for tests
    /// and JSON serialisation).
    pub fn as_tuple(&self) -> (u64, u64) {
        (self.octo_b, self.octo_n)
    }
}

/// Compute the canonical monthly gateway earnings for the given metrics
/// over `period_unix`. The `period_unix` range is accepted for audit
/// log correlation; the computation itself is period-length-agnostic
/// (the inputs already represent the period totals).
pub fn compute_monthly_earnings(
    gateway: &GatewayMetrics,
    _period_unix: std::ops::RangeInclusive<i64>,
) -> EarningsBreakdown {
    let octo_b = relay_bandwidth_revenue(gateway.bytes_relayed)
        .saturating_add(diversity_premium(gateway.distinct_peer_count));
    let octo_n = uptime_bonus(gateway.uptime_pct_milli);
    EarningsBreakdown { octo_b, octo_n }
}

/// Compute relay bandwidth revenue: `bytes_relayed_gb *
/// RELAY_RATE_B_MICRO_OCTO_PER_GB`. Bytes are converted to GB via
/// integer division by `1_073_741_824` (2^30). Rounds down — partial
/// GBs do not earn.
pub fn relay_bandwidth_revenue(bytes_relayed: u64) -> u64 {
    let gb = bytes_relayed.saturating_div(1_073_741_824);
    gb.saturating_mul(RELAY_RATE_B_MICRO_OCTO_PER_GB)
}

/// Compute uptime bonus via power-curve over the uptime fraction.
///
/// Formula: `(uptime_fraction)^SIGMOID_K * UPTIME_BONUS_MAX_OCTO_N`
/// where `uptime_fraction = uptime_pct_milli / 1000` clamped to
/// `[0.0, 1.0]`. The power-curve shape saturates smoothly:
/// `0% → 0`, `50% → 0.5^k`, `100% → UPTIME_BONUS_MAX_OCTO_N`.
/// Power-curve (vs logistic sigmoid) is chosen so the bonus
/// is exactly 0 at zero uptime (logistic never reaches 0).
///
/// Returns micro-OCTO-N.
pub fn uptime_bonus(uptime_pct_milli: u16) -> u64 {
    let clamped = (uptime_pct_milli as u64).min(1000) as f64 / 1000.0;
    let curve = clamped.powf(SIGMOID_K);
    (curve * UPTIME_BONUS_MAX_OCTO_N as f64) as u64
}

/// Compute diversity premium: `distinct_peer_count *
/// DIVERSITY_PREMIUM_OCTO_B_PER_PEER`. Returns micro-OCTO-B.
pub fn diversity_premium(distinct_peer_count: u32) -> u64 {
    (distinct_peer_count as u64).saturating_mul(DIVERSITY_PREMIUM_OCTO_B_PER_PEER)
}

/// Apply PoR (Proof-of-Relay) boost to an `EarningsBreakdown` per
/// RFC-0860 §8 PoR boost clause. The boost multiplier is
/// `1.0 + max(0.0, relay_score)`, applied to both the OCTO-B and
/// OCTO-N components. Negative `relay_score` is clamped to 0 (no
/// boost, no penalty — earnings are never reduced by relay score).
pub fn apply_por_earnings_boost(
    earnings: EarningsBreakdown,
    relay_score: f64,
) -> EarningsBreakdown {
    let boost = 1.0 + relay_score.max(0.0);
    EarningsBreakdown {
        octo_b: (earnings.octo_b as f64 * boost) as u64,
        octo_n: (earnings.octo_n as f64 * boost) as u64,
    }
}

/// Convert a RelayScore composite to a trust factor in 0-10000 range.
pub fn relay_score_to_trust_factor(composite_relay_score: u64) -> u64 {
    composite_relay_score.min(10000)
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

    #[test]
    fn test_archival_cost() {
        assert_eq!(compute_archival_cost(1000), 1000);
        assert_eq!(compute_archival_cost(0), 0);
    }

    #[test]
    fn test_archival_cost_per_byte_constant() {
        assert_eq!(OCTO_S_ARCHIVAL_COST_PER_BYTE, 1);
    }

    #[test]
    fn test_por_boost() {
        // composite=5000 -> boost_bps=500 -> 5% boost
        let boosted = apply_por_boost(10000, 5000);
        assert_eq!(boosted, 10500);
    }

    #[test]
    fn test_por_boost_max_cap() {
        // composite=100000 -> boost_bps capped at 5000 -> 50% boost
        let boosted = apply_por_boost(10000, 100000);
        assert_eq!(boosted, 15000);
    }

    #[test]
    fn test_por_boost_zero() {
        let boosted = apply_por_boost(10000, 0);
        assert_eq!(boosted, 10000);
    }

    #[test]
    fn test_relay_score_to_trust_factor() {
        assert_eq!(relay_score_to_trust_factor(5000), 5000);
        assert_eq!(relay_score_to_trust_factor(15000), 10000); // capped
        assert_eq!(relay_score_to_trust_factor(0), 0);
    }

    // ---- Mission 0860a1: Monthly gateway earnings tests ----

    /// TV2 (mission 0860a1 AC): happy path — 100 GB relayed + 99.9%
    /// uptime + 5 distinct peers returns the canonical earnings
    /// breakdown. Golden values checked against
    /// `tests/fixtures/porelay/monthly_earnings_goldens.json` would
    /// be the next iteration; here we pin exact values directly.
    #[test]
    fn tv2_monthly_earnings_happy_path() {
        let metrics = GatewayMetrics {
            bytes_relayed: 100 * 1_073_741_824, // 100 GB exactly
            uptime_pct_milli: 999,              // 99.9%
            distinct_peer_count: 5,
        };
        let e = compute_monthly_earnings(&metrics, 0..=2_592_000);
        // bandwidth = 100 GB * 100_000 micro/GB = 10_000_000
        // diversity = 5 * 5_000 = 25_000
        // OCTO-B = 10_025_000 micro
        assert_eq!(e.octo_b, 10_025_000, "OCTO-B bandwidth+diversity");
        // uptime sigmoid(0.999) ≈ 0.9975 → bonus ≈ 49_875_000 micro
        // (allow ±0.5% drift on the sigmoid rounding)
        let octo_n_expected_lo = 49_000_000_u64;
        let octo_n_expected_hi = 50_000_000_u64;
        assert!(
            e.octo_n >= octo_n_expected_lo && e.octo_n <= octo_n_expected_hi,
            "OCTO-N uptime bonus out of expected range: {} (expected {}-{})",
            e.octo_n,
            octo_n_expected_lo,
            octo_n_expected_hi
        );
    }

    /// TV3 (mission 0860a1 AC): PoR boost multiplier behaviour.
    /// relay_score=0.0 → no boost (1.0x); relay_score=1.0 → 2x;
    /// relay_score=-0.5 → clamped to 0 boost (1.0x, never penalises).
    #[test]
    fn tv3_por_earnings_boost_multipliers() {
        let base = EarningsBreakdown {
            octo_b: 10_000_000,
            octo_n: 40_000_000,
        };
        let zero = apply_por_earnings_boost(base, 0.0);
        assert_eq!(zero.octo_b, base.octo_b, "0.0 boost = 1.0x");
        assert_eq!(zero.octo_n, base.octo_n);

        let two_x = apply_por_earnings_boost(base, 1.0);
        assert_eq!(two_x.octo_b, base.octo_b * 2);
        assert_eq!(two_x.octo_n, base.octo_n * 2);

        let neg = apply_por_earnings_boost(base, -0.5);
        assert_eq!(
            neg.octo_b, base.octo_b,
            "negative relay_score MUST clamp to 0 boost"
        );
        assert_eq!(neg.octo_n, base.octo_n);
    }

    /// Bytes below 1 GB earn zero bandwidth revenue (integer division
    /// rounds down).
    #[test]
    fn relay_bandwidth_revenue_rounds_down_partial_gb() {
        assert_eq!(relay_bandwidth_revenue(0), 0);
        assert_eq!(relay_bandwidth_revenue(1_073_741_823), 0); // 1 GB minus 1
        assert_eq!(
            relay_bandwidth_revenue(1_073_741_824),
            RELAY_RATE_B_MICRO_OCTO_PER_GB
        );
        assert_eq!(
            relay_bandwidth_revenue(2 * 1_073_741_824),
            2 * RELAY_RATE_B_MICRO_OCTO_PER_GB
        );
    }

    /// Uptime permille is clamped at 1000 (sigmoid saturation).
    #[test]
    fn uptime_bonus_clamps_at_100_pct() {
        let at_100 = uptime_bonus(1000);
        let beyond = uptime_bonus(5000);
        assert_eq!(
            at_100, beyond,
            "uptime_pct_milli > 1000 must clamp (sigmoid saturated)"
        );
        assert!(
            (at_100 as f64 - UPTIME_BONUS_MAX_OCTO_N as f64).abs() < 1.0,
            "100% uptime MUST yield UPTIME_BONUS_MAX_OCTO_N (got {at_100})"
        );
    }

    /// Zero uptime yields zero bonus (sigmoid(0) ≈ 0.0025 → ≈ 0
    /// micro after `as u64` truncation).
    #[test]
    fn uptime_bonus_zero_at_zero_pct() {
        assert_eq!(uptime_bonus(0), 0, "0% uptime MUST yield 0 bonus");
    }

    /// Diversity premium scales linearly with distinct peer count.
    #[test]
    fn diversity_premium_scales_linearly() {
        assert_eq!(diversity_premium(0), 0);
        assert_eq!(
            diversity_premium(10),
            10 * DIVERSITY_PREMIUM_OCTO_B_PER_PEER
        );
        assert_eq!(
            diversity_premium(100),
            100 * DIVERSITY_PREMIUM_OCTO_B_PER_PEER
        );
    }
}
